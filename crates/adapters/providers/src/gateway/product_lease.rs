//! A retained lease over one exact encrypted Gateway generation.
//!
//! The App is the sole lifecycle publisher. Consumers can only acquire a
//! generation, retain it for an operation, and observe its cancellation fence.
use std::sync::{Arc, Mutex, Weak};

use super::credentials::{GatewayCredentialError, GatewayCredentialStore};
use crate::control::PreparedServerSource;
use floe_access::{AuthorizationSigner, GatewayTrustReader};
use floe_execution::{Cancellation, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};

struct GatewayGeneration {
    generation: u64,
    actor: OwnerActor,
    credentials: Arc<GatewayCredentialStore>,
    signer: Arc<dyn AuthorizationSigner>,
    cancellation: Cancellation,
}

enum RegistryPhase {
    Locked,
    Ready(Arc<GatewayGeneration>),
    Closed,
}

struct RegistryState {
    phase: RegistryPhase,
    largest_published_generation: u64,
}

struct RegistryCore {
    state: Mutex<RegistryState>,
}

/// App-owned publisher for immutable Gateway credential/signing generations.
#[derive(Clone)]
pub struct ProductGatewayLeaseRegistry {
    core: Arc<RegistryCore>,
}

/// A retained, cancellation-aware handle to one published Gateway generation.
#[derive(Clone)]
pub struct ProductGatewayLease {
    registry: Weak<RegistryCore>,
    retained: Arc<GatewayGeneration>,
    credentials: PreparedServerSource,
}

impl ProductGatewayLeaseRegistry {
    /// Construct the host-lifetime registry with admission closed until App
    /// publishes a ready encrypted Gateway generation.
    pub fn new() -> Self {
        Self {
            core: Arc::new(RegistryCore {
                state: Mutex::new(RegistryState {
                    phase: RegistryPhase::Locked,
                    largest_published_generation: 0,
                }),
            }),
        }
    }

    /// Publish the exact existing credential and owner signer for a new
    /// monotonically increasing local runtime generation.
    pub fn publish(
        &self,
        generation: u64,
        actor: OwnerActor,
        credentials: Arc<GatewayCredentialStore>,
        signer: Arc<dyn AuthorizationSigner>,
    ) -> Result<(), AgentFailure> {
        actor.validate()?;
        if generation == 0 {
            return Err(AgentFailure::PolicyDenied);
        }

        let mut state = self
            .core
            .state
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if matches!(&state.phase, RegistryPhase::Closed) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if generation <= state.largest_published_generation {
            return Err(AgentFailure::Conflict);
        }

        if let RegistryPhase::Ready(previous) = &state.phase {
            previous.cancellation.cancel();
        }
        state.phase = RegistryPhase::Ready(Arc::new(GatewayGeneration {
            generation,
            actor,
            credentials,
            signer,
            cancellation: Cancellation::new(),
        }));
        state.largest_published_generation = generation;
        Ok(())
    }

    /// Close admission and cancel the retained generation before Vault seal.
    pub fn lock(&self) -> Result<(), AgentFailure> {
        let mut state = self
            .core
            .state
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if let RegistryPhase::Ready(current) = &state.phase {
            current.cancellation.cancel();
        }
        if !matches!(&state.phase, RegistryPhase::Closed) {
            state.phase = RegistryPhase::Locked;
        }
        Ok(())
    }

    /// Retire only the exact generation whose owner is shutting down. A stale
    /// retained owner cannot close a newer unlock or reopen a closed host.
    pub fn retire(&self, expected_generation: u64) -> Result<(), AgentFailure> {
        if expected_generation == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self
            .core
            .state
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if let RegistryPhase::Ready(current) = &state.phase {
            if current.generation == expected_generation {
                current.cancellation.cancel();
                state.phase = RegistryPhase::Locked;
            }
        }
        Ok(())
    }

    /// Permanently close this host-lifetime registry and cancel every lease.
    pub fn close(&self) -> Result<(), AgentFailure> {
        let mut state = self
            .core
            .state
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if let RegistryPhase::Ready(current) = &state.phase {
            current.cancellation.cancel();
        }
        state.phase = RegistryPhase::Closed;
        Ok(())
    }

