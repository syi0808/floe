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
    Runtime(RuntimeQueryDto),
    Product(AppProductQueryDto),
}

impl AppQueryDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::NativeHost(query) => query.validate(),
            Self::Runtime(query) => query.validate(),
            Self::Product(query) => query.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum RuntimeQueryDto {
    #[serde(rename = "runtime.readiness")]
    Readiness {},
    #[serde(rename = "runtime.preparation.get")]
    PreparationGet { operation_id: Uuid },
}

impl RuntimeQueryDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Readiness {} => Ok(()),
            Self::PreparationGet { operation_id } => {
                if valid_runtime_uuid(*operation_id) {
                    Ok(())
                } else {
                    Err("query.operation_id")
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppProductQueryDto {
    #[serde(rename = "actions.destinations")]
    ActionsDestinations {},
    #[serde(rename = "actions.proposal.preview")]
    ActionsProposalPreview {
        receipt: super::TaskExecutionReceiptRefDto,
        artifact_id: super::UuidRefDto,
    },
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
    #[serde(rename = "memory.overview")]
    MemoryOverview {},
    #[serde(rename = "memory.review")]
    MemoryReview {},
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
    ConversationSessionGet {
        session_id: SessionRefDto,
        before_message_id: Option<super::UuidRefDto>,
    },
    #[serde(rename = "conversation.session.resume")]
    ConversationSessionResume {},
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
        match self {
            Self::ActionsDestinations {}
            | Self::ActionsAuthority {}
            | Self::ActionsInspect { .. } => return Ok(()),
            Self::ActionsProposalPreview { receipt, .. } => return receipt.validate(),
            Self::ActionsList { limit, .. } => {
                return if (1..=100).contains(limit) {
                    Ok(())
                } else {
                    Err("query.limit")
                };
            }
            Self::DayRefreshGet { .. } => return Ok(()),
            Self::DaySnapshot { .. } => return Ok(()),
            Self::MemoryOverview {} | Self::MemoryReview {} | Self::ConnectionsOverview {} => {
                return Ok(());
            }
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
            Self::ConversationGetCommand { .. }
            | Self::ConversationGetRun { .. }
            | Self::ConversationGetMessage { .. }
            | Self::ConversationInteractionGet { .. }
            | Self::ConversationInteractionList { .. } => return Ok(()),
        }
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
    #[serde(rename = "actions.proposal.preview")]
    ActionsProposalPreview {
        preview: super::ActionProposalPreviewDto,
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
    #[serde(rename = "memory.overview")]
    MemoryOverview {
        overview: super::AgentMemoryOverviewDto,
    },
    #[serde(rename = "memory.review")]
    MemoryReview {
        review: super::MemoryReviewDisplayDto,
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
    #[serde(rename = "runtime.readiness")]
    RuntimeReadiness {
        #[serde(flatten)]
        readiness: super::RuntimeReadinessDto,
    },
    #[serde(rename = "runtime.preparation")]
    RuntimePreparation {
        #[serde(flatten)]
        result: super::RuntimePreparationResultDto,
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

fn valid_runtime_uuid(id: Uuid) -> bool {
    !id.is_nil()
        && id.get_version() == Some(uuid::Version::Random)
        && id.get_variant() == uuid::Variant::RFC4122
}

#[cfg(test)]
mod runtime_query_tests {
    use super::{AppQueryDto, AppQueryRequestDto, RuntimeQueryDto};
    use crate::{APP_WIRE_VERSION, RequestIdDto};
    use uuid::Uuid;

    #[test]
    fn preparation_history_query_requires_uuid_v4() {
        let valid_id = Uuid::new_v4();
        let valid = AppQueryRequestDto {
            schema_version: APP_WIRE_VERSION,
            request_id: RequestIdDto::new(Uuid::new_v4()).expect("non-nil request ID"),
            query: AppQueryDto::Runtime(RuntimeQueryDto::PreparationGet {
                operation_id: valid_id,
            }),
        };
        assert_eq!(valid.validate(), Ok(()));
        let value = serde_json::to_value(valid).expect("serialize Runtime query envelope");
        assert_eq!(value["query"]["kind"], "runtime.preparation.get");
        assert_eq!(value["query"]["operation_id"], valid_id.to_string());

        let uuid_v7 = Uuid::parse_str("01890f47-2e80-7cc7-b0b5-f12c0a06e63f")
            .expect("valid UUID v7 test identity");
        let invalid = AppQueryRequestDto {
            schema_version: APP_WIRE_VERSION,
            request_id: RequestIdDto::new(Uuid::new_v4()).expect("non-nil request ID"),
            query: AppQueryDto::Runtime(RuntimeQueryDto::PreparationGet {
                operation_id: uuid_v7,
            }),
        };
        assert_eq!(invalid.validate(), Err("query.operation_id"));
    }
}

#[cfg(test)]
mod memory_query_namespace_tests {
    use super::AppProductQueryDto;

    #[test]
    fn memory_queries_use_only_the_product_namespace() {
        let overview = serde_json::to_value(AppProductQueryDto::MemoryOverview {})
            .expect("serialize Memory overview query");
        let review = serde_json::to_value(AppProductQueryDto::MemoryReview {})
            .expect("serialize Memory review query");
        assert_eq!(overview["kind"], "memory.overview");
        assert_eq!(review["kind"], "memory.review");
        assert!(
            serde_json::from_value::<AppProductQueryDto>(serde_json::json!({
                "kind": "knowledge.memory.overview"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<AppProductQueryDto>(serde_json::json!({
                "kind": "knowledge.memory.review"
            }))
            .is_err()
        );
    }
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
