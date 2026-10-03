//! Conversation orchestration over typed Connections receipts. A persisted
//! subordinate command identity precedes source mutation and survives retries.
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};
use uuid::Uuid;

use crate::{ConversationInteraction, DecideInteractionCommand, InteractionRepository, InteractionState,
    ReviewedTarget, OwnerResolutionReceipt, InteractionResolutionCommit, InteractionResolution};

pub async fn apply_source_interaction<R: InteractionRepository + ?Sized>(
    interactions: &R,
    connections: &floe_connections::ConnectionsService,
    actor: &OwnerActor,
    command: DecideInteractionCommand,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, AgentFailure> {
    actor.validate()?;
    if command.principal != actor.person_id.to_string() { return Err(AgentFailure::PolicyDenied); }
    let current = interactions.get_interaction(actor.person_id, command.interaction_id).await?.ok_or(AgentFailure::NotFound)?;
    if matches!(command.kind, crate::InteractionDecisionKind::Approve)
        && !matches!(current.target, ReviewedTarget::SourceReview(_)) { return Err(AgentFailure::InvalidInput); }
    let recorded = match super::interactions::decide_interaction(interactions, command, now_unix_ms).await? {
        crate::DecisionAdmission::Applied(record) | crate::DecisionAdmission::Rejoined(record) => record,
    };
    if !matches!(recorded.state, InteractionState::Resolving { .. }) { return Ok(recorded); }
    resolve_source_record(interactions, connections, actor, recorded, now_unix_ms, scope).await
}

pub async fn recover_source_interaction<R: InteractionRepository + ?Sized>(
    interactions: &R,
    connections: &floe_connections::ConnectionsService,
    actor: &OwnerActor,
    interaction_id: Uuid,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ConversationInteraction, AgentFailure> {
    actor.validate()?;
    let record = interactions.get_interaction(actor.person_id, interaction_id).await?.ok_or(AgentFailure::NotFound)?;
    if !matches!(record.state, InteractionState::Resolving { .. }) { return Ok(record); }
    resolve_source_record(interactions, connections, actor, record, now_unix_ms, scope).await
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
    let InteractionState::Resolving { decision_id, owner_command_id } = record.state else { return Err(AgentFailure::Conflict); };
    let ReviewedTarget::SourceReview(reference) = &record.target else { return Err(AgentFailure::InvalidInput); };
    if record.person_id != actor.person_id || record.projection.as_ref().is_some_and(|projection| projection.device_id != actor.device_id) {
        return Err(AgentFailure::PolicyDenied);
    }
    let operation = connections.apply_source_review(actor, owner_command_id, reference.clone(), scope).await?;
    let Some(floe_access::GrantOperationReceipt::Committed(receipt)) = connections.operation_receipt(actor, &operation, scope).await? else {
        return Err(AgentFailure::Conflict);
    };
    if !matches!(operation.phase, floe_connections::SourceOperationPhase::Completed { receipt_id } if receipt_id == receipt.commit_id)
        || operation.command_id != owner_command_id || receipt.reservation.command_id != owner_command_id
        || receipt.reservation.operation_id != operation.operation_id
        || receipt.reservation.source.source.person_id() != actor.person_id
        || !matches!(&receipt.kind, floe_access::GrantCommitKind::Reviewed { review } if review == reference) {
        return Err(AgentFailure::Conflict);
    }
    interactions.resolve_and_request_resume(InteractionResolutionCommit {
        resolution: InteractionResolution {
            interaction_id: record.id, person_id: actor.person_id, expected_revision: record.revision,
            decision_id, owner_command_id, owner_operation_id: operation.operation_id,
            target_digest: record.target_digest, resolved_at_unix_ms: now_unix_ms,
        },
        owner_receipt: OwnerResolutionReceipt::SourceProcessing { receipt },
    }).await?;
    interactions.get_interaction(actor.person_id, record.id).await?.ok_or(AgentFailure::StorageUnavailable)
}
