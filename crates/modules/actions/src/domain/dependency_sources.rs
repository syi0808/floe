use floe_connections::{SourceConnection, SourceReservationFence};
use floe_context_contract::ContextDependency;
use floe_kernel::{AgentFailure, PersonId};

/// Rust-private source evidence retained by one live prepared invocation.
/// This is never serialized into the native broker request or product wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionDependencySourceFence {
    pub dependency: ContextDependency,
    pub source: SourceConnection,
    pub reservation: SourceReservationFence,
}

impl ActionDependencySourceFence {
    pub fn validate(&self, person: PersonId) -> Result<(), AgentFailure> {
        self.dependency.validate().map_err(|_| AgentFailure::PolicyDenied)?;
        self.source.validate().map_err(|_| AgentFailure::PolicyDenied)?;
        self.reservation.validate().map_err(|_| AgentFailure::PolicyDenied)?;
        if self.reservation.fenced || self.dependency.person_id() != person
            || self.source.person_id() != person || !self.source.is_serving()
            || self.source.connection_id() != &self.dependency.source().connection_id()
            || self.source.connector_id() != self.dependency.source().connector()
            || self.source.execution_owner_id() != self.dependency.source().execution_owner()
            || self.source.source_authority() != self.dependency.source_authority()
            || self.dependency.source_resources().iter().any(|resource|
                !self.source.resources().iter().any(|current| current.handle() == resource))
        { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
}
