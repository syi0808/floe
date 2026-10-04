use std::{fs::OpenOptions, path::Path, sync::Arc};
use turso::core::{Clock, IO};

use turso::{Builder, Connection};

use crate::{ProductStoreIdentity, RootKey, StoreError, StoreErrorCode};

pub struct TursoStore {
    database: turso::Database,
    identity: ProductStoreIdentity,
    _key: RootKey,
    // Drop after the database. Every retained repository Arc keeps the same
    // installation admission alive, including background owner retirement.
    installation_lock: Option<std::fs::File>,
}

impl TursoStore {
    pub fn with_installation_lock(mut self, lock: std::fs::File) -> Self {
        self.installation_lock = Some(lock);
        self
    }

    /// Create only a provably new encrypted product store. Existing files are never
    /// initialized, truncated or adopted by this entry point.
    pub async fn create_new(
        path: impl AsRef<Path>,
        identity: ProductStoreIdentity,
    ) -> Result<Self, StoreError> {
        let supplied = path.as_ref();
        let parent = std::fs::canonicalize(supplied.parent().ok_or_else(unsupported_profile)?)
            .map_err(storage_error)?;
        let path = parent.join(supplied.file_name().ok_or_else(unsupported_profile)?);
        let path = path.as_path();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        match options.open(path) {
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
        let key = crate::product_keys::create(path, &identity)?;
        let database = encrypted_builder(path_text, &key)
            .build()
            .await
            .map_err(storage_error)?;
        let store = Self {
            database,
            identity,
            _key: key,
            installation_lock: None,
        };
        let mut connection = store.connection().await?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(admission_error)?;
        let initialized = async {
            crate::schema::create(&transaction, crate::schema::Layout::Product)
                .await
                .map_err(|failure| failure.during_creation().into_store())?;
            transaction
                .execute(
                    "INSERT INTO product_store_identity VALUES(1,?,?)",
                    (
                        store.identity.person.to_string(),
                        store.identity.device.clone(),
                    ),
                )
                .await
                .map_err(admission_error)?;
            crate::schema::inspect(&transaction, crate::schema::Layout::Product)
                .await
                .map_err(|failure| failure.during_creation().into_store())
        }
        .await;
        match initialized {
            Ok(()) => transaction.commit().await.map_err(admission_error)?,
            Err(error) => {
                transaction.rollback().await.map_err(admission_error)?;
                return Err(error);
            }
        }
        drop(connection);
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
    pub async fn open_existing(
        path: impl AsRef<Path>,
        identity: ProductStoreIdentity,
    ) -> Result<Self, StoreError> {
        let path = std::fs::canonicalize(path.as_ref()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StoreError::new(
                    StoreErrorCode::NotFound,
                    "selected profile database is missing",
                )
            } else {
                storage_error(error)
            }
        })?;
        let probe = std::fs::File::open(&path).map_err(storage_error)?;
        if !probe.metadata().map_err(storage_error)?.is_file() {
            return Err(unsupported_profile());
        }
        if probe.metadata().map_err(storage_error)?.len() < 16 {
            return Err(unsupported_profile());
        }
        let key = crate::product_keys::load(&path, &identity)?;
        let path = path
            .to_str()
            .ok_or_else(|| {
                StoreError::new(StoreErrorCode::Validation, "local database path is invalid")
            })?
            .to_owned();
        let io = Arc::new(ExistingProfileIo::new(path.clone()).map_err(storage_error)?);
        let readonly = encrypted_builder(&path, &key)
            .with_io_impl(io.clone())
            .read_only(true)
            .build()
            .await
            .map_err(admission_error)?;
        validate_existing_database(&readonly, &identity).await?;
        drop(readonly);
        let store = Self {
            database: encrypted_builder(&path, &key)
                .with_io_impl(io)
                .build()
                .await
                .map_err(admission_error)?,
            identity,
            _key: key,
            installation_lock: None,
        };
        // Recheck the same pinned file after the writable engine is opened.
        store.validate_existing().await?;
        Ok(store)
    }
    async fn validate_existing(&self) -> Result<(), StoreError> {
        validate_existing_database(&self.database, &self.identity).await
    }

    pub(crate) async fn connection(&self) -> Result<Connection, StoreError> {
        self.database.connect().map_err(storage_error)
    }
}

fn encrypted_builder(path: &str, key: &RootKey) -> Builder {
    Builder::new_local(path)
        .experimental_encryption(true)
        .with_encryption(turso::EncryptionOpts {
            cipher: "aes256gcm".into(),
            hexkey: key.hex(),
        })
}
async fn validate_existing_database(
    database: &turso::Database,
    identity: &ProductStoreIdentity,
) -> Result<(), StoreError> {
    let connection = database.connect().map_err(admission_error)?;
    crate::schema::inspect(&connection, crate::schema::Layout::Product)
        .await
        .map_err(crate::schema::SchemaFailure::into_store)?;
    let mut rows = connection
        .query(
            "SELECT person_id,device_id FROM product_store_identity WHERE id=1",
            (),
        )
        .await
        .map_err(admission_error)?;
    let row = rows
        .next()
        .await
        .map_err(admission_error)?
        .ok_or_else(unsupported_profile)?;
    if row.get::<String>(0).map_err(admission_error)? != identity.person.to_string()
        || row.get::<String>(1).map_err(admission_error)? != identity.device
        || rows.next().await.map_err(admission_error)?.is_some()
    {
        return Err(StoreError::new(
            StoreErrorCode::Validation,
            "product store identity does not match installation",
        ));
    }
    Ok(())
}

pub(crate) fn storage_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::new(StoreErrorCode::Storage, error.to_string())
}

/// Admission preserves typed failures; no error grants reset authority.
pub(crate) fn admission_error(error: turso::Error) -> StoreError {
    crate::schema::SchemaFailure::from_database(error).into_store()
}

pub(crate) fn unsupported_profile() -> StoreError {
    StoreError::new(
        StoreErrorCode::UnsupportedSchema,
        "selected profile database schema is unsupported",
    )
}

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
