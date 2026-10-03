use floe_context_contract::{ConnectionId, ConnectorId};
use floe_kernel::PersonId;
use thiserror::Error;

use crate::{
    SourceConnection, SourceConnectionError, SourceRepository,
    SourceRepositoryError,
};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SourceServiceError {
    #[error("source connection not found")]
    NotFound,
    #[error(transparent)]
    Invalid(#[from] SourceConnectionError),
    #[error(transparent)]
    Repository(#[from] SourceRepositoryError),
}

pub struct SourceConnectionService<'a, Repository: SourceRepository + ?Sized> {
    repository: &'a Repository,
}

impl<'a, Repository: SourceRepository + ?Sized> SourceConnectionService<'a, Repository> {
    pub fn new(repository: &'a Repository) -> Self {
        Self { repository }
    }

    pub async fn load(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
    ) -> Result<Option<SourceConnection>, SourceServiceError> {
        Ok(self.repository.load(person_id, connection_id).await?)
    }

    pub async fn list_current(
        &self,
        person_id: PersonId,
        connector_id: &ConnectorId,
    ) -> Result<Vec<SourceConnection>, SourceServiceError> {
        Ok(self
            .repository
            .list_current(person_id, connector_id)
            .await?)
    }

}
