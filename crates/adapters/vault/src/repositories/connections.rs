use floe_connections::{
    ConnectionId, ConnectorId, SourceConnection, SourceRepository, SourceRepositoryError,
    SourceState,
};
use floe_execution::BoxFuture;
use floe_kernel::PersonId;
use turso::Row;

use crate::TursoStore;

const MAX_SOURCE_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

impl SourceRepository for TursoStore {
    fn list_sources<'a>(
        &'a self,
        person_id: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, SourceRepositoryError>> {
        Box::pin(async move {
            if limit == 0 || limit > 512 {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(storage_error)?;
            let mut rows=connection.query("SELECT connection_id,person_id,connector_id,revision,payload FROM source_connections WHERE person_id=? ORDER BY connection_id LIMIT ?",(person_id.to_string(),limit as i64)).await.map_err(storage_error)?;
            let mut sources = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage_error)? {
                let source = decode_source(&row)?;
                if source.person_id() != person_id {
                    return Err(SourceRepositoryError::Corrupt);
                }
                sources.push(source);
            }
            Ok(sources)
        })
    }
    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<Option<SourceConnection>, SourceRepositoryError>> {
        Box::pin(async move {
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
        })
    }

    fn list_current<'a>(
        &'a self,
        person_id: PersonId,
        connector_id: &'a ConnectorId,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, SourceRepositoryError>> {
        Box::pin(async move {
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
        })
    }

    fn create<'a>(
        &'a self,
        source: &'a SourceConnection,
    ) -> BoxFuture<'a, Result<(), SourceRepositoryError>> {
        Box::pin(async move {
            source
                .validate()
                .map_err(|_| SourceRepositoryError::Corrupt)?;
            if source.revision() != 1 {
                return Err(SourceRepositoryError::Conflict);
            }
            let payload =
                serde_json::to_string(source).map_err(|_| SourceRepositoryError::Corrupt)?;
            if payload.len() > MAX_SOURCE_PAYLOAD_BYTES {
                return Err(SourceRepositoryError::Corrupt);
            }
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
            if fenced_on(&connection, source.person_id(), source.connection_id()).await? { return Err(SourceRepositoryError::Conflict); }
        let changed = connection.execute(
            "INSERT OR IGNORE INTO source_connections(connection_id, person_id, connector_id, revision, payload) VALUES (?, ?, ?, ?, ?)",
            (source.connection_id().as_str(), source.person_id().to_string(), source.connector_id().as_str(), source.revision() as i64, payload),
        ).await.map_err(storage_error)?;
        if changed != 1 {
            return Err(SourceRepositoryError::Conflict);
        }
            Ok(())
        }.await;
            finish_source_transaction(&connection, result).await?;

            Ok(())
        })
    }

    fn update<'a>(
        &'a self,
        source: &'a SourceConnection,
        expected_revision: u64,
    ) -> BoxFuture<'a, Result<(), SourceRepositoryError>> {
        Box::pin(async move {
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
            let payload =
                serde_json::to_string(source).map_err(|_| SourceRepositoryError::Corrupt)?;
            if payload.len() > MAX_SOURCE_PAYLOAD_BYTES {
                return Err(SourceRepositoryError::Corrupt);
            }
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
            if fenced_on(&connection, source.person_id(), source.connection_id()).await? { return Err(SourceRepositoryError::Conflict); }
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
        })
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

use floe_connections::{
    SourceOperationAdmission, SourceOperationChange, SourceOperationPhase, SourceOperationRecord,
    SourceOperationRepository, SourceOperationReservation,
};
use uuid::Uuid;
const MAX_OPERATION_BYTES: usize = 16_384;

