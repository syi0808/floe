use super::*;
use std::os::unix::fs::PermissionsExt;

fn lifecycle_job(person: PersonId, command: crate::VaultLifecycleCommand) -> Job {
    let intent = LocalOperationIntent::VaultCommand(command);
    let caller = remote_caller(person, "activation-device");
    Job {
        person,
        id: Uuid::new_v4(),
        action: Box::new(intent.action(&caller)),
        command_identity: None,
        local_admission: Some(LocalOperationAdmission { caller, intent }),
        cancellation: Cancellation::default(),
        run_cancellations: Arc::new(floe_conversation::RunCancellationRegistry::default()),
        admission: Mutex::new(None),
        admission_ready: Condvar::new(),
        progress: Mutex::new(Progress::default()),
        finished: Condvar::new(),
        app_events: Arc::new(crate::events::AppEventBuffer::default()),
    }
}

#[test]
fn create_prepares_before_fresh_resume_and_disabled_all_reopens_idempotently() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let keys = Keys::default();
    let person = PersonId::new();
    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    assert_eq!(
        perform_vault_lifecycle(
            &worker,
            person,
            "activation-device",
            crate::VaultLifecycleCommand::Create
        )
        .state,
        Some(VaultState::Ready)
    );
    let before = perform(&worker, person, WorkerAction::Registry { change: None })
        .registry
        .unwrap();
    assert_eq!(
        before.assignments.len(),
        floe_experts_builtin::manifests().len()
    );
    let resumed = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Resume,
        },
    );
    assert_eq!(resumed.failure, None);
    assert!(resumed.session.is_some());
    assert_eq!(
        perform(&worker, person, WorkerAction::Registry { change: None })
            .registry
            .unwrap(),
        before
    );
    let mut configured = before;
    for assignment in configured.assignments.clone() {
        configured = perform(
            &worker,
            person,
            WorkerAction::Registry {
                change: Some(floe_experts::RegistryConfiguration {
                    instance_id: configured.instance_id,
                    expected_revision: configured.revision,
                    target: floe_experts::RegistryConfigurationTarget::Assignment {
                        id: assignment.id,
                        enabled: false,
                    },
                }),
            },
        )
        .registry
        .unwrap();
    }
    for _ in 0..3 {
        assert_eq!(perform(&worker, person, WorkerAction::Lock).failure, None);
        assert_eq!(
            perform(&worker, person, WorkerAction::Unlock).state,
            Some(VaultState::Ready)
        );
        assert_eq!(
            perform(&worker, person, WorkerAction::Registry { change: None })
                .registry
                .unwrap(),
            configured
        );
    }
    perform(&worker, person, WorkerAction::Lock);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let vault = EncryptedAgentVault::open(&root, person, keys)
            .await
            .unwrap();
        assert!(vault.enabled_expert_cards().await.unwrap().is_empty());
        let digest = floe_experts::manifest_set_digest(&floe_experts_builtin::manifests()).unwrap();
        assert!(
            vault
                .expert_install_overview(&digest)
                .await
                .unwrap()
                .is_some()
        );
        let open = OpenVault::activate(
            vault,
            Arc::new(FloeCore::open(":memory:").await.unwrap()),
            Arc::new(LocalContextHost::default()),
            CurrentSavedConnectionStore::fixed(None),
            conversation_turn::expert_dispatch::shipped_registrations(),
            RootAgentEnvironmentAdmission {
                device_id: "activation-device".into(),
                operation_id: Uuid::new_v4(),
                cancellation: Cancellation::default(),
            },
        )
        .await
        .unwrap();
        assert!(
            open.task_coordinator
                .environment(&person.to_string())
                .unwrap()
                .catalog()
                .cards
                .is_empty()
        );
    });
}

