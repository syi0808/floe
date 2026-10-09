//! Conversation orchestration over typed Connections receipts. A persisted
//! subordinate command identity precedes source mutation and survives retries.
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor};
use uuid::Uuid;

use crate::{
    ConversationInteraction, DecideInteractionCommand, InteractionDecisionKind,
    InteractionRepository, InteractionResolution, InteractionResolutionCommit, InteractionState,
    OwnerResolutionReceipt, ReviewedTarget,
};

pub async fn apply_source_interaction<R: InteractionRepository + ?Sized>(
    interactions: &R,
    connections: &floe_connections::ConnectionsService,
    operations: &floe_calendar_operations::CalendarOperationsService,
    actor: &OwnerActor,
    command: DecideInteractionCommand,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, CommandFailure<AgentFailure>> {
    actor.validate().map_err(CommandFailure::NotAdmitted)?;
    if command.principal != actor.person_id.to_string() {
        return Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied));
    }
    let recorded =
        match super::interactions::decide_interaction(interactions, command, now_unix_ms).await? {
            crate::DecisionAdmission::Applied(record)
            | crate::DecisionAdmission::Rejoined(record) => record,
        };
    if !matches!(recorded.state, InteractionState::Resolving { .. }) {
        return Ok(recorded);
    }
    match &recorded.target {
        ReviewedTarget::OperationApproval(_) => resolve_operation_record(
            interactions,
            operations,
            actor,
            recorded,
            now_unix_ms,
            scope,
        )
        .await
        .map_err(CommandFailure::Admitted),
        _ => resolve_source_record(
            interactions,
            connections,
            actor,
            recorded,
            now_unix_ms,
            scope,
        )
        .await
        .map_err(CommandFailure::Admitted),
    }
}

