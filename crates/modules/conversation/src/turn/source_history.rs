//! What a turn may still see of history that came from a source.
//!
//! A source read is authorized for the turn that made it. Once its result is in
//! the transcript, every answer written after it may be derived from it, so a
//! later turn that is no longer authorized must not be shown either. Which
//! results carry source data is the owner's to say, through
//! [`SourceHistoryBoundary`]; which messages a turn then keeps is this module's.

use floe_agent_contract::{AgentFailure, HistoryMessageSize, SourceHistoryBoundary};
use uuid::Uuid;

use crate::turn::{AgentMessage, ModelRequest};

fn crosses_boundary(message: &AgentMessage, boundary: &dyn SourceHistoryBoundary) -> bool {
    match message {
        // A compaction summary has no provenance, so it cannot be shown to be
        // free of source data.
        AgentMessage::Compaction { .. } => true,
        AgentMessage::Delegation { task, .. } => boundary.delegation_carries_source(
            &task.agent_id,
            task.state == floe_experts::A2ATaskState::Completed,
            !task.artifacts.is_empty(),
        ),
        AgentMessage::Capability {
            capability_id,
            result: Ok(_),
            ..
        } => boundary.capability_carries_source(capability_id),
        _ => false,
    }
}

/// Whether any of these messages carries source-derived history.
pub fn carries_source_history(
    messages: &[AgentMessage],
    boundary: &dyn SourceHistoryBoundary,
) -> bool {
    messages
        .iter()
        .any(|message| crosses_boundary(message, boundary))
}

/// Drop the source-derived history this turn is not authorized for.
///
/// A replay receipt describes the request that produced it, so a request whose
/// history changed cannot reuse one.
pub fn project_source_history(request: &mut ModelRequest, boundary: &dyn SourceHistoryBoundary) {
    if project_messages(&mut request.messages, request.turn_id, boundary) {
        request.replay.clear();
    }
}

fn project_messages(
    messages: &mut Vec<AgentMessage>,
    turn_id: Uuid,
    boundary: &dyn SourceHistoryBoundary,
) -> bool {
    let mut dependent = false;
    let mut removed = false;
    messages.retain(|message| {
        if message.turn_id() == turn_id {
            return true;
        }
        dependent |= crosses_boundary(message, boundary);
        // What the Person themselves said is theirs, not the source's.
        let retain = !dependent || matches!(message, AgentMessage::User { .. });
        removed |= !retain;
        retain
    });
    removed
}