#[tokio::test]
async fn activation_binds_only_absent_registry_and_preserves_explicit_configuration() {
    for existing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let keys = Keys::default();
        let person = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        if existing {
            let mut manifest = floe_experts_builtin::manifests()
                .into_iter()
                .find(|manifest| manifest.package.id == "floe.builtin.focus-attention")
                .unwrap();
            manifest.package.id = "example.preexisting.expert".into();
            manifest.definition.card.id = manifest.package.id.clone();
            vault
                .install_expert_bundle(
                    floe_experts::ExpertInstallOperation {
                        instance_id: vault.registry_instance_id(),
                        expected_revision: 0,
                        operation_id: Uuid::new_v4(),
                    },
                    &[manifest],
                    Cancellation::default(),
                )
                .await
                .unwrap();
        }
        drop(vault);
        let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
        core.source_service()
            .establish_reviewed_native(
                person,
                floe_context_contract::ConnectorId::try_new("attention.macos").unwrap(),
                floe_context_contract::ConnectionId::try_new("attention.macos.local").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("macos:activation-device")
                    .unwrap(),
                floe_connections::ResourceMode::AllAvailable,
                vec![
                    floe_connections::ConnectionResource::new(
                        floe_context_contract::ResourceHandle::try_new("attention.coarse").unwrap(),
                        "Attention".into(),
                    )
                    .unwrap(),
                ],
                "a".repeat(64),
            )
            .await
            .unwrap();
        let local_context = Arc::new(LocalContextHost::default());
        let connections = CurrentSavedConnectionStore::fixed(None);
        let mut current = None;
        let job = lifecycle_job(person, crate::VaultLifecycleCommand::Unlock);
        assert_eq!(
            execute_action(
                root.path(),
                &keys,
                &core,
                &local_context,
                &connections,
                &mut current,
                &job
            )
            .await
            .unwrap()
            .state,
            VaultState::Ready
        );
        let open = current.as_ref().unwrap().1.clone();
        assert_eq!(
            open.task_coordinator
                .environment(&person.to_string())
                .unwrap()
                .catalog()
                .cards
                .len(),
            8
        );
        let before = open.vault.expert_registry().await.unwrap().unwrap();
        let installation = before
            .installations
            .iter()
            .find(|installation| installation.package.id == "floe.builtin.focus-attention")
            .unwrap();
        let assignment = before
            .assignments
            .iter()
            .find(|assignment| assignment.installation_id == installation.id)
            .unwrap();
        let entry = assignment
            .binding
            .entries
            .iter()
            .find(|entry| entry.capability == "attention.coarse")
            .unwrap();
        assert_eq!(entry.selected.len(), usize::from(!existing));
        assert_eq!(assignment.binding.revision, if existing { 1 } else { 2 });
        if !existing {
            assert_eq!(
                assignment
                    .binding
                    .last_operation
                    .as_ref()
                    .unwrap()
                    .operation_id,
                Uuid::new_v5(
                    &job.id,
                    format!(
                        "floe.initial-expert-binding.v1:{}:{}",
                        assignment.id, entry.requirement_key
                    )
                    .as_bytes()
                )
            );
            open.vault
                .replace_expert_binding(
                    Uuid::new_v4(),
                    floe_experts::ExpertBindingCommand {
                        assignment_id: assignment.id,
                        package: installation.package.clone(),
                        definition_revision: before
                            .manifests
                            .iter()
                            .find(|manifest| manifest.package == installation.package)
                            .unwrap()
                            .definition
                            .definition_revision,
                        requirement_key: entry.requirement_key.clone(),
                        expected_binding_revision: assignment.binding.revision,
                        selected: vec![],
                    },
                )
                .await
                .unwrap();
        }
        let configured = open.vault.expert_registry().await.unwrap().unwrap();
        let mut configuration_job = lifecycle_job(person, crate::VaultLifecycleCommand::Lock);
        configuration_job.action = Box::new(WorkerAction::Registry {
            change: Some(floe_experts::RegistryConfiguration {
                instance_id: configured.instance_id,
                expected_revision: configured.revision,
                target: floe_experts::RegistryConfigurationTarget::Assignment {
                    id: assignment.id,
                    enabled: false,
                },
            }),
        });
        execute_action(
            root.path(),
            &keys,
            &core,
            &local_context,
            &connections,
            &mut current,
            &configuration_job,
        )
        .await
        .unwrap();
        assert_eq!(
            open.task_coordinator
                .environment(&person.to_string())
                .unwrap()
                .catalog()
                .cards
                .len(),
            7
        );
        let configured = open.vault.expert_registry().await.unwrap().unwrap();
        let digest = floe_experts::manifest_set_digest(&floe_experts_builtin::manifests()).unwrap();
        let receipt = open
            .vault
            .expert_install_overview(&digest)
            .await
            .unwrap()
            .unwrap()
            .receipt;
        drop(open);
        current = None;
        core.source_service()
            .establish_reviewed_native(
                person,
                floe_context_contract::ConnectorId::try_new("contacts.apple").unwrap(),
                floe_context_contract::ConnectionId::try_new("contacts.apple.local").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("apple:activation-device")
                    .unwrap(),
                floe_connections::ResourceMode::Selected,
                vec![
                    floe_connections::ConnectionResource::new(
                        floe_context_contract::ResourceHandle::try_new("person.identity:a")
                            .unwrap(),
                        "A".into(),
                    )
                    .unwrap(),
                ],
                "b".repeat(64),
            )
            .await
            .unwrap();
        for _ in 0..2 {
            execute_action(
                root.path(),
                &keys,
                &core,
                &local_context,
                &connections,
                &mut current,
                &lifecycle_job(person, crate::VaultLifecycleCommand::Unlock),
            )
            .await
            .unwrap();
            let open = &current.as_ref().unwrap().1;
            assert_eq!(
                open.vault.expert_registry().await.unwrap().unwrap(),
                configured
            );
            assert_eq!(
                open.vault
                    .expert_install_overview(&digest)
                    .await
                    .unwrap()
                    .unwrap()
                    .receipt,
                receipt
            );
            current = None;
        }
    }
}