pub async fn recover_source_interaction<R: InteractionRepository + ?Sized>(
    interactions: &R,
    connections: &floe_connections::ConnectionsService,
    operations: &floe_calendar_operations::CalendarOperationsService,
    actor: &OwnerActor,
    interaction_id: Uuid,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, AgentFailure> {
    actor.validate()?;
    let record = interactions
        .get_interaction(actor.person_id, interaction_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if !matches!(record.state, InteractionState::Resolving { .. }) {
        return Ok(record);
    }
    match &record.target {
        ReviewedTarget::OperationApproval(_) => {
            resolve_operation_record(interactions, operations, actor, record, now_unix_ms, scope)
                .await
        }
        _ => {
            resolve_source_record(interactions, connections, actor, record, now_unix_ms, scope)
                .await
        }
    }
}

async fn resolve_operation_record<R: InteractionRepository + ?Sized>(
    interactions: &R,
    operations: &floe_calendar_operations::CalendarOperationsService,
    actor: &OwnerActor,
    record: ConversationInteraction,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, AgentFailure> {
    record.validate()?;
    let InteractionState::Resolving {
        decision_id,
        owner_command_id,
        decision_kind,
        decided_at_unix_ms,
    } = record.state
    else {
        return Err(AgentFailure::Conflict);
    };
    let ReviewedTarget::OperationApproval(reference) = &record.target else {
        return Err(AgentFailure::InvalidInput);
    };
    if record.person_id != actor.person_id || record.audit.device_id != actor.device_id {
        return Err(AgentFailure::PolicyDenied);
    }
    let decided_at = decided_at_unix_ms
        .and_then(chrono::DateTime::from_timestamp_millis)
        .ok_or(AgentFailure::Conflict)?;
    if decided_at.timestamp_millis() < record.created_at_unix_ms
        || decided_at.timestamp_millis() >= record.expires_at_unix_ms
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let decision = match decision_kind {
        InteractionDecisionKind::Approve => floe_access::OperationDecisionKind::Approve,
        InteractionDecisionKind::Deny => floe_access::OperationDecisionKind::Reject,
        InteractionDecisionKind::Dismiss => floe_access::OperationDecisionKind::Cancel,
    };
    let (operation, access_receipt) = operations
        .decide_with_receipt_at(
            actor,
            owner_command_id,
            reference.operation_id,
            reference.clone(),
            decision,
            1,
            decided_at,
            scope,
        )
        .await
        .map_err(|failure| failure.into_failure())?;
    if operation.action_ref != reference.operation_id || operation.review_ref != *reference {
        return Err(AgentFailure::Conflict);
    }
    let owner_receipt = OwnerResolutionReceipt::CalendarOperation {
        operation_id: reference.operation_id,
        decision: access_receipt,
    };
    owner_receipt.validate()?;
    if owner_receipt.command_id() != owner_command_id {
        return Err(AgentFailure::Conflict);
    }
    interactions
        .resolve_and_request_resume(InteractionResolutionCommit {
            resolution: InteractionResolution {
                interaction_id: record.id,
                person_id: actor.person_id,
                expected_revision: record.revision,
                cause: crate::InteractionResolutionCause::Decision {
                    command_id: decision_id,
                },
                owner_command_id,
                owner_operation_id: reference.operation_id,
                target_digest: record.target_digest,
                resolved_at_unix_ms: now_unix_ms,
            },
            owner_receipt,
        })
        .await?;
    interactions
        .get_interaction(actor.person_id, record.id)
        .await?
        .ok_or(AgentFailure::StorageUnavailable)
}

async fn resolve_source_record<R: InteractionRepository + ?Sized>(
    interactions: &R,
    connections: &floe_connections::ConnectionsService,
    actor: &OwnerActor,
    record: ConversationInteraction,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, AgentFailure> {
    record.validate()?;
    let InteractionState::Resolving {
        decision_id,
        owner_command_id,
        ..
    } = record.state
    else {
        return Err(AgentFailure::Conflict);
    };
    let ReviewedTarget::SourceReview(reference) = &record.target else {
        return Err(AgentFailure::InvalidInput);
    };
    if record.person_id != actor.person_id || record.audit.device_id != actor.device_id {
        return Err(AgentFailure::PolicyDenied);
    }
    let operation = connections
        .apply_source_review(actor, owner_command_id, reference.clone(), scope)
        .await
        .map_err(floe_connections::ConnectionsCommandFailure::into_failure)?;
    let Some(floe_access::GrantOperationReceipt::Committed(receipt)) = connections
        .operation_receipt(actor, &operation, scope)
        .await?
    else {
        return Err(AgentFailure::Conflict);
    };
    if !matches!(operation.phase, floe_connections::SourceOperationPhase::Completed { receipt_id } if receipt_id == receipt.commit_id)
        || operation.command_id != owner_command_id
        || receipt.reservation.command_id != owner_command_id
        || receipt.reservation.operation_id != operation.operation_id
        || receipt.reservation.source.source.person_id() != actor.person_id
        || !matches!(&receipt.kind, floe_access::GrantCommitKind::Reviewed { review } if review == reference)
    {
        return Err(AgentFailure::Conflict);
    }
    interactions
        .resolve_and_request_resume(InteractionResolutionCommit {
            resolution: InteractionResolution {
                interaction_id: record.id,
                person_id: actor.person_id,
                expected_revision: record.revision,
                cause: crate::InteractionResolutionCause::Decision {
                    command_id: decision_id,
                },
                owner_command_id,
                owner_operation_id: operation.operation_id,
                target_digest: record.target_digest,
                resolved_at_unix_ms: now_unix_ms,
            },
            owner_receipt: OwnerResolutionReceipt::SourceProcessing { receipt },
        })
        .await?;
    interactions
        .get_interaction(actor.person_id, record.id)
        .await?
        .ok_or(AgentFailure::StorageUnavailable)
}

/// Reconcile an explicit refresh with the exact consumed binding review.
/// This never replaces a binding or interprets registry state as approval.
pub(super) async fn recover_binding_interaction<R: InteractionRepository + ?Sized>(
    interactions: &R,
    experts: &dyn floe_experts::ExpertsOwner,
    actor: &OwnerActor,
    request: &crate::RefreshInteraction,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, AgentFailure> {
    let record = interactions
        .get_interaction(actor.person_id, request.interaction_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    record.validate()?;
    if record.person_id != actor.person_id
        || record.audit.device_id != actor.device_id
        || record.session_id != request.session_id
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let ReviewedTarget::ExpertBinding(reference) = &record.target else {
        return Err(AgentFailure::InvalidInput);
    };
    if !matches!(record.state, InteractionState::Pending) {
        return Ok(record);
    }
    let Some(receipt) = experts
        .binding_review_receipt(actor, reference.clone(), scope)
        .await?
    else {
        return Ok(record);
    };
    OwnerResolutionReceipt::ExpertBinding {
        receipt: receipt.clone(),
    }
    .validate()?;
    if receipt.review_ref != *reference
        || receipt.committed_at_unix_ms < record.created_at_unix_ms
        || receipt.committed_at_unix_ms >= record.expires_at_unix_ms
        || now_unix_ms < receipt.committed_at_unix_ms
    {
        return Err(AgentFailure::Conflict);
    }
    interactions
        .resolve_and_request_resume(InteractionResolutionCommit {
            resolution: InteractionResolution {
                interaction_id: record.id,
                person_id: actor.person_id,
                expected_revision: record.revision,
                cause: crate::InteractionResolutionCause::Refresh {
                    command_id: request.command_id,
                },
                owner_command_id: receipt.command_id.as_uuid(),
                owner_operation_id: receipt.command_id.as_uuid(),
                target_digest: record.target_digest,
                resolved_at_unix_ms: now_unix_ms,
            },
            owner_receipt: OwnerResolutionReceipt::ExpertBinding { receipt },
        })
        .await?;
    interactions
        .get_interaction(actor.person_id, record.id)
        .await?
        .ok_or(AgentFailure::StorageUnavailable)
}
