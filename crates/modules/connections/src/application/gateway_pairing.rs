use crate::ports::gateway_pairing::*;
use floe_execution::BoxFuture;
use floe_kernel::OwnerActor;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingState {
    #[serde(rename = "starting")]
    Pending,
    AwaitingLocalConfirmation,
    #[serde(rename = "awaiting_gateway_approval")]
    AwaitingApproval,
    Verifying,
    Committing,
    #[serde(rename = "connected")]
    Paired,
    Rejected,
    Expired,
    Cancelled,
    RepairRequired,
}
impl PairingState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Paired | Self::Rejected | Self::Expired | Self::Cancelled | Self::RepairRequired
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingRecord {
    pub operation_id: Uuid,
    pub command_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub target_ref: Uuid,
    pub revision: u64,
    pub generation: u64,
    pub state: PairingState,
    pub handle: Option<PairingHandle>,
    pub display_code: Option<String>,
    pub expires_at_unix_ms: Option<i64>,
    pub reviewed: Option<ReviewedGatewayIdentity>,
    pub enrollment: Option<VerifiedEnrollment>,
    pub gateway: Option<GatewaySummary>,
    pub confirmation_command: Option<Uuid>,
    pub cancellation_command: Option<Uuid>,
}
impl PairingRecord {
    /// Recovery can only reobserve the exact locally confirmed operation.
    /// Missing private inputs never authorize a new enrollment behind repair.
    pub fn can_reconcile_repair(&self) -> bool {
        self.state == PairingState::RepairRequired
            && self.handle.is_some()
            && self.reviewed.is_some()
            && self.confirmation_command.is_some()
    }
    pub fn validate(&self) -> Result<(), PairingError> {
        if self.operation_id.is_nil()
            || self.command_id.is_nil()
            || self.target_ref.is_nil()
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.revision == 0
            || self.generation == 0
        {
            return Err(PairingError::InvalidInput);
        }
        if self.handle.as_ref().is_some_and(|handle| {
            handle.operation_id != self.operation_id
                || handle.generation != self.generation
                || handle.attempt_id.is_nil()
        }) {
            return Err(PairingError::Conflict);
        }
        if !matches!(
            self.state,
            PairingState::Pending | PairingState::RepairRequired
        ) && self.handle.is_none()
        {
            return Err(PairingError::Conflict);
        }
        if (self.state == PairingState::Paired) != self.gateway.is_some() {
            return Err(PairingError::Conflict);
        }
        Ok(())
    }
    pub fn snapshot(&self) -> PairingSnapshot {
        PairingSnapshot {
            operation_ref: self.operation_id,
            revision: self.revision,
            state: self.state,
            display_code: self.display_code.clone(),
            expires_at: self
                .expires_at_unix_ms
                .and_then(chrono::DateTime::from_timestamp_millis),
            gateway: self.gateway.clone(),
            failure: pairing_failure_projection(self),
            allowed_actions: match self.state {
                PairingState::AwaitingLocalConfirmation => {
                    vec![ConnectionAction::Confirm, ConnectionAction::Cancel]
                }
                PairingState::Pending
                | PairingState::AwaitingApproval
                | PairingState::Verifying
                | PairingState::Committing => {
                    vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
                }
                PairingState::RepairRequired if self.can_reconcile_repair() => {
                    vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
                }
                _ => vec![],
            },
            next_observation_after_ms: (!self.state.terminal() || self.can_reconcile_repair())
                .then_some(2000),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingSnapshot {
    pub operation_ref: Uuid,
    pub revision: u64,
    pub state: PairingState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewaySummary>,
    pub allowed_actions: Vec<ConnectionAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::ConnectionFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_observation_after_ms: Option<u32>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySetupRecord {
    pub command_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub address_digest: [u8; 32],
    pub setup: GatewaySetup,
}
pub trait PairingRepository: Send + Sync {
    fn setup<'a>(
        &'a self,
        target_ref: Uuid,
    ) -> BoxFuture<'a, Result<Option<GatewaySetupRecord>, PairingError>>;
    fn store_setup<'a>(
        &'a self,
        record: GatewaySetupRecord,
    ) -> BoxFuture<'a, Result<GatewaySetupRecord, PairingError>>;
    fn insert<'a>(
        &'a self,
        record: PairingRecord,
    ) -> BoxFuture<'a, Result<PairingRecord, PairingError>>;
    fn load<'a>(
        &'a self,
        operation_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<PairingRecord>, PairingError>>;
    fn compare_and_swap<'a>(
        &'a self,
        expected_revision: u64,
        next: PairingRecord,
    ) -> BoxFuture<'a, Result<PairingRecord, PairingError>>;
    fn pending<'a>(
        &'a self,
        person_id: floe_kernel::PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<PairingRecord>, PairingError>>;
}
pub struct GatewayPairingService {
    repository: Arc<dyn PairingRepository>,
    transport: Arc<dyn GatewayPairingPort>,
    enrollment: Arc<dyn GatewayEnrollmentPort>,
    credentials: Arc<dyn GatewayCredentialCommit>,
}
impl GatewayPairingService {
    pub fn new(
        repository: Arc<dyn PairingRepository>,
        transport: Arc<dyn GatewayPairingPort>,
        enrollment: Arc<dyn GatewayEnrollmentPort>,
        credentials: Arc<dyn GatewayCredentialCommit>,
    ) -> Self {
        Self {
            repository,
            transport,
            enrollment,
            credentials,
        }
    }
    pub async fn pending(
        &self,
        person: floe_kernel::PersonId,
    ) -> Result<Vec<PairingRecord>, PairingError> {
        self.repository.pending(person, 64).await
    }
    pub async fn prepare_gateway_setup(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        address: &str,
        scope: &OperationScope,
    ) -> Result<GatewaySetup, PairingError> {
        use sha2::{Digest, Sha256};
        check(actor, scope)?;
        let target_ref = super::source_operation::derived_id(
            b"floe.gateway.setup.v1",
            actor.person_id,
            command_id,
        );
        let address_digest = Sha256::digest(address.as_bytes()).into();
        if let Some(record) = self.repository.setup(target_ref).await.map_err(|error| {
            trace_prepare_failure(command_id, target_ref, "setup_receipt_lookup", error)
        })? {
            if record.command_id != command_id
                || record.person_id != actor.person_id
                || record.device_id != actor.device_id
                || record.address_digest != address_digest
            {
                return Err(PairingError::Conflict);
            }
            return Ok(record.setup);
        }
        let setup = self
            .transport
            .prepare_setup(target_ref, address, scope)
            .await.map_err(|error| {
                trace_prepare_failure(command_id, target_ref, "adapter_prepare", error)
            })?;
        let record = self
            .repository
            .store_setup(GatewaySetupRecord {
                command_id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                address_digest,
                setup,
            })
            .await.map_err(|error| {
                trace_prepare_failure(command_id, target_ref, "setup_receipt_store", error)
            })?;
        Ok(record.setup)
    }
    pub async fn start_pairing(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        target_ref: Uuid,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        let operation_id = super::source_operation::derived_id(
            b"floe.pairing.operation.v1",
            actor.person_id,
            command_id,
        );
        if let Some(existing) = self.repository.load(operation_id).await? {
            authorize(actor, &existing)?;
            if existing.command_id != command_id || existing.target_ref != target_ref {
                return Err(PairingError::Conflict);
            }
            return Ok(existing.snapshot());
        }
        let setup = self
            .repository
            .setup(target_ref)
            .await?
            .ok_or(PairingError::InvalidInput)?;
        if setup.person_id != actor.person_id
            || setup.device_id != actor.device_id
            || setup.setup.expires_at <= chrono::Utc::now()
        {
            return Err(PairingError::Expired);
        }
        let record = self
            .repository
            .insert(PairingRecord {
                operation_id,
                command_id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                target_ref,
                revision: 1,
                generation: 1,
                state: PairingState::Pending,
                handle: None,
                display_code: None,
                expires_at_unix_ms: None,
                reviewed: None,
                enrollment: None,
                gateway: None,
                confirmation_command: None,
                cancellation_command: None,
            })
            .await?;
        Ok(record.snapshot())
    }
    pub async fn confirm_pairing(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        operation_id: Uuid,
        expected_revision: u64,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        let mut record = self.current(actor, operation_id).await?;
        if record.confirmation_command == Some(command_id) {
            return Ok(record.snapshot());
        }
        if command_id.is_nil()
            || record.revision != expected_revision
            || record.state != PairingState::AwaitingLocalConfirmation
            || record.confirmation_command.is_some()
        {
            return Err(PairingError::Conflict);
        }
        record.confirmation_command = Some(command_id);
        record.state = PairingState::AwaitingApproval;
        Ok(self.save(record).await?.snapshot())
    }
    pub async fn get_pairing(
        &self,
        actor: &OwnerActor,
        operation_id: Uuid,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        Ok(self.current(actor, operation_id).await?.snapshot())
    }
    /// Owner worker recovery, separate from the pure product get query.
    pub async fn reconcile_pairing(
        &self,
        actor: &OwnerActor,
        operation_id: Uuid,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        let mut record = self.current(actor, operation_id).await?;
        if record.state.terminal() && !record.can_reconcile_repair() {
            return Ok(record.snapshot());
        }
        if record.state == PairingState::Committing {
            if let Some(summary) = self.credentials.readback(record.operation_id).await? {
                self.verify_summary(&record, &summary)?;
                record.gateway = Some(summary);
                record.state = PairingState::Paired;
                return Ok(self.save(record).await?.snapshot());
            }
        }
        if record.state == PairingState::Pending {
            // start() rejoins the same private, durably staged request and
            // proof after a lost handoff. It never creates another operation.
            let challenge = match self
                .transport
                .start(
                    PairingStartRequest {
                        operation_id: record.operation_id,
                        target_ref: record.target_ref,
                        person_id: record.person_id,
                        device_id: record.device_id.clone(),
                        generation: record.generation,
                    },
                    scope,
                )
                .await
            {
                Ok(challenge) => challenge,
                Err(PairingError::RepairRequired) => {
                    record.state = PairingState::RepairRequired;
                    return Ok(self.save(record).await?.snapshot());
                }
                Err(error) => return Err(error),
            };
            record.handle = Some(challenge.handle);
            record.reviewed = Some(challenge.reviewed);
            record.display_code = Some(challenge.display_code);
            record.expires_at_unix_ms = Some(challenge.expires_at_unix_ms);
            record.state = PairingState::AwaitingLocalConfirmation;
            record = self.save(record).await?;
        }
        let handle = record.handle.as_ref().ok_or(PairingError::Conflict)?;
        let mut observed = if record.cancellation_command.is_some() {
            self.transport.cancel(handle, scope).await?
        } else {
            self.transport.observe(handle, scope).await?
        };
        if record.confirmation_command.is_some()
            && record.cancellation_command.is_none()
            && observed.progress == PairingProgress::AwaitingLocalConfirmation
        {
            observed = self
                .transport
                .confirm(
                    PairingConfirmation {
                        handle: handle.clone(),
                        reviewed: record.reviewed.clone().ok_or(PairingError::Conflict)?,
                    },
                    scope,
                )
                .await?;
        }
        self.apply_observation(record, observed, scope)
            .await
            .map(|record| record.snapshot())
    }
    pub async fn cancel_pairing(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        operation_id: Uuid,
        expected_revision: u64,
        scope: &OperationScope,
    ) -> Result<PairingSnapshot, PairingError> {
        check(actor, scope)?;
        let mut record = self.current(actor, operation_id).await?;
        if record.cancellation_command == Some(command_id) {
            return Ok(record.snapshot());
        }
        if record.revision != expected_revision
            || (record.state.terminal() && !record.can_reconcile_repair())
            || command_id.is_nil()
        {
            return Err(PairingError::Conflict);
        }
        record.cancellation_command = Some(command_id);
        Ok(self.save(record).await?.snapshot())
    }
    async fn apply_observation(
        &self,
        mut record: PairingRecord,
        observation: PairingObservation,
        scope: &OperationScope,
    ) -> Result<PairingRecord, PairingError> {
        if record.handle.as_ref() != Some(&observation.handle)
            || record.reviewed.as_ref() != Some(&observation.reviewed)
        {
            return Err(PairingError::ChangedProducer);
        }
        match observation.progress {
            PairingProgress::AwaitingLocalConfirmation => return Ok(record),
            PairingProgress::AwaitingApproval => record.state = PairingState::AwaitingApproval,
            PairingProgress::Rejected => record.state = PairingState::Rejected,
            PairingProgress::Expired => record.state = PairingState::Expired,
            PairingProgress::Cancelled => record.state = PairingState::Cancelled,
            PairingProgress::RepairRequired => {
                if record.state == PairingState::RepairRequired {
                    return Ok(record);
                }
                record.state = PairingState::RepairRequired;
            }
            PairingProgress::Approved => {
                if record.confirmation_command.is_none() {
                    return Err(PairingError::Conflict);
                }
                record.state = PairingState::Verifying;
                record = self.save(record).await?;
                let enrollment = self
                    .enrollment
                    .complete(&observation.handle, &observation.reviewed, scope)
                    .await?;
                if enrollment.operation_id != record.operation_id
                    || enrollment.binding.person_id != record.person_id.to_string()
                    || enrollment.binding.device_id != record.device_id
                {
                    return Err(PairingError::ForeignIdentity);
                }
                record.enrollment = Some(enrollment.clone());
                record.state = PairingState::Committing;
                record = self.save(record).await?;
                let result = self
                    .credentials
                    .commit(
                        StagedCredentialRef {
                            operation_id: record.operation_id,
                            generation: record.generation,
                        },
                        &enrollment,
                    )
                    .await;
                let summary = self
                    .credentials
                    .readback(record.operation_id)
                    .await?
                    .ok_or(result.err().unwrap_or(PairingError::CredentialUnavailable))?;
                self.verify_summary(&record, &summary)?;
                record.gateway = Some(summary);
                record.state = PairingState::Paired;
            }
        }
        self.save(record).await
    }
    fn verify_summary(
        &self,
        record: &PairingRecord,
        summary: &GatewaySummary,
    ) -> Result<(), PairingError> {
        let enrollment = record.enrollment.as_ref().ok_or(PairingError::Conflict)?;
        if summary.gateway_ref != record.operation_id
            || summary.revision != enrollment.binding.credential_generation
            || summary.state != GatewayState::Paired
        {
            return Err(PairingError::Conflict);
        }
        Ok(())
    }
    async fn current(&self, actor: &OwnerActor, id: Uuid) -> Result<PairingRecord, PairingError> {
        let record = self
            .repository
            .load(id)
            .await?
            .ok_or(PairingError::InvalidInput)?;
        authorize(actor, &record)?;
        Ok(record)
    }
    async fn save(&self, mut record: PairingRecord) -> Result<PairingRecord, PairingError> {
        let revision = record.revision;
        record.revision = revision.checked_add(1).ok_or(PairingError::Conflict)?;
        record.validate()?;
        self.repository.compare_and_swap(revision, record).await
    }
}
fn authorize(actor: &OwnerActor, record: &PairingRecord) -> Result<(), PairingError> {
    if actor.person_id != record.person_id || actor.device_id != record.device_id {
        Err(PairingError::ForeignIdentity)
    } else {
        Ok(())
    }
}
fn check(actor: &OwnerActor, scope: &OperationScope) -> Result<(), PairingError> {
    actor
        .validate()
        .map_err(|_| PairingError::ForeignIdentity)?;
    if scope.cancellation().is_cancelled() {
        return Err(PairingError::Cancelled);
    }
    if scope.deadline() <= tokio::time::Instant::now() {
        return Err(PairingError::DeadlineExceeded);
    }
    Ok(())
}

