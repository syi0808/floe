//! Host composition: the services the app host exposes, their lifetime and the
//! concrete dependencies injected into them.

use std::sync::Arc;

use crate::owner_handles::{EXECUTOR_STACK_BYTES, execute_on_runtime};
use tokio::runtime::{Builder, Runtime};

use crate::{AppHost, FloeCore, HostError, HostServices, local_context, vault_lifecycle};

pub struct AppComposition {
    pub(crate) runtime: Runtime,
    pub(crate) core: Arc<FloeCore>,
    pub(crate) local_context: Arc<local_context::LocalContextHost>,
    #[cfg(unix)]
    pub(crate) agent_vault: vault_lifecycle::VaultBridge,
}

impl HostServices for AppComposition {
    fn shutdown(&self) -> Result<(), crate::HostError> {
        self.core.day.close_admission();
        let gateway = self.core.product_gateway.close();
        #[cfg(unix)]
        let vault = self.agent_vault.shutdown();
        let scope = crate::host_scope(
            uuid::Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            std::time::Duration::from_secs(35),
        );
        let day_owner = self.core.day.clone();
        let day = self.execute_owner(async move { day_owner.shutdown(&scope).await });
        vault.map_err(|_| crate::HostError::Shutdown)?;
        gateway.map_err(|_| crate::HostError::Shutdown)?;
        day.map_err(|_| crate::HostError::Shutdown)
    }
}

/// Open an explicit existing installation database. Owner tasks run after an FFI
/// admission returns, on this host's bounded execution runtime.
pub fn open(path: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    let lease =
        floe_provider_adapters::lock_existing_local_installation(std::path::Path::new(path))
            .map_err(installation_error)?;
    let identity = crate::bootstrap::local_identity_for_database(std::path::Path::new(path))
        .map_err(AppOpenError::Host)?
        .ok_or(AppOpenError::Host(HostError::IdentityUnavailable))?;
    let runtime = runtime()?;
    let database_path = std::path::PathBuf::from(path);
    let store = execute_on_runtime(&runtime, async move {
        floe_vault::TursoStore::open_existing(&database_path).await
    })
    .map_err(|error| AppOpenError::Store(error.to_string()))?;
    compose(
        path,
        identity,
        runtime,
        store.with_installation_lock(lease.into_lock_file()),
    )
}

/// Open the one internal installation. Fresh identity and plain-store creation
/// do not create/unlock an encrypted Vault or any provider credential.
#[cfg_attr(not(debug_assertions), allow(clippy::never_loop))]
pub fn open_default(support_directory: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    let runtime = runtime()?;
    let mut installation =
        floe_provider_adapters::prepare_local_installation(std::path::Path::new(support_directory))
            .map_err(installation_error)?;
    #[cfg(debug_assertions)]
    let mut reset_attempted = false;
    let store = loop {
        let database_path = installation.database_path().to_path_buf();
        let admission = installation.database_admission();
        let opened = execute_on_runtime(&runtime, async move {
            match admission {
                floe_provider_adapters::LocalDatabaseAdmission::Existing => {
                    floe_vault::TursoStore::open_existing(&database_path).await
                }
                floe_provider_adapters::LocalDatabaseAdmission::CreateNew => {
                    floe_vault::TursoStore::create_new(&database_path).await
                }
            }
        });
        let store = match opened {
            Ok(store) => store,
            #[cfg(debug_assertions)]
            Err(error)
                if matches!(
                    error.code,
                    floe_vault::StoreErrorCode::UnsupportedSchema
                        | floe_vault::StoreErrorCode::StoredDataCorrupt
                ) && !reset_attempted =>
            {
                installation = installation
                    .reset_before_open(
                        floe_provider_adapters::DevelopmentResetReason::UnsupportedDatabase,
                        None,
                    )
                    .map_err(installation_error)?;
                reset_attempted = true;
                continue;
            }
            Err(error) => return Err(AppOpenError::Store(error.to_string())),
        };
        #[cfg(debug_assertions)]
        {
            let root = std::path::PathBuf::from(format!(
                "{}.agent-vaults",
                installation.database_path().display(),
            ));
            let person = floe_kernel::PersonId(installation.identity().person_id());
            match execute_on_runtime(&runtime, async move {
                floe_vault::inspect_existing_vault(&root, person, &floe_vault::KeyringVaultKeys)
                    .await
            }) {
                floe_vault::VaultOpenInspection::Absent
                | floe_vault::VaultOpenInspection::Compatible => {}
                floe_vault::VaultOpenInspection::Unavailable(failure) => {
                    return Err(AppOpenError::Runtime(format!(
                        "existing Vault preflight failed: {failure:?}"
                    )));
                }
                floe_vault::VaultOpenInspection::Resettable(evidence) => {
                    if reset_attempted {
                        return Err(AppOpenError::Runtime(
                            "fresh installation failed Vault validation".into(),
                        ));
                    }
                    let reason = match evidence.reason() {
                        floe_vault::VaultResetReason::MissingKey => floe_provider_adapters::DevelopmentResetReason::MissingVaultKey,
                        floe_vault::VaultResetReason::MalformedKey => floe_provider_adapters::DevelopmentResetReason::MalformedVaultKey,
                        floe_vault::VaultResetReason::UnsupportedSchema => floe_provider_adapters::DevelopmentResetReason::UnsupportedVaultSchema,
                        floe_vault::VaultResetReason::IdentityMismatch => floe_provider_adapters::DevelopmentResetReason::VerifiedVaultIdentityMismatch,
                        floe_vault::VaultResetReason::StoredDataCorrupt => floe_provider_adapters::DevelopmentResetReason::StoredVaultDataCorrupt,
                    };
                    // Neither database nor any domain owner is live during the
                    // move. The encrypted preflight lock survives the rename.
                    drop(store);
                    installation = installation
                        .reset_before_open(reason, Some(evidence.lock_file()))
                        .map_err(installation_error)?;
                    drop(evidence);
                    reset_attempted = true;
                    continue;
                }
            }
        }
        break store;
    };
    installation.complete().map_err(installation_error)?;
    let identity = crate::bootstrap::installation_identity(&installation);
    let path = installation
        .database_path()
        .to_str()
        .ok_or_else(|| AppOpenError::Runtime("local database path is invalid".into()))?
        .to_owned();
    let store = store.with_installation_lock(installation.into_lease().into_lock_file());
    compose(&path, identity, runtime, store)
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
    .map_err(|error| AppOpenError::Store(error.to_string()))?;
    let core = Arc::new(crate::FloeCore {
        store,
        installation_root,
        lease_registry: Arc::new(floe_context::SourceLeaseRegistry::new()),
        day,
        product_gateway,
    });
    let services = AppComposition {
        runtime,
        core: core.clone(),
        local_context: local_context.clone(),
        #[cfg(unix)]
        agent_vault: vault_lifecycle::VaultBridge::new(path, core, local_context),
    };
    AppHost::with_caller(services, caller).map_err(AppOpenError::Host)
}

#[derive(Clone, Debug)]
pub enum AppOpenError {
    Host(HostError),
    Runtime(String),
    Store(String),
}
