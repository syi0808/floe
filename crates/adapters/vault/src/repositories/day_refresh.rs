//! Day refresh persistence and the source/mirror compare-and-swap boundary.

use crate::TursoStore;
use floe_connections::{SourceConnection, SourceState};
use floe_context_contract::GrantSourceBinding;
use floe_day::{
    CalendarAcquisition, CalendarMirror, CalendarSelection, CalendarSourceOutcome,
    CalendarSourceVersion, DayError, DayRefreshFailure, DayRefreshState, MAX_REFRESH_SOURCES,
    MirrorExpectation, RefreshAdmission, RefreshAdmissionResult, RefreshCommit,
    RefreshExecutorReplacement, RefreshLookup, RefreshRecord, RefreshTransition,
};
use floe_execution::BoxFuture;
use floe_kernel::{CommandFailure, PersonId};
use sha2::{Digest, Sha256};
use turso::{Connection, Row};

const MAX_SOURCE_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
const REFRESH_COLUMNS: &str =
    "operation_id, person_id, device_id, command_id, executor_generation, revision, payload";

fn calendar_source_query(person_id: PersonId) -> (String, Vec<turso::Value>) {
    let bindings = floe_access::supported_product_calendar_bindings();
    let placeholders = vec!["?"; bindings.len()].join(", ");
    let query = format!(
        "SELECT connection_id, person_id, connector_id, revision, payload FROM source_connections WHERE person_id = ? AND connector_id IN ({placeholders}) ORDER BY connection_id"
    );
    let mut parameters: Vec<turso::Value> = Vec::with_capacity(bindings.len() + 1);
    parameters.push(person_id.to_string().into());
    parameters.extend(
        bindings
            .iter()
            .map(|binding| binding.connector_id.to_owned().into()),
    );
    (query, parameters)
}

pub(super) struct CurrentCalendarSource {
    pub(super) source: SourceConnection,
    pub(super) version: CalendarSourceVersion,
}

fn storage_error(error: impl std::fmt::Display) -> DayError {
    DayError::storage(error.to_string())
}

fn conflict(message: impl Into<String>) -> DayError {
    DayError::conflict(message)
}

pub(super) async fn finish_transaction<T>(
    connection: &Connection,
    result: Result<T, DayError>,
) -> Result<T, DayError> {
    match result {
        Ok(value) => match connection.execute("COMMIT", ()).await {
            Ok(_) => Ok(value),
            Err(error) => {
                let _ = connection.execute("ROLLBACK", ()).await;
                Err(storage_error(error))
            }
        },
        Err(error) => {
            // A definite rejection is returned only after its transaction is
            // known to have rolled back. Otherwise clients retain uncertainty.
            connection
                .execute("ROLLBACK", ())
                .await
                .map_err(storage_error)?;
            Err(error)
        }
    }
}

/// Finish a command transaction without losing whether its exact receipt was
/// committed. Rejection is definitive only after rollback succeeds; any COMMIT
/// failure remains uncertain even when a best-effort rollback is attempted.
pub(super) async fn finish_command_transaction<T>(
    connection: &Connection,
    result: Result<T, CommandFailure<DayError>>,
) -> Result<T, CommandFailure<DayError>> {
    match result {
        Ok(value) => match connection.execute("COMMIT", ()).await {
            Ok(_) => Ok(value),
            Err(error) => {
                let _ = connection.execute("ROLLBACK", ()).await;
                Err(CommandFailure::Indeterminate(storage_error(error)))
            }
        },
        Err(failure) => {
            connection
                .execute("ROLLBACK", ())
                .await
                .map_err(|error| CommandFailure::Indeterminate(storage_error(error)))?;
            Err(failure)
        }
    }
}

async fn refresh_by_id(
    connection: &Connection,
    operation_id: uuid::Uuid,
) -> Result<Option<(RefreshRecord, String)>, DayError> {
    let mut rows = connection
        .query(
            &format!("SELECT {REFRESH_COLUMNS} FROM day_refreshes WHERE operation_id = ?"),
            (operation_id.to_string(),),
        )
        .await
        .map_err(storage_error)?;
    let Some(row) = rows.next().await.map_err(storage_error)? else {
        return Ok(None);
    };
    let current = decode_refresh_row(&row)?;
    if rows.next().await.map_err(storage_error)?.is_some() {
        return Err(storage_error("duplicate Day refresh operation identity"));
    }
    drop(rows);
    Ok(Some(current))
}

