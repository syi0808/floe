//! Source owner composition. No grant policy or operation state is owned here.
use std::sync::Arc;
use floe_access::{AccessService, AuthorizationSigner, GatewayTrustReader, SourcePreviewVerifier};
use floe_connections::{ConnectionsService, GatewayPairingService};
use floe_kernel::AgentFailure;
use floe_provider_adapters::gateway::{GatewayCredentialStore, GatewayIntegrationAdapter, GatewayPairingAdapter, GatewayProofVerifier, GatewaySourceCleanup, GatewaySourcePreviewVerifier};
use floe_vault::{EncryptedAgentVault, VaultAuthorizationSigner, VaultEnrollmentSigner, VaultKeyProvider};

pub(crate) struct SourceServices {
    pub access: Arc<AccessService>,
    pub connections: Arc<ConnectionsService>,
    pub pairing: Arc<GatewayPairingService>,
    pub gateway_credentials: Arc<GatewayCredentialStore>,
    pub source_verifier: Arc<dyn SourcePreviewVerifier>,
    pub authorization_signer: Arc<dyn AuthorizationSigner>,
    pub dependency_resolver: Arc<dyn floe_access::DependencyResolver>,
    pub evidence_reader: Arc<dyn floe_context::EvidenceReader>,
}
impl SourceServices {
    pub(crate) fn new<Keys: VaultKeyProvider + 'static>(vault: Arc<EncryptedAgentVault<Keys>>, core: Arc<crate::FloeCore>, local_context: Arc<crate::LocalContextHost>, actor: floe_kernel::OwnerActor) -> Result<Self, AgentFailure> {
        actor.validate()?;
        if vault.person_id() != actor.person_id { return Err(AgentFailure::PolicyDenied); }
        let dependency_vault = vault.clone();
        let evidence_reader: Arc<dyn floe_context::EvidenceReader> = Arc::new(floe_vault::ContextEvidenceReader::new(vault.clone()));
        let dependency_local = local_context.clone();
        let trust: Arc<dyn GatewayTrustReader> = vault.clone();
        let gateway_credentials = Arc::new(GatewayCredentialStore::new(trust.clone()));
        let source_verifier: Arc<dyn SourcePreviewVerifier> = Arc::new(GatewaySourcePreviewVerifier::new(trust));
        let authorization_signer: Arc<dyn AuthorizationSigner> = Arc::new(VaultAuthorizationSigner::new(vault.clone(), Arc::new(GatewayProofVerifier)));
        let enrollment = Arc::new(VaultEnrollmentSigner::new(vault.clone(), Arc::new(GatewayProofVerifier)));
        let pairing_adapter = Arc::new(GatewayPairingAdapter::new(gateway_credentials.as_ref().clone(), enrollment, vault.clone()));
        let pairing = Arc::new(GatewayPairingService::new(vault.clone(), pairing_adapter.clone(), pairing_adapter.clone(), pairing_adapter));
        let access = Arc::new(AccessService::new(vault.clone(), Arc::new(crate::first_party_observe::StaticTrustedConsumerCatalog::shipped()?), Arc::new(floe_access::SystemAccessClock)));
        let evidence = Arc::new(crate::vault_host::review_snapshot::HostSourceEvidence { local_context: local_context.clone(), credentials: gateway_credentials.clone(), signer: authorization_signer.clone(), verifier: source_verifier.clone() });
        let cleanup = Arc::new(GatewaySourceCleanup::new(gateway_credentials.as_ref().clone()));
        let integrations = Arc::new(GatewayIntegrationAdapter::new(gateway_credentials.as_ref().clone(), source_verifier.clone()));
        let native_setup = Arc::new(crate::vault_host::review_snapshot::HostNativeSourceSetup { local_context: local_context.clone() });
        let source_catalog = Arc::new(crate::vault_host::review_snapshot::HostSourceCatalog { local_context });
        let connections = Arc::new(ConnectionsService::new(floe_connections::ConnectionsDependencies { sources: core.store.clone(), access: access.clone(), evidence, cleanup,
            pairing: pairing.clone(), gateways: gateway_credentials.clone(), remote_integrations: integrations, products: vault, source_catalog, native_setup }));
        let dependency_resolver: Arc<dyn floe_access::DependencyResolver> = Arc::new(crate::vault_host::source_resolver::OwnedSourceResolver {
            core, vault: dependency_vault, local_context: dependency_local, credentials: gateway_credentials.clone(), signer: authorization_signer.clone(), verifier: source_verifier.clone(), actor,
        });
        Ok(Self { access, connections, pairing, gateway_credentials, source_verifier, authorization_signer, dependency_resolver, evidence_reader })
    }
}
