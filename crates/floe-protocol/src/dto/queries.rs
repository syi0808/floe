use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{APP_WIRE_VERSION, AppCommandReceiptDto, AppWireErrorDto};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppQueryRequestDto {
    pub schema_version: u32,
    pub request_id: Uuid,
    pub query: AppQueryDto,
}

impl AppQueryRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != APP_WIRE_VERSION {
            return Err("schema_version");
        }
        if self.request_id.is_nil() {
            return Err("request_id");
        }
        self.query.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppQueryDto {
    #[serde(rename = "conversation.get_command")]
    ConversationGetCommand { command_id: Uuid },
    #[serde(rename = "conversation.get_run")]
    ConversationGetRun { run_id: Uuid },
    #[serde(rename = "conversation.get_message")]
    ConversationGetMessage { message_id: Uuid },
}

impl AppQueryDto {
    fn validate(&self) -> Result<(), &'static str> {
        let (field, id) = match self {
            Self::ConversationGetCommand { command_id } => ("query.command_id", command_id),
            Self::ConversationGetRun { run_id } => ("query.run_id", run_id),
            Self::ConversationGetMessage { message_id } => ("query.message_id", message_id),
        };
        if id.is_nil() { Err(field) } else { Ok(()) }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppQueryResultDto {
    CommandReceipt {
        #[serde(flatten)]
        receipt: AppCommandReceiptDto,
    },
    UnknownCommand {
        command_id: Uuid,
    },
    RunSnapshot {
        #[serde(flatten)]
        run: AppRunSnapshotDto,
    },
    Message {
        #[serde(flatten)]
        message: AppMessageDto,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppMessageDto {
    pub message_id: Uuid,
    pub role: AppMessageRoleDto,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMessageRoleDto {
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppRunSnapshotDto {
    pub run_id: Uuid,
    pub session_id: Uuid,
    pub revision: u64,
    pub runtime_epoch: u64,
    pub executor_generation: u64,
    pub state: AppRunStateDto,
    pub progress: String,
    pub task_refs: Vec<Uuid>,
    pub attempt_refs: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<AppTurnReportDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppRunStateDto {
    Accepted,
    Executing,
    Finalizing,
    Cancelling,
    Finished,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppTurnReportDto {
    pub execution: AppTurnExecutionDto,
    pub reply: AppReplyStatusDto,
    pub issues: Vec<AppWireErrorDto>,
    pub action_refs: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_message_ref: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppTurnExecutionDto {
    Completed,
    Partial,
    Blocked,
    Failed,
    Cancelled,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppReplyStatusDto {
    Generated,
    PolicyNotice,
    NotProduced,
}