fn decode_refresh_row(row: &Row) -> Result<(RefreshRecord, String), DayError> {
    let payload: String = row.get(6).map_err(storage_error)?;
    if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES + 16_384 {
        return Err(storage_error("Day refresh byte budget"));
    }
    let record: RefreshRecord = serde_json::from_str(&payload).map_err(storage_error)?;
    record
        .validate()
        .map_err(|_| storage_error("invalid persisted Day refresh record"))?;
    let operation_id: String = row.get(0).map_err(storage_error)?;
    let person_id: String = row.get(1).map_err(storage_error)?;
    let device_id: String = row.get(2).map_err(storage_error)?;
    let command_id: String = row.get(3).map_err(storage_error)?;
    let executor_generation: String = row.get(4).map_err(storage_error)?;
    let revision: i64 = row.get(5).map_err(storage_error)?;
    if revision <= 0
        || record.operation_id.to_string() != operation_id
        || record.person_id.to_string() != person_id
        || record.device_id != device_id
        || record.command_id.to_string() != command_id
        || record.executor_generation.to_string() != executor_generation
        || record.revision.0 != revision as u64
    {
        return Err(storage_error("Day refresh row does not match its record"));
    }
    Ok((record, payload))
}

pub(super) async fn mirror_on(
    connection: &Connection,
    person_id: PersonId,
) -> Result<Option<(CalendarMirror, String)>, DayError> {
    let mut rows = connection
        .query(
            "SELECT payload FROM calendar_mirrors WHERE id = ? AND person_id = ?",
            (person_id.to_string(), person_id.to_string()),
        )
        .await
        .map_err(storage_error)?;
    let Some(row) = rows.next().await.map_err(storage_error)? else {
        return Ok(None);
    };
    let payload: String = row.get(0).map_err(storage_error)?;
    if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES {
        return Err(storage_error("Day mirror byte budget"));
    }
    let mirror = serde_json::from_str(&payload).map_err(storage_error)?;
    if rows.next().await.map_err(storage_error)?.is_some() {
        return Err(storage_error("duplicate Day calendar mirror identity"));
    }
    drop(rows);
    Ok(Some((mirror, payload)))
}

pub(super) async fn persist_mirror_on(
    connection: &Connection,
    person_id: PersonId,
    expectation: MirrorExpectation,
    current: Option<(CalendarMirror, String)>,
    mirror: &CalendarMirror,
    payload: String,
) -> Result<(), DayError> {
    if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES
        || mirror.events.len() > floe_day::MAX_DAY_SNAPSHOT_ITEMS
    {
        return Err(DayError::budget("Day mirror budget"));
    }
    if mirror.mirror_revision == 0 {
        return Err(DayError::validation("invalid calendar mirror revision"));
    }
    match (expectation, current) {
        (MirrorExpectation::Absent, None) => {
            let changed = connection
                .execute(
                    "INSERT OR IGNORE INTO calendar_mirrors(id, person_id, payload) VALUES (?, ?, ?)",
                    (person_id.to_string(), person_id.to_string(), payload),
                )
                .await
                .map_err(storage_error)?;
            if changed == 1 {
                Ok(())
            } else {
                Err(conflict("calendar mirror changed during refresh"))
            }
        }
        (MirrorExpectation::Present(expected), Some((current, stored))) => {
            if MirrorExpectation::of(Some(&current))? != MirrorExpectation::Present(expected) {
                return Err(conflict("calendar mirror revision changed during refresh"));
            }
            let changed = connection
                .execute(
                    "UPDATE calendar_mirrors SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?",
                    (
                        payload,
                        person_id.to_string(),
                        person_id.to_string(),
                        stored,
                    ),
                )
                .await
                .map_err(storage_error)?;
            if changed == 1 {
                Ok(())
            } else {
                Err(conflict("calendar mirror changed during refresh"))
            }
        }
        _ => Err(conflict("calendar mirror presence changed during refresh")),
    }
}

