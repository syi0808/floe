use crate::ConnectionsRecord;
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;
/// A bounded observation page, ordered by canonical record UUID. A continuation
/// means more records existed at this read; it is never a completeness claim.
pub struct ConnectionsRecordPage {
    pub records: Vec<ConnectionsRecord>,
    pub next_after: Option<Uuid>,
}

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
    fn list_page<'a>(
        &'a self,
        person: PersonId,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<ConnectionsRecordPage, AgentFailure>>;
    /// Atomically admits the exact cancellation and fences its integration target.
    fn admit_cancellation<'a>(
        &'a self,
        receipt: ConnectionsRecord,
    ) -> BoxFuture<'a, Result<ConnectionsRecord, AgentFailure>>;
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
