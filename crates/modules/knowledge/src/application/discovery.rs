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
    // Evidence is not a semantic summary: a clipped suffix can remove a
    // correction or negation. Preserve complete linked messages or defer this
    // input until an owner-controlled compaction path can represent it safely.
    let framing = format!(
        "signal: {}\nuser evidence:\n\nassistant outcome:\n",
        signal_name(signal)
    );
    let digest_bytes = framing
        .len()
        .saturating_add(user_text.len())
        .saturating_add(answer.len());
    if digest_bytes > super::learner::MAX_OBSERVATION_DIGEST_BYTES {
        return Err(AgentFailure::BudgetExceeded);
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
            user_text,
            answer
        ),
        current_memories: memories.to_owned(),
        observed_at: now,
    };
    crate::validate_learner_input(&input, evidence.person_id)?;
    Ok(Some(input))
}

fn signal_name(signal: LearningObservationKind) -> &'static str {
    match signal {
        LearningObservationKind::ExplicitRemember => "explicit_remember",
        LearningObservationKind::UserCorrection => "user_correction",
        LearningObservationKind::OutcomeConflict => "outcome_conflict",
        LearningObservationKind::ReusableProcedure => "reusable_procedure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use floe_context_contract::DependencyCoverage;
    use uuid::Uuid;

    fn snapshot(user: String, answer: String) -> LearningSessionSnapshot {
        let turn = Uuid::new_v4();
        let message = Uuid::new_v4();
        LearningSessionSnapshot {
            evidence: LearningEvidenceSnapshot {
                person_id: floe_kernel::PersonId::new(),
                session_id: Uuid::new_v4(),
                revision: 1,
                outcome: Some(LearningOutcome::Completed),
                personal: true,
                active_turn: false,
                pending_output: false,
                turn_ids: vec![turn],
                coverage: DependencyCoverage::Independent,
                purpose: EvidenceProjectionPurpose::Learning,
            },
            messages: vec![
                LearningTranscriptMessage::User {
                    message_id: message,
                    turn_id: turn,
                    text: user,
                },
                LearningTranscriptMessage::Assistant {
                    turn_id: turn,
                    original_user_message_id: message,
                    text: answer,
                },
            ],
        }
    }

    #[test]
    fn discovery_preserves_late_user_correction_and_assistant_qualification() {
        let user = format!(
            "Please remember: {} Actually, I do NOT prefer coffee.",
            "가".repeat(530)
        );
        let answer = format!("{} Correction: tea, not coffee.", "나".repeat(530));
        let snapshot = snapshot(user.clone(), answer.clone());
        let input = explicit_review_input(&snapshot, &[], Utc::now())
            .unwrap()
            .unwrap();
        assert_eq!(
            input.digest,
            format!(
                "signal: explicit_remember\nuser evidence:\n{user}\nassistant outcome:\n{answer}"
            )
        );
    }

    #[test]
    fn oversized_learning_evidence_is_not_admitted_as_a_silent_prefix() {
        let snapshot = snapshot(
            format!("Please remember: {} End qualification.", "x".repeat(4096)),
            "Acknowledged.".into(),
        );
        assert!(matches!(
            explicit_review_input(&snapshot, &[], Utc::now()),
            Err(AgentFailure::BudgetExceeded)
        ));
    }
    #[test]
    fn discovery_budget_uses_complete_framing_and_utf8_bytes() {
        let base = snapshot("Please remember: ".into(), "Acknowledged.".into());
        let initial = explicit_review_input(&base, &[], Utc::now())
            .unwrap()
            .unwrap();
        let remaining = super::super::learner::MAX_OBSERVATION_DIGEST_BYTES - initial.digest.len();
        let mut exact = base;
        if let LearningTranscriptMessage::User { text, .. } = &mut exact.messages[0] {
            text.push_str(&"x".repeat(remaining - 1));
            // The existing trailing space is now internal, adding one byte.
        }
        let input = explicit_review_input(&exact, &[], Utc::now())
            .unwrap()
            .unwrap();
        assert_eq!(
            input.digest.len(),
            super::super::learner::MAX_OBSERVATION_DIGEST_BYTES
        );
        if let LearningTranscriptMessage::User { text, .. } = &mut exact.messages[0] {
            text.push('가');
        }
        assert!(matches!(
            explicit_review_input(&exact, &[], Utc::now()),
            Err(AgentFailure::BudgetExceeded)
        ));
    }
}
