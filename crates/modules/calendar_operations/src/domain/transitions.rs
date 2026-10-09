use floe_kernel::AgentFailure;

use super::record::*;
use crate::ports::repository::*;
use floe_access::{
    OperationAuthorizationPolicy, OperationDecisionKind, OperationDecisionReceipt,
    OperationPolicyDecision,
};

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
    admission: &OperationAdmission,
    authority: Option<&OperationAuthorizationPolicy>,
) -> Result<(), AgentFailure> {
    let record = &admission.record;
    record.validate()?;
    if admission.command_id.is_nil()
        || record.revision != 1
        || record.execution.is_some()
        || record.collection.is_some()
        || authority.is_some_and(|value| value.person_id != record.person_id)
        || !matches!(
            record.state,
            ActionState::PendingReview | ActionState::Approved | ActionState::Blocked { .. }
        )
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let policy_decision = match &record.origin {
        ActionOrigin::Direct { .. } => None,
        ActionOrigin::Expert { .. } => Some(floe_access::evaluate_operation_policy(
            &record.review.subject(),
            authority.ok_or(AgentFailure::PolicyDenied)?,
        )?),
    };
    match (&record.state, &record.authorization) {
        (
            ActionState::Approved,
            Some(OperationDecisionReceipt::DirectInstruction { command_id, .. }),
        ) if command_id == &admission.command_id => {}
        (
            ActionState::Approved,
            Some(receipt @ OperationDecisionReceipt::StandingPolicy { .. }),
        ) if policy_decision == Some(OperationPolicyDecision::Allow)
            && authority.is_some_and(|value| {
                floe_access::validate_operation_receipt(
                    &record.review.subject(),
                    Some(&record.review),
                    receipt,
                    None,
                    Some(value),
                    record.created_at,
                )
                .is_ok()
            }) => {}
        (ActionState::PendingReview, None)
            if policy_decision == Some(OperationPolicyDecision::Ask) => {}
        (
            ActionState::Blocked {
                reason: ActionBlockedReason::PolicyDenied,
            },
            None,
        ) if policy_decision == Some(OperationPolicyDecision::Deny) => {}
        _ => return Err(AgentFailure::PolicyDenied),
    }
    Ok(())
}

