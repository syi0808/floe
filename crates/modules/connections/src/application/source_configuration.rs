//! Reviewed source successor execution and recovery. Presentation-only updates
//! commit locally; authority changes retain the Access invalidation protocol.
use super::product::native_source;
use super::source_operation::{check, source_error};
use crate::*;
use floe_context_contract::GrantSourceBinding;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};

impl ConnectionsService {
    pub(super) async fn drive_source_configuration(
        &self,
        actor: &OwnerActor,
        mut record: ConnectionsRecord,
        scope: &ExecutionScope,
    ) -> Result<SourceSummary, AgentFailure> {
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
        let presentation_only = descriptor
            .source
            .is_presentation_successor(&successor)
            .map_err(|_| AgentFailure::Conflict)?;
        let kind = if presentation_only {
            SourceOperationKind::ConnectionPresentation
        } else {
            SourceOperationKind::ConnectionConfigure
        };
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
                .map_err(source_error)?
                .ok_or(AgentFailure::Conflict)?;
            if current != descriptor.source {
                return Err(AgentFailure::Conflict);
            }
            if let Some(expected) = &descriptor.expected {
                if &self.evidence.observe(actor, &current, scope).await? != expected {
                    return Err(AgentFailure::Conflict);
                }
            }
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
        if presentation_only {
            if !matches!(operation.phase, SourceOperationPhase::PresentationCommitted) {
                if !matches!(operation.phase, SourceOperationPhase::Reserved) {
                    return Err(AgentFailure::Conflict);
                }
                let actual = self
                    .evidence
                    .inspect_selection(actor, &successor, successor.resources(), scope)
                    .await?;
                if Some(actual.as_str()) != successor.native_subject_fingerprint() {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                self.advance(
                    &operation,
                    SourceOperationPhase::PresentationCommitted,
                    SourceOperationProof::Presentation,
                    Some(successor.clone()),
                )
                .await?;
            }
        } else {
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
                self.complete_committed(actor, operation, receipt, Some(successor.clone()), scope)
                    .await?;
            }
        }
        let source = self
            .sources
            .load(actor.person_id, successor.connection_id())
            .await
            .map_err(source_error)?
            .ok_or(AgentFailure::Conflict)?;
        if source != successor {
            return Err(AgentFailure::Conflict);
        }
        let summary = self.source_summary(actor, &source, scope).await?;
        let previous = record.revision;
        record.revision += 1;
        record.payload = ConnectionsPayload::SourceMutation {
            source,
            summary: summary.clone(),
        };
        self.products.compare_and_swap(previous, record).await?;
        Ok(summary)
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
