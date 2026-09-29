use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CallerContext, FeasibilityAccessChange,
    FeasibilityAccessOverview, ServiceError, VaultState, WorkerAction,
};

#[derive(Clone, Debug, PartialEq)]
pub enum LocalAccessCommand {
    Feasibility { change: FeasibilityAccessChange },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalAccessInspection {
    Feasibility,
}

#[derive(Clone)]
pub struct LocalAccessResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub feasibility_access: Option<FeasibilityAccessOverview>,
    pub failure: Option<AgentFailure>,
}

pub trait LocalAccessCommands {
    fn local_access_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: LocalAccessCommand,
    ) -> Result<LocalAccessResult, ServiceError>;
}

pub trait LocalAccessQueries {
    fn inspect_local_access(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: LocalAccessInspection,
    ) -> Result<LocalAccessResult, ServiceError>;
    fn read_local_access_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<LocalAccessResult, ServiceError>;
}

impl LocalAccessCommands for AppComposition {
    fn local_access_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: LocalAccessCommand,
    ) -> Result<LocalAccessResult, ServiceError> {
        self.local_access_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::LocalAccessCommand(command)),
            false,
        )
    }
}

impl LocalAccessQueries for AppComposition {
    fn inspect_local_access(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: LocalAccessInspection,
    ) -> Result<LocalAccessResult, ServiceError> {
        self.local_access_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::LocalAccessInspection(inspection)),
            false,
        )
    }
    fn read_local_access_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<LocalAccessResult, ServiceError> {
        self.local_access_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn local_access_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<LocalAccessResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Access,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(LocalAccessResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            feasibility_access: result.feasibility_access,
            failure: result.failure,
        })
    }
}

impl LocalAccessCommand {
    pub(crate) fn action(&self, caller: &CallerContext) -> WorkerAction {
        match self {
            Self::Feasibility { change } => WorkerAction::FeasibilityAccess {
                change: Box::new(crate::FeasibilityAccessConfiguration {
                    device_id: caller.device_id().into(),
                    consumers: Vec::new(),
                    change: change.clone(),
                }),
            },
        }
    }
}

impl LocalAccessInspection {
    pub(crate) fn action(&self, caller: &CallerContext) -> WorkerAction {
        match self {
            Self::Feasibility => LocalAccessCommand::Feasibility {
                change: FeasibilityAccessChange::Inspect,
            }
            .action(caller),
        }
    }
}
