//! Owned physical source handles for one ready Vault generation. Context and
//! Access own every re-authorization; assembly does not retain turn borrows.
use floe_access::{
    AuthorizationSigner, DependencyAuthorization, DependencyResolver, SourcePreviewVerifier,
};
use floe_context_contract::ContextDependency;
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, OwnerActor};
use floe_provider_adapters::gateway::GatewayCredentialStore;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use std::sync::Arc;

pub(crate) struct OwnedSourceResolver<Keys: VaultKeyProvider> {
    pub core: Arc<crate::FloeCore>,
    pub vault: Arc<EncryptedAgentVault<Keys>>,
    pub local_context: Arc<crate::LocalContextHost>,
    pub credentials: Arc<GatewayCredentialStore>,
    pub signer: Arc<dyn AuthorizationSigner>,
    pub verifier: Arc<dyn SourcePreviewVerifier>,
    pub actor: OwnerActor,
}
impl<Keys: VaultKeyProvider> DependencyResolver for OwnedSourceResolver<Keys> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            if dependency.person_id() != self.actor.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let grant = self
                .vault
                .get_data_access_grant(dependency.grant_id())
                .await?;
            floe_access::validate_grant_dependency(&grant, dependency)?;
            let connector = dependency.source().connector().as_str();
            if matches!(connector, "calendar.event_kit" | "calendar.android") {
                let resolver = super::calendar_access::NativeCalendarDependencyResolver {
                    core: &self.core,
                    vault: &self.vault,
                    person_id: self.actor.person_id,
                    device_id: &self.actor.device_id,
                };
                return resolver.authorize(dependency, request).await;
            }
            if floe_access::is_device_local_source(connector) {
                return floe_context::authorize_personal_dependency(
                    &super::personal_grants::CorePersonalConnections { core: &self.core },
                    self.vault.as_ref(),
                    &super::personal_grants::native_driver(&self.local_context),
                    self.actor.person_id,
                    &self.actor.device_id,
                    dependency,
                    request.deadline,
                    &request.cancellation,
                )
                .await;
            }
            let client =
                floe_provider_adapters::sources::ServerSourceClient::from_current_connection(
                    &self.credentials,
                    &self.actor.person_id.to_string(),
                    &self.actor.device_id,
                )
                .await?
                .ok_or(AgentFailure::PolicyDenied)?;
            let transport = floe_provider_adapters::sources::AuthorizedSourceClient::new(
                &client,
                self.signer.as_ref(),
            );
            let person_text = self.actor.person_id.to_string();
            let pairing = floe_access::RemotePairingIdentity {
                person_id: &person_text,
                device_id: &self.actor.device_id,
                client_id: client.source().client_id(),
            };
            floe_context::authorize_remote_dependency(
                self.vault.as_ref(),
                self.verifier.as_ref(),
                self.core.store.as_ref(),
                self.core.store.as_ref(),
                &transport,
                self.actor.person_id,
                pairing,
                dependency,
                request,
            )
            .await
        })
    }
}