#[tokio::test]
async fn lifecycle_rejects_unadmitted_mismatched_and_cancelled_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let person = PersonId::new();
    let keys = Keys::default();
    let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
    let local_context = Arc::new(LocalContextHost::default());
    let connections = CurrentSavedConnectionStore::fixed(None);
    let mut current = None;
    for command in [
        crate::VaultLifecycleCommand::Create,
        crate::VaultLifecycleCommand::Unlock,
    ] {
        let mut job = lifecycle_job(person, command);
        job.local_admission = None;
        assert!(matches!(
            execute_action(
                &root,
                &keys,
                &core,
                &local_context,
                &connections,
                &mut current,
                &job
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        ));
        job.local_admission =
            lifecycle_job(person, crate::VaultLifecycleCommand::Lock).local_admission;
        assert!(matches!(
            execute_action(
                &root,
                &keys,
                &core,
                &local_context,
                &connections,
                &mut current,
                &job
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        ));
        assert!(current.is_none());
        assert!(!root.exists());
    }
    let job = lifecycle_job(person, crate::VaultLifecycleCommand::Create);
    job.cancellation.cancel();
    assert!(matches!(
        execute_action(
            &root,
            &keys,
            &core,
            &local_context,
            &connections,
            &mut current,
            &job
        )
        .await,
        Err(AgentFailure::Cancelled)
    ));
    assert!(current.is_none());
    let mut session_job = lifecycle_job(person, crate::VaultLifecycleCommand::Lock);
    session_job.action = Box::new(WorkerAction::ConversationSession {
        operation: ConversationSessionOperation::Resume,
    });
    assert!(matches!(
        execute_action(
            &root,
            &keys,
            &core,
            &local_context,
            &connections,
            &mut current,
            &session_job
        )
        .await,
        Err(AgentFailure::VaultUnavailable)
    ));
    let vault = EncryptedAgentVault::open(&root, person, keys.clone())
        .await
        .unwrap();
    assert!(vault.expert_registry().await.unwrap().is_none());
    drop(vault);
    let job = lifecycle_job(person, crate::VaultLifecycleCommand::Unlock);
    job.cancellation.cancel();
    assert!(matches!(
        execute_action(
            &root,
            &keys,
            &core,
            &local_context,
            &connections,
            &mut current,
            &job
        )
        .await,
        Err(AgentFailure::Cancelled)
    ));
    assert!(current.is_none());
}
