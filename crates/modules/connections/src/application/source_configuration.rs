//! Reviewed source successor execution and recovery. Presentation-only updates
//! commit locally; authority changes retain the Access invalidation protocol.
use super::product::native_source;
use super::source_operation::{check, prepare_source_reservation, source_error};
use crate::*;
use floe_context_contract::GrantSourceBinding;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};

pub(super) enum SourceConfigurationOutcome {
    Applied(SourceSummary),
    NotApplied(AgentFailure),
}
pub(super) fn aborted_reason(reason: SourceAbortReason) -> AgentFailure {
    match reason {
        SourceAbortReason::Cancelled => AgentFailure::Cancelled,
        SourceAbortReason::ReviewChanged | SourceAbortReason::ReviewExpired => {
            AgentFailure::AccessReviewRequired
        }
    }
}
pub(super) fn terminal_outcome(
    record: ConnectionsRecord,
) -> Result<SourceConfigurationOutcome, AgentFailure> {
    match record.payload {
        ConnectionsPayload::SourceMutation { summary, .. } => {
            Ok(SourceConfigurationOutcome::Applied(summary))
        }
        ConnectionsPayload::SourceConfigurationAborted { reason, .. } => Ok(
            SourceConfigurationOutcome::NotApplied(aborted_reason(reason)),
        ),
        _ => Err(AgentFailure::Conflict),
    }
}
fn reviewed_subject_continues(
    observation: Result<floe_access::SourceExpectation, AgentFailure>,
    expected: &floe_access::SourceExpectation,
) -> Result<bool, AgentFailure> {
    match observation {
        Ok(observed) => Ok(&observed == expected),
        Err(AgentFailure::AccessReviewRequired) => Ok(false),
        Err(error) => Err(error),
    }
}

