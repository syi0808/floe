use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{APP_WIRE_VERSION, AppWireErrorDto};

const MAX_TURN_TEXT_BYTES: usize = 8 * 1024;
const MAX_TURN_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppCommandRequestDto {
    pub schema_version: u32,
    pub request_id: Uuid,
    pub command_id: Uuid,
    pub command: AppCommandDto,
}

impl AppCommandRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != APP_WIRE_VERSION {
            return Err("schema_version");
        }
        if self.request_id.is_nil() {
            return Err("request_id");
        }
        if self.command_id.is_nil() {
            return Err("command_id");
        }
        self.command.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppCommandDto {
    #[serde(rename = "conversation.start_turn")]
    ConversationStartTurn {
        session_id: Uuid,
        expected_revision: u64,
        text: String,
        mode: AppTurnModeDto,
        #[serde(default, skip_serializing_if = "AppProfileSelectionDto::is_auto")]
        profile: AppProfileSelectionDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_of: Option<Uuid>,
    },
    #[serde(rename = "conversation.cancel_run")]
    ConversationCancelRun {
        run_id: Uuid,
        reason: AppCancelRunReasonDto,
    },
}

impl AppCommandDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::ConversationStartTurn {
                session_id,
                text,
                mode,
                profile,
                retry_of,
                ..
            } => {
                if session_id.is_nil() {
                    return Err("command.session_id");
                }
                let normalized = text.trim();
                if text.len() > MAX_TURN_PAYLOAD_BYTES
                    || normalized.is_empty()
                    || normalized.len() > MAX_TURN_TEXT_BYTES
                    || normalized
                        .chars()
                        .any(|character| character.is_control() && character != '\n')
                {
                    return Err("command.text");
                }
                if retry_of.is_some_and(|id| id.is_nil()) {
                    return Err("command.retry_of");
                }
                if retry_of.is_some() && !matches!(mode, AppTurnModeDto::NewTurn {}) {
                    return Err("command.retry_of");
                }
                mode.validate()?;
                profile.validate()
            }
            Self::ConversationCancelRun { run_id, .. } => {
                if run_id.is_nil() {
                    Err("command.run_id")
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppProfileSelectionDto {
    #[default]
    Auto,
    Explicit {
        profile_id: String,
    },
}

impl AppProfileSelectionDto {
    fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Auto => Ok(()),
            Self::Explicit { profile_id } => {
                if profile_id.trim() != profile_id
                    || profile_id.is_empty()
                    || profile_id.len() > 128
                    || profile_id.chars().any(char::is_control)
                {
                    Err("command.profile")
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppTurnModeDto {
    NewTurn {},
    Continue {
        continuation_ref: AppContinuationRefDto,
    },
}

impl AppTurnModeDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::NewTurn {} => Ok(()),
            Self::Continue { continuation_ref } => continuation_ref.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppContinuationRefDto {
    pub run_id: Uuid,
    pub executor_generation: u64,
    pub level: u8,
}

impl AppContinuationRefDto {
    fn validate(&self) -> Result<(), &'static str> {
        if self.run_id.is_nil() || self.executor_generation == 0 || !(1..=3).contains(&self.level) {
            Err("command.mode.continuation_ref")
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCancelRunReasonDto {
    UserRequested,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppCommandReceiptDto {
    pub command_id: Uuid,
    pub runtime_epoch: u64,
    pub admission: AppCommandStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<AppWireErrorDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCommandStatusDto {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCancelRunOutcomeDto {
    Accepted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppCommandResultDto {
    CommandReceipt {
        #[serde(flatten)]
        receipt: AppCommandReceiptDto,
    },
    CancelRunReceipt {
        command_id: Uuid,
        run_id: Uuid,
        runtime_epoch: u64,
        outcome: AppCancelRunOutcomeDto,
    },
}
