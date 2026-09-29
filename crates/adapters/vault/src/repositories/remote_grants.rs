//! The Person's remote view grants, as their vault holds them.
//!
//! Access decides whether a grant admits a read; this verifies the producer's
//! signature with the Person's own keys and commits the activation in one
//! transaction. It makes no admission judgment of its own.

use floe_access::{
    DataAccessGrant, RemoteGrantStore, RemotePairingIdentity,
    RemoteProducerIdentity, RemoteSourceQuery, RemoteViewSourceReference, SignedSourcePreview,
};
use floe_access::{GrantAuthority, GrantId, GrantScope, GrantSourceBinding};
use floe_agent_contract::{AgentFailure, BoxFuture};

use crate::{AccessGrantActivation, EncryptedAgentVault, VaultKeyProvider};

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
    ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
        Box::pin(async move {
            let resource =
                floe_context_contract::connection_view_resource(view_id, &source.connection_id())
                    .map_err(|_| AgentFailure::InvalidInput)?;
            self.data_access_grant_for_source_resource(source, &resource)
                .await
        })
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
            let resource =
                floe_context_contract::connection_view_resource(view_id, &source.connection_id())
                    .map_err(|_| AgentFailure::InvalidInput)?;
            if scope.resources() != [resource] {
                return Err(AgentFailure::InvalidInput);
            }
            self.activate_access_grants(vec![AccessGrantActivation {
                grant_id,
                expected,
                source,
                scope,
            }])
            .await?
            .into_iter()
            .next()
            .ok_or(AgentFailure::VaultUnavailable)
        })
    }
}
