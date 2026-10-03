use floe_kernel::AgentFailure;

use super::record::*;
use crate::ActionAuthorityMode;
use crate::ports::repository::*;

fn next(record: &ActionRecord) -> Result<ActionRecord, AgentFailure> {
    record.validate()?;
    let mut updated = record.clone();
    updated.revision = updated
        .revision
        .checked_add(1)
        .ok_or(AgentFailure::Conflict)?;
    Ok(updated)
}

pub fn validate_action_admission(
    admission: &ActionAdmission,
    authority: &ActionsAuthority,
) -> Result<(), AgentFailure> {
    let record = &admission.record;
    record.validate()?;
    if admission.command_id.is_nil()
        || record.revision != 1
        || record.execution.is_some()
        || record.collection.is_some()
        || authority.person_id != record.person_id
        || authority.revision != record.review.authority_revision
        || !matches!(
            record.state,
            ActionState::PendingReview | ActionState::Approved | ActionState::Blocked { .. }
        )
    {
        return Err(AgentFailure::PolicyDenied);
    }
    match (&record.state, &record.authorization) {
        (
            ActionState::Approved,
            Some(ActionAuthorization::DirectInstruction { command_id, .. }),
        ) if command_id == &admission.command_id => {}
        (
            ActionState::Approved,
            Some(ActionAuthorization::StandingPolicy {
                authority_revision, ..
            }),
        ) if *authority_revision == authority.revision
            && authority.calendar_create == ActionAuthorityMode::Allow => {}
        (ActionState::PendingReview, None)
            if authority.calendar_create == ActionAuthorityMode::Ask => {}
        (
            ActionState::Blocked {
                reason: ActionBlockedReason::PolicyDenied,
            },
            None,
        ) if authority.calendar_create == ActionAuthorityMode::Deny => {}
        _ => return Err(AgentFailure::PolicyDenied),
    }
    Ok(())
}

pub fn decide_action(
    record: &ActionRecord,
    decision: &ActionDecision,
    authority: &ActionsAuthority,
) -> Result<ActionRecord, AgentFailure> {
    if decision.command_id.is_nil()
        || decision.person_id != record.person_id
        || decision.device_id != record.device_id
        || decision.action_id != record.id
        || decision.expected_revision != record.revision
        || decision.review_ref != record.review
        || record.execution.is_some()
        || authority.person_id != record.person_id
    {
        return Err(AgentFailure::Conflict);
    }
    let mut updated = next(record)?;
    if decision.now >= record.expires_at
        && matches!(
            record.state,
            ActionState::PendingReview | ActionState::Approved
        )
    {
        updated.state = ActionState::Expired;
        updated.validate()?;
        return Ok(updated);
    }
    match decision.decision {
        ActionDecisionKind::Approve if record.state == ActionState::PendingReview => {
            if authority.revision != record.review.authority_revision
                || authority.calendar_create == ActionAuthorityMode::Deny
                || decision.now < record.created_at
                || decision.now >= record.expires_at
            {
                return Err(AgentFailure::PolicyDenied);
            }
            updated.authorization = Some(ActionAuthorization::ReviewedDecision {
                command_id: decision.command_id,
                person_id: decision.person_id,
                device_id: decision.device_id.clone(),
                review: decision.review_ref.clone(),
                decided_at: decision.now,
            });
            updated.state = ActionState::Approved;
        }
        ActionDecisionKind::Reject if record.state == ActionState::PendingReview => {
            updated.state = ActionState::Rejected;
        }
        ActionDecisionKind::Cancel
            if matches!(
                record.state,
                ActionState::PendingReview | ActionState::Approved
            ) =>
        {
            updated.state = ActionState::Cancelled;
        }
        _ => return Err(AgentFailure::Conflict),
    }
    updated.validate()?;
    Ok(updated)
}