pub fn decide_action(
    record: &ActionRecord,
    decision: &ActionDecision,
    authority: Option<&OperationAuthorizationPolicy>,
) -> Result<ActionRecord, AgentFailure> {
    if decision.command_id.is_nil()
        || decision.person_id != record.person_id
        || decision.device_id != record.device_id
        || decision.action_id != record.id
        || decision.expected_revision != record.revision
        || decision.review_ref != record.review
        || record.execution.is_some()
        || authority.is_some_and(|value| value.person_id != record.person_id)
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
        OperationDecisionKind::Approve if record.state == ActionState::PendingReview => {
            let authority = authority.ok_or(AgentFailure::PolicyDenied)?;
            updated.authorization = Some(
                floe_access::reviewed_operation_receipt(
                    decision.command_id,
                    &decision.review_ref,
                    authority,
                    OperationDecisionKind::Approve,
                    decision.now,
                )
                .map_err(|_| AgentFailure::PolicyDenied)?,
            );
            updated.state = ActionState::Approved;
        }
        OperationDecisionKind::Reject if record.state == ActionState::PendingReview => {
            let authority = authority.ok_or(AgentFailure::PolicyDenied)?;
            updated.authorization = Some(
                floe_access::reviewed_operation_receipt(
                    decision.command_id,
                    &decision.review_ref,
                    authority,
                    OperationDecisionKind::Reject,
                    decision.now,
                )
                .map_err(|_| AgentFailure::PolicyDenied)?,
            );
            updated.state = ActionState::Rejected;
        }
        OperationDecisionKind::Cancel
            if matches!(
                record.state,
                ActionState::PendingReview | ActionState::Approved
            ) =>
        {
            let authority = authority.ok_or(AgentFailure::PolicyDenied)?;
            updated.authorization = Some(
                floe_access::reviewed_operation_receipt(
                    decision.command_id,
                    &decision.review_ref,
                    authority,
                    OperationDecisionKind::Cancel,
                    decision.now,
                )
                .map_err(|_| AgentFailure::PolicyDenied)?,
            );
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
    authority: Option<&OperationAuthorizationPolicy>,
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
        || authority.is_some_and(|value| value.person_id != record.person_id)
    {
        return Err(AgentFailure::Conflict);
    }
    let direct_command = match &record.origin {
        ActionOrigin::Direct { command_id, .. } => Some(*command_id),
        ActionOrigin::Expert { .. } => None,
    };
    floe_access::validate_operation_receipt(
        &record.review.subject(),
        Some(&record.review),
        &request.authorization,
        direct_command,
        authority,
        request.now,
    )?;
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
    if !matches!(record.origin, ActionOrigin::Expert { .. })
        || record.execution.is_some()
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use floe_access::{OperationApprovalRef, OperationPolicyMode};
    use floe_context_contract::{CalendarProvider, ConnectionId, SourceAuthority};
    use floe_kernel::PersonId;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    fn direct_record() -> (OperationAdmission, OperationAuthorizationPolicy) {
        let person_id = PersonId::new();
        let device_id = "device-a".to_owned();
        let command_id = Uuid::new_v4();
        let connection_id = ConnectionId::new();
        let source = ActionSourceFence {
            connection_id: connection_id.clone(),
            revision: 1,
            authority: SourceAuthority::new(),
            execution_owner: format!("apple:{device_id}"),
            resources: vec!["calendar-work".to_owned()],
            native_subject_fingerprint: "native-subject".to_owned(),
        };
        let now = Utc::now();
        let expires_at = now + Duration::minutes(15);
        let effect = CalendarEffect::Create {
            destination: CalendarDestination {
                provider: CalendarProvider::EventKit,
                connection_id,
                connection_revision: 1,
                calendar_id: "calendar-work".to_owned(),
                calendar_name: "Work".to_owned(),
            },
            title: "Manual focus block".to_owned(),
            schedule: floe_day::TimedSchedule::new(
                now + Duration::hours(1),
                now + Duration::hours(2),
                "UTC",
            )
            .expect("valid schedule"),
        };
        let effect_digest = effect.digest().expect("valid effect digest");
        let id = action_uuid(b"floe.actions.action.v1\0", person_id, command_id);
        let review = OperationApprovalRef {
            id: action_uuid(b"floe.actions.review.v1\0", person_id, command_id),
            operation_id: id,
            effect_digest,
            source_digest: source.digest().expect("valid source digest"),
            person_id,
            device_id: device_id.clone(),
            policy_revision: None,
            created_at: now,
            expires_at,
        };
        let authorization = floe_access::direct_operation_receipt(command_id, &review.subject())
            .expect("valid direct operation receipt");
        let record = ActionRecord {
            id,
            person_id,
            device_id: device_id.clone(),
            revision: 1,
            origin: ActionOrigin::Direct {
                command_id,
                actor_device_id: device_id.clone(),
            },
            effect,
            effect_digest,
            execution_id: action_uuid(b"floe.actions.execution.v1\0", person_id, command_id),
            source: source.clone(),
            dependency: None,
            review,
            authorization: Some(authorization),
            created_at: now,
            expires_at,
            state: ActionState::Approved,
            execution: None,
            collection: None,
        };
        let admission = OperationAdmission {
            command_id,
            request_digest: [7; 32],
            publication: None,
            record,
        };
        let policy = OperationAuthorizationPolicy {
            person_id,
            revision: 2,
            calendar_create: OperationPolicyMode::Deny,
        };
        (admission, policy)
    }

    #[test]
    fn manual_operation_dispatch_ignores_agent_policy_revision_but_keeps_local_fences() {
        let (admission, changed_policy) = direct_record();
        assert_eq!(changed_policy.calendar_create, OperationPolicyMode::Deny);
        validate_action_admission(&admission, Some(&changed_policy))
            .expect("manual admission is not subject to agent policy");
        assert!(
            invalidate_action_policy(&admission.record)
                .expect("policy transition")
                .is_none(),
            "agent policy changes must not cancel manual Day work"
        );

        let record = &admission.record;
        let request = DispatchIntent {
            action_id: record.id,
            person_id: record.person_id,
            device_id: record.device_id.clone(),
            expected_revision: record.revision,
            execution_id: record.execution_id,
            effect_digest: record.effect_digest,
            authorization: record
                .authorization
                .clone()
                .expect("manual instruction receipt"),
            current_source_fence: record.source.clone(),
            executor_generation: 1,
            now: record.created_at + Duration::seconds(1),
        };
        let (executing, intent) = prepare_action_dispatch(record, &request, Some(&changed_policy))
            .expect("changed agent policy does not fence manual dispatch");
        assert!(matches!(executing.state, ActionState::Executing { .. }));

        // The scripted provider seam observes one durable intent and performs
        // one effect only after the dispatch transition returns it.
        let applied = AtomicUsize::new(0);
        assert_eq!(intent.effect_digest, record.effect_digest);
        applied.fetch_add(1, Ordering::AcqRel);
        assert_eq!(applied.load(Ordering::Acquire), 1);

        let mut stale_source = request.clone();
        stale_source.current_source_fence.revision += 1;
        assert!(prepare_action_dispatch(record, &stale_source, Some(&changed_policy)).is_err());
        let mut foreign_actor = request;
        foreign_actor.device_id = "other-device".to_owned();
        assert!(prepare_action_dispatch(record, &foreign_actor, Some(&changed_policy)).is_err());
    }
}
