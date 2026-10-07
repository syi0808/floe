//! Host composition: the services the app host exposes, their lifetime and the
//! concrete dependencies injected into them.

use std::sync::Arc;

use crate::owner_handles::{EXECUTOR_STACK_BYTES, execute_on_runtime};
use tokio::runtime::{Builder, Runtime};

use crate::{AppHost, FloeCore, HostError, HostServices, local_context, runtime_preparation};

/// Rust-only external model-adapter composition input. The selected factory is
/// retained by this App host and used to build a provider for each Vault generation.
pub trait ModelProviderFactory: Send + Sync + 'static {
    fn create(
        &self,
        gateway_credentials: floe_provider_adapters::gateway::GatewayCredentialStore,
    ) -> Arc<dyn floe_inference::ModelProvider>;
}

struct CompositeModelProviderFactory;

impl ModelProviderFactory for CompositeModelProviderFactory {
    fn create(
        &self,
        gateway_credentials: floe_provider_adapters::gateway::GatewayCredentialStore,
    ) -> Arc<dyn floe_inference::ModelProvider> {
        Arc::new(floe_provider_adapters::gateway::CompositeModelProvider::new(gateway_credentials))
    }
}

/// Immutable Rust-only options for opening the default development/production
/// installation. FFI and persisted app configuration do not carry this seam.
#[derive(Clone)]
pub struct AppOpenOptions {
    model_provider_factory: Arc<dyn ModelProviderFactory>,
}

impl AppOpenOptions {
    pub fn with_model_provider_factory(mut self, factory: impl ModelProviderFactory) -> Self {
        self.model_provider_factory = Arc::new(factory);
        self
    }
}

impl Default for AppOpenOptions {
    fn default() -> Self {
        Self {
            model_provider_factory: Arc::new(CompositeModelProviderFactory),
        }
    }
}

pub struct AppComposition {
    pub(crate) runtime: Runtime,
    pub(crate) core: Arc<FloeCore>,
    pub(crate) local_context: Arc<local_context::LocalContextHost>,
    #[cfg(unix)]
    pub(crate) runtime_preparation: runtime_preparation::RuntimePreparationHost,
}

impl HostServices for AppComposition {
    fn shutdown(&self) -> Result<(), crate::HostError> {
        self.core.day.close_admission();
        let gateway = self.core.product_gateway.close();
        #[cfg(unix)]
        let runtime = self.runtime_preparation.shutdown();
        let scope = crate::host_scope(
            uuid::Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            std::time::Duration::from_secs(35),
        );
        let day_owner = self.core.day.clone();
        let day = self.execute_owner(async move { day_owner.shutdown(&scope).await });
        runtime.map_err(|_| crate::HostError::Shutdown)?;
        gateway.map_err(|_| crate::HostError::Shutdown)?;
        day.map_err(|_| crate::HostError::Shutdown)
    }
}

/// Open an explicit existing installation database. Owner tasks run after an FFI
/// admission returns, on this host's bounded execution runtime.
pub fn open(path: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    let database = std::fs::canonicalize(path)
        .map_err(|_| AppOpenError::Host(HostError::IdentityUnavailable))?;
    crate::storage_profile::validate_database(&database)?;
    let lease = floe_provider_adapters::lock_existing_local_installation(&database)
        .map_err(installation_error)?;
    let identity = crate::bootstrap::local_identity_for_database(&database)
        .map_err(AppOpenError::Host)?
        .ok_or(AppOpenError::Host(HostError::IdentityUnavailable))?;
    let path = database
        .to_str()
        .ok_or(AppOpenError::Host(HostError::IdentityUnavailable))?;
    let runtime = runtime()?;
    let database_path = std::path::PathBuf::from(path);
    let storage_identity = floe_vault::ProductStoreIdentity::new(
        floe_kernel::PersonId(identity.person_id),
        identity.device_id.clone(),
    )
    .map_err(storage_error)?;
    let store = execute_on_runtime(&runtime, async move {
        floe_vault::TursoStore::open_existing(&database_path, storage_identity).await
    })
    .map_err(storage_error)?;
    compose(
        path,
        identity,
        runtime,
        store.with_installation_lock(lease.into_lock_file()),
        AppOpenOptions::default(),
    )
}

