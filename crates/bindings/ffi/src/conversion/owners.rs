use floe_app::{VaultLifecycleFailureProjection, VaultLifecycleRecovery, VaultState};
use floe_protocol::*;

pub(crate) fn failure_envelope(
    projection: VaultLifecycleFailureProjection,
    stage: &str,
    request_id: &str,
) -> AgentVaultFailureDto {
    // The serialized failure name is the exact shared wire identity. All
    // classification and recovery decisions come from the lifecycle owner.
    let kind = serde_json::to_value(projection.failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    AgentVaultFailureDto {
        schema_version: PROTOCOL_VERSION,
        domain: projection.domain,
        category: projection.category,
        reason_code: kind.clone(),
        kind,
        stage: stage.into(),
        safe_actions: projection.safe_actions,
        affected_refs: vec![],
        incident_id: request_id.into(),
        retry_policy: projection.retry_policy,
        retryable: projection.retryable,
        recovery_action: match projection.recovery {
            VaultLifecycleRecovery::None => AgentVaultRecoveryActionDto::None,
            VaultLifecycleRecovery::ReopenVault => AgentVaultRecoveryActionDto::ReopenVault,
        },
        reload_required: projection.reload_required,
        seal_session: projection.seal_session,
        correlation_request_id: request_id.into(),
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
