//! Stable public Session message and cursor aliases.
//!
//! Snapshot projection and the explicit frozen-snapshot preparer share this
//! pure computation so imported aliases cannot diverge from existing cursors.

use std::collections::HashMap;

use uuid::Uuid;

use super::AgentMessage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SessionMessageAlias {
    pub message_id: Uuid,
    /// Zero-based occurrence used by the existing Preamble and Compaction
    /// aliases. Other message kinds do not use a synthetic occurrence ordinal.
    pub synthetic_occurrence_ordinal: Option<usize>,
}

pub(crate) fn session_message_aliases(messages: &[AgentMessage]) -> Vec<SessionMessageAlias> {
    let mut occurrences = HashMap::new();
    messages
        .iter()
        .map(|message| {
            let kind = match message {
                AgentMessage::Preamble { .. } => 0u8,
                AgentMessage::Compaction { .. } => 1u8,
                _ => 2u8,
            };
            let ordinal = occurrences
                .entry((message.turn_id(), kind))
                .or_insert(0usize);
            let occurrence = *ordinal;
            *ordinal += 1;
            SessionMessageAlias {
                message_id: projected_message_id(message, occurrence),
                synthetic_occurrence_ordinal: matches!(
                    message,
                    AgentMessage::Preamble { .. } | AgentMessage::Compaction { .. }
                )
                .then_some(occurrence),
            }
        })
        .collect()
}

fn projected_message_id(message: &AgentMessage, ordinal: usize) -> Uuid {
    let turn = message.turn_id();
    match message {
        AgentMessage::User { message_id, .. } => *message_id,
        AgentMessage::Assistant { .. } => turn,
        AgentMessage::Preamble { text, .. } => {
            Uuid::new_v5(&turn, format!("preamble:{ordinal}:{text}").as_bytes())
        }
        AgentMessage::Compaction { .. } => {
            Uuid::new_v5(&turn, format!("compaction:{ordinal}").as_bytes())
        }
        AgentMessage::Capability { call_id, .. } => *call_id,
        AgentMessage::Delegation { task, .. } => task.task_id.as_uuid(),
        AgentMessage::Interaction { interaction_id, .. } => *interaction_id,
    }
}
