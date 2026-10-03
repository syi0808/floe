use crate::{EncryptedAgentVault, VaultKeyProvider};
use floe_access::DependencyCoverage;
use floe_context::EvidenceReader;
use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
use uuid::Uuid;

pub struct ContextEvidenceReader<Keys> {
    vault: std::sync::Arc<EncryptedAgentVault<Keys>>,
}
impl<Keys: VaultKeyProvider> ContextEvidenceReader<Keys> {
    pub fn new(vault: std::sync::Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}
impl<Keys: VaultKeyProvider> EvidenceReader for ContextEvidenceReader<Keys> {
    fn read_turn_coverage<'a>(
        &'a self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
        Box::pin(async move {
            if session_id.is_nil() || turn_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            self.vault.read_turn_coverage(session_id, turn_id).await
        })
    }
}
