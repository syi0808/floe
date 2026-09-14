use floe_agent::AgentFailure;
use floe_context::EvidenceReader;
use floe_domain::DependencyCoverage;
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

pub(crate) struct ContextEvidenceReader<'vault, Keys> {
    vault: &'vault EncryptedAgentVault<Keys>,
    session_id: Uuid,
}

impl<'vault, Keys: VaultKeyProvider> ContextEvidenceReader<'vault, Keys> {
    pub(crate) fn new(vault: &'vault EncryptedAgentVault<Keys>, session_id: Uuid) -> Self {
        Self { vault, session_id }
    }
}

impl<Keys: VaultKeyProvider> EvidenceReader for ContextEvidenceReader<'_, Keys> {
    async fn read_turn_coverage(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<DependencyCoverage, AgentFailure> {
        if session_id.is_nil() || turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if session_id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        self.vault.read_turn_coverage(session_id, turn_id).await
    }
}
