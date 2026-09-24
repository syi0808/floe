//! The Person's encrypted Expert registry, as a builtin setup ensure writes it.
//!
//! Experts decides whether to install or leave the setup alone. This
//! holds the packaging the builtin Experts declare for themselves and the
//! cancellation the run is under, and performs what it is told.

use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;
use floe_experts::{
    BoxFuture, BuiltinExpertSetup, BuiltinExpertSetupResult, BuiltinExpertStore, ExpertSetupSpec,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

/// The namespace the builtin setup is installed under, so that one Person's
/// registry always names the same setup.
const BUILTIN_SETUP_NAMESPACE: &[u8] = b"floe.builtin.experts.v1";

pub(super) struct VaultBuiltinExperts<'a, Keys> {
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub specs: Vec<ExpertSetupSpec>,
    pub cancellation: Cancellation,
}

impl<Keys: VaultKeyProvider> BuiltinExpertStore for VaultBuiltinExperts<'_, Keys> {
    fn instance_id(&self) -> Uuid {
        self.vault.registry_instance_id()
    }

    fn setup_id(&self) -> Uuid {
        Uuid::new_v5(&self.vault.registry_instance_id(), BUILTIN_SETUP_NAMESPACE)
    }

    fn overview<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Option<BuiltinExpertSetupResult>, AgentFailure>> {
        Box::pin(self.vault.builtin_expert_overview())
    }

    fn registry_revision<'a>(&'a self) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .vault
                .registry_overview()
                .await?
                .map_or(0, |registry| registry.revision))
        })
    }

    fn install<'a>(
        &'a self,
        setup: BuiltinExpertSetup,
    ) -> BoxFuture<'a, Result<BuiltinExpertSetupResult, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .install_builtin_experts_enabled(setup, &self.specs, self.cancellation.clone())
                .await
        })
    }
}
