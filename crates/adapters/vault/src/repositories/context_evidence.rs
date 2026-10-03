use floe_access::DependencyCoverage;
use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
use floe_context::EvidenceReader;
use uuid::Uuid;
use crate::{EncryptedAgentVault, VaultKeyProvider};

pub struct ContextEvidenceReader<Keys> { vault: std::sync::Arc<EncryptedAgentVault<Keys>> }
impl<Keys: VaultKeyProvider> ContextEvidenceReader<Keys> {
    pub fn new(vault: std::sync::Arc<EncryptedAgentVault<Keys>>) -> Self { Self { vault } }
}
impl<Keys: VaultKeyProvider> EvidenceReader for ContextEvidenceReader<Keys> {
    fn read_turn_coverage<'a>(&'a self, session_id: Uuid, turn_id: Uuid) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
        Box::pin(async move {
            if session_id.is_nil() || turn_id.is_nil() { return Err(AgentFailure::InvalidInput); }
            self.vault.read_turn_coverage(session_id, turn_id).await
        })
    }
}
