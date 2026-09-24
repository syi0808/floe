//! What a remote view read is assembled from on this host.
//!
//! Which grant admits a read, whether the producer is the one the Person
//! pinned, and whether a recorded dependency may still be relied on are all
//! decided by Access and sequenced by Context. What is left here is the wiring:
//! the paired server this device talks to, the vault that holds the Person's
//! keys and grants, and the pairing identity both sides quote.

use std::{future::Future, pin::Pin};

use floe_access::{
    DependencyAuthorization, DependencyLiveness, DependencyResolver, RemoteCallWindow,
    RemotePairingIdentity, remote_dependency_live,
};
use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_provider_adapters::sources::AuthorizedSourceClient;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

pub(crate) struct RemoteViewReader<'a, Keys: VaultKeyProvider> {
    pub(crate) vault: &'a EncryptedAgentVault<Keys>,
    pub(crate) source_client: &'a floe_provider_adapters::sources::ServerSourceClient,
    pub(crate) person_id: floe_kernel::PersonId,
    /// The pairing identity is compared as text on both sides of the wire.
    pub(crate) person_text: String,
    pub(crate) client_id: &'a str,
    pub(crate) device_id: &'a str,
}

impl<'a, Keys: VaultKeyProvider> RemoteViewReader<'a, Keys> {
    pub(crate) fn new(
        vault: &'a EncryptedAgentVault<Keys>,
        source_client: &'a floe_provider_adapters::sources::ServerSourceClient,
        person_id: floe_kernel::PersonId,
        client_id: &'a str,
        device_id: &'a str,
    ) -> Self {
        Self {
            vault,
            source_client,
            person_id,
            person_text: person_id.to_string(),
            client_id,
            device_id,
        }
    }

    fn pairing(&self) -> RemotePairingIdentity<'_> {
        RemotePairingIdentity {
            person_id: &self.person_text,
            client_id: self.client_id,
            device_id: self.device_id,
        }
    }

    fn transport(&self) -> AuthorizedSourceClient<'_, EncryptedAgentVault<Keys>> {
        AuthorizedSourceClient::new(self.source_client, self.vault)
    }
}

pub(crate) struct RemoteDependencyResolver<'a, Keys: VaultKeyProvider> {
    pub(crate) reader: &'a RemoteViewReader<'a, Keys>,
}

impl<Keys: VaultKeyProvider> floe_context::SourceReader for RemoteViewReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        request: &'a floe_context::SourceReadRequest,
    ) -> Pin<Box<dyn Future<Output = Result<floe_context::SourceRead, AgentFailure>> + Send + 'a>>
    {
        Box::pin(async move {
            let (payload, bindings) = floe_context::read_remote_view(
                self.vault,
                &self.transport(),
                self.person_id,
                self.pairing(),
                request.source().as_str(),
                request.consumer().identifier(),
                request.query().clone(),
                &RemoteCallWindow {
                    deadline: request.deadline(),
                    cancellation: request.cancellation().clone(),
                },
                request.process_incarnation_id(),
                request.query_fingerprint(),
            )
            .await?;
            Ok(floe_context::SourceRead::with_bindings(
                request.source().clone(),
                payload,
                bindings,
            ))
        })
    }
}

impl<Keys: VaultKeyProvider> floe_context::SourceReader for &RemoteViewReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        request: &'a floe_context::SourceReadRequest,
    ) -> Pin<Box<dyn Future<Output = Result<floe_context::SourceRead, AgentFailure>> + Send + 'a>>
    {
        <RemoteViewReader<'_, Keys> as floe_context::SourceReader>::read(*self, request)
    }
}

impl<Keys: VaultKeyProvider> DependencyLiveness for RemoteDependencyResolver<'_, Keys> {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure> {
        remote_dependency_live(dependency, self.reader.person_id, chrono::Utc::now())
    }
}

impl<Keys: VaultKeyProvider> DependencyResolver for RemoteDependencyResolver<'_, Keys> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a DependencyAuthorization,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            floe_context::authorize_remote_dependency(
                self.reader.vault,
                &self.reader.transport(),
                self.reader.person_id,
                self.reader.pairing(),
                dependency,
                request,
            )
            .await
        })
    }
}
