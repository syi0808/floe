use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{AgentFailure, AppComposition, CallerContext, ServiceError, VaultState};

pub use floe_experts::{RegistryConfiguration, RegistryConfigurationTarget, RegistryOverview};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertCommand {
    ConfigureRegistry(RegistryConfiguration),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpertInspection {
    Registry,
}

#[derive(Clone, Debug)]
pub struct ExpertOperationResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub registry: Option<RegistryOverview>,
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
            failure: result.failure,
        })
    }
}
