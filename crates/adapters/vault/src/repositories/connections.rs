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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            let mut rows=connection.query("SELECT connection_id,person_id,connector_id,revision,payload FROM source_connections WHERE person_id=? ORDER BY connection_id LIMIT ?",(person_id.to_string(),limit as i64)).await.map_err(source_database_failure)?;
            let mut sources = Vec::new();
            while let Some(row) = rows.next().await.map_err(source_database_failure)? {
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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            let mut rows = connection.query(
            "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE connection_id = ?",
            (connection_id.as_str(),),
        ).await.map_err(source_database_failure)?;
            let Some(row) = rows.next().await.map_err(source_database_failure)? else {
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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            let mut rows = connection.query(
            "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE person_id = ? AND connector_id = ? ORDER BY connection_id",
            (person_id.to_string(), connector_id.as_str()),
        ).await.map_err(source_database_failure)?;
            let mut sources = Vec::new();
            while let Some(row) = rows.next().await.map_err(source_database_failure)? {
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
            let mut writer = self
                .source_write_guard()
                .map_err(source_connection_failure)?;
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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            begin_source_transaction(&mut writer, &connection).await?;
            let result = async {
            if fenced_on(&connection, source.person_id(), source.connection_id()).await? { return Err(SourceRepositoryError::Conflict); }
        let changed = connection.execute(
            "INSERT OR IGNORE INTO source_connections(connection_id, person_id, connector_id, revision, payload) VALUES (?, ?, ?, ?, ?)",
            (source.connection_id().as_str(), source.person_id().to_string(), source.connector_id().as_str(), source.revision() as i64, payload),
        ).await.map_err(source_database_failure)?;
        if changed != 1 {
            return Err(SourceRepositoryError::Conflict);
        }
            Ok(())
        }.await;
            let outcome = finish_source_transaction(self, &connection, result).await;
            writer.settled();
            outcome?;

            Ok(())
        })
    }

    fn update<'a>(
        &'a self,
        source: &'a SourceConnection,
        expected_revision: u64,
    ) -> BoxFuture<'a, Result<(), SourceRepositoryError>> {
        Box::pin(async move {
            let mut writer = self
                .source_write_guard()
                .map_err(source_connection_failure)?;
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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            begin_source_transaction(&mut writer, &connection).await?;
            let result = async {
            if fenced_on(&connection, source.person_id(), source.connection_id()).await? { return Err(SourceRepositoryError::Conflict); }
            let mut rows = connection.query(
                "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE connection_id = ?",
                (source.connection_id().as_str(),),
            ).await.map_err(source_database_failure)?;
            let row = rows.next().await.map_err(source_database_failure)?.ok_or(SourceRepositoryError::Conflict)?;
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
            ).await.map_err(source_database_failure)?;
            if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            Ok(())
        }.await;
            let outcome = finish_source_transaction(self, &connection, result).await;
            writer.settled();
            outcome
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

fn source_connection_failure(error: crate::StoreError) -> SourceRepositoryError {
    match error.code {
        crate::StoreErrorCode::StorageBusy => SourceRepositoryError::StorageBusy,
        _ => SourceRepositoryError::StorageUnavailable,
    }
}

fn storage_error(_: impl std::fmt::Display) -> SourceRepositoryError {
    SourceRepositoryError::StorageUnavailable
}

use floe_connections::{
    SourceOperationAdmission, SourceOperationChange, SourceOperationPhase, SourceOperationRecord,
    SourceOperationRepository, SourceOperationReservation, SourceReservationFence,
    SourceReservationWatermark,
};
use uuid::Uuid;
const MAX_OPERATION_BYTES: usize = 16_384;

async fn fenced_on(
    connection: &turso::Connection,
    person: PersonId,
    source: &ConnectionId,
) -> Result<bool, SourceRepositoryError> {
    let mut rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE connection_id = ? AND fence = 1", (source.as_str(),)).await.map_err(source_database_failure)?;
    let Some(row) = rows.next().await.map_err(source_database_failure)? else {
        return Ok(false);
    };
    let operation = decode_operation(&row)?;
    if operation.expected.source.person_id() != person {
        return Err(SourceRepositoryError::Conflict);
    }
    Ok(operation.phase.holds_fence())
}
fn decode_operation(row: &Row) -> Result<SourceOperationRecord, SourceRepositoryError> {
    let payload: String = row.get(6).map_err(|_| SourceRepositoryError::Corrupt)?;
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
    let mut rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE operation_id = ?", (id.to_string(),)).await.map_err(source_database_failure)?;
    rows.next()
        .await
        .map_err(source_database_failure)?
        .as_ref()
        .map(decode_operation)
        .transpose()
}
async fn source_on(
    connection: &turso::Connection,
    id: &ConnectionId,
) -> Result<Option<SourceConnection>, SourceRepositoryError> {
    let mut rows = connection.query("SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE connection_id = ?", (id.as_str(),)).await.map_err(source_database_failure)?;
    rows.next()
        .await
        .map_err(source_database_failure)?
        .as_ref()
        .map(decode_source)
        .transpose()
}
async fn begin_source_transaction(
    writer: &mut crate::write_fence::JournalWriteGuard<'_>,
    connection: &turso::Connection,
) -> Result<(), SourceRepositoryError> {
    writer
        .arm()
        .map_err(|_| SourceRepositoryError::StorageUnavailable)?;
    match connection.execute("BEGIN IMMEDIATE", ()).await {
        Ok(_) => Ok(()),
        Err(error) => {
            let failure = source_database_failure(error);
            if failure == SourceRepositoryError::StorageBusy {
                writer.settled();
            }
            Err(failure)
        }
    }
}

async fn finish_source_transaction<T>(
    store: &TursoStore,
    connection: &turso::Connection,
    result: Result<T, SourceRepositoryError>,
) -> Result<T, SourceRepositoryError> {
    let result = result.and_then(|value| {
        store
            .check_available()
            .map_err(source_connection_failure)
            .map(|_| value)
    });
    match result {
        Ok(value) => {
            if connection.execute("COMMIT", ()).await.is_err() {
                store.latch_unavailable();
                return Err(SourceRepositoryError::StorageUnavailable);
            }
            store.check_available().map_err(source_connection_failure)?;
            Ok(value)
        }
        Err(error) => {
            if connection.execute("ROLLBACK", ()).await.is_err() {
                store.latch_unavailable();
                return Err(SourceRepositoryError::StorageUnavailable);
            }
            Err(error)
        }
    }
}

impl SourceOperationRepository for TursoStore {
    fn settle_presentation<'a>(
        &'a self,
        request: SourceOperationReservation,
        decision: floe_connections::SourcePresentationDecision,
    ) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>> {
        Box::pin(async move {
            request.record.validate()?;
            if request.record.kind != floe_connections::SourceOperationKind::ConnectionPresentation
                || request.record.phase != SourceOperationPhase::Reserved
                || request.record.revision != 1
            {
                return Err(SourceRepositoryError::Conflict);
            }
            let mut writer = self
                .source_write_guard()
                .map_err(source_connection_failure)?;
            let connection = self.connection().await.map_err(source_connection_failure)?;
            begin_source_transaction(&mut writer, &connection).await?;
            let result = async {
                let requested = &request.record;
                let mut rows = connection.query(
                    "SELECT operation_id,command_id,person_id,connection_id,revision,fence,payload FROM source_operations WHERE operation_id=? OR (person_id=? AND command_id=?)",
                    (requested.operation_id.to_string(), requested.expected.source.person_id().to_string(), requested.command_id.to_string()),
                ).await.map_err(source_database_failure)?;
                if let Some(row) = rows.next().await.map_err(source_database_failure)? {
                    let existing = decode_operation(&row)?;
                    if rows.next().await.map_err(source_database_failure)?.is_some() || existing.phase.holds_fence() {
                        return Err(SourceRepositoryError::Conflict);
                    }
                    let mut initial = existing.clone();
                    initial.phase = SourceOperationPhase::Reserved;
                    initial.revision = 1;
                    if initial != *requested { return Err(SourceRepositoryError::Conflict); }
                    return Ok(existing);
                }
                drop(rows);
                if source_command_rejection_on(&connection, requested.expected.source.person_id(), requested.command_id).await?.is_some() {
                    return Err(SourceRepositoryError::Conflict);
                }
                let id = requested.expected.source.connection_id();
                if fenced_on(&connection, requested.expected.source.person_id(), &id).await? {
                    return Err(SourceRepositoryError::Conflict);
                }
                let source = source_on(&connection, &id).await?;
                let (terminal, successor) = request.settle_presentation(source.as_ref(), decision)?;
                if let Some(successor) = successor {
                    let payload = serde_json::to_string(&successor).map_err(|_| SourceRepositoryError::Corrupt)?;
                    if payload.len() > MAX_SOURCE_PAYLOAD_BYTES { return Err(SourceRepositoryError::Corrupt); }
                    let changed = connection.execute(
                        "UPDATE source_connections SET revision=?,payload=? WHERE connection_id=? AND person_id=? AND revision=?",
                        (successor.revision() as i64, payload, id.as_str(), successor.person_id().to_string(), requested.expected.revision.ok_or(SourceRepositoryError::Conflict)? as i64),
                    ).await.map_err(source_database_failure)?;
                    if changed != 1 { return Err(SourceRepositoryError::Conflict); }
                }
                let payload = serde_json::to_string(&terminal).map_err(|_| SourceRepositoryError::Corrupt)?;
                if payload.len() > MAX_OPERATION_BYTES { return Err(SourceRepositoryError::Corrupt); }
                let changed = connection.execute(
                    "INSERT OR IGNORE INTO source_operations(operation_id,command_id,person_id,connection_id,revision,fence,payload) VALUES(?,?,?,?,1,0,?)",
                    (terminal.operation_id.to_string(), terminal.command_id.to_string(), terminal.expected.source.person_id().to_string(), id.as_str(), payload),
                ).await.map_err(source_database_failure)?;
                if changed != 1 { return Err(SourceRepositoryError::Conflict); }
                Ok(terminal)
            }.await;
            let outcome = finish_source_transaction(self, &connection, result).await;
            writer.settled();
            outcome
        })
    }
    fn rejected_operation_command<'a>(
        &'a self,
        identity: floe_connections::ConnectionsCommandIdentity,
    ) -> BoxFuture<'a, Result<Option<floe_kernel::AgentFailure>, SourceRepositoryError>> {
        Box::pin(async move {
            identity
                .validate()
                .map_err(|_| SourceRepositoryError::Conflict)?;
            if identity.journal != floe_connections::ConnectionsCommandJournal::SourceOperation {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(source_connection_failure)?;
            match source_command_rejection_on(&connection, identity.person_id, identity.command_id)
                .await?
            {
                Some(receipt) if receipt.identity == identity => Ok(Some(receipt.reason)),
                Some(_) => Err(SourceRepositoryError::Conflict),
                None => Ok(None),
            }
        })
    }
    fn reject_unadmitted_operation_command<'a>(
        &'a self,
        identity: floe_connections::ConnectionsCommandIdentity,
        reason: floe_kernel::AgentFailure,
    ) -> BoxFuture<'a, Result<floe_connections::ConnectionsCommandResolution, SourceRepositoryError>>
    {
        Box::pin(async move {
            let mut writer = self
                .source_write_guard()
                .map_err(source_connection_failure)?;
            if reason == floe_kernel::AgentFailure::StorageBusy {
                return Err(SourceRepositoryError::StorageBusy);
            }
            use floe_connections::{ConnectionsCommandRejection, ConnectionsCommandResolution};
            identity
                .validate()
                .map_err(|_| SourceRepositoryError::Conflict)?;
            if identity.journal != floe_connections::ConnectionsCommandJournal::SourceOperation {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(source_connection_failure)?;
            begin_source_transaction(&mut writer, &connection).await?;
            let result=async {
                let mut rows=connection.query("SELECT operation_id,command_id,person_id,connection_id,revision,fence,payload FROM source_operations WHERE operation_id=? OR (person_id=? AND command_id=?)",
                    (identity.record_ref.to_string(),identity.person_id.to_string(),identity.command_id.to_string())).await.map_err(source_database_failure)?;
                if let Some(row)=rows.next().await.map_err(source_database_failure)? {
                    let operation=decode_operation(&row)?;
                    if operation.operation_id != identity.record_ref || operation.command_id != identity.command_id
                        || operation.expected.source.person_id() != identity.person_id || operation.device_id != identity.device_id
                        || operation.request_digest != identity.intent_digest { return Err(SourceRepositoryError::Conflict); }
                    return Ok(ConnectionsCommandResolution::Admitted);
                }
                drop(rows);
                if let Some(receipt)=source_command_rejection_on(&connection,identity.person_id,identity.command_id).await? {
                    if receipt.identity != identity { return Err(SourceRepositoryError::Conflict); }
                    return Ok(ConnectionsCommandResolution::NotApplied(receipt.reason));
                }
                let receipt=ConnectionsCommandRejection{identity,reason};
                let payload=serde_json::to_string(&receipt).map_err(|_|SourceRepositoryError::Corrupt)?;
                if payload.len()>MAX_OPERATION_BYTES {return Err(SourceRepositoryError::Corrupt);}
                connection.execute("INSERT INTO source_command_rejections VALUES(?,?,?)",(receipt.identity.person_id.to_string(),receipt.identity.command_id.to_string(),payload)).await.map_err(source_database_failure)?;
                Ok(ConnectionsCommandResolution::NotApplied(reason))
            }.await;
            let outcome = finish_source_transaction(self, &connection, result).await;
            writer.settled();
            outcome
        })
    }

    fn reserve<'a>(
        &'a self,
        request: SourceOperationReservation,
    ) -> BoxFuture<'a, Result<SourceOperationAdmission, SourceRepositoryError>> {
        Box::pin(async move {
            let mut writer = self
                .source_write_guard()
                .map_err(source_connection_failure)?;
            let requested = request.record;
            requested.validate()?;
            if requested.revision != 1 || requested.phase != SourceOperationPhase::Reserved
                || requested.kind == floe_connections::SourceOperationKind::ConnectionPresentation {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(source_connection_failure)?;
            begin_source_transaction(&mut writer, &connection).await?;
            let result = async {
            if source_command_rejection_on(&connection,requested.expected.source.person_id(),requested.command_id).await?.is_some() {
                return Err(SourceRepositoryError::Conflict);
            }
            let mut replay_rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE operation_id = ? OR (person_id = ? AND command_id = ?)",
                (requested.operation_id.to_string(), requested.expected.source.person_id().to_string(), requested.command_id.to_string())).await.map_err(source_database_failure)?;
            if let Some(row) = replay_rows.next().await.map_err(source_database_failure)? {
                let current = decode_operation(&row)?;
                if replay_rows.next().await.map_err(source_database_failure)?.is_some() { return Err(SourceRepositoryError::Conflict); }
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
                (requested.operation_id.to_string(),requested.command_id.to_string(),requested.expected.source.person_id().to_string(),requested.expected.source.connection_id().as_str().to_owned(),payload)).await.map_err(source_database_failure)?;
            if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            Ok(SourceOperationAdmission { record: requested, replayed: false })
        }.await;
            let outcome = finish_source_transaction(self, &connection, result).await;
            writer.settled();
            outcome
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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            operation_on(&connection, id).await
        })
    }
    fn compare_and_swap_operation<'a>(
        &'a self,
        change: SourceOperationChange,
    ) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>> {
        Box::pin(async move {
            let mut writer = self
                .source_write_guard()
                .map_err(source_connection_failure)?;
            let connection = self.connection().await.map_err(source_connection_failure)?;
            begin_source_transaction(&mut writer, &connection).await?;
            let result = async {
            let current = operation_on(&connection, change.operation_id).await?.ok_or(SourceRepositoryError::Conflict)?;
            let source = source_on(&connection, &current.expected.source.connection_id()).await?;
            let next = change.validate(&current, source.as_ref())?;
            if next == current { return Ok(current); }
            if let Some(successor) = &change.successor {
                let payload = serde_json::to_string(successor).map_err(|_| SourceRepositoryError::Corrupt)?;
                if payload.len() > MAX_SOURCE_PAYLOAD_BYTES { return Err(SourceRepositoryError::Corrupt); }
                let changed = if let Some(source) = source {
                    connection.execute("UPDATE source_connections SET revision = ?, payload = ? WHERE connection_id = ? AND revision = ?",
                        (successor.revision() as i64,payload,successor.connection_id().as_str(),source.revision() as i64)).await.map_err(source_database_failure)?
                } else {
                    connection.execute("INSERT OR IGNORE INTO source_connections(connection_id,person_id,connector_id,revision,payload) VALUES (?,?,?,?,?)",
                        (successor.connection_id().as_str(),successor.person_id().to_string(),successor.connector_id().as_str(),successor.revision() as i64,payload)).await.map_err(source_database_failure)?
                };
                if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            }
            let payload = serde_json::to_string(&next).map_err(|_| SourceRepositoryError::Corrupt)?;
            if payload.len() > MAX_OPERATION_BYTES { return Err(SourceRepositoryError::Corrupt); }
            let changed = connection.execute("UPDATE source_operations SET revision = ?, fence = ?, payload = ? WHERE operation_id = ? AND revision = ?",
                (next.revision as i64,i64::from(next.phase.holds_fence()),payload,next.operation_id.to_string(),current.revision as i64)).await.map_err(source_database_failure)?;
            if changed != 1 { return Err(SourceRepositoryError::Conflict); }
            Ok(next)
        }.await;
            let outcome = finish_source_transaction(self, &connection, result).await;
            writer.settled();
            outcome
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
            let connection = self.connection().await.map_err(source_connection_failure)?;
            let mut rows = connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, payload FROM source_operations WHERE person_id = ? AND fence = 1 ORDER BY operation_id LIMIT ?", (person_id.to_string(),limit as i64)).await.map_err(source_database_failure)?;
            let mut records = Vec::new();
            while let Some(row) = rows.next().await.map_err(source_database_failure)? {
                records.push(decode_operation(&row)?);
            }
            Ok(records)
        })
    }
    fn read_reservation_fence<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<SourceReservationFence, SourceRepositoryError>> {
        Box::pin(async move {
            if !person_id.is_valid() {
                return Err(SourceRepositoryError::Conflict);
            }
            let connection = self.connection().await.map_err(source_connection_failure)?;
            connection
                .execute("BEGIN", ())
                .await
                .map_err(source_database_failure)?;
            let result=async {
                // New reservations are inserted with fence=1 in one immediate
                // transaction. Rows are retained; command replay never inserts.
                let mut rows=connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, CASE WHEN length(CAST(payload AS BLOB)) <= 16384 THEN payload ELSE NULL END FROM source_operations WHERE connection_id = ? ORDER BY rowid DESC LIMIT 1",(connection_id.as_str(),)).await.map_err(source_database_failure)?;
                let latest=rows.next().await.map_err(source_database_failure)?.as_ref().map(decode_operation).transpose()?;
                drop(rows);
                let mut rows=connection.query("SELECT operation_id, command_id, person_id, connection_id, revision, fence, CASE WHEN length(CAST(payload AS BLOB)) <= 16384 THEN payload ELSE NULL END FROM source_operations WHERE connection_id = ? AND fence = 1",(connection_id.as_str(),)).await.map_err(source_database_failure)?;
                let active=rows.next().await.map_err(source_database_failure)?.as_ref().map(decode_operation).transpose()?;
                if rows.next().await.map_err(source_database_failure)?.is_some(){return Err(SourceRepositoryError::Corrupt);}
                let fence=match latest {
                    None if active.is_none()=>SourceReservationFence{watermark:SourceReservationWatermark::NeverReserved,fenced:false},
                    Some(record)=>{
                        if record.expected.source.person_id()!=person_id || record.expected.source.connection_id()!=*connection_id
                            || active.as_ref().is_some_and(|operation|operation.operation_id!=record.operation_id)
                            || active.is_some()!=record.phase.holds_fence(){return Err(SourceRepositoryError::Corrupt);}
                        SourceReservationFence{watermark:SourceReservationWatermark::Reserved{
                            operation_id:record.operation_id,reservation_id:record.reservation_id,reservation_generation:record.reservation_generation},
                            fenced:record.phase.holds_fence()}
                    },
                    _=>return Err(SourceRepositoryError::Corrupt),
                };
                fence.validate()?;
                Ok(fence)
            }.await;
            finish_source_transaction(self, &connection, result).await
        })
    }
    fn source_is_fenced<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<bool, SourceRepositoryError>> {
        Box::pin(async move {
            let connection = self.connection().await.map_err(source_connection_failure)?;
            fenced_on(&connection, person_id, connection_id).await
        })
    }
}

