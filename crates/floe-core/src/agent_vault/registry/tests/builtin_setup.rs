use super::*;

#[tokio::test]
async fn builtin_assignments_and_card_visibility_survive_vault_reopen() {
    let mut fixture = Fixture::new().await;
    let request = BuiltinExpertSetup {
        instance_id: fixture.vault.registry_instance_id(),
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        sources: vec![BuiltinSourceBinding {
            source: BuiltinContextSource::Mail,
            view_handle: Uuid::new_v4(),
            state: BuiltinSourceState::Available,
        }],
    };
    let result = fixture
        .vault
        .install_builtin_experts(request.clone(), Cancellation::default())
        .await
        .unwrap();
    let communication = result
        .setup
        .assignments
        .iter()
        .find(|entry| entry.expert == BuiltinExpertKind::Communication)
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