/// Open the one internal installation and its encrypted product store. Its own
/// root-key custody is independent of Agent Vault activation or provider credentials.
pub fn open_default(support_directory: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    open_default_with_options(support_directory, AppOpenOptions::default())
}

/// Open the one internal installation with an explicit Rust-only external
/// model-provider factory. Both this entry and `open_default` share the same
/// installation, store, and App composition path.
pub fn open_default_with_options(
    support_directory: &str,
    options: AppOpenOptions,
) -> Result<AppHost<AppComposition>, AppOpenError> {
    let runtime = runtime()?;
    let support_directory =
        crate::storage_profile::support_directory(std::path::Path::new(support_directory))?;
    let mut installation = floe_provider_adapters::prepare_local_installation(&support_directory)
        .map_err(installation_error)?;
    let database_path = installation.database_path().to_path_buf();
    let admission = installation.database_admission();
    let storage_identity = floe_vault::ProductStoreIdentity::new(
        floe_kernel::PersonId(installation.identity().person_id()),
        installation.identity().device_id().to_owned(),
    )
    .map_err(storage_error)?;
    let store = execute_on_runtime(&runtime, async move {
        match admission {
            floe_provider_adapters::LocalDatabaseAdmission::Existing
            | floe_provider_adapters::LocalDatabaseAdmission::InspectUnfinishedCreation => {
                floe_vault::TursoStore::open_existing(&database_path, storage_identity).await
            }
            floe_provider_adapters::LocalDatabaseAdmission::CreateNew => {
                floe_vault::TursoStore::create_new(&database_path, storage_identity).await
            }
        }
    })
    .map_err(|error| {
        if admission == floe_provider_adapters::LocalDatabaseAdmission::InspectUnfinishedCreation
            && matches!(
                error.code,
                floe_vault::StoreErrorCode::UnsupportedSchema
                    | floe_vault::StoreErrorCode::StoredDataCorrupt
            )
        {
            AppOpenError::Store {
                failure: AppStorageFailure::IncompleteCreation,
                message:
                    "product store creation is incomplete; existing files and keys were preserved"
                        .into(),
            }
        } else {
            storage_error(error)
        }
    })?;
    // Every build preserves failed admissions. Runtime preparation owns the
    // host's Vault generation; a build profile is never reset authority.
    installation.complete().map_err(installation_error)?;
    let identity = crate::bootstrap::installation_identity(&installation);
    let path = installation
        .database_path()
        .to_str()
        .ok_or_else(|| AppOpenError::Runtime("local database path is invalid".into()))?
        .to_owned();
    let store = store.with_installation_lock(installation.into_lease().into_lock_file());
    compose(&path, identity, runtime, store, options)
}

fn installation_error(error: floe_provider_adapters::NativeInstallationError) -> AppOpenError {
    AppOpenError::Runtime(error.to_string())
}

fn runtime() -> Result<Runtime, AppOpenError> {
    Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(EXECUTOR_STACK_BYTES)
        .enable_all()
        .build()
        .map_err(|error| AppOpenError::Runtime(error.to_string()))
}

