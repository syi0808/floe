use std::{fs::OpenOptions, io::Read, path::Path, sync::Arc};
use turso::core::{Clock, IO};

use turso::{Builder, Connection};

use crate::{StoreError, StoreErrorCode};

#[path = "schema_sql.rs"]
mod schema_sql;

pub struct TursoStore {
    database: turso::Database,
    // Drop after the database. Every retained repository Arc keeps the same
    // installation admission alive, including background owner retirement.
    installation_lock: Option<std::fs::File>,
}

impl TursoStore {
    pub fn with_installation_lock(mut self, lock: std::fs::File) -> Self {
        self.installation_lock = Some(lock);
        self
    }

    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        match Self::create_new(path.as_ref()).await {
            Err(error) if error.code == StoreErrorCode::Conflict => Self::open_existing(path).await,
            result => result,
        }
    }

    /// Create only a provably new plain store. Existing files are never
    /// initialized, truncated or adopted by this entry point.
    pub async fn create_new(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => drop(file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StoreError::new(
                    StoreErrorCode::Conflict,
                    "local database already exists",
                ));
            }
            Err(error) => return Err(storage_error(error)),
        }
        let path_text = path.to_str().ok_or_else(|| {
            StoreError::new(StoreErrorCode::Validation, "local database path is invalid")
        })?;
        let database = Builder::new_local(path_text)
            .build()
            .await
            .map_err(storage_error)?;
        let store = Self {
            database,
            installation_lock: None,
        };
        store.initialize().await?;
        store.validate_existing().await?;
        // Finish the newly created main file before installation.ready can be
        // published. Do not rely on schema pages remaining only in the WAL.
        let connection = store.connection().await?;
        let mut rows = connection
            .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
            .await
            .map_err(storage_error)?;
        let row = rows.next().await.map_err(storage_error)?.ok_or_else(|| {
            StoreError::new(
                StoreErrorCode::Storage,
                "new database checkpoint returned no result",
            )
        })?;
        if row.get::<i64>(0).map_err(storage_error)? != 0
            || rows.next().await.map_err(storage_error)?.is_some()
        {
            return Err(StoreError::new(
                StoreErrorCode::Storage,
                "new database checkpoint did not complete",
            ));
        }
        drop(rows);
        drop(connection);
        std::fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(storage_error)?;
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(storage_error)?;
        }
        Ok(store)
    }

    /// Open an explicitly selected, already supported profile. The main file
    /// is opened without Create and retained by the IO adapter through both
    /// validation and ordinary use; no missing-file race can create a profile.
    pub async fn open_existing(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = std::fs::canonicalize(path.as_ref()).map_err(|_| {
            StoreError::new(
                StoreErrorCode::NotFound,
                "selected profile database is missing",
            )
        })?;
        let mut probe = std::fs::File::open(&path).map_err(storage_error)?;
        if !probe.metadata().map_err(storage_error)?.is_file() {
            return Err(unsupported_profile());
        }
        if probe.metadata().map_err(storage_error)?.len() < 16 {
            return Err(unsupported_profile());
        }
        let mut header = [0u8; 16];
        probe.read_exact(&mut header).map_err(storage_error)?;
        if &header != b"SQLite format 3\0" {
            return Err(unsupported_profile());
        }
        let path = path.to_str().ok_or_else(unsupported_profile)?.to_owned();
        let io = Arc::new(ExistingProfileIo::new(path.clone()).map_err(storage_error)?);
        let readonly = Self {
            database: Builder::new_local(&path)
                .with_io_impl(io.clone())
                .read_only(true)
                .build()
                .await
                .map_err(admission_error)?,
            installation_lock: None,
        };
        readonly.validate_existing().await?;
        drop(readonly);
        let store = Self {
            database: Builder::new_local(&path)
                .with_io_impl(io)
                .build()
                .await
                .map_err(admission_error)?,
            installation_lock: None,
        };
        // Recheck the same pinned file after the writable engine is opened.
        store.validate_existing().await?;
        Ok(store)
    }
    async fn validate_existing(&self) -> Result<(), StoreError> {
        let connection = self.database.connect().map_err(admission_error)?;
        // Successfully read metadata or a typed corruption result is evidence;
        // generic query, busy and I/O errors cannot authorize development reset.
        require_schema(&connection,"floe_source_schema","CREATE TABLE floe_source_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))").await?;
        let mut rows = connection
            .query("SELECT version FROM floe_source_schema WHERE id = 1", ())
            .await
            .map_err(admission_error)?;
        let row = rows
            .next()
            .await
            .map_err(admission_error)?
            .ok_or_else(unsupported_profile)?;
        if row.get::<i64>(0).map_err(admission_error)? != 1
            || rows.next().await.map_err(admission_error)?.is_some()
        {
            return Err(unsupported_profile());
        }
        drop(rows);
        for table in ["calendar_actions", "action_authorities"] {
            require_schema(&connection,table,&format!("CREATE TABLE {table} (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)")).await?;
            require_schema(
                &connection,
                &format!("{table}_person"),
                &format!("CREATE INDEX {table}_person ON {table}(person_id)"),
            )
            .await?;
        }
        crate::repositories::validate_day_schema(&connection).await?;
        require_schema(&connection,"source_connections","CREATE TABLE source_connections (connection_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector_id TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL)").await?;
        require_schema(&connection,"source_connections_person_connector","CREATE INDEX source_connections_person_connector ON source_connections(person_id, connector_id)").await?;
        require_schema(&connection,"source_operations","CREATE TABLE source_operations (operation_id TEXT PRIMARY KEY, command_id TEXT NOT NULL, person_id TEXT NOT NULL, connection_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), fence INTEGER NOT NULL CHECK(fence IN (0,1)), payload TEXT NOT NULL, UNIQUE(person_id,command_id))").await?;
        require_schema(&connection,"source_operation_fence","CREATE UNIQUE INDEX source_operation_fence ON source_operations(connection_id) WHERE fence = 1").await?;
        require_schema(
            &connection,
            "source_operation_history",
            "CREATE INDEX source_operation_history ON source_operations(connection_id)",
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn connection(&self) -> Result<Connection, StoreError> {
        self.database.connect().map_err(storage_error)
    }

    async fn initialize(&self) -> Result<(), StoreError> {
        let connection = self.connection().await?;
        connection.execute("CREATE TABLE IF NOT EXISTS floe_source_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",()).await.map_err(storage_error)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO floe_source_schema(id,version) VALUES (1,1)",
                (),
            )
            .await
            .map_err(storage_error)?;
        for table in ["calendar_actions", "action_authorities"] {
            connection.execute(
                &format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)"),
                (),
            ).await.map_err(storage_error)?;
            connection
                .execute(
                    &format!("CREATE INDEX IF NOT EXISTS {table}_person ON {table}(person_id)"),
                    (),
                )
                .await
                .map_err(storage_error)?;
        }
        crate::repositories::initialize_day_schema(&connection).await?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS source_connections (connection_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector_id TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL)",
            (),
        ).await.map_err(storage_error)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS source_connections_person_connector ON source_connections(person_id, connector_id)",
            (),
        ).await.map_err(storage_error)?;
        crate::repositories::initialize_source_operations(&connection)
            .await
            .map_err(storage_error)?;
        Ok(())
    }
}

