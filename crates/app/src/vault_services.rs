use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{AgentFailure, AppComposition, CallerContext, ServiceError, VaultState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultLifecycleCommand {
    Create,
    Unlock,
    Lock,
}

#[derive(Clone, Debug)]
pub struct VaultLifecycleResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub failure: Option<AgentFailure>,
}

pub trait VaultLifecycleCommands {
    fn vault_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: VaultLifecycleCommand,
    ) -> Result<VaultLifecycleResult, ServiceError>;
}

pub trait VaultLifecycleQueries {
    fn vault_status(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<VaultLifecycleResult, ServiceError>;

    fn read_vault_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<VaultLifecycleResult, ServiceError>;
}

impl VaultLifecycleCommands for AppComposition {
    fn vault_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: VaultLifecycleCommand,
    ) -> Result<VaultLifecycleResult, ServiceError> {
        self.agent_vault
            .local_request(
                caller,
                operation_id,
                Some(LocalOperationIntent::VaultCommand(command)),
                LocalOperationOwner::Vault,
                false,
            )
            .map(vault_result)
            .map_err(crate::composition::service_failure)
    }
}

impl VaultLifecycleQueries for AppComposition {
    fn vault_status(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<VaultLifecycleResult, ServiceError> {
        self.agent_vault
            .local_request(
                caller,
                operation_id,
                Some(LocalOperationIntent::VaultStatus),
                LocalOperationOwner::Vault,
                false,
            )
            .map(vault_result)
            .map_err(crate::composition::service_failure)
    }

    fn read_vault_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<VaultLifecycleResult, ServiceError> {
        self.agent_vault
            .local_request(
                caller,
                operation_id,
                None,
                LocalOperationOwner::Vault,
                release,
            )
            .map(vault_result)
            .map_err(crate::composition::service_failure)
    }
}

fn vault_result(result: crate::WorkerResult) -> VaultLifecycleResult {
    VaultLifecycleResult {
        operation_id: result.request_id,
        stage: result.stage,
        done: result.done,
        state: result.state,
        failure: result.failure,
    }
}
