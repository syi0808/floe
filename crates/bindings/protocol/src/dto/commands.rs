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
    Runtime(RuntimeCommandDto),
    Product(AppProductCommandDto),
}

impl AppCommandDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::NativeHost(command) => command.validate(),
            Self::Runtime(command) => command.validate(),
            Self::Product(command) => command.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum RuntimeCommandDto {
    #[serde(rename = "runtime.prepare")]
    Prepare {},
    #[serde(rename = "runtime.preparation.acknowledge")]
    PreparationAcknowledge {},
}

impl RuntimeCommandDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Prepare {} | Self::PreparationAcknowledge {} => Ok(()),
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
    #[serde(rename = "day.refresh")]
    DayRefresh { day: super::DayQueryDto },
    #[serde(rename = "day.mutate")]
    DayMutate {
        day: super::DayQueryDto,
        mutation: super::DayMutationDto,
    },
    #[serde(rename = "day.external_calendar_operation")]
    DayExternalCalendarOperation {
        operation: super::ManualCalendarOperationDto,
    },
    #[serde(rename = "day.external_calendar_operation.reconcile")]
    DayExternalCalendarOperationReconcile {
        operation_ref: super::OperationRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "memory.decide")]
    MemoryDecide {
        candidate_id: Uuid,
        decision: super::AgentMemoryReviewDecisionKindDto,
    },
    #[serde(rename = "conversation.experts.installation.set_enabled")]
    ExpertsSetInstallationEnabled {
        installation_ref: super::UuidRefDto,
        expected_revision: u64,
        enabled: bool,
    },
    #[serde(rename = "conversation.experts.binding.prepare_review")]
    ExpertsPrepareBindingReview {
        assignment_ref: super::AssignmentRefDto,
        requirement_ref: String,
        expected_binding_revision: u64,
    },
    #[serde(rename = "conversation.experts.binding.replace")]
    ExpertsBindingReplace {
        review_ref: super::BindingReviewRefDto,
        expected_binding_revision: u64,
        candidate_refs: Vec<super::UuidRefDto>,
    },
    #[serde(rename = "conversation.session.start")]
    ConversationSessionStart {},
    #[serde(rename = "conversation.calendar_policy.set")]
    ConversationSetCalendarPolicy {
        mode: super::OperationPolicyModeDto,
        expected_revision: u64,
    },
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
    #[serde(rename = "conversation.calendar_proposal.submit")]
    ConversationCalendarProposalSubmit {
        session_id: SessionRefDto,
        origin_run_id: RunRefDto,
        receipt: super::TaskExecutionReceiptRefDto,
        artifact_id: super::UuidRefDto,
        destination_ref: super::UuidRefDto,
    },
    #[serde(rename = "conversation.cancel_run")]
    ConversationCancelRun { run_id: RunRefDto },
    #[serde(rename = "conversation.interaction.resolve")]
    ConversationInteractionResolve {
        interaction_id: InteractionRefDto,
        session_id: SessionRefDto,
        expected_revision: u64,
        decision: super::AppInteractionDecisionDto,
        target_digest: super::DigestHex64Dto,
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
            Self::DayRefresh { .. } => Ok(()),
            Self::DayMutate { mutation, .. } => mutation.validate(),
            Self::DayExternalCalendarOperation { operation } => operation.validate(),
            Self::DayExternalCalendarOperationReconcile {
                expected_revision, ..
            } => validate_revision(*expected_revision),
            Self::MemoryDecide { candidate_id, .. } => {
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
            Self::ConversationSetCalendarPolicy {
                expected_revision, ..
            } => validate_revision(*expected_revision),
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
            Self::ConversationCalendarProposalSubmit { receipt, .. } => receipt.validate(),
            Self::ConversationInteractionResolve {
                expected_revision,
                target_digest: _,
                ..
            } => validate_revision(*expected_revision),
            Self::ConversationInteractionRefresh {
                expected_revision, ..
            } => validate_revision(*expected_revision),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppCommandDto, AppCommandRequestDto, AppProductCommandDto};

    #[test]
    fn flutter_interaction_resolve_wire_shape_decodes() {
        let request: AppCommandRequestDto = serde_json::from_str(include_str!(
            "../../tests/fixtures/app_wire_v2/interaction_resolve.json"
        ))
        .expect("Flutter interaction resolve request must decode");
        request
            .validate()
            .expect("Flutter interaction resolve request must validate");

        assert!(matches!(
            request.command,
            AppCommandDto::Product(AppProductCommandDto::ConversationInteractionResolve {
                target_digest,
                ..
            }) if target_digest.as_str() == "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
        ));
    }
}

