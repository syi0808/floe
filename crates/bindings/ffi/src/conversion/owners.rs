use floe_app::{AgentFailure,VaultState};
use floe_protocol::*;

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
        AgentFailure::VaultUnavailable | AgentFailure::VaultLocked | AgentFailure::StorageUnavailable => (
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
        AgentFailure::VaultUnavailable | AgentFailure::VaultLocked | AgentFailure::StorageUnavailable => {
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
            AgentFailure::VaultUnavailable | AgentFailure::VaultLocked
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
        AgentFailure::VaultUnavailable | AgentFailure::VaultLocked | AgentFailure::StorageUnavailable => {
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

