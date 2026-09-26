use super::conversation_turn::expert_dispatch::{
    BoundExpertRegistration, BoundExpertRunner, DelegatedMessageExperts,
};
use super::*;
use floe_agent_contract::{
    AgentContext, ArtifactPart, DelegationExecutionContext, DelegationPort, DelegationRequest,
    ExecutionScope, InvocationKey, ModelPlacement, TaskId, TaskState, TraceContext,
};
use floe_context_contract::SourceReadOutcome;
use floe_execution::budget::{BudgetConfig, BudgetLedger};
use floe_experts_builtin::{BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest};
use floe_kernel::RunId;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::{
    OnceLock,
    atomic::{AtomicUsize, Ordering},
};

fn inventory_connection(
    person: PersonId,
    requests: usize,
) -> (CurrentSavedConnectionStore, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let size = socket.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..size]).starts_with("GET /v1/inference-purposes")
            );
            let body = serde_json::json!({
                "schema_version": 1,
                "purposes": {"everyday_assistance": {
                    "available": true,
                    "requires_external_consent": false,
                    "placement": "server_local"
                }}
            })
            .to_string();
            socket.write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ).as_bytes()).unwrap();
        }
    });
    let connection = floe_inference::SavedServerConnection {
        base_url: format!("http://{address}"),
        token: "t".repeat(32),
        client_id: "test-client".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    };
    (CurrentSavedConnectionStore::fixed(Some(connection)), server)
}

static RUNNER_CALLS: AtomicUsize = AtomicUsize::new(0);
static RUNNER_A_CALLS: AtomicUsize = AtomicUsize::new(0);
static RUNNER_B_CALLS: AtomicUsize = AtomicUsize::new(0);
static RUNNER_A_ENTERED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static RUNNER_A_RELEASE: OnceLock<tokio::sync::Notify> = OnceLock::new();

fn runner_a<'turn, 'model, 'msg, 'call>(
    _host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    _request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        RUNNER_A_CALLS.fetch_add(1, Ordering::SeqCst);
        RUNNER_A_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
        RUNNER_A_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        BuiltinExpertOutput::from_result(
            "pinned-result",
            "application/vnd.example.result+json",
            "runner-A-marker".into(),
            &serde_json::json!({"runner": "A"}),
        )
    })
}

fn runner_b<'turn, 'model, 'msg, 'call>(
    _host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    _request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        RUNNER_B_CALLS.fetch_add(1, Ordering::SeqCst);
        BuiltinExpertOutput::from_result(
            "pinned-result",
            "application/vnd.example.result+json",
            "runner-B-marker".into(),
            &serde_json::json!({"runner": "B"}),
        )
    })
}

fn example_manifest() -> floe_experts::ExpertManifest {
    let mut manifest = floe_experts_builtin::manifests()
        .into_iter()
        .find(|manifest| manifest.package.id == "floe.builtin.focus-attention")
        .unwrap();
    manifest.package.id = "example.test.expert".into();
    manifest.package.version = "1.0.0".into();
    manifest.definition.card.id = manifest.package.id.clone();
    manifest.definition.card.version = manifest.package.version.clone();
    manifest.definition.card.name = "Example test Expert".into();
    manifest.definition.card.supported_placements = vec![ModelPlacement::Remote];
    manifest.publisher = "example.test".into();
    manifest.source_requirements.clear();
    manifest.validate().unwrap();
    manifest
}

fn example_runner<'turn, 'model, 'msg, 'call>(
    _host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    _request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        RUNNER_CALLS.fetch_add(1, Ordering::SeqCst);
        BuiltinExpertOutput::from_result(
            "example-result",
            "application/vnd.example.result+json",
            "example-bound-runner-marker".into(),
            &serde_json::json!({"marker": "example-bound-runner-marker"}),
        )
    })
}

fn example_registration() -> BoundExpertRegistration {
    BoundExpertRegistration {
        manifest: example_manifest(),
        runner: BoundExpertRunner::Supplied(example_runner),
    }
}