pub(crate) async fn initialize_source_operations(
    connection: &turso::Connection,
) -> Result<(), SourceRepositoryError> {
    connection.execute("CREATE TABLE IF NOT EXISTS source_operations (operation_id TEXT PRIMARY KEY, command_id TEXT NOT NULL, person_id TEXT NOT NULL, connection_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), fence INTEGER NOT NULL CHECK(fence IN (0,1)), payload TEXT NOT NULL, UNIQUE(person_id,command_id))", ()).await.map_err(storage_error)?;
    connection.execute("CREATE UNIQUE INDEX IF NOT EXISTS source_operation_fence ON source_operations(connection_id) WHERE fence = 1", ()).await.map_err(storage_error)?;
    Ok(())
}
async fn fenced_on(
    connection: &turso::Connection,
    person: PersonId,
    source: &ConnectionId,
) -> Result<bool, SourceRepositoryError> {
    let mut rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE connection_id = ? AND fence = 1", (source.as_str(),)).await.map_err(storage_error)?;
    let Some(row) = rows.next().await.map_err(storage_error)? else {
        return Ok(false);
    };
    let operation = decode_operation(&row)?;
    if operation.expected.source.person_id() != person {
        return Err(SourceRepositoryError::Conflict);
    }
    Ok(operation.phase.holds_fence())
}
fn decode_operation(row: &Row) -> Result<SourceOperationRecord, SourceRepositoryError> {
    let payload: String = row.get(6).map_err(storage_error)?;
    if payload.len() > MAX_OPERATION_BYTES {
        return Err(SourceRepositoryError::Corrupt);
    }
    let record: SourceOperationRecord =
        serde_json::from_str(&payload).map_err(|_| SourceRepositoryError::Corrupt)?;
    record.validate()?;
    if record.operation_id.to_string() != row.get::<String>(0).map_err(storage_error)?
        || record.command_id.to_string() != row.get::<String>(1).map_err(storage_error)?
        || record.expected.source.person_id().to_string()
            != row.get::<String>(2).map_err(storage_error)?
        || record.expected.source.connection_id().as_str()
            != row.get::<String>(3).map_err(storage_error)?
        || record.revision as i64 != row.get::<i64>(4).map_err(storage_error)?
        || i64::from(record.phase.holds_fence()) != row.get::<i64>(5).map_err(storage_error)?
    {
        return Err(SourceRepositoryError::Corrupt);
    }
    Ok(record)
}
async fn operation_on(
    connection: &turso::Connection,
    id: Uuid,
) -> Result<Option<SourceOperationRecord>, SourceRepositoryError> {
    let mut rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE operation_id = ?", (id.to_string(),)).await.map_err(storage_error)?;
    rows.next()
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(decode_operation)
        .transpose()
}
async fn source_on(
    connection: &turso::Connection,
    id: &ConnectionId,
) -> Result<Option<SourceConnection>, SourceRepositoryError> {
    let mut rows = connection.query("SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE connection_id = ?", (id.as_str(),)).await.map_err(storage_error)?;
    rows.next()
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(decode_source)
        .transpose()
}
async fn finish_source_transaction<T>(
    connection: &turso::Connection,
    result: Result<T, SourceRepositoryError>,
) -> Result<T, SourceRepositoryError> {
    match result {
        Ok(value) => {
            connection
                .execute("COMMIT", ())
                .await
                .map_err(storage_error)?;
            Ok(value)
        }
        Err(error) => {
            let _ = connection.execute("ROLLBACK", ()).await;
            Err(error)
        }
    }
}
impl SourceOperationRepository for TursoStore {
    fn reserve<'a>(
        &'a self,
        request: SourceOperationReservation,
    ) -> BoxFuture<'a, Result<SourceOperationAdmission, SourceRepositoryError>> {
        Box::pin(async move {
            let requested = request.record;
            requested.validate()?;
            if requested.revision != 1 || requested.phase != SourceOperationPhase::Reserved {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
            let mut replay_rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE operation_id = ? OR (person_id = ? AND command_id = ?)",
                (requested.operation_id.to_string(), requested.expected.source.person_id().to_string(), requested.command_id.to_string())).await.map_err(storage_error)?;
            if let Some(row) = replay_rows.next().await.map_err(storage_error)? {
                let current = decode_operation(&row)?;
                if replay_rows.next().await.map_err(storage_error)?.is_some() { return Err(SourceRepositoryError::Conflict); }
                let mut initial = current.clone(); initial.revision = 1; initial.phase = SourceOperationPhase::Reserved;
                if initial != requested { return Err(SourceRepositoryError::Conflict); }
                return Ok(SourceOperationAdmission { record: current, replayed: true });
            }
            drop(replay_rows);
            let source = source_on(&connection, &requested.expected.source.connection_id()).await?;
            if !requested.expected.matches(source.as_ref()) || fenced_on(&connection, requested.expected.source.person_id(), &requested.expected.source.connection_id()).await? { return Err(SourceRepositoryError::Conflict); }
            let payload = serde_json::to_string(&requested).map_err(|_| SourceRepositoryError::Corrupt)?;
            if payload.len() > MAX_OPERATION_BYTES { return Err(SourceRepositoryError::Corrupt); }
            let changed = connection.execute("INSERT OR IGNORE INTO source_operations(operation_id,command_id,person_id,connection_id,revision,fence,payload) VALUES (?,?,?,?,1,1,?)",
                (requested.operation_id.to_string(),requested.command_id.to_string(),requested.expected.source.person_id().to_string(),requested.expected.source.connection_id().to_string(),payload)).await.map_err(storage_error)?;
            if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            Ok(SourceOperationAdmission { record: requested, replayed: false })
        }.await;
            finish_source_transaction(&connection, result).await
        })
    }
    fn load_operation<'a>(
        &'a self,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<SourceOperationRecord>, SourceRepositoryError>> {
        Box::pin(async move {
            if id.is_nil() {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(storage_error)?;
            operation_on(&connection, id).await
        })
    }
    fn compare_and_swap_operation<'a>(
        &'a self,
        change: SourceOperationChange,
    ) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>> {
        Box::pin(async move {
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
            let current = operation_on(&connection, change.operation_id).await?.ok_or(SourceRepositoryError::Conflict)?;
            let source = source_on(&connection, &current.expected.source.connection_id()).await?;
            let next = change.validate(&current, source.as_ref())?;
            if let Some(successor) = &change.successor {
                let payload = serde_json::to_string(successor).map_err(|_| SourceRepositoryError::Corrupt)?;
                if payload.len() > MAX_SOURCE_PAYLOAD_BYTES { return Err(SourceRepositoryError::Corrupt); }
                let changed = if let Some(source) = source {
                    connection.execute("UPDATE source_connections SET revision = ?, payload = ? WHERE connection_id = ? AND revision = ?",
                        (successor.revision() as i64,payload,successor.connection_id().as_str(),source.revision() as i64)).await.map_err(storage_error)?
                } else {
                    connection.execute("INSERT OR IGNORE INTO source_connections(connection_id,person_id,connector_id,revision,payload) VALUES (?,?,?,?,?)",
                        (successor.connection_id().as_str(),successor.person_id().to_string(),successor.connector_id().as_str(),successor.revision() as i64,payload)).await.map_err(storage_error)?
                };
                if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            }
            let payload = serde_json::to_string(&next).map_err(|_| SourceRepositoryError::Corrupt)?;
            if payload.len() > MAX_OPERATION_BYTES { return Err(SourceRepositoryError::Corrupt); }
            let changed = connection.execute("UPDATE source_operations SET revision = ?, fence = ?, payload = ? WHERE operation_id = ? AND revision = ?",
                (next.revision as i64,i64::from(next.phase.holds_fence()),payload,next.operation_id.to_string(),current.revision as i64)).await.map_err(storage_error)?;
            if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            Ok(next)
        }.await;
            finish_source_transaction(&connection, result).await
        })
    }
    fn list_nonterminal<'a>(
        &'a self,
        person_id: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SourceOperationRecord>, SourceRepositoryError>> {
        Box::pin(async move {
            if limit == 0 || limit > 128 {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(storage_error)?;
            let mut rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE person_id = ? AND fence = 1 ORDER BY operation_id LIMIT ?", (person_id.to_string(),limit as i64)).await.map_err(storage_error)?;
            let mut records = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage_error)? {
                records.push(decode_operation(&row)?);
            }
            Ok(records)
        })
    }
    fn source_is_fenced<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<bool, SourceRepositoryError>> {
        Box::pin(async move {
            let connection = self.connection().await.map_err(storage_error)?;
            fenced_on(&connection, person_id, connection_id).await
        })
    }
}
