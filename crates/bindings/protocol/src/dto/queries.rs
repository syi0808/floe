use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    APP_WIRE_VERSION, ActionRefDto, AppCommandReceiptDto, AppWireErrorDto, AttemptRefDto,
    CommandIdDto, GatewayRefDto, InteractionRefDto, MessageRefDto, NativeHostQueryDto,
    OperationRefDto, RequestIdDto, ReviewRefDto, RunRefDto, SessionRefDto, TaskRefDto,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppQueryRequestDto {
    pub schema_version: u32,
    pub request_id: RequestIdDto,
    pub query: AppQueryDto,
}

impl AppQueryRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != APP_WIRE_VERSION {
            return Err("schema_version");
        }
        self.query.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AppQueryDto {
    NativeHost(NativeHostQueryDto),
    Product(AppProductQueryDto),
}

impl AppQueryDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::NativeHost(query) => query.validate(),
            Self::Product(query) => query.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppProductQueryDto {
    #[serde(rename = "actions.destinations")]
    ActionsDestinations {},
    #[serde(rename = "actions.authority.get")]
    ActionsAuthority {},
    #[serde(rename = "actions.list")]
    ActionsList {
        cursor: Option<super::ActionRefDto>,
        limit: u16,
    },
    #[serde(rename = "actions.inspect")]
    ActionsInspect { action_ref: super::ActionRefDto },
    #[serde(rename = "day.refresh.get")]
    DayRefreshGet { operation_ref: OperationRefDto },
    #[serde(rename = "day.snapshot")]
    DaySnapshot { day: super::DayQueryDto },
    #[serde(rename = "knowledge.memory.overview")]
    KnowledgeMemoryOverview {},
    #[serde(rename = "knowledge.memory.review")]
    KnowledgeMemoryReview {},
    #[serde(rename = "connections.overview")]
    ConnectionsOverview {},
    #[serde(rename = "connections.pairing.get")]
    ConnectionsPairingGet { operation_ref: OperationRefDto },
    #[serde(rename = "connections.gateway.get")]
    ConnectionsGatewayGet { gateway_ref: GatewayRefDto },
    #[serde(rename = "connections.integration.inspect_review")]
    ConnectionsIntegrationInspectReview { review_ref: ReviewRefDto },
    #[serde(rename = "connections.operation.get")]
    ConnectionsOperationGet { operation_ref: OperationRefDto },
    #[serde(rename = "connections.source.inspect_review")]
    ConnectionsSourceInspectReview { review_ref: ReviewRefDto },
    #[serde(rename = "connections.observe.inspect_review")]
    ConnectionsObserveInspectReview { review_ref: ReviewRefDto },
    #[serde(rename = "experts.directory")]
    ExpertsDirectory {},
    #[serde(rename = "experts.binding.inspect")]
    ExpertsInspectBinding {
        assignment_ref: super::AssignmentRefDto,
        requirement_ref: String,
    },
    #[serde(rename = "experts.binding.inspect_review")]
    ExpertsInspectBindingReview {
        review_ref: super::BindingReviewRefDto,
    },
    #[serde(rename = "conversation.session.get")]
    ConversationSessionGet { session_id: SessionRefDto },
    #[serde(rename = "conversation.session.resume")]
    ConversationSessionResume {},
    #[serde(rename = "vault.status")]
    VaultStatus {},
    #[serde(rename = "vault.read_result")]
    VaultReadResult { operation_id: Uuid, release: bool },
    #[serde(rename = "conversation.get_command")]
    ConversationGetCommand { command_id: CommandIdDto },
    #[serde(rename = "conversation.get_run")]
    ConversationGetRun { run_id: RunRefDto },
    #[serde(rename = "conversation.get_message")]
    ConversationGetMessage { message_id: MessageRefDto },
    #[serde(rename = "conversation.interaction.get")]
    ConversationInteractionGet { interaction_id: InteractionRefDto },
    #[serde(rename = "conversation.interaction.list")]
    ConversationInteractionList { session_id: SessionRefDto },
}

