use serde::{Deserialize, Serialize};

use super::{
    AppInteractionKindDto, AppWireErrorDto, ContinuationRefDto, InteractionRefDto, MessageRefDto,
    RunRefDto, SessionRefDto, TaskRefDto, UuidRefDto,
};

const MAX_SESSION_MESSAGES: usize = 256;
const MAX_SESSION_TEXT_BYTES: usize = 16 * 1024;
// Matches the existing Conversation reviewed-identifier metadata bound.
const MAX_SESSION_METADATA_BYTES: usize = 256;
const MAX_SESSION_ARTIFACTS: usize = 16;
const MAX_SESSION_MEDIA_TYPES: usize = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSessionUsageDto {
    pub unknown_token_attempts: u32,
    pub unknown_cost_attempts: u32,
    pub model_attempts: u32,
    pub estimated_tokens: u64,
    pub estimated_cost_micros: u64,
    pub iterations: u32,
    pub capability_calls: u32,
    pub tokens: u64,
    pub cost_micros: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConversationSessionOutcomeDto {
    Completed {},
    Blocked {
        run_id: RunRefDto,
        review_group_id: UuidRefDto,
    },
    Halted {
        reason: AppWireErrorDto,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationTaskStateDto {
    Submitted,
    Working,
    Blocked,
    Completed,
    Failed,
    Cancelled,
    Rejected,
    TimedOut,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSessionArtifactDto {
    pub artifact_id: UuidRefDto,
    pub name: String,
    pub media_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSessionTaskDto {
    pub task_id: TaskRefDto,
    pub agent_id: String,
    pub state: ConversationTaskStateDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<AppWireErrorDto>,
    pub artifacts: Vec<ConversationSessionArtifactDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConversationSessionMessageDto {
    User {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        text: String,
    },
    Assistant {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        text: String,
    },
    Preamble {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        text: String,
    },
    Compaction {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        summary: String,
    },
    Capability {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        call_id: UuidRefDto,
        capability_id: String,
        // Serde's default Result representation preserves `{"Ok": ...}` / `{"Err": ...}`.
        result: Result<String, AppWireErrorDto>,
    },
    Delegation {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        task: ConversationSessionTaskDto,
    },
    Interaction {
        message_id: MessageRefDto,
        turn_id: UuidRefDto,
        interaction_id: InteractionRefDto,
        interaction_kind: AppInteractionKindDto,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSessionSnapshotDto {
    pub id: SessionRefDto,
    pub person_id: UuidRefDto,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn: Option<UuidRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outcome: Option<ConversationSessionOutcomeDto>,
    pub usage: ConversationSessionUsageDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_ref: Option<ContinuationRefDto>,
    pub messages: Vec<ConversationSessionMessageDto>,
    pub has_earlier_messages: bool,
}

impl ConversationSessionSnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.revision == 0 || self.revision > i64::MAX as u64 {
            return Err("conversation.session.revision");
        }
        if self.messages.len() > MAX_SESSION_MESSAGES {
            return Err("conversation.session.messages");
        }
        for message in &self.messages {
            message.validate()?;
        }
        Ok(())
    }
}

impl ConversationSessionMessageDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::User { text, .. }
            | Self::Assistant { text, .. }
            | Self::Preamble { text, .. } => validate_session_text(text, "text"),
            Self::Compaction { summary, .. } => validate_session_text(summary, "summary"),
            Self::Capability {
                capability_id,
                result,
                ..
            } => {
                validate_metadata(capability_id, "capability_id")?;
                if let Ok(value) = result {
                    validate_session_text(value, "result")?;
                }
                Ok(())
            }
            Self::Delegation { task, .. } => task.validate(),
            Self::Interaction { .. } => Ok(()),
        }
    }
}

impl ConversationSessionTaskDto {
    fn validate(&self) -> Result<(), &'static str> {
        validate_metadata(&self.agent_id, "agent_id")?;
        if self.artifacts.len() > MAX_SESSION_ARTIFACTS
            || self.artifacts.iter().any(|artifact| {
                !valid_metadata(&artifact.name)
                    || artifact.media_types.len() > MAX_SESSION_MEDIA_TYPES
                    || artifact
                        .media_types
                        .iter()
                        .any(|media_type| !valid_metadata(media_type))
            })
        {
            return Err("conversation.session.task.artifacts");
        }
        if let Some(result) = &self.result {
            validate_session_text(result, "task.result")?;
        }
        Ok(())
    }
}

fn validate_session_text(value: &str, field: &'static str) -> Result<(), &'static str> {
    if value.len() > MAX_SESSION_TEXT_BYTES {
        Err(field)
    } else {
        Ok(())
    }
}

fn validate_metadata(value: &str, field: &'static str) -> Result<(), &'static str> {
    if valid_metadata(value) {
        Ok(())
    } else {
        Err(field)
    }
}

fn valid_metadata(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_METADATA_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
