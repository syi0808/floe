//! The Person's remote view grants, as their vault holds them.
//!
//! Access decides whether a grant admits a read; this verifies the producer's
//! signature with the Person's own keys and commits the activation in one
//! transaction. It makes no admission judgment of its own.

use floe_access::{
    DataAccessGrant, RemoteCalendarQuery, RemoteCalendarSourceReference, RemoteGrantBinding,
    RemoteGrantStore, RemotePairingIdentity, RemoteProducerIdentity, RemoteSourceQuery,
    RemoteViewSourceReference, SignedCalendarPreview, SignedSourcePreview,
};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_access::{GrantAuthority, GrantId, GrantScope, GrantSourceBinding, SourceAuthority};

use crate::{EncryptedAgentVault, VaultKeyProvider};

impl<Keys: VaultKeyProvider> RemoteGrantStore for EncryptedAgentVault<Keys> {
    fn pinned_producer<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(async move { self.remote_pinned_producer().await })
    }

    fn verify_view_source_preview<'a>(
        &'a self,
        preview: &'a SignedSourcePreview,
        pairing: RemotePairingIdentity<'a>,
        query: RemoteSourceQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>> {
        Box::pin(async move {
            self.verify_remote_view_source_preview(
                &preview.descriptor_b64url,
                &preview.producer_signature,
                pairing.person_id,
                pairing.client_id,
                pairing.device_id,
                query.view_id,
                query.connector_id,
                query.connection_id,
                query.resource,
            )
            .await
        })
    }

    fn grants<'a>(
        &'a self,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
        Box::pin(async move { self.list_data_access_grants(limit).await })
    }

    fn find_view_grant<'a>(
        &'a self,
        view_id: &'a str,
        source: &'a GrantSourceBinding,
        consumer: &'a str,
    ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
        Box::pin(async move { self.find_remote_view_grant(view_id, source, consumer).await })
    }

    fn activate_view_grant<'a>(
        &'a self,
        view_id: &'a str,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
        Box::pin(async move {
            self.review_and_activate_remote_view_grant(
                view_id, grant_id, expected, source, scope, None,
            )
            .await
        })
    }

    fn verify_calendar_source_preview<'a>(
        &'a self,
        preview: &'a SignedCalendarPreview,
        pairing: RemotePairingIdentity<'a>,
        query: RemoteCalendarQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteCalendarSourceReference, AgentFailure>> {
        Box::pin(async move {
            self.verify_remote_calendar_source_preview(
                &preview.descriptor_b64url,
                &preview.producer_signature,
                pairing.person_id,
                pairing.client_id,
                pairing.device_id,
                query.connector_id,
                query.connection_id,
                query.resource,
            )
            .await
        })
    }

    fn activate_calendar_grant<'a>(
        &'a self,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
        Box::pin(async move {
            self.review_and_activate_remote_calendar_grant(
                grant_id, expected, source, scope, None,
            )
            .await
        })
    }

    fn calendar_grant<'a>(
        &'a self,
        grant_id: GrantId,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
        Box::pin(async move { self.get_data_access_grant(grant_id).await })
    }

    fn pause_calendar_grant<'a>(
        &'a self,
        grant_id: GrantId,
        expected: GrantAuthority,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
        Box::pin(async move { self.pause_remote_calendar_grant(grant_id, expected).await })
    }

    fn view_grant_binding<'a>(
        &'a self,
        view_id: &'a str,
        connector_id: &'a str,
        connection_id: &'a str,
        source_authority: SourceAuthority,
    ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
        Box::pin(async move {
            let binding = self
                .remote_view_grant_binding(view_id, connector_id, connection_id, source_authority)
                .await?;
            Ok(RemoteGrantBinding {
                grant: binding.grant,
                consumer_policy: binding.consumer_policy,
            })
        })
    }
}