fn pairing_failure_projection(record: &PairingRecord) -> Option<crate::ConnectionFailure> {
    let (reason, recovery) = match record.state {
        PairingState::Rejected => (
            crate::ConnectionFailureReason::Rejected,
            crate::ConnectionRecovery::NewReview,
        ),
        PairingState::Expired => (
            crate::ConnectionFailureReason::Expired,
            crate::ConnectionRecovery::NewReview,
        ),
        PairingState::RepairRequired => (
            crate::ConnectionFailureReason::OperationUncertain,
            crate::ConnectionRecovery::Reconcile,
        ),
        _ => return None,
    };
    Some(crate::ConnectionFailure {
        domain: crate::ConnectionFailureDomain::Connections,
        category: floe_kernel::AgentFailureCategory::Integrity,
        reason,
        incident_id: record.operation_id,
        correlation_id: record.operation_id,
        reload_required: true,
        seal_session: false,
        recovery,
        safe_actions: if record.can_reconcile_repair() {
            vec![ConnectionAction::Cancel, ConnectionAction::Reobserve]
        } else {
            vec![]
        },
    })
}

// Only closed failure kinds and opaque correlation IDs are emitted. Never log
// the address, credential record, proof, service/account or SQL error text.
fn trace_prepare_failure(
    command_id: Uuid,
    target_ref: Uuid,
    stage: &'static str,
    error: PairingError,
) -> PairingError {
    #[cfg(debug_assertions)]
    tracing::warn!(
        component = "gateway_prepare",
        %command_id,
        %target_ref,
        stage,
        error_kind = ?error,
        "gateway_prepare_failed"
    );
    #[cfg(not(debug_assertions))]
    let _ = (command_id, target_ref, stage);
    error
}