fn calendar_source_version(source: &SourceConnection) -> Result<CalendarSourceVersion, DayError> {
    source.validate().map_err(storage_error)?;
    let provider = floe_access::supported_product_calendar_binding(source.connector_id().as_str())
        .map(|binding| binding.provider)
        .ok_or_else(|| storage_error("unexpected connector in Calendar inventory"))?;
    let binding = GrantSourceBinding::try_new(
        source.person_id(),
        source.connection_id().clone(),
        source.connector_id().clone(),
        source.execution_owner_id().clone(),
    )
    .map_err(storage_error)?;
    let mut calendars = source
        .resources()
        .iter()
        .map(|resource| CalendarSelection {
            calendar_id: resource.handle().as_str().to_owned(),
            calendar_name: resource.qualified_label(),
        })
        .collect::<Vec<_>>();
    calendars.sort_by(|left, right| left.calendar_id.cmp(&right.calendar_id));
    let configuration = serde_json::to_vec(source).map_err(storage_error)?;
    let configuration_digest: [u8; 32] = Sha256::digest(configuration).into();
    let version = CalendarSourceVersion {
        source: binding,
        provider,
        revision: floe_kernel::Revision(source.revision()),
        authority: source.source_authority(),
        configuration_digest,
        calendars,
    };
    version.validate(source.person_id())?;
    Ok(version)
}

fn decode_source_row(row: &Row) -> Result<SourceConnection, DayError> {
    let connection_id: String = row.get(0).map_err(storage_error)?;
    let person_id: String = row.get(1).map_err(storage_error)?;
    let connector_id: String = row.get(2).map_err(storage_error)?;
    let revision: i64 = row.get(3).map_err(storage_error)?;
    let payload: String = row.get(4).map_err(storage_error)?;
    if payload.len() > MAX_SOURCE_PAYLOAD_BYTES || revision <= 0 {
        return Err(storage_error("invalid persisted Calendar source row"));
    }
    let source: SourceConnection = serde_json::from_str(&payload).map_err(storage_error)?;
    source.validate().map_err(storage_error)?;
    if source.connection_id().as_str() != connection_id
        || source.person_id().to_string() != person_id
        || source.connector_id().as_str() != connector_id
        || source.revision() != revision as u64
    {
        return Err(storage_error(
            "Calendar source row does not match its configured source",
        ));
    }
    Ok(source)
}

pub(super) async fn current_calendar_sources_on(
    connection: &Connection,
    person_id: PersonId,
) -> Result<Vec<CurrentCalendarSource>, DayError> {
    let (query, parameters) = calendar_source_query(person_id);
    let mut rows = connection
        .query(&query, parameters)
        .await
        .map_err(storage_error)?;
    let mut current = Vec::new();
    while let Some(row) = rows.next().await.map_err(storage_error)? {
        let source = decode_source_row(&row)?;
        if source.person_id() != person_id {
            return Err(storage_error(
                "Calendar inventory contains a foreign Person",
            ));
        }
        if source.state() == SourceState::Disconnected {
            continue;
        }
        if current.len() == MAX_REFRESH_SOURCES {
            return Err(
                conflict("current Calendar source inventory exceeds the maximum of 64")
                    .with_metadata("limit", MAX_REFRESH_SOURCES.to_string()),
            );
        }
        let version = calendar_source_version(&source)?;
        current.push(CurrentCalendarSource { source, version });
    }
    drop(rows);
    current.sort_by(|left, right| {
        left.version
            .source
            .connection_id()
            .cmp(&right.version.source.connection_id())
    });
    Ok(current)
}

pub(super) fn versions_of(sources: &[CurrentCalendarSource]) -> Vec<CalendarSourceVersion> {
    sources
        .iter()
        .map(|source| source.version.clone())
        .collect()
}

