use crate::ConnectionsRecord;
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;
pub trait ConnectionsProductRepository: Send + Sync {
    fn rejected_command<'a>(
        &'a self,
        identity: crate::ConnectionsCommandIdentity,
    ) -> BoxFuture<'a, Result<Option<AgentFailure>, AgentFailure>>;
    /// Serializes absence proof and a rejection fence with the first product
    /// commit. It cannot classify a commit failure by its error name.
    fn reject_unadmitted_command<'a>(
        &'a self,
        identity: crate::ConnectionsCommandIdentity,
        reason: AgentFailure,
    ) -> BoxFuture<'a, Result<crate::ConnectionsCommandResolution, AgentFailure>>;
    fn load<'a>(
        &'a self,
        person: PersonId,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ConnectionsRecord>, AgentFailure>>;
    fn list<'a>(
        &'a self,
        person: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<ConnectionsRecord>, AgentFailure>>;
    fn insert<'a>(
        &'a self,
        record: ConnectionsRecord,
    ) -> BoxFuture<'a, Result<ConnectionsRecord, AgentFailure>>;
    fn compare_and_swap<'a>(
        &'a self,
        expected_revision: u64,
        record: ConnectionsRecord,
    ) -> BoxFuture<'a, Result<ConnectionsRecord, AgentFailure>>;
}