pub(crate) fn storage_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::new(StoreErrorCode::Storage, error.to_string())
}

/// Only admission uses typed corruption as development-reset evidence. Generic
/// engine errors, busy, permission and I/O failures remain unavailable storage.
pub(crate) fn admission_error(error: turso::Error) -> StoreError {
    match error {
        turso::Error::Corrupt(_) | turso::Error::NotAdb(_) => StoreError::new(
            StoreErrorCode::Validation,
            "local database contains corrupt stored data",
        ),
        error => storage_error(error),
    }
}

pub(crate) fn unsupported_profile() -> StoreError {
    StoreError::new(
        StoreErrorCode::Validation,
        "selected profile database schema is unsupported",
    )
}
pub(crate) async fn require_schema(
    connection: &Connection,
    name: &str,
    expected: &str,
) -> Result<(), StoreError> {
    let mut rows = connection
        .query(
            "SELECT sql FROM sqlite_master WHERE name = ? AND type IN ('table','index')",
            (name,),
        )
        .await
        .map_err(admission_error)?;
    let row = rows
        .next()
        .await
        .map_err(admission_error)?
        .ok_or_else(|| schema_mismatch(name, "missing_object"))?;
    let sql: String = row.get(0).map_err(admission_error)?;
    if rows.next().await.map_err(admission_error)?.is_some() {
        return Err(schema_mismatch(name, "duplicate_object"));
    }
    match schema_sql::compare(&sql, expected) {
        schema_sql::Comparison::Equivalent => Ok(()),
        schema_sql::Comparison::Different => Err(schema_mismatch(name, "definition_mismatch")),
        schema_sql::Comparison::InvalidStored => Err(schema_mismatch(name, "invalid_definition")),
        schema_sql::Comparison::InvalidExpected => Err(StoreError::new(
            StoreErrorCode::Storage,
            "internal schema definition is invalid",
        )
        .with_metadata("schema_object", name)),
    }
}

