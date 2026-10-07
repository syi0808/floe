//! App-owned runtime readiness and the narrow preparation operation contract.

use crate::{AgentFailure, AppComposition, CallerContext};
use floe_kernel::{
    AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction, PersonId,
};
use uuid::Uuid;

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

/// Safe owner projection for the current runtime observation.
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
    /// Only intrinsic encrypted-generation loss is shared across feature calls.
    /// Storage, provider, and ordinary owner failures keep their own projection.
    pub fn access_failure(
        reason: AgentFailure,
        correlation_id: Uuid,
        preparation_available: bool,
    ) -> Option<Self> {
        matches!(reason, AgentFailure::VaultLocked | AgentFailure::VaultUnavailable)
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
            F::IncompleteCreation
            | F::Conflict
            | F::StaleContext
            | F::UnsupportedVersion => AgentFailureCategory::Integrity,
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
            domain: AgentFailureDomain::App,
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

/// The sole transport-neutral Runtime control surface exposed by App.
pub trait RuntimeControl {
    fn prepare_runtime(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure>;

    fn acknowledge_runtime_preparation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure>;

    fn runtime_readiness(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
    ) -> Result<RuntimeReadiness, AgentFailure>;

    fn get_runtime_preparation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure>;
}

impl RuntimeControl for AppComposition {
    fn prepare_runtime(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure> {
        self.agent_vault
            .prepare(caller, operation_id)
    }

    fn acknowledge_runtime_preparation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure> {
        self.agent_vault.acknowledge(caller, operation_id)
    }

    fn runtime_readiness(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
    ) -> Result<RuntimeReadiness, AgentFailure> {
        if request_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.agent_vault.readiness(caller, request_id)
    }

    fn get_runtime_preparation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure> {
        self.agent_vault.get_preparation(caller, operation_id)
    }
}

pub(crate) fn validate_runtime_caller(
    expected: &CallerContext,
    caller: &CallerContext,
) -> Result<(), AgentFailure> {
    caller.owner_actor().validate()?;
    if expected != caller
        || expected.person_id() != PersonId(caller.person_id()).0
        || expected.device_id() != caller.device_id()
        || expected.runtime_epoch() != caller.runtime_epoch()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
