use floe_app::{RuntimeFailureProjection, RuntimeRecovery};
use floe_protocol::*;

pub(crate) fn runtime_owner_failure(projection: RuntimeFailureProjection) -> OwnerFailureDto {
    OwnerFailureDto {
        domain: projection.domain,
        category: projection.category,
        reason: projection.reason,
        incident_id: UuidRefDto::new(projection.incident_id)
            .expect("owner incident identity is non-nil"),
        correlation_id: UuidRefDto::new(projection.correlation_id)
            .expect("owner correlation identity is non-nil"),
        reload_required: projection.reload_required,
        seal_session: projection.seal_session,
        recovery: match projection.recovery {
            RuntimeRecovery::None => OwnerRecoveryDto::None,
            RuntimeRecovery::Reobserve => OwnerRecoveryDto::Reobserve,
        },
        safe_actions: projection.safe_actions,
    }
}
