//! What a personal source read stands on, on this host.
//!
//! Which grant admits a read, what a review may change, and whether a recorded
//! dependency still holds are Access's and Context's. What is left here is the
//! pair this host injects: the vault that holds the Person's grants and the
//! device driver that answers for them.

use std::{future::Future, pin::Pin};

#[cfg(test)]
use floe_access::DependencyLiveness;
use floe_access::{DependencyAuthorization, DependencyResolver};
use floe_agent_contract::AgentFailure;
use floe_context_contract::ConnectionId;
use floe_context_contract::ContextDependency;
use floe_kernel::PersonId;
use floe_provider_adapters::sources::NativePersonalDriver;
use floe_vault::{EncryptedAgentVault, VaultGrantRecords, VaultKeyProvider};

use crate::FloeCore;
use crate::local_context::LocalContextHost;

pub(crate) struct CorePersonalConnections<'a> {
    pub(crate) core: &'a FloeCore,
}

impl floe_context::PersonalConnectionReader for CorePersonalConnections<'_> {
    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<Option<floe_connections::SourceConnection>, AgentFailure>,
    > {
        Box::pin(async move {
            self.core
                .source_service()
                .load(person_id, connection_id)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)
        })
    }
}

pub(crate) struct PersonalDependencyResolver<'a, Keys: VaultKeyProvider> {
    pub(crate) core: &'a FloeCore,
    pub(crate) vault: &'a EncryptedAgentVault<Keys>,
    pub(crate) local_context: &'a LocalContextHost,
    pub(crate) person_id: PersonId,
    pub(crate) device_id: &'a str,
}

/// Test-only liveness: the canonical turn validates dependencies through
/// its resolver instead.
#[cfg(test)]
pub(crate) struct PersonalDependencyLiveness<'a> {
    pub(crate) local_context: &'a LocalContextHost,
    pub(crate) person_id: PersonId,
    pub(crate) device_id: &'a str,
}

/// The device driver this host's local context owns.
pub(crate) fn native_driver(local_context: &LocalContextHost) -> NativePersonalDriver<'_> {
    NativePersonalDriver {
        attention: local_context.attention(),
        personal: local_context.personal(),
        observations: local_context.observations(),
    }
}

#[cfg(test)]
impl DependencyLiveness for PersonalDependencyLiveness<'_> {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure> {
        floe_context::personal_dependency_holds(
            &native_driver(self.local_context),
            self.person_id,
            self.device_id,
            dependency,
        )
    }
}

impl<Keys: VaultKeyProvider> DependencyResolver for PersonalDependencyResolver<'_, Keys> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a DependencyAuthorization,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            floe_context::authorize_personal_dependency(
                &CorePersonalConnections { core: self.core },
                &VaultGrantRecords::new(self.vault),
                &native_driver(self.local_context),
                self.person_id,
                self.device_id,
                dependency,
                request.deadline,
                &request.cancellation,
            )
            .await
        })
    }
}