/// Where a turn's history may start, given its byte budget.
///
/// Trimming for budget can leave a source boundary behind while keeping what
/// was derived from it, so when that happens the history starts at the current
/// turn instead.
pub fn bounded_source_history_start(
    messages: &[AgentMessage],
    current_turn: Uuid,
    max_bytes: usize,
    boundary: &dyn SourceHistoryBoundary,
) -> Result<usize, AgentFailure> {
    let sizes = messages
        .iter()
        .map(|message| {
            Ok(HistoryMessageSize {
                turn_id: message.turn_id(),
                encoded_bytes: serde_json::to_vec(message)
                    .map_err(|_| AgentFailure::InvalidInput)?
                    .len(),
            })
        })
        .collect::<Result<Vec<_>, AgentFailure>>()?;
    let selected = floe_agent_contract::bounded_history_start(&sizes, current_turn, max_bytes)?;
    if carries_source_history(&messages[..selected], boundary) {
        let current_start = messages
            .iter()
            .position(|message| message.turn_id() == current_turn)
            .ok_or(AgentFailure::InvalidInput)?;
        if messages[current_start..]
            .iter()
            .any(|message| message.turn_id() != current_turn)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        return Ok(current_start);
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    struct CalendarBoundary;

    impl SourceHistoryBoundary for CalendarBoundary {
        fn capability_carries_source(&self, capability_id: &str) -> bool {
            capability_id.starts_with("calendar.") || capability_id.starts_with("schedule.")
        }

        fn delegation_carries_source(
            &self,
            agent_id: &str,
            completed: bool,
            has_artifacts: bool,
        ) -> bool {
            agent_id == "schedule" && (completed || has_artifacts)
        }
    }

    #[test]
    fn old_source_results_and_later_generated_answers_are_not_reused() {
        let old = Uuid::new_v4();
        let later = Uuid::new_v4();
        let current = Uuid::new_v4();
        let mut messages = vec![
            AgentMessage::Assistant {
                turn_id: old,
                text: "Unrelated greeting".into(),
            },
            AgentMessage::Capability {
                turn_id: old,
                call_id: Uuid::new_v4(),
                capability_id: "calendar.read".into(),
                input: "{}".into(),
                result: Ok("secret event".into()),
            },
            AgentMessage::Assistant {
                turn_id: old,
                text: "Derived secret".into(),
            },
            AgentMessage::User {
                turn_id: later,
                text: "What does that mean?".into(),
            },
            AgentMessage::Assistant {
                turn_id: later,
                text: "Another derived secret".into(),
            },
            AgentMessage::User {
                turn_id: current,
                text: "Hello".into(),
            },
        ];
        let saved = messages.clone();
        assert!(project_messages(&mut messages, current, &CalendarBoundary));
        assert_eq!(
            messages,
            vec![saved[0].clone(), saved[3].clone(), saved[5].clone()]
        );
        assert!(carries_source_history(&saved, &CalendarBoundary));
        assert!(!carries_source_history(&messages, &CalendarBoundary));
    }

    #[test]
    fn current_results_and_source_independent_conversations_are_preserved() {
        let current = Uuid::new_v4();
        let mut messages = vec![
            AgentMessage::Assistant {
                turn_id: Uuid::new_v4(),
                text: "Hello".into(),
            },
            AgentMessage::Capability {
                turn_id: current,
                call_id: Uuid::new_v4(),
                capability_id: "calendar.read".into(),
                input: "{}".into(),
                result: Ok("fresh event".into()),
            },
        ];
        let original = messages.clone();
        assert!(!project_messages(&mut messages, current, &CalendarBoundary));
        assert_eq!(messages, original);
    }

    #[test]
    fn compacted_history_has_no_provenance_and_cannot_bypass_projection() {
        let old = Uuid::new_v4();
        let mut messages = vec![AgentMessage::Compaction {
            turn_id: old,
            summary: "A summary containing source details".into(),
            recovery: crate::turn::SessionRecoveryPointer {
                archive_id: Uuid::new_v4(),
                source_revision: 1,
                through_turn_id: old,
                archived_message_count: 3,
            },
        }];
        assert!(project_messages(
            &mut messages,
            Uuid::new_v4(),
            &CalendarBoundary
        ));
        assert!(messages.is_empty());
    }

    #[test]
    fn removing_a_legacy_boundary_cannot_expose_later_derived_history() {
        let current = Uuid::new_v4();
        let messages = [
            AgentMessage::Capability {
                turn_id: Uuid::new_v4(),
                call_id: Uuid::new_v4(),
                capability_id: "calendar.read".into(),
                input: "{}".into(),
                result: Ok("private event".repeat(100)),
            },
            AgentMessage::Assistant {
                turn_id: Uuid::new_v4(),
                text: "Later derived answer".into(),
            },
            AgentMessage::User {
                turn_id: current,
                text: "Hello".into(),
            },
        ];
        let suffix_bytes = serde_json::to_vec(&messages[1..]).unwrap().len();
        assert_eq!(
            bounded_source_history_start(&messages, current, suffix_bytes, &CalendarBoundary),
            Ok(2)
        );
    }

    #[test]
    fn serialized_unicode_and_escapes_use_the_exact_array_budget() {
        let current = Uuid::new_v4();
        let messages = [
            AgentMessage::Assistant {
                turn_id: Uuid::new_v4(),
                text: "과거 답변 \"quoted\"\n".repeat(8),
            },
            AgentMessage::User {
                turn_id: current,
                text: "안녕\n".into(),
            },
        ];
        let all_bytes = serde_json::to_vec(&messages).unwrap().len();
        let current_bytes = serde_json::to_vec(&messages[1..]).unwrap().len();
        assert_eq!(
            bounded_source_history_start(&messages, current, all_bytes, &CalendarBoundary),
            Ok(0)
        );
        assert_eq!(
            bounded_source_history_start(&messages, current, current_bytes, &CalendarBoundary),
            Ok(1)
        );
        assert_eq!(
            bounded_source_history_start(&messages, current, current_bytes - 1, &CalendarBoundary),
            Err(AgentFailure::BudgetExceeded)
        );
    }
}
