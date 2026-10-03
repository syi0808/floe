//! Immutable trusted package registrations copied at host assembly.
use floe_access::{GrantConsumer, TrustedConsumerCatalog, TrustedConsumerRegistration};
use floe_experts::ExpertManifest;
use floe_kernel::AgentFailure;

pub struct ContextTrustedConsumerCatalog { registrations: Vec<TrustedConsumerRegistration> }
impl ContextTrustedConsumerCatalog {
    /// The caller supplies the already trusted namespace. Identity equality
    /// validates the pair and never changes an extension into a builtin.
    pub fn new(manifests: Vec<(GrantConsumer, ExpertManifest)>) -> Result<Self, AgentFailure> {
        if manifests.len() > 64 { return Err(AgentFailure::BudgetExceeded); }
        let mut registrations = Vec::new();
        for (consumer_identity, manifest) in manifests {
            manifest.validate()?;
            if consumer_identity.identifier() != manifest.package.id { return Err(AgentFailure::PolicyDenied); }
            if manifest.source_requirements.is_empty() { continue; }
            let mut declared_view_capabilities = Vec::new();
            for requirement in &manifest.source_requirements {
                if matches!(requirement.capability.as_str(), "floe.tasks" | "floe.notes" | "memory.confirmed" | "relationships.confirmed_interactions") { continue; }
                let capability = floe_access::trusted_view_capability(&requirement.capability)?;
                if !declared_view_capabilities.contains(&capability) { declared_view_capabilities.push(capability); }
            }
            if declared_view_capabilities.is_empty() { continue; }
            let registration = TrustedConsumerRegistration { package_identity: manifest.package.id, manifest_revision: manifest.definition.definition_revision, declared_view_capabilities, consumer_identity };
            if registrations.iter().any(|prior: &TrustedConsumerRegistration| prior.consumer_identity == registration.consumer_identity) { return Err(AgentFailure::Conflict); }
            registrations.push(registration);
        }
        Ok(Self { registrations })
    }
}
impl TrustedConsumerCatalog for ContextTrustedConsumerCatalog {
    fn registrations(&self) -> &[TrustedConsumerRegistration] { &self.registrations }
}