impl AppProductQueryDto {
    fn validate(&self) -> Result<(), &'static str> {
        let (field, id) = match self {
            Self::ActionsDestinations {}
            | Self::ActionsAuthority {}
            | Self::ActionsInspect { .. } => return Ok(()),
            Self::ActionsList { limit, .. } => {
                return if (1..=100).contains(limit) {
                    Ok(())
                } else {
                    Err("query.limit")
                };
            }
            Self::DayRefreshGet { .. } => return Ok(()),
            Self::DaySnapshot { .. } => return Ok(()),
            Self::KnowledgeMemoryOverview {}
            | Self::KnowledgeMemoryReview {}
            | Self::ConnectionsOverview {} => return Ok(()),
            Self::ConnectionsPairingGet { .. } | Self::ConnectionsOperationGet { .. } => {
                return Ok(());
            }
            Self::ConnectionsGatewayGet { .. } => return Ok(()),
            Self::ConnectionsIntegrationInspectReview { review_ref }
            | Self::ConnectionsSourceInspectReview { review_ref }
            | Self::ConnectionsObserveInspectReview { review_ref } => return review_ref.validate(),
            Self::ExpertsDirectory {} => return Ok(()),
            Self::ExpertsInspectBinding {
                requirement_ref, ..
            } => {
                return if requirement_ref.is_empty()
                    || requirement_ref.len() > 128
                    || requirement_ref.trim() != requirement_ref
                    || requirement_ref.chars().any(char::is_control)
                {
                    Err("query.requirement_ref")
                } else {
                    Ok(())
                };
            }
            Self::ExpertsInspectBindingReview { review_ref } => return review_ref.validate(),
            Self::ConversationSessionGet { .. } => return Ok(()),
            Self::ConversationSessionResume {} => return Ok(()),
            Self::VaultStatus {} => return Ok(()),
            Self::VaultReadResult { operation_id, .. } => ("query.operation_id", operation_id),
            Self::ConversationGetCommand { .. }
            | Self::ConversationGetRun { .. }
            | Self::ConversationGetMessage { .. }
            | Self::ConversationInteractionGet { .. }
            | Self::ConversationInteractionList { .. } => return Ok(()),
        };
        if id.is_nil() { Err(field) } else { Ok(()) }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppQueryResultDto {
    #[serde(rename = "calendar_acquisitions")]
    NativeHostCalendarAcquisitions {
        acquisitions: Vec<super::LocalContextAcquisitionRequestDto>,
    },
    #[serde(rename = "attention_acquisitions")]
    NativeHostAttentionAcquisitions {
        acquisitions: Vec<super::LocalContextAttentionAcquisitionRequestDto>,
    },
    #[serde(rename = "personal_acquisitions")]
    NativeHostPersonalAcquisitions {
        acquisitions: Vec<super::LocalContextPersonalAcquisitionRequestDto>,
    },
    #[serde(rename = "actions.destinations")]
    ActionsDestinations {
        destinations: Vec<super::ActionDestinationChoiceDto>,
    },
    #[serde(rename = "actions.authority")]
    ActionsAuthority {
        authority: super::ActionsAuthorityDto,
    },
    #[serde(rename = "actions.action")]
    Action {
        action: super::ActionSnapshotDto,
    },
    #[serde(rename = "actions.page")]
    ActionsPage {
        page: super::ActionsPageDto,
    },
    DaySnapshot {
        snapshot: super::DaySnapshotDto,
    },
    #[serde(rename = "day.refresh")]
    DayRefresh {
        refresh: super::DayRefreshStateDto,
    },
    #[serde(rename = "knowledge.memory.overview")]
    KnowledgeOverview {
        overview: super::AgentMemoryOverviewDto,
    },
    #[serde(rename = "knowledge.memory.review")]
    KnowledgeReview {
        review: super::MemoryReviewDisplayDto,
    },
    #[serde(rename = "connections.gateway_setup")]
    ConnectionsGatewaySetup {
        setup: super::GatewaySetupDto,
    },
    #[serde(rename = "connections.gateway")]
    ConnectionsGateway {
        gateway: super::GatewaySummaryDto,
    },
    #[serde(rename = "connections.pairing")]
    ConnectionsPairing {
        pairing: super::PairingSnapshotDto,
    },
    #[serde(rename = "connections.overview")]
    ConnectionsOverview {
        overview: super::ConnectionsOverviewDto,
    },
    #[serde(rename = "connections.integration_review")]
    ConnectionsIntegrationReview {
        review: super::IntegrationReviewDto,
    },
    #[serde(rename = "connections.operation")]
    ConnectionsOperation {
        operation: super::ConnectionOperationSnapshotDto,
    },
    #[serde(rename = "connections.source")]
    ConnectionsSource {
        source: super::SourceSummaryDto,
    },
    #[serde(rename = "connections.source_review")]
    ConnectionsSourceReview {
        review: super::SourceReviewDto,
    },
    #[serde(rename = "connections.observe_review")]
    ConnectionsObserveReview {
        review: super::ObserveReviewDto,
    },
    #[serde(rename = "connections.launch")]
    ConnectionsLaunch {
        launch_action: super::LaunchActionDto,
    },
    #[serde(rename = "experts.directory")]
    ExpertsDirectory {
        directory: super::ExpertDirectorySnapshotDto,
    },
    #[serde(rename = "experts.binding")]
    ExpertsBinding {
        binding: super::BindingInspectionDto,
    },
    #[serde(rename = "experts.binding_review")]
    ExpertsBindingReview {
        review: super::BindingReviewDto,
    },
    ConversationSession {
        session: super::ConversationSessionSnapshotDto,
    },
    ConversationSessionAbsent {},
    VaultOperation {
        #[serde(flatten)]
        result: super::VaultLifecycleResultDto,
    },
    CommandReceipt {
        #[serde(flatten)]
        receipt: AppCommandReceiptDto,
    },
    UnknownCommand {
        command_id: CommandIdDto,
    },
    RunSnapshot {
        #[serde(flatten)]
        run: AppRunSnapshotDto,
    },
    Message {
        #[serde(flatten)]
        message: AppMessageDto,
    },
    Interaction {
        #[serde(flatten)]
        snapshot: super::AppInteractionSnapshotDto,
    },
    InteractionList {
        #[serde(flatten)]
        list: super::AppInteractionListDto,
    },
    UnknownInteraction {
        interaction_id: InteractionRefDto,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppMessageDto {
    pub message_id: MessageRefDto,
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
    pub run_id: RunRefDto,
    pub session_id: SessionRefDto,
    pub revision: u64,
    pub runtime_epoch: u64,
    pub executor_generation: u64,
    pub state: AppRunStateDto,
    pub progress: String,
    pub task_refs: Vec<TaskRefDto>,
    pub attempt_refs: Vec<AttemptRefDto>,
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
    Blocked,
    Finished,
}

impl AppRunSnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.revision == 0 || self.revision > i64::MAX as u64 {
            return Err("run.revision");
        }
        if self.runtime_epoch == 0 || self.executor_generation == 0 {
            return Err("run.generation");
        }
        if self
            .task_refs
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != self.task_refs.len()
            || self
                .attempt_refs
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.attempt_refs.len()
        {
            return Err("run.references");
        }
        if let Some(report) = &self.report {
            report.validate()?;
        }
        if self.state == AppRunStateDto::Blocked {
            let report = self.report.as_ref().ok_or("run.report")?;
            if report.execution != AppTurnExecutionDto::Blocked
                || report.reply != AppReplyStatusDto::NotProduced
                || report.final_message_ref.is_some()
            {
                return Err("run.blocked_report");
            }
        }
        if self
            .report
            .as_ref()
            .is_some_and(|report| report.execution == AppTurnExecutionDto::Blocked)
            && self.state != AppRunStateDto::Blocked
        {
            return Err("run.blocked_state");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppTurnReportDto {
    pub execution: AppTurnExecutionDto,
    pub reply: AppReplyStatusDto,
    pub issues: Vec<AppWireErrorDto>,
    pub action_refs: Vec<ActionRefDto>,
    pub interaction_refs: Vec<InteractionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_message_ref: Option<MessageRefDto>,
}

impl AppTurnReportDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .interaction_refs
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != self.interaction_refs.len()
        {
            return Err("run.report.interaction_refs");
        }
        if self.execution == AppTurnExecutionDto::Blocked
            && (self.reply != AppReplyStatusDto::NotProduced
                || self.final_message_ref.is_some()
                || self.interaction_refs.is_empty())
        {
            return Err("run.report.blocked");
        }
        if self.execution == AppTurnExecutionDto::Blocked && !self.issues.is_empty() {
            return Err("run.report.blocked_issues");
        }
        Ok(())
    }
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