fn validate_mirror_inventory(
    mirror: &CalendarMirror,
    current: &[CalendarSourceVersion],
) -> Result<(), DayError> {
    let mirror_sources = mirror
        .state
        .sources
        .iter()
        .map(|source| source.source.clone())
        .collect::<Vec<_>>();
    if mirror_sources != current {
        return Err(DayError::validation(
            "calendar mirror does not contain the complete configured source inventory",
        ));
    }
    Ok(())
}

async fn ensure_successful_sources_unfenced(
    connection: &Connection,
    acquisition: &CalendarAcquisition,
) -> Result<(), DayError> {
    for outcome in &acquisition.sources {
        if !outcome.has_success() {
            continue;
        }
        let connection_id = outcome.source().source.connection_id();
        let mut rows = connection
            .query(
                "SELECT person_id FROM source_operations WHERE connection_id = ? AND fence = 1",
                (connection_id.as_str(),),
            )
            .await
            .map_err(storage_error)?;
        if let Some(row) = rows.next().await.map_err(storage_error)? {
            let person_id: String = row.get(0).map_err(storage_error)?;
            if person_id != acquisition.person_id.to_string() {
                return Err(storage_error(
                    "source operation fence does not match Calendar owner",
                ));
            }
            drop(rows);
            return Err(conflict(
                "a successful Calendar source is held by a pending source operation",
            )
            .with_metadata("connection_id", connection_id.as_str()));
        }
    }
    Ok(())
}

pub(super) async fn require_executor(
    connection: &Connection,
    person_id: PersonId,
    device_id: &str,
    generation: uuid::Uuid,
) -> Result<(), DayError> {
    let mut rows = connection.query("SELECT executor_generation,active FROM day_executors WHERE person_id=? AND device_id=?", (person_id.to_string(),device_id.to_owned())).await.map_err(storage_error)?;
    let row = rows
        .next()
        .await
        .map_err(storage_error)?
        .ok_or_else(|| conflict("Day executor is unavailable"))?;
    if row.get::<String>(0).map_err(storage_error)? != generation.to_string()
        || row.get::<i64>(1).map_err(storage_error)? != 1
        || rows.next().await.map_err(storage_error)?.is_some()
    {
        return Err(conflict("Day executor changed"));
    }
    Ok(())
}

async fn update_refresh_on(
    connection: &Connection,
    previous: &RefreshRecord,
    next: &RefreshRecord,
    stored_payload: &str,
) -> Result<(), DayError> {
    let payload = serde_json::to_string(next).map_err(storage_error)?;
    let changed = connection
        .execute(
            "UPDATE day_refreshes SET revision = ?, payload = ? WHERE operation_id = ? AND person_id = ? AND device_id = ? AND command_id = ? AND executor_generation = ? AND revision = ? AND payload = ?",
            (
                next.revision.0 as i64,
                payload,
                previous.operation_id.to_string(),
                previous.person_id.to_string(),
                previous.device_id.clone(),
                previous.command_id.to_string(),
                previous.executor_generation.to_string(),
                previous.revision.0 as i64,
                stored_payload.to_owned(),
            ),
        )
        .await
        .map_err(storage_error)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(conflict("Day refresh changed; reload and retry"))
    }
}

