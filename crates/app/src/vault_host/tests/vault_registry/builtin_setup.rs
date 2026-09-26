use super::*;
use floe_execution::Cancellation;
use floe_experts::{ExpertInstallOperation, RegistryConfiguration, RegistryConfigurationTarget};

fn operation(instance_id: Uuid, expected_revision: u64) -> ExpertInstallOperation {
    ExpertInstallOperation { instance_id, expected_revision, operation_id: Uuid::new_v4() }
}

#[tokio::test]
async fn generic_bundle_install_is_source_independent_and_idempotent() {
    let fixture = Fixture::new().await;
    let request = operation(fixture.vault.registry_instance_id(), 0);
    let manifests = floe_experts_builtin::manifests();
    let installed = fixture.vault.install_expert_bundle(request.clone(), &manifests, Cancellation::default()).await.unwrap();
    assert_eq!(installed.receipt.installed.len(), manifests.len());
    assert_eq!(fixture.vault.enabled_expert_cards().await.unwrap().len(), manifests.len());
    assert_eq!(fixture.vault.install_expert_bundle(request.clone(), &manifests, Cancellation::default()).await.unwrap(), installed);
    assert_eq!(fixture.vault.expert_registry().await.unwrap().unwrap().revision, installed.registry.revision);
    let mut changed = manifests.clone();
    changed[0].prompt_contract.revision += 1;
    assert_eq!(fixture.vault.install_expert_bundle(request, &changed, Cancellation::default()).await, Err(AgentFailure::Conflict));
}

#[tokio::test]
async fn generic_assignment_disablement_survives_reopen_and_ensure() {
    let mut fixture = Fixture::new().await;
    let request = operation(fixture.vault.registry_instance_id(), 0);
    let manifests = floe_experts_builtin::manifests();
    let result = fixture.vault.install_expert_bundle(request.clone(), &manifests, Cancellation::default()).await.unwrap();
    let assignment_id = result.receipt.installed[0].assignment_id;
    let disabled = fixture.vault.configure_registry(RegistryConfiguration {
        instance_id: request.instance_id,
        expected_revision: result.registry.revision,
        target: RegistryConfigurationTarget::Assignment { id: assignment_id, enabled: false },
    }, Cancellation::default()).await.unwrap();
    assert_eq!(fixture.vault.enabled_expert_cards().await.unwrap().len(), manifests.len() - 1);
    drop(fixture.vault);
    fixture.vault = EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone()).await.unwrap();
    let persisted = fixture.vault.expert_install_overview(&floe_experts::manifest_set_digest(&manifests).unwrap()).await.unwrap().unwrap();
    assert_eq!(persisted.receipt.operation_id, request.operation_id);
    assert_eq!(persisted.registry.revision, disabled.revision);
    assert_eq!(fixture.vault.install_expert_bundle(request, &manifests, Cancellation::default()).await.unwrap().registry.revision, disabled.revision);
    assert_eq!(fixture.vault.enabled_expert_cards().await.unwrap().len(), manifests.len() - 1);
}
