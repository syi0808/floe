//! Assembly copies validated shipped registration values once. Access owns all
//! grant category, purpose, consumer and processing policy derived from them.
use floe_access::{GrantConsumer, TrustedConsumerCatalog, TrustedConsumerRegistration};
use floe_kernel::AgentFailure;

pub(crate) struct StaticTrustedConsumerCatalog { registrations: Vec<TrustedConsumerRegistration> }
impl StaticTrustedConsumerCatalog {
    pub(crate) fn shipped() -> Result<Self, AgentFailure> {
        let mut registrations = Vec::new();
        for manifest in floe_experts_builtin::manifests() {
            manifest.validate()?;
            if manifest.source_requirements.is_empty() { continue; }
            let mut capabilities = Vec::new();
            for requirement in &manifest.source_requirements {
                let capability = floe_access::trusted_view_capability(&requirement.capability)?;
                if !capabilities.contains(&capability) { capabilities.push(capability); }
            }
            registrations.push(TrustedConsumerRegistration {
                package_identity: manifest.package.id.clone(),
                manifest_revision: manifest.definition.definition_revision,
                declared_view_capabilities: capabilities,
                consumer_identity: GrantConsumer::builtin(manifest.package.id).map_err(|_| AgentFailure::InvalidInput)?,
            });
        }
        Ok(Self { registrations })
    }
}
impl TrustedConsumerCatalog for StaticTrustedConsumerCatalog {
    fn registrations(&self) -> &[TrustedConsumerRegistration] { &self.registrations }
}
