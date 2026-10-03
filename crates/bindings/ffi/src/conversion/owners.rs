use floe_app::{
    AgentFailure, CalendarActionOperation, CalendarActionProposal, CalendarActionState,
    CalendarProposalInspection, MemoryReviewResult, VaultState,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::*;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

fn parse_uuid(value: &str, field: &'static str) -> WireResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| invalid(field, "must be a UUID"))
}

fn calendar_action(action: floe_app::CalendarAction) -> AgentProposalActionDto {
    AgentProposalActionDto {
        action_id: action.id.to_string(),
        execution_id: action.execution_id.to_string(),
        expires_at: action.expires_at,
        starts_at: action.schedule.starts_at,
        ends_at: action.schedule.ends_at,
        status: match action.state {
            CalendarActionState::Pending => AgentProposalStatusDto::Pending,
            CalendarActionState::Approved => AgentProposalStatusDto::Approved,
            CalendarActionState::Rejected => AgentProposalStatusDto::Rejected,
            CalendarActionState::Executing => AgentProposalStatusDto::Executing,
            CalendarActionState::Blocked { .. } => AgentProposalStatusDto::Blocked,
            CalendarActionState::Unknown { .. } => AgentProposalStatusDto::Unknown,
            CalendarActionState::Succeeded { .. } => AgentProposalStatusDto::Succeeded,
        },
    }
}

fn encode_contract<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, AgentFailure> {
    let value = serde_json::to_value(value).map_err(|_| AgentFailure::InvalidModelOutput)?;
    serde_json::from_value(value).map_err(|_| AgentFailure::InvalidModelOutput)
}

pub(crate) fn failure_envelope(
    failure: &AgentFailure,
    stage: &str,
    request_id: &str,
) -> AgentVaultFailureDto {
    let kind = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let classification = classify_failure(failure, stage);
    let recovery = recovery_action(failure, stage);
    AgentVaultFailureDto {
        schema_version: PROTOCOL_VERSION,
        domain: classification.domain,
        category: classification.category,
        reason_code: classification.reason_code,
        kind: kind.clone(),
        stage: stage.into(),
        safe_actions: classification.safe_actions,
        affected_refs: vec![],
        incident_id: request_id.into(),
        retry_policy: classification.retry_policy,
        retryable: classification.retryable,
        recovery_action: recovery,
        reload_required: reload_required(recovery),
        seal_session: seal_session(failure, recovery),
        correlation_request_id: request_id.into(),
    }
}

struct FailureClassification {
    domain: AgentFailureDomain,
    category: AgentFailureCategory,
    reason_code: String,
    safe_actions: Vec<AgentFailureSafeAction>,
    retry_policy: AgentRetryPolicy,
    retryable: bool,
}

fn classify_failure(failure: &AgentFailure, stage: &str) -> FailureClassification {
    let reason_code = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let source_stage = matches!(
        stage,
        "calendar_action"
            | "remote_authority_inspect_producer"
            | "remote_authority_review_and_enroll"
            | "remote_authority_enrollment_status"
            | "remote_pairing_prepare"
            | "remote_pairing_confirm"
            | "remote_pairing_status"
            | "remote_pairing_finalize"
            | "remote_connection_observe_inspect"
            | "remote_connection_observe_enable"
            | "remote_connection_observe_disable"
            | "remote_connection_observe_review"
    );
    let (domain, category, reason_code) = match failure {
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => (
            AgentFailureDomain::Vault,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        AgentFailure::PolicyDenied if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            "session_integrity".into(),
        ),
        AgentFailure::PolicyDenied if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Security,
            "data_release_or_policy_block".into(),
        ),
        AgentFailure::PolicyDenied if source_stage => (
            AgentFailureDomain::Source,
            AgentFailureCategory::Security,
            "source_access_denied".into(),
        ),
        AgentFailure::PolicyDenied => (
            AgentFailureDomain::App,
            AgentFailureCategory::Internal,
            "internal_policy_invariant".into(),
        ),
        AgentFailure::CapabilityDenied => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::Security,
            "capability_access_denied".into(),
        ),
        AgentFailure::AccessReviewRequired => (
            AgentFailureDomain::Source,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::ConsentRequired => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::CapabilityUnavailable => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if source_stage => (
            AgentFailureDomain::Source,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::ServerModelRequestRejected
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => (
            AgentFailureDomain::Capability,
            if matches!(
                failure,
                AgentFailure::InvalidModelOutput
                    | AgentFailure::LocalModelInvalidOutput
                    | AgentFailure::ServerModelInvalidOutput
            ) {
                AgentFailureCategory::Integrity
            } else {
                AgentFailureCategory::Transient
            },
            reason_code.clone(),
        ),
        AgentFailure::CredentialExpired | AgentFailure::QuotaExceeded => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::Interrupted | AgentFailure::DeadlineExceeded | AgentFailure::Stalled => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        _ if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        _ if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        _ => (
            AgentFailureDomain::App,
            AgentFailureCategory::Internal,
            reason_code,
        ),
    };

    let mut safe_actions = match failure {
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => {
            vec![AgentFailureSafeAction::ReopenVault]
        }
        _ if stage == "conversation_session" => {
            vec![AgentFailureSafeAction::StartNewSession]
        }
        AgentFailure::PolicyDenied if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ExportDiagnostics,
        ],
        AgentFailure::PolicyDenied if source_stage => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ReviewSource,
        ],
        AgentFailure::AccessReviewRequired => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ReviewSource,
        ],
        AgentFailure::ConsentRequired => vec![],
        AgentFailure::Conflict if stage == "conversation_session" => vec![
            AgentFailureSafeAction::StartNewSession,
            AgentFailureSafeAction::RefreshSession,
        ],
        AgentFailure::Conflict if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::RefreshSession,
            AgentFailureSafeAction::StartNewSession,
        ],
        AgentFailure::StaleContext if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::RefreshSession,
            AgentFailureSafeAction::StartNewSession,
        ],
        AgentFailure::StaleContext => vec![AgentFailureSafeAction::ReviewSource],
        AgentFailure::CredentialExpired => vec![AgentFailureSafeAction::RefreshSession],
        AgentFailure::CapabilityUnavailable if source_stage => {
            vec![AgentFailureSafeAction::ContinueWithoutSource]
        }
        AgentFailure::Cancelled => vec![],
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => vec![AgentFailureSafeAction::Retry],
        _ if stage == "conversation_turn" && !matches!(failure, AgentFailure::Cancelled) => {
            vec![AgentFailureSafeAction::StartNewSession]
        }
        _ => vec![],
    };
    if !matches!(failure, AgentFailure::Cancelled)
        && matches!(
            category,
            AgentFailureCategory::Internal | AgentFailureCategory::Security
        )
        && !safe_actions.contains(&AgentFailureSafeAction::ExportDiagnostics)
    {
        safe_actions.push(AgentFailureSafeAction::ExportDiagnostics);
    }
    let retry_policy = if safe_actions.contains(&AgentFailureSafeAction::Retry) {
        if matches!(
            failure,
            AgentFailure::ServerModelTimeout
                | AgentFailure::Interrupted
                | AgentFailure::DeadlineExceeded
                | AgentFailure::Stalled
        ) {
            AgentRetryPolicy::Backoff
        } else {
            AgentRetryPolicy::Immediate
        }
    } else {
        AgentRetryPolicy::Never
    };
    let retryable = !matches!(retry_policy, AgentRetryPolicy::Never);
    FailureClassification {
        domain,
        category,
        reason_code,
        safe_actions,
        retry_policy,
        retryable,
    }
}