async fn installed_open(
    person: PersonId,
    registration: BoundExpertRegistration,
) -> (
    tempfile::TempDir,
    OpenVault<Keys>,
    std::thread::JoinHandle<()>,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
    let (connections, server) = inventory_connection(person, 1);
    let manifest = registration.manifest.clone();
    let open = OpenVault::activate(
        vault,
        core,
        Arc::new(LocalContextHost::default()),
        connections,
        vec![registration],
    )
    .await
    .unwrap();
    open.vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &[manifest],
            Cancellation::default(),
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    (root, open, server)
}

async fn installed_open_without_provider(
    person: PersonId,
    registrations: Vec<BoundExpertRegistration>,
    manifest: floe_experts::ExpertManifest,
) -> (tempfile::TempDir, OpenVault<Keys>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
    let open = OpenVault::activate(
        vault,
        core,
        Arc::new(LocalContextHost::default()),
        CurrentSavedConnectionStore::fixed(None),
        registrations,
    )
    .await
    .unwrap();
    open.vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &[manifest],
            Cancellation::default(),
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    (root, open)
}

fn required_source_registration(
    runner: super::conversation_turn::expert_dispatch::SuppliedExpertRunner,
) -> BoundExpertRegistration {
    let mut manifest = example_manifest();
    manifest.source_requirements = vec![floe_experts::ExpertSourceRequirement {
        key: "required_attention".into(),
        capability: "attention.coarse".into(),
        contract_version: 1,
        minimum_sources: 1,
        maximum_sources: 1,
    }];
    BoundExpertRegistration {
        manifest,
        runner: BoundExpertRunner::Supplied(runner),
    }
}

fn required_source_runner<'turn, 'model, 'msg, 'call>(
    host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        let outcome = host
            .read_requirement(
                request,
                "required_attention",
                serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
            )
            .await?;
        let marker = match outcome {
            SourceReadOutcome::Ready(_) => "source-ready",
            SourceReadOutcome::Unavailable(_) => "source-unavailable",
            SourceReadOutcome::NeedsUserAction(_) => "source-needs-user-action",
        };
        BuiltinExpertOutput::from_result(
            "source-outcome",
            "application/vnd.example.result+json",
            marker.into(),
            &serde_json::json!({"outcome": marker}),
        )
    })
}

fn undeclared_source_runner<'turn, 'model, 'msg, 'call>(
    host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        let failure = host
            .read_requirement(request, "not_declared", serde_json::Value::Null)
            .await
            .err();
        assert_eq!(failure, Some(AgentFailure::CapabilityDenied));
        BuiltinExpertOutput::from_result(
            "undeclared-outcome",
            "application/vnd.example.result+json",
            "undeclared-denied-before-io".into(),
            &serde_json::json!({"denied": true}),
        )
    })
}

fn task_scope(run_id: RunId, task_id: TaskId) -> ExecutionScope {
    let budget = BudgetLedger::new(BudgetConfig::new(50_000, 100_000), Default::default());
    let root = ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + Duration::from_secs(20),
        budget.work_lease(),
        TraceContext::new(Uuid::new_v4()).with_run_id(run_id),
    );
    root.child_scope(root.deadline(), 20_000, 50_000, Some(task_id))
}

fn request(person: PersonId, run_id: RunId, task_id: TaskId) -> DelegationRequest {
    DelegationRequest {
        task_id,
        parent_run_id: Some(run_id.as_uuid()),
        principal: person.to_string(),
        invocation_key: InvocationKey::new(),
        selected_agent_id: "example.test.expert".into(),
        selected_definition_revision: 1,
        message: "execute example".into(),
        context_refs: vec![],
        execution_context: DelegationExecutionContext {
            session_id: Uuid::new_v4(),
            device_id: "mac-local".into(),
            agent_context: AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            max_output_bytes: 16 * 1024,
        },
    }
}