#[cfg(test)]
mod runtime_command_tests {
    use super::{AppCommandDto, AppCommandRequestDto, RuntimeCommandDto};
    use crate::{APP_WIRE_VERSION, CommandIdDto, RequestIdDto};
    use uuid::Uuid;

    #[test]
    fn runtime_command_uses_existing_wire_kind_in_its_own_lane() {
        let request = AppCommandRequestDto {
            schema_version: APP_WIRE_VERSION,
            request_id: RequestIdDto::new(Uuid::new_v4()).expect("request UUID"),
            command_id: CommandIdDto::new(Uuid::new_v4()).expect("command UUID"),
            command: AppCommandDto::Runtime(RuntimeCommandDto::Prepare {}),
        };
        assert_eq!(request.validate(), Ok(()));

        let value = serde_json::to_value(&request).expect("serialize Runtime envelope");
        assert_eq!(value["command"]["kind"], "runtime.prepare");
        let decoded: AppCommandRequestDto =
            serde_json::from_value(value).expect("decode Runtime command lane");
        assert!(matches!(
            decoded.command,
            AppCommandDto::Runtime(RuntimeCommandDto::Prepare {})
        ));
    }
}

#[cfg(test)]
mod memory_command_namespace_tests {
    use super::AppProductCommandDto;
    use crate::AgentMemoryReviewDecisionKindDto;
    use uuid::Uuid;

    #[test]
    fn memory_decision_uses_only_the_product_namespace() {
        let command = AppProductCommandDto::MemoryDecide {
            candidate_id: Uuid::new_v4(),
            decision: AgentMemoryReviewDecisionKindDto::Approve,
        };
        let value = serde_json::to_value(&command).expect("serialize Memory command");
        assert_eq!(value["kind"], "memory.decide");
        assert!(
            serde_json::from_value::<AppProductCommandDto>(serde_json::json!({
                "kind": "knowledge.memory.decide",
                "candidate_id": Uuid::new_v4(),
                "decision": "approve"
            }))
            .is_err()
        );
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
    #[serde(rename = "connections.source_configuration")]
    ConnectionsSourceConfiguration {
        configuration: super::SourceConfigurationResultDto,
    },
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
    #[serde(rename = "conversation.calendar_policy")]
    ConversationCalendarPolicy {
        policy: super::CalendarOperationPolicyDto,
    },
    DayMutation {
        command_id: Uuid,
        mutation: super::MutationResultDto,
    },
    #[serde(rename = "day.external_calendar_operation")]
    DayExternalCalendarOperation {
        operation: super::ManualCalendarOperationReceiptDto,
    },
    #[serde(rename = "memory.decision")]
    MemoryDecision {
        acknowledgement: super::MemoryDecisionAcknowledgementDto,
    },
    #[serde(rename = "conversation.experts.directory")]
    ExpertsDirectory {
        directory: super::ExpertDirectorySnapshotDto,
    },
    #[serde(rename = "conversation.experts.binding_review")]
    ExpertsBindingReview { review: super::BindingReviewDto },
    ConversationSession {
        session: super::ConversationSessionSnapshotDto,
    },
    #[serde(rename = "conversation.calendar_proposal")]
    ConversationCalendarProposal {
        operation: super::ActionSnapshotDto,
        interaction: Option<super::AppInteractionSnapshotDto>,
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
