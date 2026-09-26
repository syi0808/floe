use super::*;
use floe_execution::Cancellation;
use floe_experts::AgentRegistry;
use floe_experts::BuiltinExpertSetup;
use floe_experts::RegistryConfiguration;
use floe_experts::RegistryConfigurationTarget;
use floe_experts_builtin::BuiltinExpertKind;

#[tokio::test]
async fn enabled_builtin_install_after_existing_registry_requires_the_scoped_entry_point() {
    let fixture = Fixture::new().await;
    fixture.sample().await;
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    let request = BuiltinExpertSetup {
        instance_id: fixture.vault.registry_instance_id(),
        expected_revision: before.revision,
        setup_id: Uuid::new_v4(),
    };
    let mut staged =
        AgentRegistry::restore(before.clone(), fixture.vault.registry_instance_id()).unwrap();
    staged
        .install_builtin_experts_enabled(
            fixture.person,
            &request,
            &crate::vault_host::builtin_setup_specs(),
        )
        .unwrap();
    assert_eq!(
        fixture
            .vault
            .save_expert_registry(before.revision, &staged.snapshot())
            .await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap().unwrap(),
        before
    );
    let installed = fixture
        .vault
        .install_builtin_experts_enabled(
            request.clone(),
            &crate::vault_host::builtin_setup_specs(),
            Cancellation::default(),
        )
        .await
        .unwrap();
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(
        fixture
            .vault
            .install_builtin_experts_enabled(
                request,
                &crate::vault_host::builtin_setup_specs(),
                Cancellation::default()
            )
            .await
            .unwrap(),
        installed
    );
}

#[tokio::test]
async fn builtin_assignments_and_card_visibility_survive_vault_reopen() {
    let mut fixture = Fixture::new().await;
    let request = BuiltinExpertSetup {
        instance_id: fixture.vault.registry_instance_id(),
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
    };
    let result = fixture
        .vault
        .install_builtin_experts(
            request.clone(),
            &crate::vault_host::builtin_setup_specs(),
            Cancellation::default(),
        )
        .await
        .unwrap();
    let communication = result
        .setup
        .assignments
        .iter()
        .find(|entry| entry.expert.as_str() == BuiltinExpertKind::Communication.package_id())
        .unwrap();
    let mut revision = result.registry.revision;
    for (id, installation) in [
        (communication.tool_installation_id, true),
        (communication.expert_installation_id, true),
    ] {
        fixture
            .vault
            .configure_registry(
                RegistryConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: revision,
                    target: RegistryConfigurationTarget::Installation {
                        id,
                        enabled: installation,
                    },
                },
                Cancellation::default(),
            )
            .await
            .unwrap();
        revision += 1;
    }
    for id in [
        communication.tool_assignment_id,
        communication.expert_assignment_id,
    ] {
        fixture
            .vault
            .configure_registry(
                RegistryConfiguration {
                    instance_id: request.instance_id,
                    expected_revision: revision,
                    target: RegistryConfigurationTarget::Assignment { id, enabled: true },
                },
                Cancellation::default(),
            )
            .await
            .unwrap();
        revision += 1;
    }
    assert_eq!(
        fixture
            .vault
            .enabled_expert_cards()
            .await
            .unwrap()
            .iter()
            .map(|card| card.id.as_str())
            .collect::<Vec<_>>(),
        ["floe.builtin.communication"]
    );

    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    let persisted = fixture
        .vault
        .builtin_expert_overview()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.setup.setup_id, request.setup_id);
    assert_eq!(persisted.registry.revision, revision);
    assert_eq!(fixture.vault.enabled_expert_cards().await.unwrap().len(), 1);

    fixture
        .vault
        .configure_registry(
            RegistryConfiguration {
                instance_id: request.instance_id,
                expected_revision: revision,
                target: RegistryConfigurationTarget::Assignment {
                    id: communication.expert_assignment_id,
                    enabled: false,
                },
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert!(
        fixture
            .vault
            .enabled_expert_cards()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn builtin_setup_is_source_independent_and_reinstall_is_idempotent() {
    let fixture = Fixture::new().await;
    let request = BuiltinExpertSetup {
        instance_id: fixture.vault.registry_instance_id(),
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
    };
    let installed = fixture
        .vault
        .install_builtin_experts_enabled(
            request.clone(),
            &crate::vault_host::builtin_setup_specs(),
            Cancellation::default(),
        )
        .await
        .unwrap();
    // All eight built-ins are enabled without any source input.
    assert_eq!(installed.setup.assignments.len(), 8);
    assert_eq!(fixture.vault.enabled_expert_cards().await.unwrap().len(), 8);

    // Reinstall with the same setup id is idempotent and never advances
    // the revision for source/connection changes (there is no source input).
    let revision = installed.registry.revision;
    let replayed = fixture
        .vault
        .install_builtin_experts_enabled(
            request,
            &crate::vault_host::builtin_setup_specs(),
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(replayed, installed);
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    assert_eq!(after.revision, revision);
}
