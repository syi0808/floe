use floe_context_contract::{ConnectionId, ConnectorId};
use floe_kernel::PersonId;
use thiserror::Error;
use floe_execution::BoxFuture;

use crate::SourceConnection;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SourceRepositoryError {
    #[error("source connection changed")]
    Conflict,
    #[error("source connection storage unavailable")]
    StorageUnavailable,
    #[error("source connection storage is corrupt")]
    Corrupt,
}

pub trait SourceRepository: Send + Sync {
    fn list_sources<'a>(&'a self,person_id:PersonId,limit:usize)->BoxFuture<'a,Result<Vec<SourceConnection>,SourceRepositoryError>>;
    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<Option<SourceConnection>, SourceRepositoryError>>;

    fn list_current<'a>(
        &'a self,
        person_id: PersonId,
        connector_id: &'a ConnectorId,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, SourceRepositoryError>>;

    fn create<'a>(&'a self, source: &'a SourceConnection) -> BoxFuture<'a, Result<(), SourceRepositoryError>>;

    fn update<'a>(
        &'a self,
        source: &'a SourceConnection,
        expected_revision: u64,
    ) -> BoxFuture<'a, Result<(), SourceRepositoryError>>;
}
