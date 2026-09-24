//! Access-owned recipient consents, as the Person's vault holds them.
//!
//! Access decides whether a review was admissible and derives the
//! content-bound consent id; the vault only persists, looks up, marks
//! revoked, and prunes expired rows. Nothing here authorizes a dispatch.

use floe_access::{RecipientConsent, RecipientConsentStore};
use floe_agent_contract::{AgentFailure, BoxFuture};
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

impl<Keys: VaultKeyProvider> RecipientConsentStore for EncryptedAgentVault<Keys> {
    fn grant_consent<'a>(
        &'a self,
        consent: RecipientConsent,
    ) -> BoxFuture<'a, Result<RecipientConsent, AgentFailure>> {
        Box::pin(async move { self.grant_recipient_consent_record(consent).await })
    }

    fn find_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<RecipientConsent>, AgentFailure>> {
        Box::pin(async move { self.recipient_consent(consent_id).await })
    }

    fn revoke_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move { self.revoke_recipient_consent_record(consent_id).await })
    }

    fn prune_expired<'a>(
        &'a self,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        Box::pin(async move { self.prune_expired_recipient_consents(now_unix_ms).await })
    }
}
