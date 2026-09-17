//! The keys, the pin and the transport a remote pairing or enrollment runs on.
//!
//! Connections decides what a pairing report means and Access decides which
//! producer this Person may enroll with. What this file supplies is the vault
//! that holds their owner key and their pin, and the paired producer reached
//! over HTTP.

use std::time::Duration;

use floe_access::{
    RemoteCallWindow, RemoteEnrollmentSignature, RemoteOwnerPublicKey, RemotePairingChallenge,
    RemoteProducerIdentity,
};
use floe_agent_contract::AgentFailure;
use floe_connections::{PairingIdentity, PairingOwnerKeys};
use floe_execution::Cancellation;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

/// How long a whole authority exchange is given; the transport narrows each
/// call inside it.
const AUTHORITY_DEADLINE: Duration = Duration::from_secs(30);

/// How long one pairing call is given.
pub(super) const PAIRING_DEADLINE: Duration = Duration::from_secs(10);

pub(super) fn authority_window(cancellation: Cancellation) -> RemoteCallWindow {
    RemoteCallWindow {
        deadline: tokio::time::Instant::now() + AUTHORITY_DEADLINE,
        cancellation,
    }
}

/// This Person's vault, as the owner key a pairing is signed under.
pub(super) struct VaultPairingKeys<'a, Keys> {
    pub vault: &'a EncryptedAgentVault<Keys>,
}

impl<Keys: VaultKeyProvider> PairingOwnerKeys for VaultPairingKeys<'_, Keys> {
    async fn owner_public_key(&self) -> Result<RemoteOwnerPublicKey, AgentFailure> {
        self.vault.remote_owner_public_key().await
    }

    async fn sign_pairing(
        &self,
        challenge: &RemotePairingChallenge,
        pairing: PairingIdentity<'_>,
    ) -> Result<RemoteEnrollmentSignature, AgentFailure> {
        self.vault
            .remote_sign_pairing(
                challenge,
                pairing.person_id,
                pairing.client_id,
                pairing.device_id,
            )
            .await
    }

    async fn settle_pairing(
        &self,
        pairing_id: &str,
        challenge: &RemotePairingChallenge,
        approved: bool,
    ) -> Result<(), AgentFailure> {
        self.vault
            .finalize_remote_pairing(pairing_id, challenge, approved)
            .await
            .map(|_| ())
    }
}

/// This Person's vault, as the record of who they trust and what speaks for them.
pub(super) struct VaultRemoteAuthority<'a, Keys> {
    pub vault: &'a EncryptedAgentVault<Keys>,
}

impl<Keys: VaultKeyProvider> floe_access::RemoteAuthorityStore for VaultRemoteAuthority<'_, Keys> {
    fn owner_public_key<'a>(
        &'a self,
    ) -> floe_access::BoxFuture<'a, Result<RemoteOwnerPublicKey, AgentFailure>> {
        Box::pin(self.vault.remote_owner_public_key())
    }

    fn pin_producer<'a>(
        &'a self,
        producer: RemoteProducerIdentity,
    ) -> floe_access::BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move { self.vault.remote_pin_producer(producer).await.map(|_| ()) })
    }
}
