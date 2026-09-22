use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CalendarActionOperation, CalendarActionsResult,
    CalendarProposalInspection, CallerContext, ServiceError, VaultState,
};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionInspection {
    Capabilities,
    Authority,
    List,
    Get {
        action_id: Uuid,
    },
    Proposal {
        session_id: Uuid,
        invocation_id: Uuid,
    },
}

#[derive(Clone, Debug)]
pub struct ActionOperationResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub calendar_actions: Option<CalendarActionsResult>,
    pub proposal: Option<CalendarProposalInspection>,
    pub failure: Option<AgentFailure>,
}

pub trait ActionCommands {
    fn action_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: CalendarActionOperation,
    ) -> Result<ActionOperationResult, ServiceError>;
}

pub trait ActionQueries {
    fn inspect_actions(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: ActionInspection,
    ) -> Result<ActionOperationResult, ServiceError>;
    fn read_action_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ActionOperationResult, ServiceError>;
}

impl ActionCommands for AppComposition {
    fn action_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: CalendarActionOperation,
    ) -> Result<ActionOperationResult, ServiceError> {
        self.action_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::ActionCommand(command)),
            false,
        )
    }
}

impl ActionQueries for AppComposition {
    fn inspect_actions(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: ActionInspection,
    ) -> Result<ActionOperationResult, ServiceError> {
        self.action_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::ActionInspection(inspection)),
            false,
        )
    }
    fn read_action_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ActionOperationResult, ServiceError> {
        self.action_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn action_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<ActionOperationResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Actions,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(ActionOperationResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            calendar_actions: result.calendar_actions,
            proposal: result.proposal,
            failure: result.failure,
        })
    }
}
