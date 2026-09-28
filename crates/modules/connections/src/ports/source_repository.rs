use floe_context_contract::{ConnectionId, ConnectorId};
use floe_kernel::PersonId;
use thiserror::Error;

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

#[allow(async_fn_in_trait)]
pub trait SourceRepository: Send + Sync {
    async fn load(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
    ) -> Result<Option<SourceConnection>, SourceRepositoryError>;

    async fn list_current(
        &self,
        person_id: PersonId,
        connector_id: &ConnectorId,
    ) -> Result<Vec<SourceConnection>, SourceRepositoryError>;

    async fn create(&self, source: &SourceConnection) -> Result<(), SourceRepositoryError>;

    async fn update(
        &self,
        source: &SourceConnection,
        expected_revision: u64,
    ) -> Result<(), SourceRepositoryError>;
}
