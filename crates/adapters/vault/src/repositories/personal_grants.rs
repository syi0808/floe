//! The Person's own grants, as their vault holds them.
//!
//! Context asks what the Person granted. Standing source facts live in Connections.

use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_context::PersonalGrantRecords;

use crate::{EncryptedAgentVault, VaultKeyProvider};

pub struct VaultGrantRecords<'a, Keys: VaultKeyProvider> {
    pub vault: &'a EncryptedAgentVault<Keys>,
}

impl<'a, Keys: VaultKeyProvider> VaultGrantRecords<'a, Keys> {
    pub fn new(vault: &'a EncryptedAgentVault<Keys>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider> PersonalGrantRecords for VaultGrantRecords<'_, Keys> {
    fn grants<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<floe_access::DataAccessGrant>, AgentFailure>> {
        Box::pin(async move { self.vault.list_data_access_grants(128).await })
    }
}
