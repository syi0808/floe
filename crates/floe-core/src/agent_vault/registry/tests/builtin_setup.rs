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

#[tokio::test]
async fn builtin_source_refresh_persists_updated_grants() {
    let mut fixture = Fixture::new().await;
    let mail_handle = Uuid::new_v4();
    let request = BuiltinExpertSetup {
        instance_id: fixture.vault.registry_instance_id(),
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        sources: vec![BuiltinSourceBinding {
            source: BuiltinContextSource::Mail,
            view_handle: mail_handle,
            state: BuiltinSourceState::Unavailable,
        }],
    };
    let installed = fixture
        .vault
        .install_builtin_experts_enabled(request, Cancellation::default())
        .await
        .unwrap();
    let communication = installed
        .setup
        .assignments
        .iter()
        .find(|entry| entry.expert == BuiltinExpertKind::Communication)
        .unwrap();

    let refreshed = fixture
        .vault
        .refresh_builtin_expert_sources(
            installed.registry.revision,
            vec![BuiltinSourceBinding {
                source: BuiltinContextSource::Mail,
                view_handle: mail_handle,
                state: BuiltinSourceState::Available,
            }],
            Cancellation::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        refreshed
            .setup
            .assignments
            .iter()
            .find(|entry| entry.expert == BuiltinExpertKind::Communication)
            .unwrap()
            .granted_view_handles,
        [mail_handle]
    );
    assert_eq!(refreshed.registry.revision, installed.registry.revision + 1);

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
    assert_eq!(
        persisted.setup.sources[0].state,
        BuiltinSourceState::Available
    );
    assert_eq!(
        persisted
            .registry
            .assignments
            .iter()
            .find(|assignment| assignment.id == communication.expert_assignment_id)
            .unwrap()
            .granted_view_count,
        1
    );
}