async fn journal_origin(open: &OpenVault<Keys>, request: &mut DelegationRequest, run_id: RunId) {
    let repository = open.conversation_repository.as_ref();
    let started = floe_conversation::start_session(
        repository,
        floe_conversation::SessionRequest {
            principal: request.principal.clone(),
        },
    )
    .await
    .unwrap();
    request.execution_context.session_id = started.session_id;
    let command_id = floe_agent_contract::CommandId::new();
    floe_conversation::ConversationRepository::admit_turn(
        repository,
        floe_conversation::TurnAdmissionRequest {
            run_id,
            command_id,
            session_id: started.session_id,
            expected_session_revision: 0,
            principal: request.principal.clone(),
            request_digest: [7; 32],
            mode: floe_conversation::TurnMode::New,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
            user_message: floe_agent_contract::AgentMessage {
                message_id: command_id.as_uuid(),
                role: floe_agent_contract::MessageRole::User,
                text: request.message.clone(),
                call_id: None,
                coverage: floe_agent_contract::DependencyCoverage::Independent,
            },
        },
    )
    .await
    .unwrap();
    floe_conversation::ConversationRepository::journal(repository, run_id)
        .unwrap()
        .record_intent(floe_agent_contract::JournalEvent::DelegationIntent {
            request: request.clone(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn registered_runner_nonbuiltin_uses_product_endpoint_and_durable_task() {
    RUNNER_CALLS.store(0, Ordering::SeqCst);
    assert!(
        !floe_experts_builtin::manifests()
            .iter()
            .any(|manifest| manifest.package.id == "example.test.expert")
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
    let (connections, server) = inventory_connection(person, 1);
    let open = OpenVault::activate(
        vault,
        core,
        Arc::new(LocalContextHost::default()),
        connections,
        vec![example_registration()],
    )
    .await
    .unwrap();
    let manifest = example_manifest();
    open.vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &[manifest],
            Cancellation::default(),
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    assert_eq!(
        open.task_coordinator
            .catalog(&person.to_string())
            .unwrap()
            .cards
            .len(),
        1
    );
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let task_request = request(person, run_id, task_id);
    let scope = task_scope(run_id, task_id);
    let receipt = open
        .task_coordinator
        .delegate(task_request.clone(), &scope)
        .await
        .unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed, "{receipt:?}");
    assert_eq!(
        receipt.snapshot.result.as_deref(),
        Some("example-bound-runner-marker")
    );
    assert_eq!(
        receipt.snapshot.coverage,
        floe_agent_contract::DependencyCoverage::Independent
    );
    assert!(
        receipt
            .snapshot
            .artifacts
            .iter()
            .all(|artifact| artifact.coverage == receipt.snapshot.coverage)
    );
    assert!(receipt.snapshot.artifacts.iter().any(|artifact| artifact.parts.iter().any(|part| matches!(part, ArtifactPart::Data { media_type, data } if media_type == "application/vnd.example.result+json" && data.contains("example-bound-runner-marker")))));
    assert_eq!(RUNNER_CALLS.load(Ordering::SeqCst), 1);
    server.join().unwrap();
    let replay = open
        .task_coordinator
        .delegate(task_request, &scope)
        .await
        .unwrap();
    assert_eq!(replay.snapshot, receipt.snapshot);
    assert_eq!(RUNNER_CALLS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn registered_runner_product_endpoint_pins_a_across_b_publication_and_replay() {
    RUNNER_A_CALLS.store(0, Ordering::SeqCst);
    RUNNER_B_CALLS.store(0, Ordering::SeqCst);
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
    let (connections, server) = inventory_connection(person, 2);
    let open = OpenVault::activate(
        vault,
        core,
        Arc::new(LocalContextHost::default()),
        connections,
        vec![BoundExpertRegistration {
            manifest: example_manifest(),
            runner: BoundExpertRunner::Supplied(runner_a),
        }],
    )
    .await
    .unwrap();
    let first_install = open
        .vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &[example_manifest()],
            Cancellation::default(),
        )
        .await
        .unwrap();
    let first_admission = open.vault.enabled_expert_admissions().await.unwrap()[0]
        .1
        .clone();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let first_request = request(person, run_id, task_id);
    let first_scope = task_scope(run_id, task_id);
    let first = open
        .task_coordinator
        .delegate(first_request.clone(), &first_scope);
    let replacement = async {
        RUNNER_A_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        let disabled = open
            .vault
            .configure_registry(
                floe_experts::RegistryConfiguration {
                    instance_id: open.vault.registry_instance_id(),
                    expected_revision: first_install.registry.revision,
                    target: floe_experts::RegistryConfigurationTarget::Assignment {
                        id: first_admission.assignment_id,
                        enabled: false,
                    },
                },
                Cancellation::default(),
            )
            .await
            .unwrap();
        let mut second_manifest = example_manifest();
        second_manifest.package.version = "2.0.0".into();
        second_manifest.definition.card.version = "2.0.0".into();
        second_manifest.definition.definition_revision = 2;
        let installed = open
            .vault
            .install_expert_bundle(
                floe_experts::ExpertInstallOperation {
                    instance_id: open.vault.registry_instance_id(),
                    expected_revision: disabled.revision,
                    operation_id: Uuid::new_v4(),
                },
                &[second_manifest.clone()],
                Cancellation::default(),
            )
            .await
            .unwrap();
        let second_admission = open.vault.enabled_expert_admissions().await.unwrap()[0]
            .1
            .clone();
        assert_ne!(first_admission.package, second_admission.package);
        assert_eq!(second_admission.definition_revision, 2);
        let second_set = validated_expert_registrations(vec![BoundExpertRegistration {
            manifest: second_manifest,
            runner: BoundExpertRunner::Supplied(runner_b),
        }])
        .unwrap();
        open.publish_expert_directory(&second_set).await.unwrap();
        let blocked_run_id = RunId::new();
        let blocked_task_id = TaskId::new();
        assert!(
            open.task_coordinator
                .delegate(
                    request(person, blocked_run_id, blocked_task_id),
                    &task_scope(blocked_run_id, blocked_task_id),
                )
                .await
                .is_err()
        );
        let second_run_id = RunId::new();
        let second_task_id = TaskId::new();
        let mut second_request = request(person, second_run_id, second_task_id);
        second_request.selected_definition_revision = 2;
        let second_scope = task_scope(second_run_id, second_task_id);
        let second = open
            .task_coordinator
            .delegate(second_request, &second_scope)
            .await
            .unwrap();
        RUNNER_A_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
        assert_eq!(second.snapshot.state, TaskState::Completed, "{second:?}");
        assert_eq!(second.snapshot.result.as_deref(), Some("runner-B-marker"));
        assert_eq!(RUNNER_A_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(RUNNER_B_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(
            installed.receipt.installed[0].package,
            second_admission.package
        );
        (second_task_id, second_admission)
    };
    let (first, (second_task_id, second_admission)) = tokio::join!(first, replacement);
    let first = first.unwrap();
    assert_eq!(first.snapshot.state, TaskState::Completed, "{first:?}");
    assert_eq!(first.snapshot.result.as_deref(), Some("runner-A-marker"));
    let repository = VaultTaskRepository::new(Arc::clone(&open.vault));
    let first_record = floe_experts::TaskRepository::get(&repository, task_id)
        .await
        .unwrap()
        .unwrap();
    let second_record = floe_experts::TaskRepository::get(&repository, second_task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_record.admission, first_admission);
    assert_eq!(second_record.admission, second_admission);
    let saved = open
        .task_coordinator
        .get_task(
            &person.to_string(),
            Some(run_id.as_uuid()),
            task_id,
            &first_scope,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.snapshot, first.snapshot);
    let replay = open
        .task_coordinator
        .delegate(first_request, &first_scope)
        .await
        .unwrap();
    assert_eq!(replay.snapshot, first.snapshot);
    assert_eq!(RUNNER_A_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(RUNNER_B_CALLS.load(Ordering::SeqCst), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn registered_runner_required_unconfigured_source_returns_typed_outcome() {
    let person = PersonId::new();
    let (_root, open, server) =
        installed_open(person, required_source_registration(required_source_runner)).await;
    assert_eq!(
        open.task_coordinator
            .catalog(&person.to_string())
            .unwrap()
            .cards
            .len(),
        1
    );
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let mut task_request = request(person, run_id, task_id);
    journal_origin(&open, &mut task_request, run_id).await;
    let receipt = open
        .task_coordinator
        .delegate(task_request, &task_scope(run_id, task_id))
        .await
        .unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed, "{receipt:?}");
    assert_eq!(
        receipt.snapshot.result.as_deref(),
        Some("source-needs-user-action")
    );
    assert!(receipt.snapshot.artifacts.iter().any(|artifact| artifact.parts.iter().any(|part| matches!(part, ArtifactPart::Data { media_type, .. } if media_type == floe_agent_contract::USER_INTERACTION_MEDIA_TYPE))));
    let interactions = floe_conversation::InteractionRepository::list_run_interactions(
        open.conversation_repository.as_ref(),
        person,
        run_id,
    )
    .await
    .unwrap();
    assert_eq!(interactions.len(), 1);
    assert_eq!(
        interactions[0].origin,
        floe_conversation::InteractionOrigin::Task {
            task_id: task_id.as_uuid(),
            capability_call_id: None,
        }
    );
    server.join().unwrap();
}

#[tokio::test]
async fn registered_runner_undeclared_requirement_is_denied_before_source_io() {
    let person = PersonId::new();
    let (_root, open, server) = installed_open(
        person,
        required_source_registration(undeclared_source_runner),
    )
    .await;
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let receipt = open
        .task_coordinator
        .delegate(
            request(person, run_id, task_id),
            &task_scope(run_id, task_id),
        )
        .await
        .unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed, "{receipt:?}");
    assert_eq!(
        receipt.snapshot.result.as_deref(),
        Some("undeclared-denied-before-io")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn registered_runner_admission_and_manifest_mismatches_fail_closed() {
    RUNNER_CALLS.store(0, Ordering::SeqCst);
    let person = PersonId::new();
    let (_root, open) =
        installed_open_without_provider(person, vec![example_registration()], example_manifest())
            .await;
    let original = open.task_coordinator.catalog(&person.to_string()).unwrap();
    assert_eq!(original.cards.len(), 1);
    for mismatch in 0..4 {
        let run_id = RunId::new();
        let task_id = TaskId::new();
        let mut attempted = request(person, run_id, task_id);
        match mismatch {
            0 => attempted.principal = PersonId::new().to_string(),
            1 => attempted.selected_agent_id = "example.test.other".into(),
            2 => attempted.selected_definition_revision = 2,
            3 => attempted.execution_context.device_id.clear(),
            _ => unreachable!(),
        }
        assert!(
            open.task_coordinator
                .delegate(attempted, &task_scope(run_id, task_id))
                .await
                .is_err()
        );
    }
    let mut changed_manifest = example_manifest();
    changed_manifest.publisher = "example.changed".into();
    let conflict = validated_expert_registrations(vec![BoundExpertRegistration {
        manifest: changed_manifest,
        runner: BoundExpertRunner::Supplied(example_runner),
    }])
    .unwrap();
    assert_eq!(
        open.publish_expert_directory(&conflict).await,
        Err(AgentFailure::Conflict)
    );
    let duplicate = vec![
        Arc::new(example_registration()),
        Arc::new(example_registration()),
    ];
    assert_eq!(
        open.publish_expert_directory(&duplicate).await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        open.task_coordinator.catalog(&person.to_string()).unwrap(),
        original
    );
    assert_eq!(RUNNER_CALLS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn registered_runner_missing_and_duplicate_supplied_implementations_do_not_fallback() {
    let person = PersonId::new();
    let (_root, open) = installed_open_without_provider(
        person,
        super::conversation_turn::expert_dispatch::shipped_registrations(),
        example_manifest(),
    )
    .await;
    assert!(
        open.task_coordinator
            .catalog(&person.to_string())
            .unwrap()
            .cards
            .is_empty()
    );
    let run_id = RunId::new();
    let task_id = TaskId::new();
    assert!(
        open.task_coordinator
            .delegate(
                request(person, run_id, task_id),
                &task_scope(run_id, task_id),
            )
            .await
            .is_err()
    );
    let duplicate = vec![example_registration(), example_registration()];
    assert!(matches!(
        validated_expert_registrations(duplicate),
        Err(AgentFailure::Conflict)
    ));
    let mut conflicting = example_manifest();
    conflicting.package.version = "2.0.0".into();
    conflicting.definition.card.version = "2.0.0".into();
    conflicting.definition.definition_revision = 2;
    assert!(matches!(
        validated_expert_registrations(vec![
            example_registration(),
            BoundExpertRegistration {
                manifest: conflicting,
                runner: BoundExpertRunner::Supplied(runner_b)
            },
        ]),
        Err(AgentFailure::Conflict)
    ));
    assert!(matches!(
        validated_expert_registrations(vec![BoundExpertRegistration {
            manifest: example_manifest(),
            runner: BoundExpertRunner::Shipped(floe_experts_builtin::registrations()[0].runner),
        }]),
        Err(AgentFailure::Conflict)
    ));
    assert!(
        open.task_coordinator
            .catalog(&person.to_string())
            .unwrap()
            .cards
            .is_empty()
    );
    let mut wrong_version = example_manifest();
    wrong_version.package.version = "2.0.0".into();
    wrong_version.definition.card.version = "2.0.0".into();
    let (_version_root, version_open) = installed_open_without_provider(
        PersonId::new(),
        vec![BoundExpertRegistration {
            manifest: wrong_version,
            runner: BoundExpertRunner::Supplied(runner_b),
        }],
        example_manifest(),
    )
    .await;
    assert!(
        version_open
            .task_coordinator
            .catalog(&version_open.vault.person_id().to_string())
            .unwrap()
            .cards
            .is_empty()
    );
}

#[tokio::test]
async fn registered_runner_extension_does_not_change_first_party_observe_policy() {
    let before = crate::first_party_observe::calendar_policy().unwrap();
    let fingerprint = crate::first_party_observe::policy_fingerprint(&before).unwrap();
    let remote =
        crate::first_party_observe::member_policy_fingerprint("gmail", "mail.communication")
            .unwrap();
    let attention = crate::first_party_observe::native_consumers("attention.macos").unwrap();
    let attention_fingerprint =
        crate::first_party_observe::member_policy_fingerprint("attention.macos", "attention.macos")
            .unwrap();
    let person = PersonId::new();
    let registration = required_source_registration(required_source_runner);
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    assert_eq!(
        open.task_coordinator
            .catalog(&person.to_string())
            .unwrap()
            .cards
            .len(),
        1
    );
    let after = crate::first_party_observe::calendar_policy().unwrap();
    assert_eq!(after, before);
    assert_eq!(
        crate::first_party_observe::native_consumers("attention.macos").unwrap(),
        attention
    );
    assert!(
        !attention
            .iter()
            .any(|consumer| consumer == "example.test.expert")
    );
    assert_eq!(
        crate::first_party_observe::member_policy_fingerprint("attention.macos", "attention.macos")
            .unwrap(),
        attention_fingerprint
    );
    assert!(
        !after
            .consumers
            .iter()
            .any(|consumer| consumer.identifier() == "example.test.expert")
    );
    assert_eq!(
        crate::first_party_observe::policy_fingerprint(&after).unwrap(),
        fingerprint
    );
    assert_eq!(
        crate::first_party_observe::member_policy_fingerprint("gmail", "mail.communication")
            .unwrap(),
        remote
    );
}
