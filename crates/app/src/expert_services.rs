use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{AgentFailure, AppComposition, CallerContext, ServiceError, VaultState};

pub use floe_experts::{RegistryConfiguration, RegistryConfigurationTarget, RegistryOverview};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertCommand {
    ConfigureRegistry(RegistryConfiguration),
    ReplaceBinding(ExpertBindingSelectionIntent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertInspection {
    Registry,
    Candidates {
        assignment_id: Uuid,
        requirement_key: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpertBindingSelectionIntent {
    pub assignment_id: Uuid,
    pub package_id: String,
    pub package_version: String,
    pub definition_revision: u64,
    pub requirement_key: String,
    pub expected_binding_revision: u64,
    pub candidate_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertSourceCandidateView {
    pub candidate_id: String,
    pub title: String,
    pub detail: String,
    pub availability: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertCandidateCatalog {
    pub assignment_id: Uuid,
    pub requirement_key: String,
    pub binding_revision: u64,
    pub candidates: Vec<ExpertSourceCandidateView>,
}

#[derive(Clone, Debug)]
pub struct ExpertOperationResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub registry: Option<RegistryOverview>,
    pub candidates: Option<ExpertCandidateCatalog>,
    pub failure: Option<AgentFailure>,
}

pub trait ExpertCommands {
    fn expert_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: ExpertCommand,
    ) -> Result<ExpertOperationResult, ServiceError>;
}

pub trait ExpertQueries {
    fn inspect_experts(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: ExpertInspection,
    ) -> Result<ExpertOperationResult, ServiceError>;
    fn read_expert_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ExpertOperationResult, ServiceError>;
}

impl ExpertCommands for AppComposition {
    fn expert_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: ExpertCommand,
    ) -> Result<ExpertOperationResult, ServiceError> {
        self.expert_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::ExpertCommand(command)),
            false,
        )
    }
}

impl ExpertQueries for AppComposition {
    fn inspect_experts(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: ExpertInspection,
    ) -> Result<ExpertOperationResult, ServiceError> {
        self.expert_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::ExpertInspection(inspection)),
            false,
        )
    }
    fn read_expert_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ExpertOperationResult, ServiceError> {
        self.expert_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn expert_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<ExpertOperationResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Experts,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(ExpertOperationResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            registry: result.registry,
            candidates: result.expert_candidates,
            failure: result.failure,
        })
    }
}