fn reload_required(recovery: AgentVaultRecoveryActionDto) -> bool {
    !matches!(
        recovery,
        AgentVaultRecoveryActionDto::RetryRead
            | AgentVaultRecoveryActionDto::ReviewSource
            | AgentVaultRecoveryActionDto::RefreshContext
    )
}

fn seal_session(failure: &AgentFailure, recovery: AgentVaultRecoveryActionDto) -> bool {
    matches!(recovery, AgentVaultRecoveryActionDto::ReopenVault)
        || matches!(
            failure,
            AgentFailure::VaultUnavailable
                | AgentFailure::StorageUnavailable
                | AgentFailure::Interrupted
        )
}

fn recovery_action(failure: &AgentFailure, stage: &str) -> AgentVaultRecoveryActionDto {
    match failure {
        AgentFailure::Conflict | AgentFailure::DeadlineExceeded | AgentFailure::Interrupted
            if stage == "calendar_action" =>
        {
            AgentVaultRecoveryActionDto::Reconcile
        }
        AgentFailure::Conflict if matches!(stage, "conversation_session" | "conversation_turn") => {
            AgentVaultRecoveryActionDto::RefreshSession
        }
        AgentFailure::Conflict | AgentFailure::StaleContext => {
            AgentVaultRecoveryActionDto::RefreshContext
        }
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput
            if stage == "conversation_session" =>
        {
            AgentVaultRecoveryActionDto::None
        }
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => AgentVaultRecoveryActionDto::RetryRead,
        AgentFailure::AccessReviewRequired
            if matches!(stage, "calendar_action" | "conversation_turn") =>
        {
            AgentVaultRecoveryActionDto::ReviewSource
        }
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => {
            AgentVaultRecoveryActionDto::ReopenVault
        }
        _ => AgentVaultRecoveryActionDto::None,
    }
}

