//! Explicit learning signals and original-message linkage are Knowledge decisions.
use crate::{
    ContextMemory, LearnerReviewInput, LearningObservationKind, LearningSessionSnapshot,
    LearningTranscriptMessage,
};
use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;

fn explicit_turn(
    messages: &[LearningTranscriptMessage],
) -> Result<Option<(uuid::Uuid, &str, uuid::Uuid, &str, LearningObservationKind)>, AgentFailure> {
    let Some((user_index, user_id, user_turn, user_text)) = messages
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, message)| match message {
            LearningTranscriptMessage::User {
                message_id,
                turn_id,
                text,
            } => Some((index, *message_id, *turn_id, text.trim())),
            _ => None,
        })
    else {
        return Ok(None);
    };
    if user_id.is_nil() || user_turn.is_nil() {
        return Err(AgentFailure::StorageUnavailable);
    }
    let Some(signal) = crate::explicit_learning_signal(user_text) else {
        return Ok(None);
    };
    let Some((answer_turn, answer)) =
        messages[user_index + 1..]
            .iter()
            .rev()
            .find_map(|message| match message {
                LearningTranscriptMessage::Assistant {
                    turn_id,
                    original_user_message_id,
                    text,
                } if *original_user_message_id == user_id => Some((*turn_id, text.trim())),
                _ => None,
            })
    else {
        return Ok(None);
    };
    if answer_turn.is_nil() {
        return Err(AgentFailure::StorageUnavailable);
    }
    if answer.is_empty() {
        return Ok(None);
    }
    Ok(Some((user_turn, user_text, answer_turn, answer, signal)))
}

/// Storage fetches coverage for these actual linked turns; this selector grants no authority.
pub fn explicit_learning_evidence_turns(
    messages: &[LearningTranscriptMessage],
) -> Result<Vec<uuid::Uuid>, AgentFailure> {
    let Some((user_turn, _, answer_turn, _, _)) = explicit_turn(messages)? else {
        return Ok(vec![]);
    };
    let mut turns = vec![user_turn];
    if answer_turn != user_turn {
        turns.push(answer_turn);
    }
    Ok(turns)
}

pub fn explicit_review_input(
    snapshot: &LearningSessionSnapshot,
    memories: &[ContextMemory],
    now: DateTime<Utc>,
) -> Result<Option<LearnerReviewInput>, AgentFailure> {
    let evidence = &snapshot.evidence;
    evidence
        .coverage
        .validate()
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if evidence.purpose != crate::EvidenceProjectionPurpose::Learning {
        return Err(AgentFailure::PolicyDenied);
    }
    if evidence.coverage != floe_context_contract::DependencyCoverage::Independent
        || evidence.active_turn
        || evidence.pending_output
        || !evidence.personal
        || evidence.outcome != Some(crate::LearningOutcome::Completed)
    {
        return Ok(None);
    }
    let Some((user_turn, user_text, answer_turn, answer, signal)) =
        explicit_turn(&snapshot.messages)?
    else {
        return Ok(None);
    };
    let mut turn_ids = vec![user_turn];
    if answer_turn != user_turn {
        turn_ids.push(answer_turn);
    }
    if turn_ids.iter().any(|id| !evidence.turn_ids.contains(id)) {
        return Err(AgentFailure::StorageUnavailable);
    }
    let input = LearnerReviewInput {
        schema_version: crate::KNOWLEDGE_VERSION,
        run_id: uuid::Uuid::nil(),
        person_id: evidence.person_id,
        session_id: evidence.session_id,
        session_revision: evidence.revision,
        turn_ids,
        outcome: crate::LearningOutcome::Completed,
        digest: format!(
            "signal: {}\nuser evidence:\n{}\nassistant outcome:\n{}",
            signal_name(signal),
            bounded_text(user_text),
            bounded_text(answer)
        ),
        current_memories: memories.to_owned(),
        observed_at: now,
    };
    crate::validate_learner_input(&input, evidence.person_id)?;
    Ok(Some(input))
}

fn bounded_text(value: &str) -> &str {
    let mut end = value.len().min(1536);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}
fn signal_name(signal: LearningObservationKind) -> &'static str {
    match signal {
        LearningObservationKind::ExplicitRemember => "explicit_remember",
        LearningObservationKind::UserCorrection => "user_correction",
        LearningObservationKind::OutcomeConflict => "outcome_conflict",
        LearningObservationKind::ReusableProcedure => "reusable_procedure",
    }
}