async fn source_command_rejection_on(
    connection: &turso::Connection,
    person: PersonId,
    command_id: Uuid,
) -> Result<Option<floe_connections::ConnectionsCommandRejection>, SourceRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT payload FROM source_command_rejections WHERE person_id=? AND command_id=?",
            (person.to_string(), command_id.to_string()),
        )
        .await
        .map_err(source_database_failure)?;
    let Some(row) = rows.next().await.map_err(source_database_failure)? else {
        return Ok(None);
    };
    let payload = row.get::<String>(0).map_err(storage_error)?;
    if payload.len() > MAX_OPERATION_BYTES {
        return Err(SourceRepositoryError::Corrupt);
    }
    let receipt: floe_connections::ConnectionsCommandRejection =
        serde_json::from_str(&payload).map_err(|_| SourceRepositoryError::Corrupt)?;
    receipt
        .identity
        .validate()
        .map_err(|_| SourceRepositoryError::Corrupt)?;
    if receipt.identity.person_id != person
        || receipt.identity.command_id != command_id
        || receipt.identity.journal != floe_connections::ConnectionsCommandJournal::SourceOperation
    {
        return Err(SourceRepositoryError::Corrupt);
    }
    Ok(Some(receipt))
}

fn source_database_failure(error: turso::Error) -> SourceRepositoryError {
    match crate::vault::database_failure(error) {
        floe_kernel::AgentFailure::StorageBusy => SourceRepositoryError::StorageBusy,
        _ => SourceRepositoryError::StorageUnavailable,
    }
}
