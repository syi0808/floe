use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CallerContext, ConversationSessionOperation, ServiceError,
    VaultState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationSessionCommand {
    Start,
    Resume,
    Recover {
        session_id: Uuid,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug)]
pub struct ConversationSessionResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub session: Option<floe_conversation::AgentSession>,
    pub failure: Option<AgentFailure>,
}

pub trait ConversationSessionCommands {
    fn session_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: ConversationSessionCommand,
    ) -> Result<ConversationSessionResult, ServiceError>;
}

pub trait ConversationSessionQueries {
    fn get_session(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        session_id: Uuid,
    ) -> Result<ConversationSessionResult, ServiceError>;
    fn read_session_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ConversationSessionResult, ServiceError>;
}

impl ConversationSessionCommands for AppComposition {
    fn session_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: ConversationSessionCommand,
    ) -> Result<ConversationSessionResult, ServiceError> {
        let operation = match command {
            ConversationSessionCommand::Start => ConversationSessionOperation::Start,
            ConversationSessionCommand::Resume => ConversationSessionOperation::Resume,
            ConversationSessionCommand::Recover {
                session_id,
                expected_revision,
            } => {
                if session_id.is_nil() {
                    return Err(ServiceError::InvalidInput);
                }
                ConversationSessionOperation::Recover {
                    session_id,
                    expected_revision,
                }
            }
        };
        self.session_operation(caller, operation_id, Some(operation), false)
    }
}

impl ConversationSessionQueries for AppComposition {
    fn get_session(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        session_id: Uuid,
    ) -> Result<ConversationSessionResult, ServiceError> {
        if session_id.is_nil() {
            return Err(ServiceError::InvalidInput);
        }
        self.session_operation(
            caller,
            operation_id,
            Some(ConversationSessionOperation::Get { session_id }),
            false,
        )
    }

    fn read_session_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ConversationSessionResult, ServiceError> {
        self.session_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn session_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        operation: Option<ConversationSessionOperation>,
        release: bool,
    ) -> Result<ConversationSessionResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                operation.map(LocalOperationIntent::ConversationSession),
                LocalOperationOwner::Conversation,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        let failure = result.failure.or_else(|| {
            match result
                .session
                .as_ref()
                .and_then(|session| session.last_outcome.as_ref())
            {
                Some(floe_conversation::AgentOutcome::Halted { reason }) => Some(*reason),
                _ => None,
            }
        });
        Ok(ConversationSessionResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            session: result.session,
            failure,
        })
    }
}
