//! What a personal source read stands on, on this host.
//!
//! Which grant admits a read, what a review may change, and whether a recorded
//! dependency still holds are Access's and Context's. What is left here is the
//! pair this host injects: the vault that holds the Person's grants and the
//! device driver that answers for them.

use std::{future::Future, pin::Pin};

use floe_access::{DependencyAuthorization, DependencyLiveness, DependencyResolver};
use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_kernel::PersonId;
use floe_provider_adapters::sources::NativePersonalDriver;
use floe_vault::{EncryptedAgentVault, VaultGrantRecords, VaultKeyProvider};

use crate::local_context::LocalContextHost;

pub(crate) struct PersonalDependencyResolver<'a, Keys: VaultKeyProvider> {
    pub(crate) vault: &'a EncryptedAgentVault<Keys>,
    pub(crate) local_context: &'a LocalContextHost,
    pub(crate) person_id: PersonId,
    pub(crate) device_id: &'a str,
}

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
                &VaultGrantRecords::new(self.vault),
                &native_driver(self.local_context),
                self.person_id,
                self.device_id,
                dependency,
                &request.allowed_placements,
                request.deadline,
                &request.cancellation,
            )
            .await
        })
    }
}
