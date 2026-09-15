use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{APP_WIRE_VERSION, AppWireErrorDto};

const MAX_TURN_TEXT_BYTES: usize = 64 * 1024;

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
                retry_of,
                ..
            } => {
                if session_id.is_nil() {
                    return Err("command.session_id");
                }
                if text.trim().is_empty() || text.len() > MAX_TURN_TEXT_BYTES {
                    return Err("command.text");
                }
                if retry_of.is_some_and(|id| id.is_nil()) {
                    return Err("command.retry_of");
                }
                if retry_of.is_some() && !matches!(mode, AppTurnModeDto::NewTurn {}) {
                    return Err("command.retry_of");
                }
                mode.validate()
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
