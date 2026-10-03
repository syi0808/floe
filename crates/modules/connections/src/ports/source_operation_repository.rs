use floe_context_contract::ConnectionId;
use floe_kernel::PersonId;
use uuid::Uuid;
use floe_execution::BoxFuture;
use crate::{SourceOperationAdmission, SourceOperationChange, SourceOperationRecord, SourceOperationReservation, SourceRepositoryError};
pub trait SourceOperationRepository: Send + Sync {
    fn reserve<'a>(&'a self, request: SourceOperationReservation) -> BoxFuture<'a, Result<SourceOperationAdmission, SourceRepositoryError>>;
    fn load_operation<'a>(&'a self, id: Uuid) -> BoxFuture<'a, Result<Option<SourceOperationRecord>, SourceRepositoryError>>;
    fn compare_and_swap_operation<'a>(&'a self, change: SourceOperationChange) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>>;
    fn list_nonterminal<'a>(&'a self, person_id: PersonId, limit: usize) -> BoxFuture<'a, Result<Vec<SourceOperationRecord>, SourceRepositoryError>>;
    fn source_is_fenced<'a>(&'a self, person_id: PersonId, connection_id: &'a ConnectionId) -> BoxFuture<'a, Result<bool, SourceRepositoryError>>;
}

pub trait ConnectionsRepository: crate::SourceRepository + SourceOperationRepository {}
impl<T: crate::SourceRepository + SourceOperationRepository + ?Sized> ConnectionsRepository for T {}
