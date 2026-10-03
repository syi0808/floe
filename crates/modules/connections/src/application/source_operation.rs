//! Durable source/Access coordination across two independent stores. A missing
//! receipt is not an abort; unresolved operations keep their source fenced.
use crate::{
    ConnectionsCommandFailure, SourceAbortReason, SourceConnection, SourceOperationAdmission,
    SourceOperationChange, SourceOperationExpectation, SourceOperationKind, SourceOperationPhase,
    SourceOperationProof, SourceOperationRecord, SourceOperationReservation, SourceRepairReason,
    SourceRepositoryError,
};
use floe_access::{
    AccessService, ConnectionReview, GrantAbort, GrantAbortOutcome, GrantCommitReceipt,
    GrantOperationReceipt, GrantReceiptQuery, ReviewRef, SourceExpectation,
    SourceReservationEvidence,
};
use floe_context_contract::{SourceAccessBlockers, SourceAccessRequirement};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

pub struct PrepareProjectionReviews {
    pub run_id: floe_kernel::RunId,
    pub projection_operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub blockers: SourceAccessBlockers,
}
pub struct PreparedProjectionReviews {
    pub projection_operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub reviews: Vec<ConnectionReview>,
}
pub struct PrepareSourceReviews {
    pub run_id: floe_kernel::RunId,
    pub operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub blockers: SourceAccessBlockers,
}
pub struct PreparedSourceReviews {
    pub operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub reviews: Vec<ConnectionReview>,
}
/// Observes native/provider continuity outside any SQL transaction. Only the
/// actual source adapter can supply the subject and exact physical resources.
pub trait SourceReviewEvidence: Send + Sync {
    fn observe<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceExpectation, AgentFailure>>;
    fn inspect_selection<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        selected_resources: &'a [crate::ConnectionResource],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<String, AgentFailure>>;
}
pub enum SourceCleanupOutcome {
    Completed,
    Uncertain,
}
pub trait SourceCleanup: Send + Sync {
    fn disconnect<'a>(
        &'a self,
        operation: &'a SourceOperationRecord,
        receipt: &'a GrantCommitReceipt,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceCleanupOutcome, AgentFailure>>;
}
pub struct ConnectionsDependencies {
    pub sources: Arc<dyn crate::ConnectionsRepository>,
    pub access: Arc<AccessService>,
    pub evidence: Arc<dyn SourceReviewEvidence>,
    pub cleanup: Arc<dyn SourceCleanup>,
    pub pairing: Arc<crate::GatewayPairingService>,
    pub gateways: Arc<dyn crate::GatewayRegistry>,
    pub remote_integrations: Arc<dyn crate::RemoteIntegrationPort>,
    pub products: Arc<dyn crate::ConnectionsProductRepository>,
    pub source_catalog: Arc<dyn crate::SourceCatalogPort>,
    pub native_setup: Arc<dyn crate::NativeSourceSetupPort>,
}
#[derive(Clone)]
pub struct ConnectionsService {
    pub(super) sources: Arc<dyn crate::ConnectionsRepository>,
    pub(super) access: Arc<AccessService>,
    pub(super) evidence: Arc<dyn SourceReviewEvidence>,
    pub(super) cleanup: Arc<dyn SourceCleanup>,
    pub(super) pairing: Arc<crate::GatewayPairingService>,
    pub(super) gateways: Arc<dyn crate::GatewayRegistry>,
    pub(super) remote_integrations: Arc<dyn crate::RemoteIntegrationPort>,
    pub(super) products: Arc<dyn crate::ConnectionsProductRepository>,
    pub(super) source_catalog: Arc<dyn crate::SourceCatalogPort>,
    pub(super) native_setup: Arc<dyn crate::NativeSourceSetupPort>,
    pub(super) native_active: Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
    pub(super) jobs_active: Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
    pub(super) jobs_cancel:
        Arc<std::sync::Mutex<std::collections::HashMap<Uuid, floe_execution::Cancellation>>>,
    pub(super) closed: Arc<std::sync::atomic::AtomicBool>,
    pub(super) catalog_status: Arc<std::sync::Mutex<Option<Result<(), AgentFailure>>>>,
    pub(super) catalog_gateway: Arc<std::sync::Mutex<Option<Uuid>>>,
}
impl ConnectionsService {
    pub fn new(dependencies: ConnectionsDependencies) -> Self {
        Self {
            sources: dependencies.sources,
            access: dependencies.access,
            evidence: dependencies.evidence,
            cleanup: dependencies.cleanup,
            pairing: dependencies.pairing,
            gateways: dependencies.gateways,
            remote_integrations: dependencies.remote_integrations,
            products: dependencies.products,
            source_catalog: dependencies.source_catalog,
            native_setup: dependencies.native_setup,
            native_active: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            jobs_active: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            jobs_cancel: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            catalog_status: Arc::new(std::sync::Mutex::new(None)),
            catalog_gateway: Arc::new(std::sync::Mutex::new(None)),
        }
    }
    pub fn shutdown(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        if let Ok(jobs) = self.jobs_cancel.lock() {
            for cancellation in jobs.values() {
                cancellation.cancel();
            }
        }
    }
    /// Close admission, cancel owned work, then wait only within the caller's
    /// shutdown budget. An unfinished durable operation remains recoverable.
    pub async fn shutdown_and_drain(&self, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        self.shutdown();
        loop {
            if self
                .jobs_active
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .is_empty()
            {
                return Ok(());
            }
            if scope.cancellation().is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if scope.deadline() <= tokio::time::Instant::now() {
                return Err(AgentFailure::DeadlineExceeded);
            }
            tokio::select! {
                _=tokio::time::sleep_until(scope.deadline())=>return Err(AgentFailure::DeadlineExceeded),
                _=tokio::time::sleep(std::time::Duration::from_millis(10))=>{},
            }
        }
    }
    pub(super) fn register_job(
        &self,
        id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let mut jobs = self
            .jobs_cancel
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        self.ensure_open()?;
        jobs.insert(id, scope.cancellation().clone());
        Ok(())
    }
    pub(super) fn ensure_open(&self) -> Result<(), AgentFailure> {
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            Err(AgentFailure::Interrupted)
        } else {
            Ok(())
        }
    }
    pub async fn prepare_source_reviews(
        &self,
        actor: &OwnerActor,
        request: PrepareSourceReviews,
        scope: &ExecutionScope,
    ) -> Result<PreparedSourceReviews, AgentFailure> {
        let prepared = self
            .prepare_projection_reviews(
                actor,
                PrepareProjectionReviews {
                    run_id: request.run_id,
                    projection_operation_id: request.operation_id,
                    target_digest: request.target_digest,
                    blockers: request.blockers,
                },
                scope,
            )
            .await?;
        Ok(PreparedSourceReviews {
            operation_id: prepared.projection_operation_id,
            target_digest: prepared.target_digest,
            reviews: prepared.reviews,
        })
    }
    pub async fn prepare_projection_reviews(
        &self,
        actor: &OwnerActor,
        request: PrepareProjectionReviews,
        scope: &ExecutionScope,
    ) -> Result<PreparedProjectionReviews, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        request
            .blockers
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if request.projection_operation_id.is_nil() || request.target_digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        let mut groups: std::collections::BTreeMap<String, Vec<SourceAccessRequirement>> =
            std::collections::BTreeMap::new();
        for blocker in request.blockers.blockers() {
            let id = blocker
                .connection_id()
                .ok_or(AgentFailure::AccessReviewRequired)?
                .as_str()
                .to_owned();
            let group = groups.entry(id).or_default();
            if !group.contains(blocker) {
                group.push(blocker.clone());
            }
        }
        let mut reviews = Vec::new();
        for (id, requirements) in groups {
            let id = floe_context_contract::ConnectionId::try_new(id)
                .map_err(|_| AgentFailure::InvalidInput)?;
            let origin = floe_access::ProjectionReviewOrigin::for_requirements(
                request.run_id,
                request.projection_operation_id,
                request.target_digest,
                id.clone(),
                &requirements,
            )?;
            if let Some(review) = self
                .access
                .find_projection_source_review(actor, origin.clone(), scope)
                .await?
            {
                reviews.push(review);
                continue;
            }
            if self
                .sources
                .source_is_fenced(actor.person_id, &id)
                .await
                .map_err(source_error)?
            {
                return Err(AgentFailure::Conflict);
            }
            let source = self
                .sources
                .load(actor.person_id, &id)
                .await
                .map_err(source_error)?
                .ok_or(AgentFailure::AccessReviewRequired)?;
            if requirements.iter().any(|blocker| {
                blocker.connector_id() != Some(source.connector_id())
                    || blocker.source_authority() != Some(source.source_authority())
            }) {
                return Err(AgentFailure::Conflict);
            }
            let expected = self.evidence.observe(actor, &source, scope).await?;
            if expected.revision != Some(source.revision())
                || expected.authority != source.source_authority()
                || expected.source.person_id() != actor.person_id
                || expected.source.connection_id() != *source.connection_id()
                || expected.source.connector() != source.connector_id()
                || expected.source.execution_owner() != source.execution_owner_id()
                || requirements.iter().any(|blocker| {
                    !blocker.source_resources().is_empty()
                        && expected.physical_resources != blocker.source_resources()
                })
            {
                return Err(AgentFailure::Conflict);
            }
            let review = self
                .access
                .prepare_projection_source_review(actor, origin, expected, requirements, scope)
                .await?;
            reviews.push(review);
        }
        Ok(PreparedProjectionReviews {
            projection_operation_id: request.projection_operation_id,
            target_digest: request.target_digest,
            reviews,
        })
    }
    pub async fn operation_receipt(
        &self,
        actor: &OwnerActor,
        operation: &SourceOperationRecord,
        scope: &ExecutionScope,
    ) -> Result<Option<GrantOperationReceipt>, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let current = self
            .sources
            .load_operation(operation.operation_id)
            .await
            .map_err(source_error)?
            .ok_or(AgentFailure::Conflict)?;
        if current != *operation
            || current.device_id != actor.device_id
            || current.expected.source.person_id() != actor.person_id
        {
            return Err(AgentFailure::Conflict);
        }
        let receipt = self
            .access
            .receipt(
                actor,
                GrantReceiptQuery {
                    identity: current.identity(),
                },
                scope,
            )
            .await?;
        if let Some(GrantOperationReceipt::Committed(receipt)) = &receipt {
            if !current.matches_evidence(&receipt.reservation) {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(receipt)
    }
    pub async fn apply_source_review(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        reference: ReviewRef,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, ConnectionsCommandFailure> {
        if command_id.is_nil() {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::InvalidInput,
            ));
        }
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let intent = intent_digest(&("observe", command_id, &reference))
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let identity = crate::ConnectionsCommandIdentity {
            journal: crate::ConnectionsCommandJournal::SourceOperation,
            record_ref: derived_id(b"floe.source.operation.v1", actor.person_id, command_id),
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            command_id,
            intent_digest: intent,
        };
        let command_result: Result<SourceOperationRecord, ConnectionsCommandFailure> = async {
            if let Some(replay) = self.replay(actor, command_id, intent, scope).await? {
                return Ok(replay);
            }
            let review = self
                .access
                .inspect_review(actor, reference.clone(), scope)
                .await
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            let current = self
                .sources
                .load(actor.person_id, &review.source.source.connection_id())
                .await
                .map_err(source_error)
                .map_err(ConnectionsCommandFailure::NotAdmitted)?
                .ok_or(AgentFailure::Conflict)
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            let observed = self
                .evidence
                .observe(actor, &current, scope)
                .await
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            if observed != review.source {
                return Err(ConnectionsCommandFailure::NotAdmitted(
                    AgentFailure::Conflict,
                ));
            }
            let (admission, reservation) = self
                .reserve(
                    actor,
                    command_id,
                    intent,
                    Some(reference.clone()),
                    observed,
                    SourceOperationKind::ConnectionReviewApply,
                )
                .await?;
            let result: Result<SourceOperationRecord, AgentFailure> = async {
                if !admission.record.phase.holds_fence() {
                    return Ok(admission.record);
                }
                // Always read the immutable receipt following a possibly lost commit
                // response. There is no second grant application in this invocation.
                let applied = self
                    .access
                    .apply_review(actor, reference, reservation.clone(), scope)
                    .await;
                match self
                    .access
                    .receipt(
                        actor,
                        GrantReceiptQuery {
                            identity: reservation.identity(),
                        },
                        scope,
                    )
                    .await?
                {
                    Some(GrantOperationReceipt::Committed(receipt)) => {
                        self.complete_committed(actor, admission.record, receipt, None, scope)
                            .await
                    }
                    Some(GrantOperationReceipt::Aborted(_)) => Err(AgentFailure::Conflict),
                    None => Err(applied.err().unwrap_or(AgentFailure::StorageUnavailable)),
                }
            }
            .await;
            result.map_err(ConnectionsCommandFailure::Admitted)
        }
        .await;
        self.finish_source_command(command_result, identity).await
    }
    pub async fn disconnect_source_operation(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_id: &floe_context_contract::ConnectionId,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, ConnectionsCommandFailure> {
        if command_id.is_nil() {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::InvalidInput,
            ));
        }
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let intent = intent_digest(&("disconnect", command_id, source_id, expected_revision))
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let identity = crate::ConnectionsCommandIdentity {
            journal: crate::ConnectionsCommandJournal::SourceOperation,
            record_ref: derived_id(b"floe.source.operation.v1", actor.person_id, command_id),
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            command_id,
            intent_digest: intent,
        };
        let command_result: Result<SourceOperationRecord, ConnectionsCommandFailure> = async {
            if let Some(replay) = self.replay(actor, command_id, intent, scope).await? {
                return Ok(replay);
            }
            let source = self
                .sources
                .load(actor.person_id, source_id)
                .await
                .map_err(source_error)
                .map_err(ConnectionsCommandFailure::NotAdmitted)?
                .ok_or(AgentFailure::CapabilityUnavailable)
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            if source.revision() != expected_revision {
                return Err(ConnectionsCommandFailure::NotAdmitted(
                    AgentFailure::Conflict,
                ));
            }
            let expected = self
                .evidence
                .observe(actor, &source, scope)
                .await
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            let (admission, reservation) = self
                .reserve(
                    actor,
                    command_id,
                    intent,
                    None,
                    expected,
                    SourceOperationKind::ConnectionDisconnect,
                )
                .await?;
            let result: Result<SourceOperationRecord, AgentFailure> = async {
                if !admission.record.phase.holds_fence() {
                    return Ok(admission.record);
                }
                let result = self
                    .access
                    .disconnect(actor, reservation.clone(), scope)
                    .await;
                match self
                    .access
                    .receipt(
                        actor,
                        GrantReceiptQuery {
                            identity: reservation.identity(),
                        },
                        scope,
                    )
                    .await?
                {
                    Some(GrantOperationReceipt::Committed(receipt)) => {
                        let mut successor = source;
                        successor
                            .disconnect(expected_revision)
                            .map_err(|_| AgentFailure::Conflict)?;
                        self.complete_committed(
                            actor,
                            admission.record,
                            receipt,
                            Some(successor),
                            scope,
                        )
                        .await
                    }
                    _ => Err(result.err().unwrap_or(AgentFailure::StorageUnavailable)),
                }
            }
            .await;
            result.map_err(ConnectionsCommandFailure::Admitted)
        }
        .await;
        self.finish_source_command(command_result, identity).await
    }
    pub(super) async fn reserve(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        digest: [u8; 32],
        review: Option<ReviewRef>,
        source: SourceExpectation,
        kind: SourceOperationKind,
    ) -> Result<(SourceOperationAdmission, SourceReservationEvidence), ConnectionsCommandFailure>
    {
        if command_id.is_nil() {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::InvalidInput,
            ));
        }
        actor
            .validate()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        source
            .validate()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        if source.source.person_id() != actor.person_id {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::PolicyDenied,
            ));
        }
        let operation_id = derived_id(
            b"floe.source.operation.v1",
            source.source.person_id(),
            command_id,
        );
        let reservation_id = derived_id(
            b"floe.source.reservation.v1",
            source.source.person_id(),
            operation_id,
        );
        let reservation = SourceReservationEvidence {
            device_id: actor.device_id.clone(),
            operation_id,
            command_id,
            request_digest: digest,
            reservation_id,
            reservation_generation: 1,
            source: source.clone(),
        };
        reservation
            .validate()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let record = SourceOperationRecord {
            device_id: actor.device_id.clone(),
            operation_id,
            command_id,
            request_digest: digest,
            reservation_id,
            reservation_generation: 1,
            expected: SourceOperationExpectation {
                source: source.source,
                revision: source.revision,
                authority: source.authority,
            },
            review,
            kind,
            phase: SourceOperationPhase::Reserved,
            revision: 1,
        };
        let admission = self
            .sources
            .reserve(SourceOperationReservation { record })
            .await
            .map_err(source_error)
            .map_err(ConnectionsCommandFailure::Indeterminate)?;
        Ok((admission, reservation))
    }
    pub(super) async fn complete_committed(
        &self,
        actor: &OwnerActor,
        mut operation: SourceOperationRecord,
        receipt: GrantCommitReceipt,
        successor: Option<SourceConnection>,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, AgentFailure> {
        if operation.device_id != actor.device_id
            || operation.expected.source.person_id() != actor.person_id
            || !operation.matches_evidence(&receipt.reservation)
        {
            return Err(AgentFailure::Conflict);
        }
        if matches!(operation.phase, SourceOperationPhase::Completed { .. }) {
            return Ok(operation);
        }
        if matches!(operation.phase, SourceOperationPhase::Reserved) {
            operation = self
                .advance(
                    &operation,
                    SourceOperationPhase::GrantCommitted {
                        receipt_id: receipt.commit_id,
                        receipt_digest: receipt.digest()?,
                    },
                    SourceOperationProof::Committed(receipt.clone()),
                    None,
                )
                .await?;
        }
        let current = self
            .sources
            .load(
                operation.expected.source.person_id(),
                &operation.expected.source.connection_id(),
            )
            .await
            .map_err(source_error)?;
        if !operation.expected.matches(current.as_ref()) {
            return self
                .advance(
                    &operation,
                    SourceOperationPhase::RepairRequired {
                        receipt_id: receipt.commit_id,
                        reason: SourceRepairReason::SourceChanged,
                    },
                    SourceOperationProof::Committed(receipt),
                    None,
                )
                .await;
        }
        if let Some(current) = current
            .as_ref()
            .filter(|source| source.state() != crate::SourceState::Pending)
        {
            let fresh = self.evidence.observe(actor, current, scope).await?;
            if fresh != receipt.reservation.source {
                return self
                    .advance(
                        &operation,
                        SourceOperationPhase::RepairRequired {
                            receipt_id: receipt.commit_id,
                            reason: SourceRepairReason::SourceChanged,
                        },
                        SourceOperationProof::Committed(receipt),
                        None,
                    )
                    .await;
            }
        }
        if operation.kind == SourceOperationKind::ConnectionDisconnect
            && receipt.reservation.source.gateway.is_some()
        {
            if matches!(
                operation.phase,
                SourceOperationPhase::GrantCommitted { .. }
                    | SourceOperationPhase::RepairRequired {
                        reason: SourceRepairReason::CleanupUncertain,
                        ..
                    }
            ) {
                operation = self
                    .advance(
                        &operation,
                        SourceOperationPhase::CleaningUp {
                            receipt_id: receipt.commit_id,
                            cleanup_revision: 1,
                        },
                        SourceOperationProof::Committed(receipt.clone()),
                        None,
                    )
                    .await?;
            }
            match self.cleanup.disconnect(&operation, &receipt, scope).await? {
                SourceCleanupOutcome::Completed => {}
                SourceCleanupOutcome::Uncertain => {
                    return self
                        .advance(
                            &operation,
                            SourceOperationPhase::RepairRequired {
                                receipt_id: receipt.commit_id,
                                reason: SourceRepairReason::CleanupUncertain,
                            },
                            SourceOperationProof::Committed(receipt),
                            None,
                        )
                        .await;
                }
            }
        }
        self.advance(
            &operation,
            SourceOperationPhase::Completed {
                receipt_id: receipt.commit_id,
            },
            SourceOperationProof::Committed(receipt),
            successor,
        )
        .await
    }
    async fn advance(
        &self,
        operation: &SourceOperationRecord,
        next_phase: SourceOperationPhase,
        proof: SourceOperationProof,
        successor: Option<SourceConnection>,
    ) -> Result<SourceOperationRecord, AgentFailure> {
        self.ensure_open()?;
        self.sources
            .compare_and_swap_operation(SourceOperationChange {
                operation_id: operation.operation_id,
                expected_revision: operation.revision,
                expected_source: operation.expected.clone(),
                next_phase,
                proof,
                successor,
            })
            .await
            .map_err(source_error)
    }
    async fn finish_source_command<T>(
        &self,
        result: Result<T, ConnectionsCommandFailure>,
        identity: crate::ConnectionsCommandIdentity,
    ) -> Result<T, ConnectionsCommandFailure> {
        match result {
            Ok(value) => Ok(value),
            Err(
                failure @ (ConnectionsCommandFailure::Admitted(_)
                | ConnectionsCommandFailure::NotApplied(_)),
            ) => Err(failure),
            Err(failure) => {
                let reason = failure.into_failure();
                match self
                    .sources
                    .reject_unadmitted_operation_command(identity, reason)
                    .await
                {
                    Ok(crate::ConnectionsCommandResolution::NotApplied(reason)) => {
                        Err(ConnectionsCommandFailure::NotApplied(reason))
                    }
                    Ok(crate::ConnectionsCommandResolution::Admitted) => {
                        Err(ConnectionsCommandFailure::Admitted(reason))
                    }
                    Err(error) => Err(ConnectionsCommandFailure::Indeterminate(source_error(
                        error,
                    ))),
                }
            }
        }
    }
    async fn replay(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        intent: [u8; 32],
        scope: &ExecutionScope,
    ) -> Result<Option<SourceOperationRecord>, ConnectionsCommandFailure> {
        let id = derived_id(b"floe.source.operation.v1", actor.person_id, command_id);
        if let Some(reason) = self
            .sources
            .rejected_operation_command(crate::ConnectionsCommandIdentity {
                journal: crate::ConnectionsCommandJournal::SourceOperation,
                record_ref: id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                command_id,
                intent_digest: intent,
            })
            .await
            .map_err(source_error)
            .map_err(ConnectionsCommandFailure::Indeterminate)?
        {
            return Err(ConnectionsCommandFailure::NotApplied(reason));
        }
        let Some(operation) = self
            .sources
            .load_operation(id)
            .await
            .map_err(source_error)
            .map_err(ConnectionsCommandFailure::NotAdmitted)?
        else {
            return Ok(None);
        };
        if operation.command_id != command_id
            || operation.request_digest != intent
            || operation.device_id != actor.device_id
            || operation.expected.source.person_id() != actor.person_id
        {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::Conflict,
            ));
        }
        if !operation.phase.holds_fence() {
            return Ok(Some(operation));
        }
        Ok(Some(
            self.reconcile(actor, id, None, scope)
                .await
                .map_err(ConnectionsCommandFailure::Admitted)?,
        ))
    }
    pub async fn pause_observe_operation(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        source_id: &floe_context_contract::ConnectionId,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, ConnectionsCommandFailure> {
        if command_id.is_nil() {
            return Err(ConnectionsCommandFailure::NotAdmitted(
                AgentFailure::InvalidInput,
            ));
        }
        self.ensure_open()
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        check(actor, scope).map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let intent = intent_digest(&("pause_observe", command_id, source_id, expected_revision))
            .map_err(ConnectionsCommandFailure::NotAdmitted)?;
        let identity = crate::ConnectionsCommandIdentity {
            journal: crate::ConnectionsCommandJournal::SourceOperation,
            record_ref: derived_id(b"floe.source.operation.v1", actor.person_id, command_id),
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            command_id,
            intent_digest: intent,
        };
        let command_result: Result<SourceOperationRecord, ConnectionsCommandFailure> = async {
            if let Some(replay) = self.replay(actor, command_id, intent, scope).await? {
                return Ok(replay);
            }
            let source = self
                .sources
                .load(actor.person_id, source_id)
                .await
                .map_err(source_error)
                .map_err(ConnectionsCommandFailure::NotAdmitted)?
                .ok_or(AgentFailure::CapabilityUnavailable)
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            if source.revision() != expected_revision {
                return Err(ConnectionsCommandFailure::NotAdmitted(
                    AgentFailure::Conflict,
                ));
            }
            let expected = self
                .evidence
                .observe(actor, &source, scope)
                .await
                .map_err(ConnectionsCommandFailure::NotAdmitted)?;
            let (admission, reservation) = self
                .reserve(
                    actor,
                    command_id,
                    intent,
                    None,
                    expected,
                    SourceOperationKind::ConnectionObservePause,
                )
                .await?;
            let result: Result<SourceOperationRecord, AgentFailure> = async {
                let applied = self
                    .access
                    .pause_observe(actor, reservation.clone(), scope)
                    .await;
                match self
                    .access
                    .receipt(
                        actor,
                        GrantReceiptQuery {
                            identity: reservation.identity(),
                        },
                        scope,
                    )
                    .await?
                {
                    Some(GrantOperationReceipt::Committed(receipt)) => {
                        self.complete_committed(actor, admission.record, receipt, None, scope)
                            .await
                    }
                    _ => Err(applied.err().unwrap_or(AgentFailure::StorageUnavailable)),
                }
            }
            .await;
            result.map_err(ConnectionsCommandFailure::Admitted)
        }
        .await;
        self.finish_source_command(command_result, identity).await
    }
    pub async fn reconcile(
        &self,
        actor: &OwnerActor,
        operation_id: Uuid,
        abort_reason: Option<SourceAbortReason>,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        let operation = self
            .sources
            .load_operation(operation_id)
            .await
            .map_err(source_error)?
            .ok_or(AgentFailure::InvalidInput)?;
        if operation.device_id != actor.device_id
            || operation.expected.source.person_id() != actor.person_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if !operation.phase.holds_fence() {
            return Ok(operation);
        }
        if operation.kind == SourceOperationKind::ConnectionConfigure && abort_reason.is_none() {
            return Ok(operation);
        }
        let identity = operation.identity();
        match self
            .access
            .receipt(
                actor,
                GrantReceiptQuery {
                    identity: identity.clone(),
                },
                scope,
            )
            .await?
        {
            Some(GrantOperationReceipt::Committed(receipt)) => {
                self.reconcile_committed(actor, operation, receipt, scope)
                    .await
            }
            Some(GrantOperationReceipt::Aborted(receipt)) => {
                self.advance(
                    &operation,
                    SourceOperationPhase::Aborted {
                        reason: abort_reason.unwrap_or(SourceAbortReason::ReviewChanged),
                    },
                    SourceOperationProof::Aborted(receipt),
                    None,
                )
                .await
            }
            None => {
                let Some(reason) = abort_reason else {
                    if operation.kind != SourceOperationKind::ConnectionReviewApply {
                        return Ok(operation);
                    }
                    let reference = operation.review.clone().ok_or(AgentFailure::Conflict)?;
                    let expected = self
                        .access
                        .replay_review_source(actor, reference.clone(), scope)
                        .await?;
                    let current = self
                        .sources
                        .load(actor.person_id, &operation.expected.source.connection_id())
                        .await
                        .map_err(source_error)?
                        .ok_or(AgentFailure::Conflict)?;
                    if !operation.expected.matches(Some(&current))
                        || self.evidence.observe(actor, &current, scope).await? != expected
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    let reservation = SourceReservationEvidence {
                        device_id: operation.device_id.clone(),
                        operation_id: operation.operation_id,
                        command_id: operation.command_id,
                        request_digest: operation.request_digest,
                        reservation_id: operation.reservation_id,
                        reservation_generation: operation.reservation_generation,
                        source: expected,
                    };
                    let attempted = self
                        .access
                        .apply_review(actor, reference, reservation, scope)
                        .await;
                    return match self
                        .access
                        .receipt(actor, GrantReceiptQuery { identity }, scope)
                        .await?
                    {
                        Some(GrantOperationReceipt::Committed(receipt)) => {
                            self.reconcile_committed(actor, operation, receipt, scope)
                                .await
                        }
                        _ => Err(attempted.err().unwrap_or(AgentFailure::StorageUnavailable)),
                    };
                };
                match self
                    .access
                    .abort(actor, GrantAbort { identity }, scope)
                    .await?
                {
                    GrantAbortOutcome::Aborted(receipt) => {
                        self.advance(
                            &operation,
                            SourceOperationPhase::Aborted { reason },
                            SourceOperationProof::Aborted(receipt),
                            None,
                        )
                        .await
                    }
                    GrantAbortOutcome::AlreadyCommitted(receipt) => {
                        self.reconcile_committed(actor, operation, receipt, scope)
                            .await
                    }
                }
            }
        }
    }
    async fn reconcile_committed(
        &self,
        actor: &OwnerActor,
        operation: SourceOperationRecord,
        receipt: GrantCommitReceipt,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, AgentFailure> {
        if !operation.matches_evidence(&receipt.reservation) {
            return Err(AgentFailure::Conflict);
        }
        // A configuration successor is persisted in the encrypted product
        // record. Generic cancellation must not release its source fence.
        if operation.kind == SourceOperationKind::ConnectionConfigure {
            return Ok(operation);
        }
        let successor = if operation.kind == SourceOperationKind::ConnectionDisconnect {
            let current = self
                .sources
                .load(
                    operation.expected.source.person_id(),
                    &operation.expected.source.connection_id(),
                )
                .await
                .map_err(source_error)?;
            if !operation.expected.matches(current.as_ref()) {
                return self
                    .advance(
                        &operation,
                        SourceOperationPhase::RepairRequired {
                            receipt_id: receipt.commit_id,
                            reason: SourceRepairReason::SourceChanged,
                        },
                        SourceOperationProof::Committed(receipt),
                        None,
                    )
                    .await;
            }
            let mut source = current.ok_or(AgentFailure::Conflict)?;
            source
                .disconnect(operation.expected.revision.ok_or(AgentFailure::Conflict)?)
                .map_err(|_| AgentFailure::Conflict)?;
            Some(source)
        } else {
            None
        };
        self.complete_committed(actor, operation, receipt, successor, scope)
            .await
    }
}
pub(super) fn source_error(error: SourceRepositoryError) -> AgentFailure {
    match error {
        SourceRepositoryError::Conflict => AgentFailure::Conflict,
        SourceRepositoryError::StorageUnavailable => AgentFailure::StorageUnavailable,
        SourceRepositoryError::Corrupt => AgentFailure::PolicyDenied,
    }
}
pub(super) fn check(actor: &OwnerActor, scope: &ExecutionScope) -> Result<(), AgentFailure> {
    actor.validate()?;
    if scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if scope.deadline() <= tokio::time::Instant::now() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

pub(super) fn intent_digest(value: &impl serde::Serialize) -> Result<[u8; 32], AgentFailure> {
    Ok(Sha256::digest(serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?).into())
}
pub(super) fn derived_id(domain: &[u8], person: floe_kernel::PersonId, id: Uuid) -> Uuid {
    let mut h = Sha256::new();
    h.update(domain);
    h.update([0]);
    h.update(person.to_string().as_bytes());
    h.update(id.as_bytes());
    let hash = h.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Uuid::from_bytes(bytes)
}