pub fn stop_action(
    record: &ActionRecord,
    stop: &PreDispatchStop,
) -> Result<ActionRecord, AgentFailure> {
    if stop.person_id != record.person_id
        || stop.action_id != record.id
        || stop.expected_revision != record.revision
        || record.execution.is_some()
        || !matches!(
            record.state,
            ActionState::PendingReview | ActionState::Approved
        )
    {
        return Err(AgentFailure::Conflict);
    }
    let mut updated = next(record)?;
    updated.state = match stop.state {
        PreDispatchState::Expired => ActionState::Expired,
        PreDispatchState::Cancelled => ActionState::Cancelled,
        PreDispatchState::Blocked { reason } => ActionState::Blocked { reason },
    };
    updated.validate()?;
    Ok(updated)
}

pub fn prepare_action_dispatch(
    record: &ActionRecord,
    request: &DispatchIntent,
    authority: &ActionsAuthority,
) -> Result<(ActionRecord, ExecutionIntent), AgentFailure> {
    if request.person_id != record.person_id
        || request.device_id != record.device_id
        || request.action_id != record.id
        || request.execution_id != record.execution_id
        || request.effect_digest != record.effect_digest
        || request.expected_revision != record.revision
        || record.state != ActionState::Approved
        || record.execution.is_some()
        || record.authorization.as_ref() != Some(&request.authorization)
        || request.current_source_fence != record.source
        || request.executor_generation == 0
        || authority.person_id != record.person_id
        || authority.revision != request.authorization.authority_revision()
    {
        return Err(AgentFailure::Conflict);
    }
    request.authorization.validate_for(record, request.now)?;
    if matches!(record.origin, ActionOrigin::Expert { .. })
        && (authority.calendar_create == ActionAuthorityMode::Deny
            || matches!(
                request.authorization,
                ActionAuthorization::StandingPolicy { .. }
            ) && authority.calendar_create != ActionAuthorityMode::Allow)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let intent = ExecutionIntent {
        action_id: record.id,
        person_id: record.person_id,
        device_id: record.device_id.clone(),
        execution_id: record.execution_id,
        effect_digest: record.effect_digest,
        effect: record.effect.clone(),
        source: record.source.clone(),
        authorization: request.authorization.clone(),
        executor_generation: request.executor_generation,
        prepared_at: request.now,
    };
    let mut updated = next(record)?;
    updated.execution = Some(intent.clone());
    updated.state = ActionState::Executing {
        execution_id: record.execution_id,
    };
    updated.validate()?;
    Ok((updated, intent))
}

pub fn settle_action(
    record: &ActionRecord,
    settlement: &ExecutionSettlement,
) -> Result<ActionRecord, AgentFailure> {
    if settlement.person_id != record.person_id
        || settlement.execution_id != record.execution_id
        || settlement.effect_digest != record.effect_digest
        || settlement.expected_revision != record.revision
        || !matches!(
            record.state,
            ActionState::Executing { .. } | ActionState::Unknown { .. }
        )
    {
        return Err(AgentFailure::Conflict);
    }
    let intent = record.execution.as_ref().ok_or(AgentFailure::Conflict)?;
    settlement.outcome.validate_for(intent)?;
    let mut updated = next(record)?;
    updated.state = match &settlement.outcome {
        CalendarEffectOutcome::Committed { receipt } => {
            let id = action_uuid(
                b"floe.actions.collection.v1\0",
                record.person_id,
                record.execution_id,
            );
            let collection = ActionCollectionState::Pending { ticket_id: id };
            updated.collection = Some(CollectionTicket {
                id,
                person_id: record.person_id,
                action_id: record.id,
                execution_id: record.execution_id,
                receipt_digest: receipt.digest()?,
                revision: 1,
                state: collection.clone(),
            });
            ActionState::Succeeded {
                receipt: receipt.clone(),
                collection,
            }
        }
        CalendarEffectOutcome::NotApplied { proof } => ActionState::Failed {
            reason: proof.reason,
            not_applied_proof: proof.clone(),
        },
        CalendarEffectOutcome::Unknown { reason, .. } => ActionState::Unknown { reason: *reason },
    };
    updated.validate()?;
    Ok(updated)
}

