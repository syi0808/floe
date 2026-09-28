use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId};
use floe_kernel::PersonId;
use thiserror::Error;

use crate::{
    ConnectionResource, ResourceMode, SourceConnection, SourceConnectionError, SourceRepository,
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

    pub async fn establish(
        &self,
        person_id: PersonId,
        connector_id: ConnectorId,
        connection_id: ConnectionId,
        execution_owner_id: ExecutionOwnerId,
        resource_mode: ResourceMode,
        resources: Vec<ConnectionResource>,
    ) -> Result<SourceConnection, SourceServiceError> {
        let source = SourceConnection::establish(
            person_id,
            connector_id,
            connection_id,
            execution_owner_id,
            resource_mode,
            resources,
        )?;
        self.repository.create(&source).await?;
        Ok(source)
    }

    pub async fn configure(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        expected_revision: u64,
        resource_mode: ResourceMode,
        resources: Vec<ConnectionResource>,
    ) -> Result<SourceConnection, SourceServiceError> {
        let mut source = self.current(person_id, connection_id).await?;
        if source.configure(expected_revision, resource_mode, resources)? {
            self.repository.update(&source, expected_revision).await?;
        }
        Ok(source)
    }

    pub async fn update_native_subject(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        expected_revision: u64,
        fingerprint: String,
    ) -> Result<SourceConnection, SourceServiceError> {
        let mut source = self.current(person_id, connection_id).await?;
        if source.update_native_subject(expected_revision, fingerprint)? {
            self.repository.update(&source, expected_revision).await?;
        }
        Ok(source)
    }

    pub async fn reconcile_inventory(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        expected_revision: u64,
        resources: Vec<ConnectionResource>,
    ) -> Result<SourceConnection, SourceServiceError> {
        let mut source = self.current(person_id, connection_id).await?;
        if source.reconcile_inventory(expected_revision, resources)? {
            self.repository.update(&source, expected_revision).await?;
        }
        Ok(source)
    }

    pub async fn disconnect(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        expected_revision: u64,
    ) -> Result<SourceConnection, SourceServiceError> {
        let mut source = self.current(person_id, connection_id).await?;
        if source.disconnect(expected_revision)? {
            self.repository.update(&source, expected_revision).await?;
        }
        Ok(source)
    }

    async fn current(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
    ) -> Result<SourceConnection, SourceServiceError> {
        self.repository
            .load(person_id, connection_id)
            .await?
            .ok_or(SourceServiceError::NotFound)
    }
}
