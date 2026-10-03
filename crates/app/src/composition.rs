//! Host composition: the services the app host exposes, their lifetime and the
//! concrete dependencies injected into them.

use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use crate::{AppHost, FloeCore, HostError, HostServices, local_context, vault_host};

pub struct AppComposition {
    pub(crate) runtime: Runtime,
    pub(crate) core: Arc<FloeCore>,
    pub(crate) local_context: Arc<local_context::LocalContextHost>,
    #[cfg(unix)]
    pub(crate) agent_vault: vault_host::VaultBridge,
}

impl HostServices for AppComposition {
    fn shutdown(&self) -> Result<(), crate::HostError> {
        self.core.day.close_admission();
        let gateway = self.core.product_gateway.close();
        #[cfg(unix)]
        self.agent_vault.shutdown();
        let scope = crate::host_scope(uuid::Uuid::new_v4(), floe_execution::Cancellation::new(), std::time::Duration::from_secs(35));
        let day = self.runtime.block_on(self.core.day.shutdown(&scope));
        gateway.map_err(|_| crate::HostError::Shutdown)?;
        day.map_err(|_| crate::HostError::Shutdown)

    }
}

#[cfg(unix)]
pub(crate) fn service_failure(failure: floe_kernel::AgentFailure) -> crate::ServiceError {
    use crate::ServiceError;
    use floe_kernel::AgentFailure;

    match failure {
        AgentFailure::InvalidInput | AgentFailure::UnsupportedVersion => ServiceError::InvalidInput,
        AgentFailure::NotFound => ServiceError::NotFound,
        AgentFailure::Conflict => ServiceError::Conflict,
        AgentFailure::PolicyDenied
        | AgentFailure::ConsentRequired
        | AgentFailure::CapabilityDenied
        | AgentFailure::AccessReviewRequired => ServiceError::AccessDenied,
        AgentFailure::StorageUnavailable
        | AgentFailure::VaultUnavailable
        | AgentFailure::VaultLocked
        | AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::ServerModelRequestRejected
        | AgentFailure::CredentialExpired
        | AgentFailure::QuotaExceeded
        | AgentFailure::CapabilityUnavailable
        | AgentFailure::StaleContext
        | AgentFailure::BudgetExceeded
        | AgentFailure::Cancelled
        | AgentFailure::DeadlineExceeded
        | AgentFailure::Interrupted => ServiceError::Unavailable,
        AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput
        | AgentFailure::Stalled => ServiceError::Internal,
    }
}

/// Open the selected existing profile. Owner tasks keep running after an FFI
/// admission returns, on this host's bounded execution runtime.
pub fn open(path: &str) -> Result<AppHost<AppComposition>, AppOpenError> {
    let identity = crate::bootstrap::local_identity_for_database(std::path::Path::new(path))
        .map_err(AppOpenError::Host)?
        .ok_or(AppOpenError::Host(HostError::IdentityUnavailable))?;
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| AppOpenError::Runtime(error.to_string()))?;
    let store = runtime
        .block_on(floe_vault::TursoStore::open_existing(path))
        .map_err(|error| AppOpenError::Store(error.to_string()))?;
    let caller = crate::CallerContext::verified(identity, crate::bootstrap::runtime_epoch())
        .map_err(AppOpenError::Host)?;
    let store = Arc::new(store);
    let local_context = Arc::new(crate::local_context::LocalContextHost::default());
    let product_gateway = Arc::new(floe_provider_adapters::gateway::ProductGatewayLeaseRegistry::new());
    let transport = Arc::new(floe_provider_adapters::sources::CalendarProductAdapter::new(
        local_context.calendar_handle(), product_gateway.clone(),
    ));
    let context = Arc::new(floe_context::ContextCore::new(store.clone(), transport,
        Arc::new(floe_access::SystemAccessClock)));
    let day = Arc::new(floe_day::DayService::new(store.clone(),
        Arc::new(floe_context::ContextCalendarAcquisition::new(context)),
        Arc::new(floe_day::SystemDayClock)));
    let scope = crate::host_scope(uuid::Uuid::new_v4(), floe_execution::Cancellation::new(), std::time::Duration::from_secs(35));
    runtime.block_on(day.activate(&caller.owner_actor(), &scope))
        .map_err(|error| AppOpenError::Store(error.to_string()))?;
    let core = Arc::new(crate::FloeCore { store,
        lease_registry: Arc::new(floe_context::SourceLeaseRegistry::new()), day, product_gateway });
    let services = AppComposition {
        runtime,
        core: core.clone(),
        local_context: local_context.clone(),
        #[cfg(unix)]
        agent_vault: vault_host::VaultBridge::new(path, core, local_context),
    };
    Ok(AppHost::with_caller(services, caller))
}

#[derive(Clone, Debug)]
pub enum AppOpenError {
    Host(HostError),
    Runtime(String),
    Store(String),
}