impl floe_day::DayRefreshRepository for TursoStore {
    fn admit_refresh<'a>(
        &'a self,
        admission: RefreshAdmission,
    ) -> BoxFuture<'a, Result<RefreshAdmissionResult, CommandFailure<DayError>>> {
        Box::pin(async move {
            if admission.command_id.is_nil() {
                return Err(CommandFailure::NotApplied(DayError::validation(
                    "invalid refresh command identity",
                )));
            }
            let connection = self
                .connection()
                .await
                .map_err(|error| CommandFailure::Indeterminate(storage_error(error)))?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(|error| CommandFailure::Indeterminate(storage_error(error)))?;
            let mut prior_command = false;
            let mut replay_checked = false;
            let result = async {
                let mut replay_rows = connection
                    .query(
                        &format!(
                            "SELECT {REFRESH_COLUMNS} FROM day_refreshes WHERE person_id = ? AND command_id = ? ORDER BY operation_id"
                        ),
                        (
                            admission.person_id.to_string(),
                            admission.command_id.to_string(),
                        ),
                    )
                    .await
                    .map_err(storage_error)?;
                let replay_row = replay_rows.next().await.map_err(storage_error)?;
                if replay_row.is_some() {
                    // A corrupt or mismatched receipt is still evidence that
                    // this command id may already have been consumed.
                    prior_command = true;
                }
                let replay = replay_row
                    .as_ref()
                    .map(|row| decode_refresh_row(row))
                    .transpose()?;
                if replay_rows.next().await.map_err(storage_error)?.is_some() {
                    return Err(storage_error(
                        "duplicate Person/command Day refresh replay",
                    ));
                }
                drop(replay_rows);
                if let Some((existing, _)) = replay {
                    let computed_digest = admission.query.refresh_intent_digest(
                        admission.person_id,
                        &admission.device_id,
                        admission.command_id,
                    )?;
                    if existing.device_id == admission.device_id
                        && existing.intent_digest == computed_digest
                    {
                        return Ok(RefreshAdmissionResult::Existing(existing));
                    }
                    return Err(conflict(
                        "Person/command replay has a different device or intent",
                    ));
                }

                let mut other = connection.query("SELECT 1 FROM day_mutation_receipts WHERE person_id=? AND command_id=?", (admission.person_id.to_string(),admission.command_id.to_string())).await.map_err(storage_error)?;
                if other.next().await.map_err(storage_error)?.is_some() { prior_command = true; return Err(conflict("Day command kind changed")); }
                drop(other);
                replay_checked = true;
                let intent_digest = admission.query.refresh_intent_digest(
                    admission.person_id,
                    &admission.device_id,
                    admission.command_id,
                )?;
                let mut counts = connection.query("SELECT (SELECT COUNT(*) FROM day_mutation_receipts WHERE person_id=?1)+(SELECT COUNT(*) FROM day_refreshes WHERE person_id=?1)", (admission.person_id.to_string(),)).await.map_err(storage_error)?;
                let count: i64 = counts.next().await.map_err(storage_error)?.ok_or_else(|| storage_error("missing Day command count"))?.get(0).map_err(storage_error)?;
                if count < 0 { return Err(storage_error("invalid Day command count")); }
                if count as usize >= floe_day::MAX_DAY_COMMAND_RECEIPTS { return Err(DayError::budget("Day command receipt capacity reached")); }
                drop(counts);
                let _ = admission.record(intent_digest, MirrorExpectation::Absent)?;
                require_executor(&connection, admission.person_id, &admission.device_id, admission.executor_generation).await?;
                if refresh_by_id(&connection, admission.operation_id)
                    .await?
                    .is_some()
                {
                    return Err(conflict("Day refresh operation id is already in use"));
                }
                let current = mirror_on(&connection, admission.person_id).await?;
                let expectation = MirrorExpectation::of(
                    current.as_ref().map(|(mirror, _)| mirror),
                )?;
                let record = admission.record(intent_digest, expectation)?;
                let payload = serde_json::to_string(&record).map_err(storage_error)?;
                let changed = connection
                    .execute(
                        "INSERT OR IGNORE INTO day_refreshes(operation_id, person_id, device_id, command_id, executor_generation, revision, payload) VALUES (?, ?, ?, ?, ?, ?, ?)",
                        (
                            record.operation_id.to_string(),
                            record.person_id.to_string(),
                            record.device_id.clone(),
                            record.command_id.to_string(),
                            record.executor_generation.to_string(),
                            record.revision.0 as i64,
                            payload,
                        ),
                    )
                    .await
                    .map_err(storage_error)?;
                if changed != 1 {
                    return Err(conflict(
                        "Day refresh admission raced another command",
                    ));
                }
                Ok(RefreshAdmissionResult::New(record))
            }
            .await;
            let result = result.map_err(|error| {
                if prior_command {
                    CommandFailure::Indeterminate(error)
                } else if replay_checked {
                    CommandFailure::NotApplied(error)
                } else {
                    CommandFailure::Indeterminate(error)
                }
            });
            finish_command_transaction(&connection, result).await
        })
    }

    fn read_refresh<'a>(
        &'a self,
        lookup: RefreshLookup,
    ) -> BoxFuture<'a, Result<Option<RefreshRecord>, DayError>> {
        Box::pin(async move {
            let connection = self.connection().await.map_err(storage_error)?;
            let mut rows = connection
                .query(
                    &format!(
                        "SELECT {REFRESH_COLUMNS} FROM day_refreshes WHERE operation_id = ? AND person_id = ? AND device_id = ?"
                    ),
                    (
                        lookup.operation_id.to_string(),
                        lookup.person_id.to_string(),
                        lookup.device_id,
                    ),
                )
                .await
                .map_err(storage_error)?;
            let Some(row) = rows.next().await.map_err(storage_error)? else {
                return Ok(None);
            };
            let (record, _) = decode_refresh_row(&row)?;
            if rows.next().await.map_err(storage_error)?.is_some() {
                return Err(storage_error("duplicate actor-scoped Day refresh identity"));
            }
            Ok(Some(record))
        })
    }

    fn transition_refresh<'a>(
        &'a self,
        transition: RefreshTransition,
    ) -> BoxFuture<'a, Result<RefreshRecord, DayError>> {
        Box::pin(async move {
            transition.validate()?;
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
                let Some((current, payload)) =
                    refresh_by_id(&connection, transition.previous.operation_id).await?
                else {
                    return Err(conflict("Day refresh no longer exists"));
                };
                if current != transition.previous {
                    return Err(conflict("Day refresh changed; reload and retry"));
                }
                require_executor(
                    &connection,
                    transition.previous.person_id,
                    &transition.previous.device_id,
                    transition.previous.executor_generation,
                )
                .await?;
                update_refresh_on(
                    &connection,
                    &transition.previous,
                    &transition.next,
                    &payload,
                )
                .await?;
                Ok(transition.next)
            }
            .await;
            finish_transaction(&connection, result).await
        })
    }

    fn commit_refresh<'a>(
        &'a self,
        commit: RefreshCommit,
    ) -> BoxFuture<'a, Result<RefreshRecord, DayError>> {
        Box::pin(async move {
            commit.previous.validate()?;
            commit.next.validate()?;
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
                let Some((current_refresh, refresh_payload)) =
                    refresh_by_id(&connection, commit.previous.operation_id).await?
                else {
                    return Err(conflict("Day refresh no longer exists"));
                };
                if current_refresh != commit.previous {
                    return Err(conflict("Day refresh changed; reload and retry"));
                }

                require_executor(
                    &connection,
                    commit.previous.person_id,
                    &commit.previous.device_id,
                    commit.previous.executor_generation,
                )
                .await?;
                let current_mirror = mirror_on(&connection, commit.previous.person_id).await?;
                let actual_expectation =
                    MirrorExpectation::of(current_mirror.as_ref().map(|(mirror, _)| mirror))?;
                if actual_expectation != commit.previous.expected_mirror_revision {
                    return Err(conflict("calendar mirror changed after refresh admission"));
                }

                let current_sources =
                    current_calendar_sources_on(&connection, commit.previous.person_id).await?;
                let current_versions = versions_of(&current_sources);
                if current_versions != commit.acquisition.inventory {
                    return Err(conflict(
                        "configured Calendar inventory changed during refresh",
                    ));
                }
                validate_mirror_inventory(&commit.mirror, &current_versions)?;

                for (source, outcome) in current_sources.iter().zip(&commit.acquisition.sources) {
                    if source.source.state() == SourceState::Pending
                        && !matches!(outcome, CalendarSourceOutcome::Unavailable { .. })
                    {
                        return Err(conflict("Pending Calendar sources must remain unavailable")
                            .with_metadata(
                                "connection_id",
                                source.version.source.connection_id().as_str(),
                            ));
                    }
                }
                ensure_successful_sources_unfenced(&connection, &commit.acquisition).await?;

                commit.validate(
                    current_mirror.as_ref().map(|(mirror, _)| mirror),
                    chrono::Utc::now(),
                )?;
                let mirror_payload =
                    serde_json::to_string(&commit.mirror).map_err(storage_error)?;
                persist_mirror_on(
                    &connection,
                    commit.previous.person_id,
                    commit.previous.expected_mirror_revision,
                    current_mirror,
                    &commit.mirror,
                    mirror_payload,
                )
                .await?;
                update_refresh_on(
                    &connection,
                    &commit.previous,
                    &commit.next,
                    &refresh_payload,
                )
                .await?;
                Ok(commit.next)
            }
            .await;
            finish_transaction(&connection, result).await
        })
    }

    fn interrupt_refreshes<'a>(
        &'a self,
        replacement: RefreshExecutorReplacement,
    ) -> BoxFuture<'a, Result<Vec<RefreshRecord>, DayError>> {
        Box::pin(async move {
            if !replacement.person_id.is_valid()
                || replacement.device_id.is_empty()
                || replacement.device_id.len() > 256
                || replacement.executor_generation.is_nil()
            {
                return Err(DayError::validation(
                    "invalid Day refresh executor replacement",
                ));
            }
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
                connection.execute("INSERT INTO day_executors(person_id,device_id,executor_generation,active) VALUES (?,?,?,1) ON CONFLICT(person_id,device_id) DO UPDATE SET executor_generation=excluded.executor_generation,active=1", (replacement.person_id.to_string(),replacement.device_id.clone(),replacement.executor_generation.to_string())).await.map_err(storage_error)?;
                let mut rows = connection
                    .query(
                        &format!(
                            "SELECT {REFRESH_COLUMNS} FROM day_refreshes WHERE person_id = ? AND device_id = ? ORDER BY operation_id"
                        ),
                        (
                            replacement.person_id.to_string(),
                            replacement.device_id.clone(),
                        ),
                    )
                    .await
                    .map_err(storage_error)?;
                let mut records = Vec::new();
                while let Some(row) = rows.next().await.map_err(storage_error)? {
                    records.push(decode_refresh_row(&row)?);
                }
                drop(rows);

                let mut interrupted = Vec::new();
                for (previous, payload) in records {
                    if previous.executor_generation == replacement.executor_generation
                        || !matches!(
                            &previous.state,
                            DayRefreshState::Pending | DayRefreshState::Running
                        )
                    {
                        continue;
                    }
                    let next = previous.transition(
                        DayRefreshState::Interrupted {
                            failure: DayRefreshFailure::HostInterrupted,
                        },
                        replacement.now,
                    )?;
                    update_refresh_on(&connection, &previous, &next, &payload).await?;
                    interrupted.push(next);
                }
                Ok(interrupted)
            }
            .await;
            finish_transaction(&connection, result).await
        })
    }
    fn retire_refresh_executor<'a>(
        &'a self,
        expected: RefreshExecutorReplacement,
    ) -> BoxFuture<'a, Result<(), DayError>> {
        Box::pin(async move {
            if !expected.person_id.is_valid()
                || expected.device_id.is_empty()
                || expected.executor_generation.is_nil()
            {
                return Err(DayError::validation("invalid Day executor retirement"));
            }
            let connection = self.connection().await.map_err(storage_error)?;
            connection
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(storage_error)?;
            let result = async {
                let changed = connection.execute("UPDATE day_executors SET active=0 WHERE person_id=? AND device_id=? AND executor_generation=? AND active=1", (expected.person_id.to_string(),expected.device_id.clone(),expected.executor_generation.to_string())).await.map_err(storage_error)?;
                if changed == 0 { return Ok(()); }
                let mut rows = connection.query(&format!("SELECT {REFRESH_COLUMNS} FROM day_refreshes WHERE person_id=? AND device_id=? AND executor_generation=?"), (expected.person_id.to_string(),expected.device_id.clone(),expected.executor_generation.to_string())).await.map_err(storage_error)?;
                let mut records = Vec::new(); while let Some(row) = rows.next().await.map_err(storage_error)? { records.push(decode_refresh_row(&row)?); } drop(rows);
                for (previous, payload) in records { if !previous.state.terminal() { let next = previous.transition(DayRefreshState::Interrupted { failure: DayRefreshFailure::HostInterrupted }, expected.now)?; update_refresh_on(&connection, &previous, &next, &payload).await?; } }
                Ok(())
            }.await;
            finish_transaction(&connection, result).await
        })
    }
}
