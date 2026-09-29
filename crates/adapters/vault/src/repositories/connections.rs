use floe_connections::{
    ConnectionId, ConnectorId, SourceConnection, SourceRepository, SourceRepositoryError,
    SourceState,
};
use floe_kernel::PersonId;
use turso::Row;

use crate::TursoStore;

const MAX_SOURCE_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

impl SourceRepository for TursoStore {
    async fn load(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
    ) -> Result<Option<SourceConnection>, SourceRepositoryError> {
        let connection = self.connection().await.map_err(storage_error)?;
        let mut rows = connection.query(
            "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE connection_id = ?",
            (connection_id.as_str(),),
        ).await.map_err(storage_error)?;
        let Some(row) = rows.next().await.map_err(storage_error)? else {
            return Ok(None);
        };
        let source = decode_source(&row)?;
        if source.person_id() != person_id {
            return Ok(None);
        }
        Ok(Some(source))
    }

    async fn list_current(
        &self,
        person_id: PersonId,
        connector_id: &ConnectorId,
    ) -> Result<Vec<SourceConnection>, SourceRepositoryError> {
        let connection = self.connection().await.map_err(storage_error)?;
        let mut rows = connection.query(
            "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE person_id = ? AND connector_id = ? ORDER BY connection_id",
            (person_id.to_string(), connector_id.as_str()),
        ).await.map_err(storage_error)?;
        let mut sources = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage_error)? {
            let source = decode_source(&row)?;
            if source.person_id() != person_id || source.connector_id() != connector_id {
                return Err(SourceRepositoryError::Corrupt);
            }
            if source.state() != SourceState::Disconnected {
                sources.push(source);
            }
        }
        Ok(sources)
    }

    async fn create(&self, source: &SourceConnection) -> Result<(), SourceRepositoryError> {
        source
            .validate()
            .map_err(|_| SourceRepositoryError::Corrupt)?;
        if source.revision() != 1 {
            return Err(SourceRepositoryError::Conflict);
        }
        let payload = serde_json::to_string(source).map_err(|_| SourceRepositoryError::Corrupt)?;
        if payload.len() > MAX_SOURCE_PAYLOAD_BYTES {
            return Err(SourceRepositoryError::Corrupt);
        }
        let connection = self.connection().await.map_err(storage_error)?;
        let changed = connection.execute(
            "INSERT OR IGNORE INTO source_connections(connection_id, person_id, connector_id, revision, payload) VALUES (?, ?, ?, ?, ?)",
            (source.connection_id().as_str(), source.person_id().to_string(), source.connector_id().as_str(), source.revision() as i64, payload),
        ).await.map_err(storage_error)?;
        if changed != 1 {
            return Err(SourceRepositoryError::Conflict);
        }
        Ok(())
    }

    async fn update(
        &self,
        source: &SourceConnection,
        expected_revision: u64,
    ) -> Result<(), SourceRepositoryError> {
        source
            .validate()
            .map_err(|_| SourceRepositoryError::Corrupt)?;
        if source.revision()
            != expected_revision
                .checked_add(1)
                .ok_or(SourceRepositoryError::Conflict)?
        {
            return Err(SourceRepositoryError::Conflict);
        }
        let payload = serde_json::to_string(source).map_err(|_| SourceRepositoryError::Corrupt)?;
        if payload.len() > MAX_SOURCE_PAYLOAD_BYTES {
            return Err(SourceRepositoryError::Corrupt);
        }
        let connection = self.connection().await.map_err(storage_error)?;
        connection
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(storage_error)?;
        let result = async {
            let mut rows = connection.query(
                "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE connection_id = ?",
                (source.connection_id().as_str(),),
            ).await.map_err(storage_error)?;
            let row = rows.next().await.map_err(storage_error)?.ok_or(SourceRepositoryError::Conflict)?;
            let stored = decode_source(&row)?;
            drop(rows);
            if stored.person_id() != source.person_id()
                || stored.connector_id() != source.connector_id()
                || stored.execution_owner_id() != source.execution_owner_id()
                || stored.revision() != expected_revision
            {
                return Err(SourceRepositoryError::Conflict);
            }
            stored
                .validate_successor(source)
                .map_err(|_| SourceRepositoryError::Corrupt)?;
            let changed = connection.execute(
                "UPDATE source_connections SET revision = ?, payload = ? WHERE connection_id = ? AND person_id = ? AND connector_id = ? AND revision = ?",
                (source.revision() as i64, payload, source.connection_id().as_str(), source.person_id().to_string(), source.connector_id().as_str(), expected_revision as i64),
            ).await.map_err(storage_error)?;
            if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            Ok(())
        }.await;
        match result {
            Ok(()) => connection
                .execute("COMMIT", ())
                .await
                .map_err(storage_error)
                .map(|_| ()),
            Err(error) => {
                let _ = connection.execute("ROLLBACK", ()).await;
                Err(error)
            }
        }
    }
}