pub(crate) fn vault_state_dto(state: VaultState) -> AgentVaultStateDto {
    match state {
        VaultState::Missing => AgentVaultStateDto::Missing,
        VaultState::Locked => AgentVaultStateDto::Locked,
        VaultState::Ready => AgentVaultStateDto::Ready,
        VaultState::Unavailable => AgentVaultStateDto::Unavailable,
    }
}

pub(crate) fn proposal_dto(proposal: CalendarProposalInspection) -> AgentProposalInspectionDto {
    AgentProposalInspectionDto {
        schema_version: PROTOCOL_VERSION,
        person_id: proposal.person_id.to_string(),
        session_id: proposal.session_id.to_string(),
        invocation_id: proposal.invocation_id.to_string(),
        action: proposal.action.map(calendar_action),
    }
}

pub(crate) fn memory_review_dto(
    review: MemoryReviewResult,
) -> Result<AgentMemoryReviewOverviewDto, AgentFailure> {
    Ok(AgentMemoryReviewOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: review.snapshot.person_id.to_string(),
        candidates: review
            .snapshot
            .candidates
            .iter()
            .map(encode_contract)
            .collect::<Result<Vec<_>, _>>()?,
        decision: review.decision.as_ref().map(encode_contract).transpose()?,
    })
}

pub(crate) fn memory_dto(
    snapshot: floe_app::MemoryOverviewSnapshot,
) -> Result<AgentMemoryOverviewDto, AgentFailure> {
    Ok(AgentMemoryOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: snapshot.person_id.to_string(),
        saved_count: snapshot.saved_count,
        pending_count: snapshot.pending_count,
        memories: snapshot
            .memories
            .into_iter()
            .map(|memory| {
                Ok::<_, AgentFailure>(AgentMemorySummaryDto {
                    target_id: memory.target_id.to_string(),
                    revision: memory.revision,
                    statement: memory.statement,
                    memory_kind: encode_contract(&memory.memory_kind)?,
                    epistemic_status: encode_contract(&memory.epistemic_status)?,
                    confidence_millis: memory.confidence_millis,
                    source_count: memory.source_count,
                    origin: match memory.origin {
                        floe_app::MemoryOrigin::UserProvided => AgentMemoryOriginDto::UserProvided,
                        floe_app::MemoryOrigin::Learned => AgentMemoryOriginDto::Learned,
                    },
                    created_at: memory.created_at,
                    valid_from: memory.valid_from,
                    valid_until: memory.valid_until,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn proposal(
    calendar_id: String,
    title: String,
    starts_at: String,
    ends_at: String,
    timezone: String,
    event_id: Option<String>,
    event_revision: Option<u64>,
    delete: bool,
) -> Box<CalendarActionProposal> {
    Box::new(CalendarActionProposal {
        calendar_id,
        title,
        starts_at,
        ends_at,
        timezone,
        event_id,
        event_revision,
        delete,
    })
}

pub(crate) fn calendar_action_operation(
    operation: CalendarActionOperationDto,
) -> WireResult<CalendarActionOperation> {
    Ok(match operation {
        CalendarActionOperationDto::Capabilities {} => CalendarActionOperation::Capabilities,
        CalendarActionOperationDto::GetAuthority {} => CalendarActionOperation::GetAuthority,
        CalendarActionOperationDto::SetAuthority { calendar_create } => {
            CalendarActionOperation::SetAuthority {
                calendar_create: match calendar_create {
                    ActionAuthorityModeDto::Allow => floe_app::ActionAuthorityMode::Allow,
                    ActionAuthorityModeDto::Ask => floe_app::ActionAuthorityMode::Ask,
                    ActionAuthorityModeDto::Deny => floe_app::ActionAuthorityMode::Deny,
                },
            }
        }
        CalendarActionOperationDto::Execute { action_id } => CalendarActionOperation::Execute {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
        },
        CalendarActionOperationDto::Recover { action_id } => CalendarActionOperation::Recover {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
        },
        CalendarActionOperationDto::List {} => CalendarActionOperation::List,
        CalendarActionOperationDto::Get { action_id } => CalendarActionOperation::Get {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
        },
        CalendarActionOperationDto::Propose {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
        } => CalendarActionOperation::Propose(proposal(
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            None,
            None,
            false,
        )),
        CalendarActionOperationDto::Direct {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            event_id,
            event_revision,
            delete,
        } => CalendarActionOperation::Direct(proposal(
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            event_id,
            event_revision,
            delete,
        )),
        CalendarActionOperationDto::Decide {
            action_id,
            decision,
        } => CalendarActionOperation::Decide {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
            approve: decision == CalendarActionDecisionDto::Approve,
        },
    })
}