fn source_command_identity(
    record: &ConnectionsRecord,
    operation_id: uuid::Uuid,
) -> ConnectionsCommandIdentity {
    ConnectionsCommandIdentity {
        journal: ConnectionsCommandJournal::SourceOperation,
        record_ref: operation_id,
        person_id: record.person_id,
        device_id: record.device_id.clone(),
        command_id: record.command_id,
        intent_digest: record.intent_digest,
    }
}
impl ConnectionsService {
    pub(super) async fn drive_source_configuration(
        &self,
        actor: &OwnerActor,
        record: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<SourceConfigurationOutcome, AgentFailure> {
        self.ensure_open()?;
        check(actor, scope)?;
        if record.person_id != actor.person_id || record.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let ConnectionsPayload::SourceConfiguration {
            descriptor,
            successor,
            operation_id,
            ..
        } = record.payload.clone()
        else {
            return Err(AgentFailure::InvalidInput);
        };
        if descriptor.source.person_id() != actor.person_id
            || successor.person_id() != actor.person_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let presentation_only = descriptor
            .source
            .is_presentation_successor(&successor)
            .map_err(|_| AgentFailure::Conflict)?;
        if presentation_only {
            let operation = self
                .settle_source_presentation(actor, &record, &descriptor, &successor, scope)
                .await?;
            let payload = match operation.phase {
                SourceOperationPhase::PresentationCommitted => ConnectionsPayload::SourceMutation {
                    summary: self.source_summary(actor, &successor, scope).await?,
                    source: successor,
                },
                SourceOperationPhase::Aborted { reason } => {
                    ConnectionsPayload::SourceConfigurationAborted {
                        source: descriptor.source,
                        reason,
                    }
                }
                _ => return Err(AgentFailure::Conflict),
            };
            return self
                .finish_source_configuration_record(record, payload)
                .await;
        }
        if let Some(reason) = self
            .sources
            .rejected_operation_command(source_command_identity(&record, operation_id))
            .await
            .map_err(source_error)?
        {
            if reason != AgentFailure::AccessReviewRequired {
                return Err(AgentFailure::Conflict);
            }
            return self
                .finish_source_configuration_record(
                    record,
                    ConnectionsPayload::SourceConfigurationAborted {
                        source: descriptor.source,
                        reason: SourceAbortReason::ReviewChanged,
                    },
                )
                .await;
        }
        let kind = SourceOperationKind::ConnectionConfigure;
        let operation = if let Some(operation) = self
            .sources
            .load_operation(operation_id)
            .await
            .map_err(source_error)?
        {
            if operation.device_id != actor.device_id
                || operation.command_id != record.command_id
                || operation.request_digest != record.intent_digest
                || operation.kind != kind
            {
                return Err(AgentFailure::Conflict);
            }
            operation
        } else {
            let current = self
                .sources
                .load(actor.person_id, descriptor.source.connection_id())
                .await
                .map_err(source_error)?;
            if current.as_ref() != Some(&descriptor.source) {
                return self
                    .reject_unreserved_configuration(record, descriptor.source, operation_id)
                    .await;
            }
            let current = current.ok_or(AgentFailure::Conflict)?;
            if let Some(expected) = &descriptor.expected {
                if !reviewed_subject_continues(
                    self.evidence.observe(actor, &current, scope).await,
                    expected,
                )? {
                    return self
                        .reject_unreserved_configuration(record, descriptor.source, operation_id)
                        .await;
                }
            }
            self.ensure_open()?;
            check(actor, scope)?;
            self.reserve(
                actor,
                record.command_id,
                record.intent_digest,
                Some(descriptor.summary.review_ref.clone()),
                configuration_expectation(&descriptor, &successor)?,
                kind,
            )
            .await
            .map_err(ConnectionsCommandFailure::into_failure)?
            .0
            .record
        };
        if matches!(operation.phase, SourceOperationPhase::Completed { .. }) {
            let summary = self.source_summary(actor, &successor, scope).await?;
            return self
                .finish_source_configuration_record(
                    record,
                    ConnectionsPayload::SourceMutation {
                        source: successor,
                        summary,
                    },
                )
                .await;
        }
        let receipt = match self
            .access
            .receipt(
                actor,
                floe_access::GrantReceiptQuery {
                    identity: operation.identity(),
                },
                scope,
            )
            .await?
        {
            Some(floe_access::GrantOperationReceipt::Committed(receipt)) => receipt,
            Some(floe_access::GrantOperationReceipt::Aborted(_)) => {
                return Err(AgentFailure::Conflict);
            }
            None => {
                let evidence = floe_access::SourceReservationEvidence {
                    device_id: operation.device_id.clone(),
                    operation_id: operation.operation_id,
                    command_id: operation.command_id,
                    request_digest: operation.request_digest,
                    reservation_id: operation.reservation_id,
                    reservation_generation: operation.reservation_generation,
                    source: configuration_expectation(&descriptor, &successor)?,
                };
                self.ensure_open()?;
                check(actor, scope)?;
                let result = self.access.invalidate_source(actor, evidence, scope).await;
                match self
                    .access
                    .receipt(
                        actor,
                        floe_access::GrantReceiptQuery {
                            identity: operation.identity(),
                        },
                        scope,
                    )
                    .await?
                {
                    Some(floe_access::GrantOperationReceipt::Committed(receipt)) => receipt,
                    _ => return Err(result.err().unwrap_or(AgentFailure::StorageUnavailable)),
                }
            }
        };
        if receipt.kind != floe_access::GrantCommitKind::InvalidateSource {
            return Err(AgentFailure::Conflict);
        }
        if !matches!(operation.phase, SourceOperationPhase::Completed { .. }) {
            // Prove the exact candidate subject again after durable grant
            // invalidation. If it drifted the source stays fenced for repair.
            let actual = self
                .evidence
                .inspect_selection(actor, &successor, successor.resources(), scope)
                .await?;
            if Some(actual.as_str()) != successor.native_subject_fingerprint() {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let completed = self
                .complete_committed(actor, operation, receipt, Some(successor.clone()), scope)
                .await?;
            if !matches!(completed.phase, SourceOperationPhase::Completed { .. }) {
                return Err(AgentFailure::AccessReviewRequired);
            }
        }
        // The source journal is the durable effect receipt. Later source changes
        // cannot undo this exact command's historical result.
        let summary = self.source_summary(actor, &successor, scope).await?;
        self.finish_source_configuration_record(
            record,
            ConnectionsPayload::SourceMutation {
                source: successor,
                summary,
            },
        )
        .await
    }
    async fn reject_unreserved_configuration(
        &self,
        record: ConnectionsRecord,
        source: SourceConnection,
        operation_id: uuid::Uuid,
    ) -> Result<SourceConfigurationOutcome, AgentFailure> {
        self.ensure_open()?;
        let identity = source_command_identity(&record, operation_id);
        // The source journal's negative receipt atomically closes future reserve.
        // Absence observed before this transaction would not be sufficient.
        match self
            .sources
            .reject_unadmitted_operation_command(identity, AgentFailure::AccessReviewRequired)
            .await
            .map_err(source_error)?
        {
            ConnectionsCommandResolution::NotApplied(_) => {
                self.finish_source_configuration_record(
                    record,
                    ConnectionsPayload::SourceConfigurationAborted {
                        source,
                        reason: SourceAbortReason::ReviewChanged,
                    },
                )
                .await
            }
            // A prior delivery admitted first. Rejoin its real operation on retry.
            ConnectionsCommandResolution::Admitted => Err(AgentFailure::StorageBusy),
        }
    }

    async fn settle_source_presentation(
        &self,
        actor: &OwnerActor,
        record: &ConnectionsRecord,
        descriptor: &SourceReviewDescriptor,
        successor: &SourceConnection,
        scope: &ExecutionScope,
    ) -> Result<SourceOperationRecord, AgentFailure> {
        let (requested, _) = prepare_source_reservation(
            actor,
            record.command_id,
            record.intent_digest,
            Some(descriptor.summary.review_ref.clone()),
            configuration_expectation(descriptor, successor)?,
            SourceOperationKind::ConnectionPresentation,
        )
        .map_err(ConnectionsCommandFailure::into_failure)?;
        if let Some(existing) = self
            .sources
            .load_operation(requested.operation_id)
            .await
            .map_err(source_error)?
        {
            let mut initial = existing.clone();
            initial.phase = SourceOperationPhase::Reserved;
            initial.revision = 1;
            if initial != requested || existing.phase.holds_fence() {
                return Err(AgentFailure::Conflict);
            }
            return Ok(existing);
        }
        if self
            .sources
            .source_is_fenced(actor.person_id, successor.connection_id())
            .await
            .map_err(source_error)?
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let decision = match self
            .evidence
            .inspect_selection(actor, successor, successor.resources(), scope)
            .await
        {
            Ok(actual) if Some(actual.as_str()) == successor.native_subject_fingerprint() => {
                SourcePresentationDecision::Apply(successor.clone())
            }
            Ok(_)
            | Err(
                AgentFailure::AccessReviewRequired
                | AgentFailure::PolicyDenied
                | AgentFailure::CapabilityDenied,
            ) => SourcePresentationDecision::Reject(SourceAbortReason::ReviewChanged),
            Err(error) => return Err(error),
        };
        self.ensure_open()?;
        check(actor, scope)?;
        self.sources
            .settle_presentation(SourceOperationReservation { record: requested }, decision)
            .await
            .map_err(source_error)
    }

    async fn finish_source_configuration_record(
        &self,
        mut record: ConnectionsRecord,
        payload: ConnectionsPayload,
    ) -> Result<SourceConfigurationOutcome, AgentFailure> {
        self.ensure_open()?;
        let previous = record.revision;
        record.revision = record
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        record.payload = payload;
        match self
            .products
            .compare_and_swap(previous, record.clone())
            .await
        {
            Ok(stored) => terminal_outcome(stored),
            Err(AgentFailure::Conflict) => {
                let winner = self
                    .products
                    .load(record.person_id, record.record_ref)
                    .await?
                    .ok_or(AgentFailure::Conflict)?;
                if winner.person_id != record.person_id
                    || winner.device_id != record.device_id
                    || winner.command_id != record.command_id
                    || winner.intent_digest != record.intent_digest
                {
                    return Err(AgentFailure::Conflict);
                }
                terminal_outcome(winner)
            }
            Err(error) => Err(error),
        }
    }
}

fn configuration_expectation(
    descriptor: &SourceReviewDescriptor,
    successor: &SourceConnection,
) -> Result<floe_access::SourceExpectation, AgentFailure> {
    if let Some(expected) = &descriptor.expected {
        return Ok(expected.clone());
    }
    if descriptor.source.state() != SourceState::Pending || !native_source(&descriptor.source) {
        return Err(AgentFailure::Conflict);
    }
    Ok(floe_access::SourceExpectation {
        source: GrantSourceBinding::try_new(
            descriptor.source.person_id(),
            descriptor.source.connection_id().clone(),
            descriptor.source.connector_id().clone(),
            descriptor.source.execution_owner_id().clone(),
        )
        .map_err(|_| AgentFailure::InvalidInput)?,
        revision: Some(descriptor.source.revision()),
        provider_revision: None,
        authority: descriptor.source.source_authority(),
        physical_resources: successor
            .resources()
            .iter()
            .map(|resource| resource.handle().clone())
            .collect(),
        subject_fingerprint: successor
            .native_subject_fingerprint()
            .ok_or(AgentFailure::AccessReviewRequired)?
            .to_owned(),
        gateway: None,
    })
}
