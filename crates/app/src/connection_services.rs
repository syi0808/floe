use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{AgentFailure, AppComposition, CallerContext, ServiceError, VaultState};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ConnectionsResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub connections: Option<Vec<floe_connections::ConnectorSnapshot>>,
    pub failure: Option<AgentFailure>,
}

pub trait ConnectionsQueries {
    fn inspect_connections(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<ConnectionsResult, ServiceError>;
    fn read_connections_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ConnectionsResult, ServiceError>;
}

impl ConnectionsQueries for AppComposition {
    fn inspect_connections(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<ConnectionsResult, ServiceError> {
        self.connections_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::Connections),
            false,
        )
    }
    fn read_connections_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ConnectionsResult, ServiceError> {
        self.connections_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn connections_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<ConnectionsResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Connections,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(ConnectionsResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            connections: result.connections,
            failure: result.failure,
        })
    }
}
