//! App-owned Runtime readiness and preparation contract.

use crate::{AgentFailure, AppComposition, CallerContext};
use floe_kernel::{AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction};
use uuid::{Uuid, Variant, Version};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeReadinessState {
    Ready,
    PreparationRequired,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeRecovery {
    None,
    Reobserve,
}

/// Owner projection for one current Runtime observation. Only intrinsic Vault
/// failures use the Vault domain and cross feature boundaries as readiness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeFailureProjection {
    pub reason: AgentFailure,
    pub domain: AgentFailureDomain,
    pub category: AgentFailureCategory,
    pub incident_id: Uuid,
    pub correlation_id: Uuid,
    pub reload_required: bool,
    pub seal_session: bool,
    pub recovery: RuntimeRecovery,
    pub safe_actions: Vec<AgentFailureSafeAction>,
}

impl RuntimeFailureProjection {
    /// Only intrinsic encrypted-generation loss is shared across features.
    pub fn access_failure(
        reason: AgentFailure,
        correlation_id: Uuid,
        preparation_available: bool,
    ) -> Option<Self> {
        matches!(
            reason,
            AgentFailure::VaultLocked | AgentFailure::VaultUnavailable
        )
        .then(|| Self::project(reason, correlation_id, preparation_available))
    }

    pub(crate) fn project(
        reason: AgentFailure,
        correlation_id: Uuid,
        preparation_available: bool,
    ) -> Self {
        use AgentFailure as F;
        let category = match reason {
            F::VaultLocked | F::NotFound | F::ConsentRequired => {
                AgentFailureCategory::UserConfiguration
            }
            F::IncompleteCreation | F::Conflict | F::StaleContext | F::UnsupportedVersion => {
                AgentFailureCategory::Integrity
            }
            F::PolicyDenied | F::CapabilityDenied => AgentFailureCategory::Security,
            F::VaultUnavailable
            | F::StorageUnavailable
            | F::StorageBusy
            | F::Interrupted
            | F::DeadlineExceeded
            | F::Cancelled
            | F::BudgetExceeded => AgentFailureCategory::Transient,
            _ => AgentFailureCategory::Internal,
        };
        let invalidates_generation = matches!(
            reason,
            F::VaultUnavailable
                | F::VaultLocked
                | F::StorageUnavailable
                | F::Interrupted
                | F::DeadlineExceeded
        );
        let incident_id = Uuid::new_v5(
            &correlation_id,
            format!("floe.runtime.failure:{reason:?}").as_bytes(),
        );
        Self {
            reason,
            domain: if matches!(reason, F::VaultUnavailable | F::VaultLocked) {
                AgentFailureDomain::Vault
            } else {
                AgentFailureDomain::App
            },
            category,
            incident_id,
            correlation_id,
            reload_required: invalidates_generation,
            seal_session: invalidates_generation,
            recovery: if preparation_available {
                RuntimeRecovery::Reobserve
            } else {
                RuntimeRecovery::None
            },
            // This affordance is explicit. The client reuses an uncertain
            // operation identity; a new identity is allocated only after the
            // archived result was acknowledged and the owner permits retry.
            safe_actions: if preparation_available {
                vec![AgentFailureSafeAction::Retry]
            } else {
                vec![]
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeReadiness {
    pub state: RuntimeReadinessState,
    pub failure: Option<RuntimeFailureProjection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimePreparationResult {
    pub operation_id: Uuid,
    pub done: bool,
    pub failure: Option<AgentFailure>,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum RuntimePreparationCommandFailure {
    #[error("runtime preparation was not admitted: {0:?}")]
    NotAdmitted(AgentFailure),
    #[error("runtime preparation admission is uncertain: {0:?}")]
    Indeterminate(AgentFailure),
}

impl RuntimePreparationCommandFailure {
    pub fn into_failure(self) -> AgentFailure {
        match self {
            Self::NotAdmitted(failure) | Self::Indeterminate(failure) => failure,
        }
    }
}

/// UUIDs from a client command lane are random v4 identities, never derived IDs.
pub fn validate_runtime_id(id: Uuid) -> Result<(), AgentFailure> {
    if id.is_nil()
        || id.get_version() != Some(Version::Random)
        || id.get_variant() != Variant::RFC4122
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

pub(crate) fn validate_runtime_caller(
    expected: &CallerContext,
    caller: &CallerContext,
) -> Result<(), AgentFailure> {
    caller.owner_actor().validate()?;
    if expected != caller {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

impl AppComposition {
    pub fn prepare_runtime(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure> {
        self.runtime_preparation.prepare(caller, operation_id)
    }

    pub fn acknowledge_runtime_preparation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure> {
        self.runtime_preparation.acknowledge(caller, operation_id)
    }

    pub fn runtime_readiness(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
    ) -> Result<RuntimeReadiness, AgentFailure> {
        validate_runtime_id(request_id)?;
        self.runtime_preparation.readiness(caller, request_id)
    }

    pub fn get_runtime_preparation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure> {
        self.runtime_preparation
            .get_preparation(caller, operation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalIdentityClaim;

    fn caller(person_id: Uuid, device_id: &str, runtime_epoch: u64) -> CallerContext {
        CallerContext::verified(
            LocalIdentityClaim {
                person_id,
                device_id: device_id.into(),
            },
            runtime_epoch,
        )
        .expect("construct verified test caller")
    }

    #[test]
    fn runtime_command_identity_requires_random_uuid_v4() {
        assert!(validate_runtime_id(Uuid::new_v4()).is_ok());
        assert_eq!(
            validate_runtime_id(Uuid::nil()),
            Err(AgentFailure::InvalidInput)
        );
        let uuid_v7 = Uuid::parse_str("01890f47-2e80-7cc7-b0b5-f12c0a06e63f")
            .expect("valid UUID v7 test identity");
        assert_eq!(
            validate_runtime_id(uuid_v7),
            Err(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn runtime_caller_scope_fails_closed_for_person_device_and_epoch() {
        let person_id = Uuid::new_v4();
        let expected = caller(person_id, "runtime-test-device", 7);
        assert!(validate_runtime_caller(&expected, &expected).is_ok());

        let other_person = caller(Uuid::new_v4(), "runtime-test-device", 7);
        let other_device = caller(person_id, "runtime-test-device-2", 7);
        let other_epoch = caller(person_id, "runtime-test-device", 8);
        for foreign in [&other_person, &other_device, &other_epoch] {
            assert_eq!(
                validate_runtime_caller(&expected, foreign),
                Err(AgentFailure::PolicyDenied)
            );
        }
    }

    #[test]
    fn only_intrinsic_vault_failures_cross_feature_readiness() {
        let correlation = Uuid::new_v4();
        let locked =
            RuntimeFailureProjection::access_failure(AgentFailure::VaultLocked, correlation, true)
                .expect("intrinsic Vault lock is shared readiness");
        assert_eq!(locked.domain, AgentFailureDomain::Vault);
        assert_eq!(locked.recovery, RuntimeRecovery::Reobserve);
        assert_eq!(locked.safe_actions, vec![AgentFailureSafeAction::Retry]);

        assert!(
            RuntimeFailureProjection::access_failure(
                AgentFailure::ModelUnavailable,
                correlation,
                true,
            )
            .is_none()
        );
        assert!(
            RuntimeFailureProjection::access_failure(
                AgentFailure::StorageUnavailable,
                correlation,
                true,
            )
            .is_none()
        );
    }
}