    /// Resolve proven absence or retain one verified credential under the exact
    /// currently published encrypted generation. Locked and unreadable stores
    /// never become an absent Gateway.
    pub async fn acquire_optional(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<Option<ProductGatewayLease>, AgentFailure> {
        self.acquire_inner(actor, None, scope).await
    }

    /// Acquire the local generation captured by the Access permit. This never
    /// substitutes a replacement ready generation or an absent credential.
    pub async fn acquire(
        &self,
        actor: &OwnerActor,
        expected_generation: u64,
        scope: &ExecutionScope,
    ) -> Result<ProductGatewayLease, AgentFailure> {
        if expected_generation == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        self.acquire_inner(actor, Some(expected_generation), scope)
            .await?
            .ok_or(AgentFailure::StaleContext)
    }

    async fn acquire_inner(
        &self,
        actor: &OwnerActor,
        expected_generation: Option<u64>,
        scope: &ExecutionScope,
    ) -> Result<Option<ProductGatewayLease>, AgentFailure> {
        actor.validate()?;
        let retained = {
            let state = self
                .core
                .state
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?;
            match &state.phase {
                RegistryPhase::Locked => return Err(AgentFailure::VaultLocked),
                RegistryPhase::Closed => return Err(AgentFailure::CapabilityUnavailable),
                RegistryPhase::Ready(current) => current.clone(),
            }
        };
        if retained.actor != *actor {
            return Err(AgentFailure::PolicyDenied);
        }
        if expected_generation.is_some_and(|expected| expected != retained.generation) {
            return Err(AgentFailure::StaleContext);
        }
        ensure_generation(&self.core, &retained)?;
        let person = actor.person_id.to_string();
        let load = scope.run(async {
            retained
                .credentials
                .load(&person, &actor.device_id)
                .await
                .map_err(credential_failure)
        });
        let connection = tokio::select! {
            biased;
            _ = retained.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            result = load => result?,
        };
        ensure_generation(&self.core, &retained)?;
        let Some(connection) = connection else {
            return Ok(None);
        };
        connection.binding.validate()?;
        let credentials =
            PreparedServerSource::new(connection, retained.credentials.as_ref().clone());
        Ok(Some(ProductGatewayLease {
            registry: Arc::downgrade(&self.core),
            retained,
            credentials,
        }))
    }
}

impl Default for ProductGatewayLeaseRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProductGatewayLease {
    pub fn generation(&self) -> u64 {
        self.retained.generation
    }

    pub fn actor(&self) -> &OwnerActor {
        &self.retained.actor
    }

    pub fn credentials(&self) -> &PreparedServerSource {
        &self.credentials
    }

    pub fn trust(&self) -> Arc<dyn GatewayTrustReader> {
        self.retained.credentials.trust()
    }

    pub fn signer(&self) -> &Arc<dyn AuthorizationSigner> {
        &self.retained.signer
    }

    pub fn cancellation(&self) -> &Cancellation {
        &self.retained.cancellation
    }

    /// Confirm that lock, replacement, close, or host teardown has not
    /// superseded this exact retained generation.
    pub fn ensure_current(&self) -> Result<(), AgentFailure> {
        if self.retained.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let registry = self
            .registry
            .upgrade()
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        ensure_generation(&registry, &self.retained)
    }

    /// Revalidate the saved secret and its current pin without replacing this
    /// lease with a fresh generation.
    pub async fn revalidate(&self) -> Result<(), AgentFailure> {
        self.ensure_current()?;
        self.credentials.revalidate().await?;
        self.ensure_current()
    }
}

fn ensure_generation(
    registry: &RegistryCore,
    retained: &Arc<GatewayGeneration>,
) -> Result<(), AgentFailure> {
    if retained.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let state = registry
        .state
        .lock()
        .map_err(|_| AgentFailure::Interrupted)?;
    match &state.phase {
        RegistryPhase::Ready(current) if Arc::ptr_eq(current, retained) => Ok(()),
        _ => Err(AgentFailure::StaleContext),
    }
}

fn credential_failure(error: GatewayCredentialError) -> AgentFailure {
    match error {
        GatewayCredentialError::Locked => AgentFailure::VaultLocked,
        GatewayCredentialError::Timeout => AgentFailure::DeadlineExceeded,
        GatewayCredentialError::Unavailable | GatewayCredentialError::Indeterminate => {
            AgentFailure::CapabilityUnavailable
        }
        GatewayCredentialError::Cancelled => AgentFailure::Cancelled,
        GatewayCredentialError::Conflict => AgentFailure::Conflict,
        GatewayCredentialError::Malformed
        | GatewayCredentialError::Unverified
        | GatewayCredentialError::ForeignIdentity => AgentFailure::PolicyDenied,
    }
}
