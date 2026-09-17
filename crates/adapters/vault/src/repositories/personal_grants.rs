//! The Person's own grants, as their vault holds them.
//!
//! Context asks what the Person has granted and what subject they reviewed; the
//! vault is where those records live. Nothing here decides whether a read is
//! allowed — it only reports what was recorded.

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

    fn reviewed_subject<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(async move { self.vault.personal_grant_subject_fingerprint(grant).await })
    }

    fn consumer_policy<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<floe_access::ConsumerPolicyAuthority, AgentFailure>> {
        Box::pin(async move { self.vault.personal_grant_consumer_policy(grant).await })
    }

    fn feasibility_query<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<floe_access::FeasibilityGrantQuery, AgentFailure>> {
        Box::pin(async move { self.vault.personal_feasibility_query(grant).await })
    }
}

/// Where a reviewed personal grant is committed.
///
/// Every write below is one transaction with its own authority check; Access
/// decides whether the review was admissible in the first place.
impl<Keys: VaultKeyProvider> floe_access::PersonalGrantStore for EncryptedAgentVault<Keys> {
    fn grants<'a>(
        &'a self,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<floe_access::DataAccessGrant>, AgentFailure>> {
        Box::pin(async move { self.list_data_access_grants(limit).await })
    }

    fn reviewed_subject<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(async move { self.personal_grant_subject_fingerprint(grant).await })
    }

    fn selected_handles<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<Vec<String>, AgentFailure>> {
        Box::pin(async move { self.personal_grant_selected_handles(grant).await })
    }

    fn feasibility_query<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<floe_access::FeasibilityGrantQuery, AgentFailure>> {
        Box::pin(async move { self.personal_feasibility_query(grant).await })
    }

    fn review_grant<'a>(
        &'a self,
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        native_subject_fingerprint: &'a str,
        expected: Option<(floe_access::GrantId, floe_access::GrantAuthority)>,
    ) -> BoxFuture<'a, Result<floe_access::DataAccessGrant, AgentFailure>> {
        Box::pin(async move {
            self.review_personal_grant(source, scope, native_subject_fingerprint, expected)
                .await
        })
    }

    fn review_grant_with_selection<'a>(
        &'a self,
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        native_subject_fingerprint: &'a str,
        expected: Option<(floe_access::GrantId, floe_access::GrantAuthority)>,
        selected_handles: &'a [String],
    ) -> BoxFuture<'a, Result<floe_access::DataAccessGrant, AgentFailure>> {
        Box::pin(async move {
            self.review_personal_grant_with_selection(
                source,
                scope,
                native_subject_fingerprint,
                expected,
                selected_handles,
            )
            .await
        })
    }

    fn review_grant_with_feasibility_query<'a>(
        &'a self,
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        native_subject_fingerprint: &'a str,
        expected: Option<(floe_access::GrantId, floe_access::GrantAuthority)>,
        query: floe_access::FeasibilityGrantQuery,
    ) -> BoxFuture<'a, Result<floe_access::DataAccessGrant, AgentFailure>> {
        Box::pin(async move {
            self.review_personal_grant_with_feasibility_query(
                source,
                scope,
                native_subject_fingerprint,
                expected,
                query,
            )
            .await
        })
    }

    fn pause_grant<'a>(
        &'a self,
        grant: floe_access::GrantId,
        authority: floe_access::GrantAuthority,
    ) -> BoxFuture<'a, Result<floe_access::DataAccessGrant, AgentFailure>> {
        Box::pin(async move { self.pause_personal_grant(grant, authority).await })
    }
}