pub fn acknowledge_action_collection(
    record: &ActionRecord,
    ack: &CollectionAck,
) -> Result<ActionRecord, AgentFailure> {
    let ticket = record.collection.as_ref().ok_or(AgentFailure::Conflict)?;
    if ack.person_id != record.person_id
        || ack.execution_id != record.execution_id
        || ack.receipt_digest != ticket.receipt_digest
        || ack.expected_ticket_revision != ticket.revision
        || !bounded(&ack.day_projection_ref, 256)
        || !matches!(ticket.state, ActionCollectionState::Pending { .. })
    {
        return Err(AgentFailure::Conflict);
    }
    let mut updated = next(record)?;
    let state = ActionCollectionState::Collected {
        day_projection_ref: ack.day_projection_ref.clone(),
    };
    let ticket = updated.collection.as_mut().ok_or(AgentFailure::Conflict)?;
    ticket.revision = ticket
        .revision
        .checked_add(1)
        .ok_or(AgentFailure::Conflict)?;
    ticket.state = state.clone();
    let ActionState::Succeeded { collection, .. } = &mut updated.state else {
        return Err(AgentFailure::Conflict);
    };
    *collection = state;
    updated.validate()?;
    Ok(updated)
}

/// Access revocation invalidates only work that has not crossed dispatch.
pub fn invalidate_action_dependency(
    record: &ActionRecord,
    evidence: &crate::ExpertProposalEvidence,
    grant: floe_context_contract::GrantId,
    authority: floe_context_contract::GrantAuthority,
) -> Result<Option<ActionRecord>, AgentFailure> {
    record.validate()?;
    if record.execution.is_some()
        || !matches!(
            record.state,
            ActionState::PendingReview | ActionState::Approved
        )
    {
        return Ok(None);
    }
    crate::validate_expert_action_evidence(record, evidence)?;
    let floe_agent_contract::DependencyCoverage::Dependent { dependencies } = &evidence.coverage
    else {
        return Err(AgentFailure::PolicyDenied);
    };
    if !dependencies.iter().any(|dependency| {
        dependency.grant_id() == grant && dependency.grant_authority() == authority
    }) {
        return Ok(None);
    }
    let mut updated = next(record)?;
    updated.state = ActionState::Blocked {
        reason: ActionBlockedReason::PolicyDenied,
    };
    updated.validate()?;
    Ok(Some(updated))
}

pub fn invalidate_action_policy(
    record: &ActionRecord,
) -> Result<Option<ActionRecord>, AgentFailure> {
    record.validate()?;
    if record.execution.is_some()
        || !matches!(
            record.state,
            ActionState::PendingReview | ActionState::Approved
        )
    {
        return Ok(None);
    }
    let mut updated = next(record)?;
    updated.state = ActionState::Blocked {
        reason: ActionBlockedReason::PolicyDenied,
    };
    updated.validate()?;
    Ok(Some(updated))
}

pub fn change_action_authority(
    current: &ActionsAuthority,
    change: &AuthorityChange,
) -> Result<ActionsAuthority, AgentFailure> {
    if change.command_id.is_nil()
        || change.person_id != current.person_id
        || change.expected_revision != current.revision
    {
        return Err(AgentFailure::Conflict);
    }
    if change.mode == current.calendar_create {
        return Ok(current.clone());
    }
    Ok(ActionsAuthority {
        person_id: current.person_id,
        revision: current
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?,
        calendar_create: change.mode,
    })
}

pub fn decision_intent_digest(decision: &ActionDecision) -> Result<ActionDigest, AgentFailure> {
    action_digest(
        b"floe.actions.decision.v1\0",
        &(
            decision.command_id,
            decision.person_id,
            &decision.device_id,
            decision.action_id,
            decision.expected_revision,
            &decision.review_ref,
            decision.decision,
        ),
    )
}
