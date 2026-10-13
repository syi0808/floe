//! Conversation-owned input to one bounded semantic-compaction execution.
use floe_agent_contract::{AgentFailure, AgentMessage};
use floe_conversation_contract::{
    ConversationCheckpoint, ConversationReference, TranscriptReference,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Start before the transcript/archive structural ceilings. Inference still
/// applies the selected transport's actual codec and model-context limits.
pub const COMPACTION_TRIGGER_MESSAGES: usize = 96;
pub const COMPACTION_TRIGGER_BYTES: usize = 96 * 1024;
pub const COMPACTION_MIN_RETAIN_ENTRIES: u64 = 2;
pub const COMPACTION_PREFERRED_RETAIN_ENTRIES: u64 = 16;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionSource {
    pub session_id: Uuid,
    pub session_revision: u64,
    pub data_classes: Vec<floe_agent_contract::DataClass>,
    pub head: ConversationReference,
    pub previous_checkpoint: Option<ConversationCheckpoint>,
    pub through: TranscriptReference,
    pub prefix_digest: [u8; 32],
    pub through_turn_id: Uuid,
    pub messages: Vec<AgentMessage>,
}

impl CompactionSource {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.head
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        self.through
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let start = if let Some(previous) = &self.previous_checkpoint {
            previous
                .validate()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            if previous.through.conversation_id != self.head.conversation_id
                || previous.through.branch_id != self.head.branch_id
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            previous.through.sequence
        } else {
            0
        };
        let count = self
            .through
            .sequence
            .checked_sub(start)
            .and_then(|count| usize::try_from(count).ok())
            .and_then(|count| count.checked_add(usize::from(self.previous_checkpoint.is_some())))
            .ok_or(AgentFailure::StorageUnavailable)?;
        if self.data_classes.is_empty()
            || self.data_classes.len() > floe_agent_contract::MAX_INPUT_DATA_CLASSES
            || self
                .data_classes
                .iter()
                .enumerate()
                .any(|(index, class)| self.data_classes[..index].contains(class))
            || self.session_id.is_nil()
            || self.session_revision == 0
            || self.through_turn_id.is_nil()
            || self.prefix_digest == [0; 32]
            || self.through.conversation_id != self.head.conversation_id
            || self.through.branch_id != self.head.branch_id
            || self.through.sequence <= start
            || self
                .head
                .head_revision
                .saturating_sub(self.through.sequence)
                < COMPACTION_MIN_RETAIN_ENTRIES
            || self.messages.len() != count
            || count > COMPACTION_TRIGGER_MESSAGES
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.messages.iter().try_for_each(AgentMessage::validate)?;
        let mut ids = std::collections::HashSet::new();
        if self
            .messages
            .iter()
            .any(|message| message.message_id.is_nil() || !ids.insert(message.message_id))
            || self.previous_checkpoint.as_ref().is_some_and(|checkpoint| {
                self.messages.first().is_none_or(|message| {
                    message.role != floe_agent_contract::MessageRole::Assistant
                        || message.text != checkpoint.summary
                })
            })
        {
            return Err(AgentFailure::StorageUnavailable);
        }

        if serde_json::to_vec(&self.messages)
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .len()
            > COMPACTION_TRIGGER_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }
}
