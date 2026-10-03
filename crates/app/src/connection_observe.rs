//! Source owner composition. No grant policy or operation state is owned here.
use floe_access::{AccessService, AuthorizationSigner, GatewayTrustReader, SourcePreviewVerifier};
use floe_connections::{ConnectionsService, GatewayPairingService};
use floe_kernel::AgentFailure;
use floe_provider_adapters::gateway::{
    GatewayCredentialStore, GatewayIntegrationAdapter, GatewayPairingAdapter, GatewayProofVerifier,
    GatewaySourceCleanup, GatewaySourcePreviewVerifier,
};
use floe_vault::{
    EncryptedAgentVault, VaultAuthorizationSigner, VaultEnrollmentSigner, VaultKeyProvider,
};
use std::sync::Arc;

pub(crate) struct SourceServices {
    pub access: Arc<AccessService>,
    pub connections: Arc<ConnectionsService>,
    pub pairing: Arc<GatewayPairingService>,
    pub gateway_credentials: Arc<GatewayCredentialStore>,
    pub source_verifier: Arc<dyn SourcePreviewVerifier>,
    pub authorization_signer: Arc<dyn AuthorizationSigner>,
    pub dependency_resolver: Arc<dyn floe_access::DependencyResolver>,
    pub evidence_reader: Arc<dyn floe_context::EvidenceReader>,
    pub personal: Arc<floe_provider_adapters::sources::NativePersonalDriver>,
    pub transport: Arc<dyn floe_context::ExpertSourceTransport>,
}
impl SourceServices {
    pub(crate) fn new<Keys: VaultKeyProvider + 'static>(
        vault: Arc<EncryptedAgentVault<Keys>>,
        core: Arc<crate::FloeCore>,
        local_context: Arc<crate::LocalContextHost>,
        actor: floe_kernel::OwnerActor,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        if vault.person_id() != actor.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let evidence_reader: Arc<dyn floe_context::EvidenceReader> =
            Arc::new(floe_vault::ContextEvidenceReader::new(vault.clone()));
        let trust: Arc<dyn GatewayTrustReader> = vault.clone();
        let gateway_credentials = Arc::new(GatewayCredentialStore::new(
            trust.clone(),
            vault.clone(),
            &actor,
        )?);
        let source_verifier: Arc<dyn SourcePreviewVerifier> =
            Arc::new(GatewaySourcePreviewVerifier::new(trust));
        let authorization_signer: Arc<dyn AuthorizationSigner> = Arc::new(
            VaultAuthorizationSigner::new(vault.clone(), Arc::new(GatewayProofVerifier)),
        );
        let enrollment = Arc::new(VaultEnrollmentSigner::new(
            vault.clone(),
            Arc::new(GatewayProofVerifier),
        ));
        let pairing_adapter = Arc::new(GatewayPairingAdapter::new(
            vault.clone(),
            enrollment,
            vault.clone(),
            &actor,
        )?);
        let pairing = Arc::new(GatewayPairingService::new(vault.clone(), pairing_adapter));
        let trusted = floe_experts_builtin::manifests()
            .into_iter()
            .map(|manifest| {
                let consumer = floe_access::GrantConsumer::builtin(manifest.package.id.clone())
                    .map_err(|_| AgentFailure::InvalidInput)?;
                Ok((consumer, manifest))
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        let access = Arc::new(AccessService::new(
            vault.clone(),
            Arc::new(floe_context::ContextTrustedConsumerCatalog::new(trusted)?),
            Arc::new(floe_access::SystemAccessClock),
        ));
        let personal = Arc::new(floe_provider_adapters::sources::NativePersonalDriver {
            attention: local_context.attention_handle(),
            personal: local_context.personal_handle(),
            observations: local_context.observations_handle(),
        });
        let transport: Arc<dyn floe_context::ExpertSourceTransport> =
            Arc::new(floe_provider_adapters::sources::ExpertSourceAdapter::new(
                local_context.calendar_handle(),
                core.store.clone(),
                core.product_gateway.clone(),
            ));
        let metadata = Arc::new(
            floe_provider_adapters::sources::NativeSourceMetadataAdapter::new(
                local_context.calendar_handle(),
                local_context.personal_handle(),
                local_context.attention_handle(),
            ),
        );
        let evidence = Arc::new(floe_context::ContextSourceReview::new(
            metadata.clone(),
            personal.clone(),
            transport.clone(),
        ));
        let dependency_resolver: Arc<dyn floe_access::DependencyResolver> =
            Arc::new(floe_context::ContextDependencyResolver::new(
                actor,
                core.store.clone(),
                vault.clone(),
                personal.clone(),
                transport.clone(),
                core.lease_registry.clone(),
            )?);
        let cleanup = Arc::new(GatewaySourceCleanup::new(
            gateway_credentials.as_ref().clone(),
        ));
        let integrations = Arc::new(GatewayIntegrationAdapter::new(
            gateway_credentials.as_ref().clone(),
            source_verifier.clone(),
        ));
        let connections = Arc::new(ConnectionsService::new(
            floe_connections::ConnectionsDependencies {
                sources: core.store.clone(),
                access: access.clone(),
                evidence,
                cleanup,
                pairing: pairing.clone(),
                gateways: vault.clone(),
                remote_integrations: integrations,
                products: vault,
                source_catalog: metadata.clone(),
                native_setup: metadata,
            },
        ));
        Ok(Self {
            access,
            connections,
            pairing,
            gateway_credentials,
            source_verifier,
            authorization_signer,
            dependency_resolver,
            evidence_reader,
            personal,
            transport,
        })
    }
}
