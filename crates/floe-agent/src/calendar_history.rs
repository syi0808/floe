use crate::{A2ATaskState, AgentMessage, ModelRequest};

fn calendar_boundary(message: &AgentMessage) -> bool {
    match message {
        AgentMessage::Compaction { .. } => true,
        AgentMessage::Delegation { task, .. } => {
            (task.agent_id == crate::BuiltinExpertKind::Schedule.package_id()
                || task.agent_id == "schedule")
                && (task.state == A2ATaskState::Completed || !task.artifacts.is_empty())
        }
        AgentMessage::Capability {
            capability_id,
            result: Ok(_),
            ..
        } => capability_id.starts_with("calendar.") || capability_id.starts_with("schedule."),
        _ => false,
    }
}

pub fn has_calendar_history(messages: &[AgentMessage]) -> bool {
    messages.iter().any(calendar_boundary)
}

pub fn project_calendar_history(request: &mut ModelRequest) {
    if project_messages(&mut request.messages, request.turn_id) {
        request.replay.clear();
    }
}

fn project_messages(messages: &mut Vec<AgentMessage>, turn_id: uuid::Uuid) -> bool {
    let mut dependent = false;
    let mut removed = false;
    messages.retain(|message| {
        if message.turn_id() == turn_id {
            return true;
        }
        dependent |= calendar_boundary(message);
        let retain = !dependent || matches!(message, AgentMessage::User { .. });
        removed |= !retain;
        retain
    });
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn old_calendar_results_and_later_generated_answers_are_not_reused() {
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
        assert!(project_messages(&mut messages, current));
        assert_eq!(
            messages,
            vec![saved[0].clone(), saved[3].clone(), saved[5].clone()]
        );
        assert!(has_calendar_history(&saved));
        assert!(!has_calendar_history(&messages));
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
        assert!(!project_messages(&mut messages, current));
        assert_eq!(messages, original);
    }

    #[test]
    fn compacted_history_has_no_provenance_and_cannot_bypass_projection() {
        let old = Uuid::new_v4();
        let mut messages = vec![AgentMessage::Compaction {
            turn_id: old,
            summary: "A summary containing source details".into(),
            recovery: crate::SessionRecoveryPointer {
                archive_id: Uuid::new_v4(),
                source_revision: 1,
                through_turn_id: old,
                archived_message_count: 3,
            },
        }];
        assert!(project_messages(&mut messages, Uuid::new_v4()));
        assert!(messages.is_empty());
    }

    #[test]
    fn builtin_schedule_delegations_use_the_catalog_identity() {
        let turn_id = Uuid::new_v4();
        let mut messages = vec![AgentMessage::Delegation {
            turn_id,
            task: crate::A2ATask {
                id: Uuid::new_v4(),
                context_id: Uuid::new_v4(),
                agent_id: crate::BuiltinExpertKind::Schedule.package_id().into(),
                state: A2ATaskState::Completed,
                history: vec![],
                artifacts: vec![],
                failure: None,
            },
        }];
        assert!(has_calendar_history(&messages));
        assert!(project_messages(&mut messages, Uuid::new_v4()));
        assert!(messages.is_empty());
    }
}