fn schema_mismatch(name: &str, reason: &str) -> StoreError {
    unsupported_profile()
        .with_metadata("schema_object", name)
        .with_metadata("schema_mismatch", reason)
}
/// Turso's high-level Builder defaults to Create. Retaining a preopened main
/// file makes existing-profile admission independent of that default.
struct ExistingProfileIo {
    path: String,
    inner: turso::core::PlatformIO,
    main: Arc<dyn turso::core::File>,
    identity: turso::core::io::FileId,
}
impl ExistingProfileIo {
    fn new(path: String) -> turso::core::Result<Self> {
        let inner = turso::core::PlatformIO::new()?;
        let identity = inner.file_id(&path)?;
        let main = inner.open_file(&path, turso::core::OpenFlags::None, false)?;
        if identity != inner.file_id(&path)? {
            return Err(turso::core::LimboError::InternalError(
                "selected profile changed during open".into(),
            ));
        }
        Ok(Self {
            path,
            inner,
            main,
            identity,
        })
    }
}
impl Clock for ExistingProfileIo {
    fn current_time_monotonic(&self) -> turso::core::MonotonicInstant {
        self.inner.current_time_monotonic()
    }
    fn current_time_wall_clock(&self) -> turso::core::WallClockInstant {
        self.inner.current_time_wall_clock()
    }
}
impl IO for ExistingProfileIo {
    fn open_file(
        &self,
        path: &str,
        flags: turso::core::OpenFlags,
        direct: bool,
    ) -> turso::core::Result<Arc<dyn turso::core::File>> {
        if path == self.path {
            return Ok(self.main.clone());
        }
        self.inner.open_file(path, flags, direct)
    }
    fn remove_file(&self, path: &str) -> turso::core::Result<()> {
        if path == self.path {
            return Err(turso::core::LimboError::InternalError(
                "cannot remove selected profile".into(),
            ));
        }
        self.inner.remove_file(path)
    }
    fn file_id(&self, path: &str) -> turso::core::Result<turso::core::io::FileId> {
        if path == self.path {
            Ok(self.identity)
        } else {
            self.inner.file_id(path)
        }
    }
    fn supports_shared_wal_coordination(&self) -> bool {
        self.inner.supports_shared_wal_coordination()
    }
    fn step(&self) -> turso::core::Result<()> {
        self.inner.step()
    }
    fn cancel(&self, completions: &[turso::core::Completion]) -> turso::core::Result<()> {
        self.inner.cancel(completions)
    }
    fn drain_completions(
        &self,
        completions: &[turso::core::Completion],
    ) -> turso::core::Result<()> {
        self.inner.drain_completions(completions)
    }
}