fn decode_source(row: &Row) -> Result<SourceConnection, SourceRepositoryError> {
    let connection_id: String = row.get(0).map_err(storage_error)?;
    let person_id: String = row.get(1).map_err(storage_error)?;
    let connector_id: String = row.get(2).map_err(storage_error)?;
    let revision: i64 = row.get(3).map_err(storage_error)?;
    let payload: String = row.get(4).map_err(storage_error)?;
    if payload.len() > MAX_SOURCE_PAYLOAD_BYTES || revision <= 0 {
        return Err(SourceRepositoryError::Corrupt);
    }
    let source: SourceConnection =
        serde_json::from_str(&payload).map_err(|_| SourceRepositoryError::Corrupt)?;
    source
        .validate()
        .map_err(|_| SourceRepositoryError::Corrupt)?;
    if source.connection_id().as_str() != connection_id
        || source.person_id().to_string() != person_id
        || source.connector_id().as_str() != connector_id
        || source.revision() != revision as u64
    {
        return Err(SourceRepositoryError::Corrupt);
    }
    Ok(source)
}

fn storage_error(_: impl std::fmt::Display) -> SourceRepositoryError {
    SourceRepositoryError::StorageUnavailable
}

#[cfg(test)]
mod tests {
    use floe_connections::{ConnectionResource, ResourceMode, SourceConnection, SourceRepository};
    use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle};
    use floe_kernel::PersonId;

    use super::*;

    fn source(person_id: PersonId) -> SourceConnection {
        SourceConnection::establish(
            person_id,
            ConnectorId::try_new("calendar.fixture").unwrap(),
            ConnectionId::new(),
            ExecutionOwnerId::try_new("device-1").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("a").unwrap(), "A".into()).unwrap(),
            ],
        )
        .unwrap()
    }

    #[tokio::test]
    async fn source_reopens_and_rejects_foreign_person_and_stale_cas() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sources.db");
        let person_id = PersonId::new();
        let mut source = source(person_id);
        let store = TursoStore::open(&path).await.unwrap();
        store.create(&source).await.unwrap();
        assert!(
            store
                .load(PersonId::new(), source.connection_id())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.create(&source).await,
            Err(SourceRepositoryError::Conflict)
        );
        drop(store);

        let store = TursoStore::open(&path).await.unwrap();
        assert_eq!(
            store.load(person_id, source.connection_id()).await.unwrap(),
            Some(source.clone())
        );
        source
            .configure(1, ResourceMode::AllAvailable, vec![])
            .unwrap();
        store.update(&source, 1).await.unwrap();
        assert_eq!(
            store.update(&source, 1).await,
            Err(SourceRepositoryError::Conflict)
        );
        assert_eq!(
            store
                .list_current(person_id, source.connector_id())
                .await
                .unwrap(),
            vec![source]
        );
    }

    #[tokio::test]
    async fn indexed_payload_mismatch_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let store = TursoStore::open(directory.path().join("sources.db"))
            .await
            .unwrap();
        let source = source(PersonId::new());
        store.create(&source).await.unwrap();
        store
            .connection()
            .await
            .unwrap()
            .execute(
                "UPDATE source_connections SET person_id = ? WHERE connection_id = ?",
                (PersonId::new().to_string(), source.connection_id().as_str()),
            )
            .await
            .unwrap();
        assert_eq!(
            store.load(source.person_id(), source.connection_id()).await,
            Err(SourceRepositoryError::Corrupt)
        );
    }

    #[tokio::test]
    async fn reviewed_contacts_creation_and_combined_change_persist_as_single_transitions() {
        let directory = tempfile::tempdir().unwrap();
        let store = TursoStore::open(directory.path().join("sources.db"))
            .await
            .unwrap();
        let person_id = PersonId::new();
        let connection_id = ConnectionId::new();
        let mut source = SourceConnection::establish_reviewed_native(
            person_id,
            ConnectorId::try_new("contacts.apple").unwrap(),
            connection_id.clone(),
            ExecutionOwnerId::try_new("apple:device-1").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("a").unwrap(), "A".into()).unwrap(),
            ],
            "a".repeat(64),
        )
        .unwrap();
        store.create(&source).await.unwrap();
        let persisted = store
            .load(person_id, &connection_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.revision(), 1);
        assert!(persisted.is_serving());
        assert_eq!(persisted, source);

        source
            .configure_reviewed_native(
                1,
                ResourceMode::Selected,
                vec![
                    ConnectionResource::new(ResourceHandle::try_new("a").unwrap(), "A".into())
                        .unwrap(),
                    ConnectionResource::new(ResourceHandle::try_new("b").unwrap(), "B".into())
                        .unwrap(),
                ],
                "b".repeat(64),
            )
            .unwrap();
        store.update(&source, 1).await.unwrap();
        assert_eq!(
            store.update(&source, 1).await,
            Err(SourceRepositoryError::Conflict)
        );
        drop(store);

        let store = TursoStore::open(directory.path().join("sources.db"))
            .await
            .unwrap();
        let persisted = store
            .load(person_id, &connection_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted, source);
        assert_eq!(persisted.revision(), 2);
        assert_eq!(persisted.source_authority().epoch().get(), 2);
    }
}
