use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;
use floe_experts::{
    BoxFuture, ExpertInstallOperation, ExpertInstallResult, ExpertInstallStore, ExpertManifest,
    manifest_set_digest,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

const SHIPPED_BUNDLE_NAMESPACE: &[u8] = b"floe.shipped.experts.v1";

pub(super) struct VaultExpertBundle<'a, Keys> {
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub manifests: Vec<ExpertManifest>,
    pub cancellation: Cancellation,
}

impl<Keys: VaultKeyProvider> ExpertInstallStore for VaultExpertBundle<'_, Keys> {
    fn instance_id(&self) -> Uuid {
        self.vault.registry_instance_id()
    }

    fn operation_id(&self) -> Uuid {
        Uuid::new_v5(&self.vault.registry_instance_id(), SHIPPED_BUNDLE_NAMESPACE)
    }

    fn manifest_digest(&self) -> Result<String, AgentFailure> {
        manifest_set_digest(&self.manifests)
    }

    fn overview<'a>(&'a self) -> BoxFuture<'a, Result<Option<ExpertInstallResult>, AgentFailure>> {
        Box::pin(async move {
            self.vault.expert_install_overview(&self.manifest_digest()?).await
        })
    }

    fn registry_revision<'a>(&'a self) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        Box::pin(async move {
            Ok(self.vault.registry_overview().await?.map_or(0, |registry| registry.revision))
        })
    }

    fn install<'a>(&'a self, operation: ExpertInstallOperation) -> BoxFuture<'a, Result<ExpertInstallResult, AgentFailure>> {
        Box::pin(async move {
            self.vault.install_expert_bundle(operation, &self.manifests, self.cancellation.clone()).await
        })
    }
}