fn compose(
    path: &str,
    identity: crate::LocalIdentityClaim,
    runtime: Runtime,
    store: floe_vault::TursoStore,
    options: AppOpenOptions,
) -> Result<AppHost<AppComposition>, AppOpenError> {
    let database = std::fs::canonicalize(path)
        .map_err(|_| AppOpenError::Host(HostError::IdentityUnavailable))?;
    let installation_root = database
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .ok_or(AppOpenError::Host(HostError::IdentityUnavailable))?
        .to_path_buf();
    let caller = crate::CallerContext::verified(identity, crate::bootstrap::runtime_epoch())
        .map_err(AppOpenError::Host)?;
    let store = Arc::new(store);
    let local_context = Arc::new(crate::local_context::LocalContextHost::default());
    let product_gateway =
        Arc::new(floe_provider_adapters::gateway::ProductGatewayLeaseRegistry::new());
    let transport = Arc::new(
        floe_provider_adapters::sources::CalendarProductAdapter::new(
            local_context.calendar_handle(),
            product_gateway.clone(),
        ),
    );
    let context = Arc::new(floe_context::ContextCore::new(
        store.clone(),
        transport,
        Arc::new(floe_access::SystemAccessClock),
    ));
    let day = Arc::new(floe_day::DayService::new(
        store.clone(),
        Arc::new(floe_context::ContextCalendarAcquisition::new(context)),
        Arc::new(floe_day::SystemDayClock),
    ));
    let scope = crate::host_scope(
        uuid::Uuid::new_v4(),
        floe_execution::Cancellation::new(),
        std::time::Duration::from_secs(35),
    );
    let activating_day = day.clone();
    let actor = caller.owner_actor();
    execute_on_runtime(&runtime, async move {
        activating_day.activate(&actor, &scope).await
    })
    .map_err(|error| AppOpenError::Runtime(error.to_string()))?;
    let core = Arc::new(crate::FloeCore {
        store,
        installation_root,
        lease_registry: Arc::new(floe_context::SourceLeaseRegistry::new()),
        day,
        product_gateway,
    });
    let runtime_handle = runtime.handle().clone();
    let services = AppComposition {
        runtime,
        core: core.clone(),
        local_context: local_context.clone(),
        #[cfg(unix)]
        runtime_preparation: runtime_preparation::RuntimePreparationHost::new(
            database
                .to_str()
                .ok_or(AppOpenError::Host(HostError::IdentityUnavailable))?,
            caller.clone(),
            runtime_handle,
            core,
            local_context,
            options.model_provider_factory,
        ),
    };
    AppHost::with_caller(services, caller).map_err(AppOpenError::Host)
}

#[derive(Clone, Debug)]
pub enum AppOpenError {
    Host(HostError),
    Runtime(String),
    Store {
        failure: AppStorageFailure,
        message: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum AppStorageFailure {
    KeyMissing,
    KeyMalformed,
    KeyUnavailable,
    IncompleteCreation,
    UnsupportedFormat,
    StoredDataInvalid,
    Conflict,
    Unavailable,
}
impl AppStorageFailure {
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::KeyMissing => "product_key_missing",
            Self::KeyMalformed => "product_key_malformed",
            Self::KeyUnavailable => "product_key_unavailable",
            Self::IncompleteCreation => "product_creation_incomplete",
            Self::UnsupportedFormat => "product_format_unsupported",
            Self::StoredDataInvalid => "product_data_invalid",
            Self::Conflict => "product_store_conflict",
            Self::Unavailable => "product_store_unavailable",
        }
    }
}
fn storage_error(error: floe_vault::StoreError) -> AppOpenError {
    use floe_vault::StoreErrorCode as C;
    let failure = match error.code {
        C::ProductKeyMissing => AppStorageFailure::KeyMissing,
        C::ProductKeyMalformed => AppStorageFailure::KeyMalformed,
        C::ProductKeyUnavailable => AppStorageFailure::KeyUnavailable,
        C::UnsupportedSchema => AppStorageFailure::UnsupportedFormat,
        C::StoredDataCorrupt | C::Validation => AppStorageFailure::StoredDataInvalid,
        C::Conflict => AppStorageFailure::Conflict,
        _ => AppStorageFailure::Unavailable,
    };
    AppOpenError::Store {
        failure,
        message: error.to_string(),
    }
}
