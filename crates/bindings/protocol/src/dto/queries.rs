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
    #[serde(rename = "context.read")]
    ContextRead { query: super::ContextQueryDto },
    #[serde(rename = "actions.capabilities")]
    ActionsCapabilities {},
    #[serde(rename = "actions.authority")]
    ActionsAuthority {},
    #[serde(rename = "actions.list")]
    ActionsList {},
    #[serde(rename = "actions.get")]
    ActionsGet { action_id: Uuid },
    #[serde(rename = "actions.proposal.inspect")]
    ActionsProposalInspect {
        session_id: Uuid,
        invocation_id: Uuid,
    },
    #[serde(rename = "actions.read_result")]
    ActionsReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "day.snapshot")]
    DaySnapshot { day: super::DayQueryDto },
    #[serde(rename = "knowledge.memory.overview")]
    KnowledgeMemoryOverview {},
    #[serde(rename = "knowledge.memory.review")]
    KnowledgeMemoryReview {},
    #[serde(rename = "knowledge.read_result")]
    KnowledgeReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "connections.overview")]
    ConnectionsOverview {},
    #[serde(rename = "connections.read_result")]
    ConnectionsReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "access.calendar.preview")]
    AccessCalendarPreview {
        request: super::CalendarSubjectIntentDto,
    },
    #[serde(rename = "access.calendar.inspect")]
    AccessCalendarInspect {},
    #[serde(rename = "access.personal.inspect")]
    AccessPersonalInspect { connector: String },
    #[serde(rename = "access.contacts.inspect")]
    AccessContactsInspect {
        connector: String,
        selected_handles: Vec<String>,
    },
    #[serde(rename = "access.local.read_result")]
    AccessLocalReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "experts.registry.inspect")]
    ExpertsRegistryInspect {},
    #[serde(rename = "experts.read_result")]
    ExpertsReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "conversation.session.get")]
    ConversationSessionGet { session_id: Uuid },
    #[serde(rename = "conversation.session.read_result")]
    ConversationSessionReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "vault.status")]
    VaultStatus {},
    #[serde(rename = "vault.read_result")]
    VaultReadResult { operation_id: Uuid, release: bool },
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
            Self::ContextRead { .. } => return Ok(()),
            Self::ActionsCapabilities {} | Self::ActionsAuthority {} | Self::ActionsList {} => {
                return Ok(());
            }
            Self::ActionsGet { action_id } => ("query.action_id", action_id),
            Self::ActionsReadResult { operation_id, .. } => ("query.operation_id", operation_id),
            Self::ActionsProposalInspect {
                session_id,
                invocation_id,
            } => {
                if session_id.is_nil() {
                    return Err("query.session_id");
                }
                ("query.invocation_id", invocation_id)
            }
            Self::DaySnapshot { .. } => return Ok(()),
            Self::KnowledgeMemoryOverview {}
            | Self::KnowledgeMemoryReview {}
            | Self::ConnectionsOverview {} => return Ok(()),
            Self::KnowledgeReadResult { operation_id, .. }
            | Self::ConnectionsReadResult { operation_id, .. } => {
                ("query.operation_id", operation_id)
            }
            Self::AccessCalendarPreview { request } => return request.validate(),
            Self::AccessCalendarInspect {} => return Ok(()),
            Self::AccessPersonalInspect { connector } => {
                return if super::local_access::identifier(connector) {
                    Ok(())
                } else {
                    Err("query.connector")
                };
            }
            Self::AccessContactsInspect {
                connector,
                selected_handles,
            } => {
                return if super::local_access::identifier(connector)
                    && super::local_access::identifiers(selected_handles, 64)
                {
                    Ok(())
                } else {
                    Err("query.selection")
                };
            }
            Self::AccessLocalReadResult { operation_id, .. } => {
                ("query.operation_id", operation_id)
            }
            Self::ExpertsRegistryInspect {} => return Ok(()),
            Self::ExpertsReadResult { operation_id, .. } => ("query.operation_id", operation_id),
            Self::ConversationSessionGet { session_id } => ("query.session_id", session_id),
            Self::ConversationSessionReadResult { operation_id, .. } => {
                ("query.operation_id", operation_id)
            }
            Self::VaultStatus {} => return Ok(()),
            Self::VaultReadResult { operation_id, .. } => ("query.operation_id", operation_id),
            Self::ConversationGetCommand { command_id } => ("query.command_id", command_id),
            Self::ConversationGetRun { run_id } => ("query.run_id", run_id),
            Self::ConversationGetMessage { message_id } => ("query.message_id", message_id),
        };
        if id.is_nil() { Err(field) } else { Ok(()) }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppQueryResultDto {
    ContextRead {
        context: super::LocalContextResultDto,
    },
    ActionOperation {
        #[serde(flatten)]
        result: super::ActionOperationResultDto,
    },
    DaySnapshot {
        snapshot: super::DaySnapshotDto,
    },
    KnowledgeOperation {
        #[serde(flatten)]
        result: super::KnowledgeOperationResultDto,
    },
    ConnectionsOperation {
        #[serde(flatten)]
        result: super::ConnectionsResultDto,
    },
    LocalAccessOperation {
        #[serde(flatten)]
        result: super::LocalAccessResultDto,
    },
    ExpertOperation {
        #[serde(flatten)]
        result: super::ExpertOperationResultDto,
    },
    ConversationSessionOperation {
        #[serde(flatten)]
        result: super::ConversationSessionResultDto,
    },
    VaultOperation {
        #[serde(flatten)]
        result: super::VaultLifecycleResultDto,
    },
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
