use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    APP_WIRE_VERSION, AppWireErrorDto, CommandIdDto, InteractionRefDto, NativeHostCommandDto,
    RequestIdDto, ReviewRefDto, RunRefDto, SessionRefDto,
};

const MAX_TURN_TEXT_BYTES: usize = 8 * 1024;
const MAX_TURN_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppCommandRequestDto {
    pub schema_version: u32,
    pub request_id: RequestIdDto,
    pub command_id: CommandIdDto,
    pub command: AppCommandDto,
}

impl AppCommandRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != APP_WIRE_VERSION {
            return Err("schema_version");
        }
        self.command.validate()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AppCommandDto {
    NativeHost(NativeHostCommandDto),
    Product(AppProductCommandDto),
}

impl AppCommandDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::NativeHost(command) => command.validate(),
            Self::Product(command) => command.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum AppProductCommandDto {
    #[serde(rename = "connections.pairing.start")]
    ConnectionsPairingStart { address_text: String },
    #[serde(rename = "connections.pairing.cancel")]
    ConnectionsPairingCancel {
        operation_ref: super::OperationRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.gateway.forget")]
    ConnectionsGatewayForget {
        gateway_ref: super::GatewayRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.integration.prepare_review")]
    ConnectionsIntegrationPrepareReview {
        integration_ref: super::IntegrationRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.integration.start")]
    ConnectionsIntegrationStart {
        integration_ref: super::IntegrationRefDto,
        review_ref: ReviewRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.operation.cancel")]
    ConnectionsOperationCancel {
        operation_ref: super::OperationRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.source.prepare_review")]
    ConnectionsSourcePrepareReview {
        source_ref: super::ConnectionsSourceRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.source.configure")]
    ConnectionsSourceConfigure {
        source_ref: super::ConnectionsSourceRefDto,
        review_ref: ReviewRefDto,
        selected_resource_refs: Vec<super::ResourceRefDto>,
        expected_revision: u64,
    },
    #[serde(rename = "connections.disconnect")]
    ConnectionsDisconnect {
        source_ref: super::ConnectionsSourceRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "connections.observe.prepare_review")]
    ConnectionsObservePrepareReview {
        source_ref: super::ConnectionsSourceRefDto,
        expected_revision: u64,
        requested_processing: super::RequestedProcessingDto,
    },
    #[serde(rename = "connections.observe.set")]
    ConnectionsObserveSet {
        mutation: super::ConnectionObserveSetMutationDto,
    },
    #[serde(rename = "connections.gateway.management_launch")]
    ConnectionsGatewayManagementLaunch {
        gateway_ref: super::GatewayRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "actions.submit")]
    ActionsSubmit { intent: super::ActionIntentDto },
    #[serde(rename = "actions.decide")]
    ActionsDecide {
        action_ref: super::ActionRefDto,
        review_ref: super::ActionReviewRefDto,
        decision: super::ActionDecisionKindDto,
        expected_revision: u64,
    },
    #[serde(rename = "actions.reconcile")]
    ActionsReconcile {
        action_ref: super::ActionRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "actions.authority.set_calendar_create")]
    ActionsSetAuthority {
        mode: super::ActionAuthorityModeDto,
        expected_revision: u64,
    },
    #[serde(rename = "day.refresh")]
    DayRefresh { day: super::DayQueryDto },
    #[serde(rename = "day.mutate")]
    DayMutate {
        day: super::DayQueryDto,
        mutation: super::DayMutationDto,
    },
    #[serde(rename = "knowledge.memory.decide")]
    KnowledgeMemoryDecide {
        candidate_id: Uuid,
        decision: super::AgentMemoryReviewDecisionKindDto,
    },
    #[serde(rename = "experts.installation.set_enabled")]
    ExpertsSetInstallationEnabled {
        installation_ref: super::UuidRefDto,
        expected_revision: u64,
        enabled: bool,
    },
    #[serde(rename = "experts.binding.prepare_review")]
    ExpertsPrepareBindingReview {
        assignment_ref: super::AssignmentRefDto,
        requirement_ref: String,
        expected_binding_revision: u64,
    },
    #[serde(rename = "experts.binding.replace")]
    ExpertsBindingReplace {
        review_ref: super::BindingReviewRefDto,
        expected_binding_revision: u64,
        candidate_refs: Vec<super::UuidRefDto>,
    },
    #[serde(rename = "conversation.session.start")]
    ConversationSessionStart {},
    #[serde(rename = "conversation.session.recover")]
    ConversationSessionRecover {
        session_id: SessionRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "vault.create")]
    VaultCreate {},
    #[serde(rename = "vault.unlock")]
    VaultUnlock {},
    #[serde(rename = "vault.lock")]
    VaultLock {},
    #[serde(rename = "conversation.start_turn")]
    ConversationStartTurn {
        session_id: SessionRefDto,
        expected_revision: u64,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation_ref: Option<ContinuationRefDto>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_of: Option<RunRefDto>,
    },
    #[serde(rename = "conversation.cancel_run")]
    ConversationCancelRun { run_id: RunRefDto },
    #[serde(rename = "conversation.interaction.resolve")]
    ConversationInteractionResolve {
        interaction_id: InteractionRefDto,
        session_id: SessionRefDto,
        expected_revision: u64,
        decision: super::AppInteractionDecisionDto,
        reviewed_digest: super::DigestHex64Dto,
    },
    #[serde(rename = "conversation.interaction.refresh")]
    ConversationInteractionRefresh {
        interaction_id: InteractionRefDto,
        session_id: SessionRefDto,
        expected_revision: u64,
    },
}

impl AppProductCommandDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::ConnectionsPairingStart { address_text } => {
                if !valid_text(address_text, 2048) || !valid_gateway_setup_address(address_text) {
                    Err("command.address_text")
                } else {
                    Ok(())
                }
            }
            Self::ConnectionsPairingCancel {
                expected_revision, ..
            }
            | Self::ConnectionsGatewayForget {
                expected_revision, ..
            }
            | Self::ConnectionsIntegrationPrepareReview {
                expected_revision, ..
            }
            | Self::ConnectionsOperationCancel {
                expected_revision, ..
            }
            | Self::ConnectionsSourcePrepareReview {
                expected_revision, ..
            }
            | Self::ConnectionsDisconnect {
                expected_revision, ..
            }
            | Self::ConnectionsObservePrepareReview {
                expected_revision, ..
            }
            | Self::ConnectionsGatewayManagementLaunch {
                expected_revision, ..
            } => validate_revision(*expected_revision),
            Self::ConnectionsSourceConfigure {
                selected_resource_refs,
                review_ref,
                expected_revision,
                ..
            } => {
                validate_revision(*expected_revision)?;
                review_ref.validate()?;
                if selected_resource_refs.len() > 4096
                    || selected_resource_refs
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        != selected_resource_refs.len()
                {
                    Err("command.selected_resource_refs")
                } else {
                    Ok(())
                }
            }
            Self::ConnectionsIntegrationStart {
                review_ref,
                expected_revision,
                ..
            } => {
                validate_revision(*expected_revision)?;
                review_ref.validate()
            }
            Self::ConnectionsObserveSet { mutation } => mutation.validate(),
            Self::ActionsSubmit { intent } => intent.validate(),
            Self::ActionsDecide {
                review_ref,
                expected_revision,
                ..
            } => {
                validate_revision(*expected_revision)?;
                review_ref.validate()
            }
            Self::ActionsReconcile {
                expected_revision, ..
            }
            | Self::ActionsSetAuthority {
                expected_revision, ..
            } => validate_revision(*expected_revision),
            Self::DayRefresh { .. } => Ok(()),
            Self::DayMutate { mutation, .. } => mutation.validate(),
            Self::KnowledgeMemoryDecide { candidate_id, .. } => {
                if candidate_id.is_nil() {
                    Err("command.candidate_id")
                } else {
                    Ok(())
                }
            }
            Self::ExpertsSetInstallationEnabled {
                expected_revision, ..
            } => validate_revision(*expected_revision),
            Self::ExpertsPrepareBindingReview {
                requirement_ref,
                expected_binding_revision,
                ..
            } => {
                validate_revision(*expected_binding_revision)?;
                if !valid_text(requirement_ref, 128) {
                    return Err("command.requirement_ref");
                }
                Ok(())
            }
            Self::ExpertsBindingReplace {
                review_ref,
                expected_binding_revision,
                candidate_refs,
            } => {
                review_ref.validate()?;
                validate_revision(*expected_binding_revision)?;
                if candidate_refs.len() > 16
                    || candidate_refs
                        .iter()
                        .enumerate()
                        .any(|(i, id)| candidate_refs[i + 1..].contains(id))
                {
                    return Err("command.candidate_refs");
                }
                Ok(())
            }
            Self::ConversationSessionStart {} => Ok(()),
            Self::ConversationSessionRecover {
                expected_revision, ..
            } => validate_revision(*expected_revision),
            Self::VaultCreate {} | Self::VaultUnlock {} | Self::VaultLock {} => Ok(()),
            Self::ConversationStartTurn {
                expected_revision,
                text,
                continuation_ref,
                retry_of,
                ..
            } => {
                // The first turn compares against the valid persisted revision 0.
                // Other owners and post-admission commands retain positive revisions.
                if *expected_revision > i64::MAX as u64 {
                    return Err("expected_revision");
                }
                if retry_of.is_some() && continuation_ref.is_some() {
                    return Err("command.retry_of");
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
                Ok(())
            }
            Self::ConversationCancelRun { .. } => Ok(()),
            Self::ConversationInteractionResolve {
                expected_revision,
                reviewed_digest: _,
                ..
            } => validate_revision(*expected_revision),
            Self::ConversationInteractionRefresh {
                expected_revision, ..
            } => validate_revision(*expected_revision),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationRefDto {
    pub id: super::UuidRefDto,
}

impl ContinuationRefDto {
    fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppCommandReceiptDto {
    pub command_id: CommandIdDto,
    pub runtime_epoch: u64,
    pub admission: AppCommandStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunRefDto>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppCommandResultDto {
    #[serde(rename = "registered")]
    NativeHostRegistered {
        registration: super::NativeHostRegistrationDto,
    },
    #[serde(rename = "acknowledged")]
    NativeHostAcknowledged {},
    #[serde(rename = "connections.gateway")]
    ConnectionsGateway { gateway: super::GatewaySummaryDto },
    #[serde(rename = "connections.pairing")]
    ConnectionsPairing { pairing: super::PairingSnapshotDto },
    #[serde(rename = "connections.overview")]
    ConnectionsOverview {
        overview: super::ConnectionsOverviewDto,
    },
    #[serde(rename = "connections.integration_review")]
    ConnectionsIntegrationReview { review: super::IntegrationReviewDto },
    #[serde(rename = "connections.operation")]
    ConnectionsOperation {
        operation: super::ConnectionOperationSnapshotDto,
    },
    #[serde(rename = "connections.source")]
    ConnectionsSource { source: super::SourceSummaryDto },
    #[serde(rename = "connections.source_review")]
    ConnectionsSourceReview { review: super::SourceReviewDto },
    #[serde(rename = "connections.observe_review")]
    ConnectionsObserveReview { review: super::ObserveReviewDto },
    #[serde(rename = "connections.launch")]
    ConnectionsLaunch {
        launch_action: super::LaunchActionDto,
    },
    #[serde(rename = "day.refresh")]
    DayRefresh { refresh: super::DayRefreshStateDto },
    #[serde(rename = "actions.action")]
    Action { action: super::ActionSnapshotDto },
    #[serde(rename = "actions.authority")]
    ActionsAuthority {
        authority: super::ActionsAuthorityDto,
    },
    DayMutation {
        command_id: Uuid,
        mutation: super::MutationResultDto,
    },
    #[serde(rename = "knowledge.memory.decision")]
    KnowledgeDecision {
        acknowledgement: super::MemoryDecisionAcknowledgementDto,
    },
    #[serde(rename = "experts.directory")]
    ExpertsDirectory {
        directory: super::ExpertDirectorySnapshotDto,
    },
    #[serde(rename = "experts.binding_review")]
    ExpertsBindingReview { review: super::BindingReviewDto },
    ConversationSession {
        session: super::ConversationSessionSnapshotDto,
    },
    VaultOperation {
        #[serde(flatten)]
        result: super::VaultLifecycleResultDto,
    },
    CommandReceipt {
        #[serde(flatten)]
        receipt: AppCommandReceiptDto,
    },
    CancelRunReceipt {
        command_id: CommandIdDto,
        run_id: RunRefDto,
        runtime_epoch: u64,
        outcome: AppCancelRunOutcomeDto,
    },
    InteractionOperation {
        #[serde(flatten)]
        result: super::AppInteractionResolveResultDto,
    },
    InteractionRefresh {
        #[serde(flatten)]
        result: super::AppInteractionRefreshResultDto,
    },
}

fn validate_revision(value: u64) -> Result<(), &'static str> {
    if value == 0 || value > i64::MAX as u64 {
        Err("expected_revision")
    } else {
        Ok(())
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_gateway_setup_address(value: &str) -> bool {
    let Some((scheme, authority)) = value.split_once("://") else {
        return false;
    };
    if scheme != "http"
        || authority
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@'))
    {
        return false;
    }
    let Some((host, port)) = authority.rsplit_once(':') else {
        return false;
    };
    if authority.matches(':').count() != 1
        || !(host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1")
    {
        return false;
    }
    valid_nonzero_port(port)
}

fn valid_nonzero_port(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u16>().is_ok_and(|port| port != 0)
}
