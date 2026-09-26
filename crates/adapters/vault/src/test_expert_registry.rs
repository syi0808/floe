use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_VERSION, AgentCard, AgentDefinition, AgentFailure, DataClass,
    ModelPlacement, PackageKind, PackageRef,
};
use floe_experts::{
    AgentRegistry, ContractRef, EXPERT_MANIFEST_SCHEMA_VERSION, ExpertAdmissionIdentity,
    ExpertInstallOperation, ExpertManifest,
};
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

pub(crate) async fn install(
    vault: &EncryptedAgentVault<impl VaultKeyProvider>,
    person_id: floe_kernel::PersonId,
    definition_revision: u64,
) -> Result<ExpertAdmissionIdentity, AgentFailure> {
    let package = PackageRef {
        kind: PackageKind::Expert,
        id: "floe.builtin.schedule".into(),
        version: "1.0.0".into(),
    };
    let manifest = ExpertManifest {
        schema_version: EXPERT_MANIFEST_SCHEMA_VERSION,
        package: package.clone(),
        publisher: "floe.test".into(),
        definition: AgentDefinition {
            card: AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: A2A_PROTOCOL_VERSION.into(),
                id: package.id.clone(),
                version: package.version.clone(),
                name: "Test Schedule".into(),
                description: "Test schedule endpoint".into(),
                domain_tags: vec![],
                skills: vec![],
                supported_placements: vec![ModelPlacement::DeviceLocal],
            },
            definition_revision,
        },
        data_class: DataClass::Personal,
        prompt_contract: ContractRef {
            id: "floe.test.schedule.prompt".into(),
            revision: 1,
        },
        result_contracts: vec![],
        source_requirements: vec![],
        capability_requirements: vec![],
        state_schema_version: 1,
    };
    let instance_id = vault.registry_instance_id();
    let mut registry = AgentRegistry::new(instance_id);
    let receipt = registry.install_bundle(
        person_id,
        &ExpertInstallOperation {
            instance_id,
            expected_revision: 0,
            operation_id: Uuid::new_v4(),
        },
        &[manifest],
    )?;
    vault
        .initialize_expert_registry(&registry.snapshot())
        .await?;
    let installed = &receipt.installed[0];
    Ok(ExpertAdmissionIdentity {
        registry_instance_id: instance_id,
        assignment_id: installed.assignment_id,
        installation_id: installed.installation_id,
        package,
        definition_revision,
    })
}
