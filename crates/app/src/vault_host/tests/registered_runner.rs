use super::conversation_turn::expert_dispatch::{
    BoundExpertRegistration, BoundExpertRunner, DelegatedMessageExperts,
};
use super::*;
use floe_agent_contract::{
    AgentContext, ArtifactPart, DelegationExecutionContext, DelegationPort, DelegationRequest,
    ExecutionScope, ExpertModel, ExpertModelCall, ExpertModelRequirement, InvocationKey,
    ModelPlacement, TaskId, TaskState, TraceContext,
};
use floe_execution::budget::{BudgetConfig, BudgetLedger};
use floe_experts::RequirementReadOutcome;
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
    inventory_connection_with_model(person, requests, false)
}

fn inventory_connection_with_model(
    person: PersonId,
    requests: usize,
    answer_model: bool,
) -> (CurrentSavedConnectionStore, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut model_calls = 0;
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let size = socket.read(&mut request).unwrap();
            let mut bytes = request[..size].to_vec();
            let headers_end = loop {
                if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    break position + 4;
                }
                let size = socket.read(&mut request).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&request[..size]);
            };
            let headers = String::from_utf8_lossy(&bytes[..headers_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < headers_end + content_length {
                let size = socket.read(&mut request).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&request[..size]);
            }
            let request = String::from_utf8_lossy(&bytes);
            let model_call = request.starts_with("POST /v1/agent ");
            assert!(
                request.starts_with("GET /v1/inference-purposes") || (answer_model && model_call)
            );
            let body = if model_call {
                model_calls += 1;
                let output = serde_json::json!({"output": [{"kind": "answer", "text": "Pinned tasks observed"}], "used_tokens": 1});
                serde_json::json!({
                    "schema_version": 1,
                    "purpose": "everyday_assistance",
                    "trace_id": "a".repeat(32),
                    "routing": {
                        "placement": "server_local",
                        "external_transfer": false,
                        "replay_source": "a".repeat(64)
                    },
                    "output": output.to_string()
                })
                .to_string()
            } else {
                serde_json::json!({
                    "schema_version": 1,
                    "purposes": {"everyday_assistance": {
                        "available": true,
                        "requires_external_consent": false,
                        "placement": "server_local"
                    }}
                })
                .to_string()
            };
            socket.write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ).as_bytes()).unwrap();
        }
        assert_eq!(model_calls, usize::from(answer_model));
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
static REQUIRED_SOURCE_RUNNER_CALLS: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_CHAIN_RUNNER_CALLS: AtomicUsize = AtomicUsize::new(0);
static RUNNER_A_ENTERED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static RUNNER_A_RELEASE: OnceLock<tokio::sync::Notify> = OnceLock::new();
static SOURCE_READ_ENTERED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static SOURCE_READ_RELEASE: OnceLock<tokio::sync::Notify> = OnceLock::new();
static OUTPUT_ENTERED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static OUTPUT_RELEASE: OnceLock<tokio::sync::Notify> = OnceLock::new();
static MODEL_DISPATCH_ENTERED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static MODEL_DISPATCH_RELEASE: OnceLock<tokio::sync::Notify> = OnceLock::new();

fn attention_source(person: PersonId) -> floe_connections::SourceConnection {
    floe_connections::SourceConnection::establish_reviewed_native(
        person,
        floe_context_contract::ConnectorId::try_new("attention.macos").unwrap(),
        floe_context_contract::ConnectionId::try_new("attention.macos.local").unwrap(),
        floe_context_contract::ExecutionOwnerId::try_new("macos:mac-local").unwrap(),
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
    .unwrap()
}

async fn establish_attention_source<Keys: floe_vault::VaultKeyProvider>(
    open: &OpenVault<Keys>,
    person: PersonId,
) {
    floe_connections::SourceRepository::create(&open.core.store, &attention_source(person))
        .await
        .unwrap();
}

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

pub(super) fn example_manifest() -> floe_experts::ExpertManifest {
    let mut manifest = floe_experts_builtin::manifests()
        .into_iter()
        .find(|manifest| manifest.package.id == "floe.builtin.focus-attention")
        .unwrap();
    manifest.package.id = "example.test.expert".into();
    manifest.package.version = "1.0.0".into();
    manifest.definition.card.id = manifest.package.id.clone();
    manifest.definition.card.version = manifest.package.version.clone();
    manifest.definition.definition_revision = 1;
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

fn extension_chain_registration() -> BoundExpertRegistration {
    let mut manifest = example_manifest();
    manifest.source_requirements = vec![floe_experts::ExpertSourceRequirement {
        key: "required_tasks".into(),
        capability: "floe.tasks".into(),
        contract_version: 1,
        minimum_sources: 1,
        maximum_sources: 1,
    }];
    BoundExpertRegistration {
        manifest,
        runner: BoundExpertRunner::Supplied(extension_chain_runner),
    }
}

fn extension_chain_runner<'turn, 'model, 'msg, 'call>(
    host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        EXTENSION_CHAIN_RUNNER_CALLS.fetch_add(1, Ordering::SeqCst);
        let outcome = host
            .read_requirement(
                request,
                "required_tasks",
                serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
            )
            .await?;
        assert!(matches!(outcome, RequirementReadOutcome::Ready(_)));
        BuiltinExpertOutput::from_result(
            "extension-tasks",
            "application/vnd.example.tasks+json",
            "Selected local tasks are available.".into(),
            &serde_json::json!({"selected_tasks": true}),
        )
    })
}

fn root_environment_admission() -> RootAgentEnvironmentAdmission {
    RootAgentEnvironmentAdmission {
        device_id: "mac-local".into(),
        operation_id: Uuid::new_v4(),
        cancellation: Cancellation::default(),
    }
}

fn registered_installation<'a>(
    snapshot: &'a floe_experts::RegistrySnapshot,
    open: &OpenVault<Keys>,
) -> &'a floe_experts::PackageInstallation {
    snapshot
        .installations
        .iter()
        .find(|installation| installation.package == open.registrations[0].manifest.package)
        .unwrap()
}

fn registered_assignment<'a>(
    snapshot: &'a floe_experts::RegistrySnapshot,
    open: &OpenVault<Keys>,
) -> &'a floe_experts::PackageAssignment {
    let installation = registered_installation(snapshot, open);
    snapshot
        .assignments
        .iter()
        .find(|assignment| assignment.installation_id == installation.id)
        .unwrap()
}

async fn install_test_manifest(open: &OpenVault<Keys>, manifest: floe_experts::ExpertManifest) {
    if floe_experts_builtin::manifests().contains(&manifest) {
        return;
    }
    open.vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: open
                    .vault
                    .expert_registry()
                    .await
                    .unwrap()
                    .unwrap()
                    .revision,
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
}

async fn installed_open(
    person: PersonId,
    registration: BoundExpertRegistration,
) -> (
    tempfile::TempDir,
    OpenVault<Keys>,
    std::thread::JoinHandle<()>,
) {
    installed_open_with_model(person, registration, false).await
}

async fn installed_open_with_model(
    person: PersonId,
    registration: BoundExpertRegistration,
    answer_model: bool,
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
    let (connections, server) =
        inventory_connection_with_model(person, if answer_model { 3 } else { 1 }, answer_model);
    let manifest = registration.manifest.clone();
    let open = OpenVault::activate(
        vault,
        core,
        Arc::new(LocalContextHost::default()),
        connections,
        vec![registration],
        root_environment_admission(),
    )
    .await
    .unwrap();
    install_test_manifest(&open, manifest).await;
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
        root_environment_admission(),
    )
    .await
    .unwrap();
    install_test_manifest(&open, manifest).await;
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
        REQUIRED_SOURCE_RUNNER_CALLS.fetch_add(1, Ordering::SeqCst);
        let outcome = host
            .read_requirement(
                request,
                "required_attention",
                serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
            )
            .await?;
        let marker = match outcome {
            RequirementReadOutcome::Ready(_) => "source-ready",
            RequirementReadOutcome::Unavailable(_) => "source-unavailable",
            RequirementReadOutcome::NeedsUserAction => "source-needs-user-action",
        };
        BuiltinExpertOutput::from_result(
            "source-outcome",
            "application/vnd.example.result+json",
            marker.into(),
            &serde_json::json!({"outcome": marker}),
        )
    })
}

fn paused_requirement_runner<'turn, 'model, 'msg, 'call>(
    host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        SOURCE_READ_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
        SOURCE_READ_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        host.read_requirement(
            request,
            "required_attention",
            serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
        )
        .await?;
        BuiltinExpertOutput::from_result(
            "source-outcome",
            "application/vnd.example.result+json",
            "source-read-after-rebind".into(),
            &serde_json::json!({"outcome": "read"}),
        )
    })
}

fn paused_output_runner<'turn, 'model, 'msg, 'call>(
    _host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    _request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        OUTPUT_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
        OUTPUT_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        BuiltinExpertOutput::from_result(
            "output",
            "application/vnd.example.result+json",
            "stale-output".into(),
            &serde_json::json!({"output": true}),
        )
    })
}

fn tasks_requirement_registration() -> BoundExpertRegistration {
    let mut manifest = example_manifest();
    manifest.source_requirements = vec![floe_experts::ExpertSourceRequirement {
        key: "required_tasks".into(),
        capability: "floe.tasks".into(),
        contract_version: 1,
        minimum_sources: 1,
        maximum_sources: 1,
    }];
    BoundExpertRegistration {
        manifest,
        runner: BoundExpertRunner::Supplied(paused_model_after_tasks_runner),
    }
}

fn paused_model_after_tasks_runner<'turn, 'model, 'msg, 'call>(
    host: &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    request: &'call BuiltinExpertRequest,
) -> floe_agent_contract::BoxFuture<'call, Result<BuiltinExpertOutput, AgentFailure>> {
    Box::pin(async move {
        assert!(matches!(
            host.read_requirement(
                request,
                "required_tasks",
                serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION}),
            )
            .await?,
            RequirementReadOutcome::Ready(_),
        ));
        MODEL_DISPATCH_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
        MODEL_DISPATCH_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        host.model()
            .answer(ExpertModelCall {
                person_id: request.person_id,
                invocation_id: request.invocation_id,
                prompt: floe_experts_builtin::prompts::focus_expert_prompt(),
                policy: host.policy().clone(),
                context: request.context.clone(),
                assignment: request.assignment.clone(),
                requirement: ExpertModelRequirement::Any,
                max_output_bytes: request.max_output_bytes,
                max_tokens: 100,
                max_cost_micros: 100,
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            })
            .await?;
        BuiltinExpertOutput::from_result(
            "model-outcome",
            "application/vnd.example.result+json",
            "model-dispatched".into(),
            &serde_json::json!({"model": true}),
        )
    })
}

#[tokio::test]
async fn read_a_then_rebind_b_completes_pinned_expert_model_dispatch() {
    let person = PersonId::new();
    let (_root, open, server) =
        installed_open_with_model(person, tasks_requirement_registration(), true).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment = &registered_assignment(&snapshot, &open);
    let selected = |device_id| {
        floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
            person_id: person,
            device_id,
            capability: "floe.tasks",
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: &[],
        })
        .unwrap()
        .remove(0)
        .reference
    };
    let source_a = selected("mac-local");
    let source_b = selected("other-device");
    let package = registered_installation(&snapshot, &open).package.clone();
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: package.clone(),
                definition_revision: 1,
                requirement_key: "required_tasks".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![source_a.clone()],
            },
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let scope = task_scope(run_id, task_id);
    let environment = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    let execution = environment.delegate(request(person, run_id, task_id), &scope);
    let rebind = async {
        MODEL_DISPATCH_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        open.vault
            .replace_expert_binding(
                Uuid::new_v4(),
                floe_experts::ExpertBindingCommand {
                    assignment_id: assignment.id,
                    package,
                    definition_revision: 1,
                    requirement_key: "required_tasks".into(),
                    expected_binding_revision: assignment.binding.revision + 1,
                    selected: vec![source_b.clone()],
                },
            )
            .await
            .unwrap();
        MODEL_DISPATCH_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
    };
    let (result, ()) = tokio::join!(execution, rebind);
    let receipt = result.unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed, "{receipt:?}");
    assert_eq!(receipt.snapshot.issue, None);
    assert_eq!(receipt.snapshot.result.as_deref(), Some("model-dispatched"));
    assert_eq!(
        open.vault
            .task(task_id)
            .await
            .unwrap()
            .unwrap()
            .selection
            .requirements[0]
            .selected[0],
        source_a,
    );
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let next = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    assert_ne!(environment.identity(), next.identity());
    let current = open.vault.expert_registry().await.unwrap().unwrap();
    let registry =
        floe_experts::AgentRegistry::restore(current, open.vault.registry_instance_id()).unwrap();
    let task = open.vault.task(task_id).await.unwrap().unwrap();
    assert_eq!(
        registry
            .execution_selection(person, &task.admission)
            .unwrap()
            .requirements[0]
            .selected,
        vec![source_b]
    );
    server.join().unwrap();
}

#[tokio::test]
async fn rebound_selection_preserves_runner_result_at_final_release() {
    let person = PersonId::new();
    let (_root, open, server) =
        installed_open(person, required_source_registration(paused_output_runner)).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment = &registered_assignment(&snapshot, &open);
    let source_a = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id: person,
        device_id: "mac-local",
        capability: "attention.coarse",
        contract_version: 1,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: &[attention_source(person)],
    })
    .unwrap()
    .remove(0)
    .reference;
    let package = registered_installation(&snapshot, &open).package.clone();
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: package.clone(),
                definition_revision: 1,
                requirement_key: "required_attention".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![source_a],
            },
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let scope = task_scope(run_id, task_id);
    let environment = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    let execution = environment.delegate(request(person, run_id, task_id), &scope);
    let rebind = async {
        OUTPUT_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        open.vault
            .replace_expert_binding(
                Uuid::new_v4(),
                floe_experts::ExpertBindingCommand {
                    assignment_id: assignment.id,
                    package,
                    definition_revision: 1,
                    requirement_key: "required_attention".into(),
                    expected_binding_revision: assignment.binding.revision + 1,
                    selected: vec![],
                },
            )
            .await
            .unwrap();
        OUTPUT_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
    };
    let (result, ()) = tokio::join!(execution, rebind);
    let receipt = result.unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed, "{receipt:?}");
    assert_eq!(receipt.snapshot.issue, None);
    assert_eq!(receipt.snapshot.result.as_deref(), Some("stale-output"));
    server.join().unwrap();
}

#[tokio::test]
async fn completed_task_replays_historical_result_after_rebinding_and_disable() {
    let person = PersonId::new();
    let (_root, open, server) =
        installed_open(person, required_source_registration(example_runner)).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment = &registered_assignment(&snapshot, &open);
    let source = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id: person,
        device_id: "mac-local",
        capability: "attention.coarse",
        contract_version: 1,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: &[attention_source(person)],
    })
    .unwrap()
    .remove(0)
    .reference;
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: registered_installation(&snapshot, &open).package.clone(),
                definition_revision: 1,
                requirement_key: "required_attention".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![source],
            },
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let original_request = request(person, run_id, task_id);
    let scope = task_scope(run_id, task_id);
    let environment = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    let completed = environment
        .delegate(original_request.clone(), &scope)
        .await
        .unwrap();
    assert_eq!(completed.snapshot.state, TaskState::Completed);
    let current = open.vault.expert_registry().await.unwrap().unwrap();
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: registered_installation(&snapshot, &open).package.clone(),
                definition_revision: 1,
                requirement_key: "required_attention".into(),
                expected_binding_revision: registered_assignment(&current, &open).binding.revision,
                selected: vec![],
            },
        )
        .await
        .unwrap();
    let current = open.vault.expert_registry().await.unwrap().unwrap();
    open.vault
        .configure_registry(
            floe_experts::RegistryConfiguration {
                instance_id: current.instance_id,
                expected_revision: current.revision,
                target: floe_experts::RegistryConfigurationTarget::Assignment {
                    id: assignment.id,
                    enabled: false,
                },
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let replay = environment
        .delegate(original_request, &scope)
        .await
        .unwrap();
    assert_eq!(replay.snapshot, completed.snapshot);
    assert_eq!(
        open.task_coordinator
            .get_task(&person.to_string(), Some(run_id.as_uuid()), task_id, &scope)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        completed.snapshot,
    );
    server.join().unwrap();
}

#[tokio::test]
async fn admitted_source_a_rebound_before_read_never_uses_b() {
    let person = PersonId::new();
    let (_root, open, server) = installed_open(
        person,
        required_source_registration(paused_requirement_runner),
    )
    .await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment = &registered_assignment(&snapshot, &open);
    let source_a = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id: person,
        device_id: "mac-local",
        capability: "attention.coarse",
        contract_version: 1,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: &[attention_source(person)],
    })
    .unwrap()
    .remove(0)
    .reference;
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: registered_installation(&snapshot, &open).package.clone(),
                definition_revision: 1,
                requirement_key: "required_attention".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![source_a.clone()],
            },
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let scope = task_scope(run_id, task_id);
    let environment = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    let execution = environment.delegate(request(person, run_id, task_id), &scope);
    let rebind = async {
        SOURCE_READ_ENTERED
            .get_or_init(tokio::sync::Notify::new)
            .notified()
            .await;
        let mut source_b = source_a.clone();
        source_b.connection_id =
            floe_context_contract::ConnectionId::try_new("attention-b").unwrap();
        let live_b = floe_connections::SourceConnection::establish_reviewed_native(
            person,
            source_b.connector_id.clone(),
            source_b.connection_id.clone(),
            source_b.execution_owner_id.clone(),
            floe_connections::ResourceMode::AllAvailable,
            attention_source(person).resources().to_vec(),
            "a".repeat(64),
        )
        .unwrap();
        floe_connections::SourceRepository::create(&open.core.store, &live_b)
            .await
            .unwrap();
        assert!(
            open.core
                .source_service()
                .load(person, &source_a.connection_id)
                .await
                .unwrap()
                .is_none()
        );
        open.vault
            .replace_expert_binding(
                Uuid::new_v4(),
                floe_experts::ExpertBindingCommand {
                    assignment_id: assignment.id,
                    package: registered_installation(&snapshot, &open).package.clone(),
                    definition_revision: 1,
                    requirement_key: "required_attention".into(),
                    expected_binding_revision: assignment.binding.revision + 1,
                    selected: vec![source_b],
                },
            )
            .await
            .unwrap();
        SOURCE_READ_RELEASE
            .get_or_init(tokio::sync::Notify::new)
            .notify_one();
    };
    let (result, ()) = tokio::join!(execution, rebind);
    let receipt = result.unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Failed, "{receipt:?}");
    assert_eq!(receipt.snapshot.issue, Some(AgentFailure::StaleContext));
    assert_eq!(
        open.vault
            .task(task_id)
            .await
            .unwrap()
            .unwrap()
            .selection
            .requirements[0]
            .selected[0]
            .connection_id
            .as_str(),
        source_a.connection_id.as_str(),
    );
    server.join().unwrap();
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
            expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
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
    struct ReplacementEndpoint(Arc<AtomicUsize>);
    impl floe_agent_contract::AgentEndpoint for ReplacementEndpoint {
        fn execute<'a>(
            &'a self,
            _: floe_agent_contract::EndpointInvocation,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ExpertReport, AgentFailure>,
        > {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(AgentFailure::CapabilityDenied) })
        }
    }
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
        root_environment_admission(),
    )
    .await
    .unwrap();
    let manifest = example_manifest();
    install_test_manifest(&open, manifest).await;
    assert_eq!(
        open.task_coordinator
            .environment(&person.to_string())
            .unwrap()
            .catalog()
            .cards
            .len(),
        1
    );
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let task_request = request(person, run_id, task_id);
    let scope = task_scope(run_id, task_id);
    let environment = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    let original_catalog = environment.catalog();
    let registry = floe_experts::AgentRegistry::restore(
        open.vault.expert_registry().await.unwrap().unwrap(),
        open.vault.registry_instance_id(),
    )
    .unwrap();
    let (_, admission) = registry
        .enabled_expert_admissions(person)
        .unwrap()
        .into_iter()
        .find(|(_, admission)| admission.package.id == task_request.selected_agent_id)
        .unwrap();
    let replacement_calls = Arc::new(AtomicUsize::new(0));
    open.directory
        .publish(
            "product.experts",
            vec![(
                DirectoryEntry {
                    definition: original_catalog.cards[0].clone(),
                    selection: registry.execution_selection(person, &admission).unwrap(),
                    admission,
                    reviewed: true,
                    enabled: true,
                    admitted_principals: vec![person.to_string()],
                    purposes: vec!["everyday-assistance".into()],
                },
                Arc::new(ReplacementEndpoint(Arc::clone(&replacement_calls))),
            )],
        )
        .unwrap();
    assert_eq!(environment.catalog(), original_catalog);
    assert_ne!(
        open.task_coordinator
            .environment(&person.to_string())
            .unwrap()
            .identity(),
        environment.identity()
    );
    let receipt = environment
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
    assert_eq!(replacement_calls.load(Ordering::SeqCst), 0);
    server.join().unwrap();
    let replay = environment.delegate(task_request, &scope).await.unwrap();
    assert_eq!(replay.snapshot, receipt.snapshot);
    assert_eq!(RUNNER_CALLS.load(Ordering::SeqCst), 1);
    let next_task = TaskId::new();
    let next = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
        .delegate(
            request(person, run_id, next_task),
            &task_scope(run_id, next_task),
        )
        .await
        .unwrap();
    assert_eq!(next.snapshot.issue, Some(AgentFailure::CapabilityDenied));
    assert_eq!(replacement_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn registered_runner_nonbuiltin_extension_chain_reads_only_its_exact_selection() {
    EXTENSION_CHAIN_RUNNER_CALLS.store(0, Ordering::SeqCst);
    let person = PersonId::new();
    let (_root, open, server) = installed_open(person, extension_chain_registration()).await;
    let overview = open.vault.registry_overview().await.unwrap().unwrap();
    assert_eq!(overview.definitions.len(), 9);
    assert!(
        overview
            .definitions
            .iter()
            .any(|definition| definition.package.id == "example.test.expert")
    );
    assert_eq!(overview.assignments.len(), 9);
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment_id = registered_assignment(&snapshot, &open).id;
    let assignment = overview
        .assignments
        .iter()
        .find(|assignment| assignment.id == assignment_id)
        .unwrap();
    assert_eq!(assignment.requirements[0].key, "required_tasks");
    let inspected = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "required_tasks",
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(inspected.candidates.len(), 1);
    assert_eq!(inspected.candidates[0].availability, "available");
    let saved = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "mac-local",
        Uuid::new_v4(),
        &crate::ExpertBindingSelectionIntent {
            assignment_id,
            package_id: "example.test.expert".into(),
            package_version: "1.0.0".into(),
            definition_revision: 1,
            requirement_key: "required_tasks".into(),
            expected_binding_revision: inspected.binding_revision,
            candidate_ids: vec![inspected.candidates[0].candidate_id.clone()],
        },
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert!(saved.candidates[0].selected);
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let selected = &registered_assignment(&snapshot, &open).binding.entries[0].selected[0];
    assert_eq!(selected.capability_id, "floe.tasks");
    assert!(
        !crate::first_party_observe::trusted_shipped_consumers("floe.tasks")
            .unwrap()
            .iter()
            .any(|consumer| consumer.identifier() == "example.test.expert")
    );
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let task_request = request(person, run_id, task_id);
    let scope = task_scope(run_id, task_id);
    let first = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
        .delegate(task_request.clone(), &scope)
        .await
        .unwrap();
    assert_eq!(first.snapshot.state, TaskState::Completed, "{first:?}");
    assert_eq!(
        first.snapshot.result.as_deref(),
        Some("Selected local tasks are available.")
    );
    assert!(first.snapshot.artifacts.iter().any(|artifact| artifact.parts.iter().any(|part| matches!(part, ArtifactPart::Data { media_type, data } if media_type == "application/vnd.example.tasks+json" && data.contains("selected_tasks")))));
    let stored = open.vault.task(task_id).await.unwrap().unwrap();
    assert_eq!(
        stored.selection.requirements[0].selected,
        vec![selected.clone()]
    );
    let replay = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
        .delegate(task_request, &scope)
        .await
        .unwrap();
    assert_eq!(replay.snapshot, first.snapshot);
    assert_eq!(EXTENSION_CHAIN_RUNNER_CALLS.load(Ordering::SeqCst), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn registered_runner_builtin_prefix_does_not_grant_first_party_observe() {
    let person = PersonId::new();
    let mut registration = extension_chain_registration();
    registration.manifest.package.id = "floe.builtin.impostor".into();
    registration.manifest.definition.card.id = "floe.builtin.impostor".into();
    registration.manifest.validate().unwrap();
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let candidate =
        floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
            person_id: person,
            device_id: "mac-local",
            capability: "floe.tasks",
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: &[],
        })
        .unwrap()
        .remove(0);
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: registered_assignment(&snapshot, &open).id,
                package: registered_installation(&snapshot, &open).package.clone(),
                definition_revision: 1,
                requirement_key: "required_tasks".into(),
                expected_binding_revision: registered_assignment(&snapshot, &open).binding.revision,
                selected: vec![candidate.reference.clone()],
            },
        )
        .await
        .unwrap();
    assert!(
        !crate::first_party_observe::trusted_shipped_consumers("floe.tasks")
            .unwrap()
            .iter()
            .any(|consumer| consumer.identifier() == "floe.builtin.impostor")
    );
}

#[tokio::test]
async fn registered_runner_extension_cannot_read_another_experts_selection() {
    EXTENSION_CHAIN_RUNNER_CALLS.store(0, Ordering::SeqCst);
    let person = PersonId::new();
    let registration = extension_chain_registration();
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    let mut other = extension_chain_registration();
    other.manifest.package.id = "example.other.expert".into();
    other.manifest.definition.card.id = "example.other.expert".into();
    other.manifest.validate().unwrap();
    let before = open.vault.expert_registry().await.unwrap().unwrap();
    open.vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: before.revision,
                operation_id: Uuid::new_v4(),
            },
            &[other.manifest.clone()],
            Cancellation::default(),
        )
        .await
        .unwrap();
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let other_installation = snapshot
        .installations
        .iter()
        .find(|installation| installation.package.id == "example.other.expert")
        .unwrap();
    let other_assignment = snapshot
        .assignments
        .iter()
        .find(|assignment| assignment.installation_id == other_installation.id)
        .unwrap();
    let selected = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id: person,
        device_id: "mac-local",
        capability: "floe.tasks",
        contract_version: 1,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: &[],
    })
    .unwrap()
    .remove(0)
    .reference;
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: other_assignment.id,
                package: other_installation.package.clone(),
                definition_revision: 1,
                requirement_key: "required_tasks".into(),
                expected_binding_revision: other_assignment.binding.revision,
                selected: vec![selected],
            },
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let mut task_request = request(person, run_id, task_id);
    journal_origin(&open, &mut task_request, run_id).await;
    let receipt = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
        .delegate(task_request, &task_scope(run_id, task_id))
        .await
        .unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed);
    assert_eq!(
        receipt.snapshot.result.as_deref(),
        Some("Expert settings are required before this task can run.")
    );
    assert_eq!(EXTENSION_CHAIN_RUNNER_CALLS.load(Ordering::SeqCst), 0);
    assert!(
        open.vault
            .expert_registry()
            .await
            .unwrap()
            .unwrap()
            .assignments
            .iter()
            .any(|assignment| assignment.id != other_assignment.id
                && assignment.binding.entries[0].selected.is_empty())
    );
}

#[tokio::test]
async fn registered_runner_product_endpoint_keeps_disabled_a_without_rerouting_to_b() {
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
        root_environment_admission(),
    )
    .await
    .unwrap();
    let first_install = open
        .vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: open.vault.registry_instance_id(),
                expected_revision: open
                    .vault
                    .expert_registry()
                    .await
                    .unwrap()
                    .unwrap()
                    .revision,
                operation_id: Uuid::new_v4(),
            },
            &[example_manifest()],
            Cancellation::default(),
        )
        .await
        .unwrap();
    let first_admission = open
        .vault
        .enabled_expert_admissions()
        .await
        .unwrap()
        .into_iter()
        .find(|(_, admission)| admission.package.id == "example.test.expert")
        .unwrap()
        .1;
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let first_request = request(person, run_id, task_id);
    let first_scope = task_scope(run_id, task_id);
    let first_environment = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap();
    let first = first_environment.delegate(first_request.clone(), &first_scope);
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
        let second_admission = open
            .vault
            .enabled_expert_admissions()
            .await
            .unwrap()
            .into_iter()
            .find(|(_, admission)| {
                admission.package.id == "example.test.expert" && admission.definition_revision == 2
            })
            .unwrap()
            .1;
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
                .environment(&person.to_string())
                .unwrap()
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
            .environment(&person.to_string())
            .unwrap()
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
    assert_eq!(first.snapshot.issue, None);
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
    let replay = first_environment
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
    let registration = required_source_registration(required_source_runner);
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    assert_eq!(
        open.task_coordinator
            .environment(&person.to_string())
            .unwrap()
            .catalog()
            .cards
            .len(),
        1
    );
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let mut task_request = request(person, run_id, task_id);
    journal_origin(&open, &mut task_request, run_id).await;
    let calls_before = REQUIRED_SOURCE_RUNNER_CALLS.load(Ordering::SeqCst);
    let receipt = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
        .delegate(task_request, &task_scope(run_id, task_id))
        .await
        .unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::Completed, "{receipt:?}");
    assert_eq!(
        receipt.snapshot.result.as_deref(),
        Some("Expert settings are required before this task can run.")
    );
    assert_eq!(
        REQUIRED_SOURCE_RUNNER_CALLS.load(Ordering::SeqCst),
        calls_before
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
        interactions[0].kind,
        floe_agent_contract::UserInteractionKind::ExpertBinding
    );
    assert_eq!(
        interactions[0].requirement.kind,
        floe_conversation::InteractionRequirementKind::ConfigureExpertBinding
    );
    assert!(
        matches!(&interactions[0].target, floe_conversation::ReviewedTarget::ExpertBinding(target) if target.requirement_key == "required_attention" && target.package.id == "example.test.expert")
    );
    assert_eq!(
        interactions[0].origin,
        floe_conversation::InteractionOrigin::Task {
            task_id: task_id.as_uuid(),
            capability_call_id: None,
        }
    );
    let caller = crate::CallerContext::verified(
        crate::LocalIdentityClaim {
            person_id: person.0,
            device_id: "mac-local".into(),
        },
        1,
    )
    .unwrap();
    let refresh = || crate::vault_host::interaction_resolution::RefreshInteractionCommand {
        interaction_id: interactions[0].id,
        command_id: Uuid::new_v4(),
        session_id: interactions[0].session_id,
        expected_revision: interactions[0].revision,
    };
    assert!(matches!(
        crate::vault_host::interaction_resolution::refresh_expert_binding(
            open.vault.as_ref(),
            open.conversation_repository.as_ref(),
            &caller,
            refresh(),
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap(),
        crate::vault_host::interaction_resolution::RefreshOutcome::StillPending { .. }
    ));
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment = &registered_assignment(&snapshot, &open);
    let source = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id: person,
        device_id: "mac-local",
        capability: "attention.coarse",
        contract_version: 1,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: &[attention_source(person)],
    })
    .unwrap()
    .remove(0)
    .reference;
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: registered_installation(&snapshot, &open).package.clone(),
                definition_revision: 1,
                requirement_key: "required_attention".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![source],
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        crate::vault_host::interaction_resolution::refresh_expert_binding(
            open.vault.as_ref(),
            open.conversation_repository.as_ref(),
            &caller,
            refresh(),
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap(),
        crate::vault_host::interaction_resolution::RefreshOutcome::Resolved { .. }
    ));
}

#[tokio::test]
async fn registered_runner_undeclared_requirement_is_denied_before_source_io() {
    let person = PersonId::new();
    let (_root, open, server) = installed_open(
        person,
        required_source_registration(undeclared_source_runner),
    )
    .await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let source = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id: person,
        device_id: "mac-local",
        capability: "attention.coarse",
        contract_version: 1,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: &[attention_source(person)],
    })
    .unwrap()
    .remove(0)
    .reference;
    open.vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: registered_assignment(&snapshot, &open).id,
                package: registered_installation(&snapshot, &open).package.clone(),
                definition_revision: 1,
                requirement_key: "required_attention".into(),
                expected_binding_revision: registered_assignment(&snapshot, &open).binding.revision,
                selected: vec![source],
            },
        )
        .await
        .unwrap();
    open.publish_expert_directory(&open.registrations)
        .await
        .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let receipt = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
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
async fn expert_settings_resolve_only_current_candidate_ids_and_rejoin_exact_save() {
    let cancellation = Cancellation::default();
    let person = PersonId::new();
    let registration = required_source_registration(required_source_runner);
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment_id = registered_assignment(&snapshot, &open).id;
    let absent = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "required_attention",
        &cancellation,
    )
    .await
    .unwrap();
    assert!(absent.candidates.is_empty());
    establish_attention_source(&open, person).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment_id = registered_assignment(&snapshot, &open).id;
    let first = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "required_attention",
        &cancellation,
    )
    .await
    .unwrap();
    assert_eq!(first.candidates.len(), 1);
    assert_eq!(first.candidates[0].availability, "available");
    let intent = crate::ExpertBindingSelectionIntent {
        assignment_id,
        package_id: "example.test.expert".into(),
        package_version: "1.0.0".into(),
        definition_revision: 1,
        requirement_key: "required_attention".into(),
        expected_binding_revision: first.binding_revision,
        candidate_ids: vec![first.candidates[0].candidate_id.clone()],
    };
    assert_eq!(
        crate::vault_host::expert_binding_settings::replace(
            &open,
            person,
            "other-device",
            Uuid::new_v4(),
            &intent,
            &cancellation,
        )
        .await,
        Err(AgentFailure::Conflict),
    );
    assert_eq!(
        registered_assignment(&open.vault.expert_registry().await.unwrap().unwrap(), &open)
            .binding
            .revision,
        first.binding_revision,
    );
    let operation_id = Uuid::new_v4();
    let saved = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "mac-local",
        operation_id,
        &intent,
        &cancellation,
    )
    .await
    .unwrap();
    assert_eq!(saved.binding_revision, first.binding_revision + 1);
    assert!(saved.candidates[0].selected);
    let disappeared = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "other-device",
        operation_id,
        &intent,
        &cancellation,
    )
    .await
    .unwrap();
    assert_eq!(disappeared.binding_revision, saved.binding_revision);
    assert!(disappeared.candidates.iter().any(|candidate| {
        candidate.availability == "unavailable"
            && candidate.selected
            && candidate.candidate_id == first.candidates[0].candidate_id
    }));
    let retry = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "mac-local",
        operation_id,
        &intent,
        &cancellation,
    )
    .await
    .unwrap();
    assert_eq!(retry, saved);
    let mut changed = intent.clone();
    changed.candidate_ids.clear();
    assert_eq!(
        crate::vault_host::expert_binding_settings::replace(
            &open,
            person,
            "mac-local",
            operation_id,
            &changed,
            &cancellation
        )
        .await
        .unwrap_err(),
        AgentFailure::Conflict,
    );
    let mut unknown = intent.clone();
    unknown.expected_binding_revision = saved.binding_revision;
    unknown.candidate_ids = vec!["b".repeat(64)];
    assert_eq!(
        crate::vault_host::expert_binding_settings::replace(
            &open,
            person,
            "mac-local",
            Uuid::new_v4(),
            &unknown,
            &cancellation
        )
        .await
        .unwrap_err(),
        AgentFailure::Conflict,
    );
    let unavailable = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "other-device",
        assignment_id,
        "required_attention",
        &cancellation,
    )
    .await
    .unwrap();
    assert_eq!(
        unavailable
            .candidates
            .iter()
            .filter(|candidate| candidate.availability == "unavailable" && candidate.selected)
            .count(),
        1
    );
    let remove = crate::ExpertBindingSelectionIntent {
        expected_binding_revision: saved.binding_revision,
        candidate_ids: vec![],
        ..intent
    };
    let remove_operation = Uuid::new_v4();
    let removed = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "other-device",
        remove_operation,
        &remove,
        &cancellation,
    )
    .await
    .unwrap();
    assert!(removed.candidates.is_empty());
    assert_eq!(removed.binding_revision, saved.binding_revision + 1);
    let rejoined = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "other-device",
        remove_operation,
        &remove,
        &cancellation,
    )
    .await
    .unwrap();
    assert_eq!(rejoined, removed);
}

#[tokio::test]
async fn contacts_source_edit_keeps_saved_expert_binding_and_logical_grant() {
    let person = PersonId::new();
    let mut registration = required_source_registration(required_source_runner);
    registration.manifest.source_requirements[0].capability = "people.identity".into();
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    let source = open
        .core
        .source_service()
        .establish_reviewed_native(
            person,
            floe_context_contract::ConnectorId::try_new("contacts.apple").unwrap(),
            floe_context_contract::ConnectionId::try_new("contacts.apple.local").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("apple:mac-local").unwrap(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    floe_context_contract::ResourceHandle::try_new("person.identity:a").unwrap(),
                    "A".into(),
                )
                .unwrap(),
            ],
            "a".repeat(64),
        )
        .await
        .unwrap();
    let policy = crate::first_party_observe::personal_policy("contacts.apple").unwrap();
    let logical = floe_context_contract::connection_view_resource(
        floe_context_contract::PEOPLE_VIEW_ID,
        source.connection_id(),
    )
    .unwrap();
    let scope = floe_access::GrantScope::try_new(
        vec![logical.clone()],
        policy.categories,
        vec![policy.operation],
        vec![policy.purpose],
        policy.consumers,
        policy.processing,
    )
    .unwrap();
    let source_binding = floe_access::GrantSourceBinding::try_new(
        person,
        source.connection_id().clone(),
        source.connector_id().clone(),
        source.execution_owner_id().clone(),
    )
    .unwrap();
    let grant = open
        .vault
        .activate_access_grants(vec![floe_vault::AccessGrantActivation {
            grant_id: floe_access::GrantId::new(),
            expected: None,
            source: source_binding.clone(),
            scope,
        }])
        .await
        .unwrap()
        .remove(0);
    assert_eq!(grant.scope().resources(), [logical]);
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment_id = registered_assignment(&snapshot, &open).id;
    let first = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "required_attention",
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(first.candidates.len(), 1);
    let saved = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "mac-local",
        Uuid::new_v4(),
        &crate::ExpertBindingSelectionIntent {
            assignment_id,
            package_id: "example.test.expert".into(),
            package_version: "1.0.0".into(),
            definition_revision: 1,
            requirement_key: "required_attention".into(),
            expected_binding_revision: first.binding_revision,
            candidate_ids: vec![first.candidates[0].candidate_id.clone()],
        },
        &Cancellation::default(),
    )
    .await
    .unwrap();
    let changed = open
        .core
        .source_service()
        .configure_reviewed_native(
            person,
            source.connection_id(),
            source.revision(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    floe_context_contract::ResourceHandle::try_new("person.identity:a").unwrap(),
                    "A".into(),
                )
                .unwrap(),
                floe_connections::ConnectionResource::new(
                    floe_context_contract::ResourceHandle::try_new("person.identity:b").unwrap(),
                    "B".into(),
                )
                .unwrap(),
            ],
            "b".repeat(64),
        )
        .await
        .unwrap();
    assert_eq!(changed.revision(), source.revision() + 1);
    assert_eq!(
        changed.source_authority(),
        source.source_authority().advance().unwrap()
    );
    let after = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "required_attention",
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(after.binding_revision, saved.binding_revision);
    assert_eq!(
        after.candidates[0].candidate_id,
        first.candidates[0].candidate_id
    );
    assert!(after.candidates[0].selected);
    let current = open
        .vault
        .data_access_grant_for_source_resource(
            &source_binding,
            &floe_context_contract::connection_view_resource(
                floe_context_contract::PEOPLE_VIEW_ID,
                changed.connection_id(),
            )
            .unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.id(), grant.id());
    assert_eq!(current.authority(), grant.authority());
}

#[tokio::test]
async fn hosted_calendar_settings_use_product_connection_and_pinned_producer() {
    use base64::Engine;
    use sha2::Digest;

    let person = PersonId::new();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let instance_id = Uuid::new_v4();
    let execution_owner = Uuid::new_v4().to_string();
    let public_key = [7u8; 32];
    vault
        .remote_pin_producer(floe_access::RemoteProducerIdentity {
            schema_version: 1,
            instance_id: instance_id.to_string(),
            execution_owner: execution_owner.clone(),
            audience: format!("floe.server:{instance_id}"),
            key_id: Uuid::new_v4().to_string(),
            public_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key),
            fingerprint: format!("{:x}", sha2::Sha256::digest(public_key)),
        })
        .await
        .unwrap();
    let core = Arc::new(FloeCore::open(":memory:").await.unwrap());
    let calendar_connection_id = Uuid::new_v4().to_string();
    core.source_service()
        .establish(
            person,
            floe_context_contract::ConnectorId::try_new("calendar.google").unwrap(),
            floe_context_contract::ConnectionId::try_new(calendar_connection_id.clone()).unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new(execution_owner.clone()).unwrap(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    floe_context_contract::ResourceHandle::try_new("primary").unwrap(),
                    "Primary".into(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
    let registration = super::conversation_turn::expert_dispatch::shipped_registrations()
        .into_iter()
        .find(|entry| entry.manifest.package.id == "floe.builtin.commitments")
        .unwrap();
    let manifest = registration.manifest.clone();
    let open = OpenVault::activate(
        vault,
        core,
        Arc::new(LocalContextHost::default()),
        CurrentSavedConnectionStore::fixed(Some(floe_inference::SavedServerConnection {
            base_url: "http://127.0.0.1:39275".into(),
            token: "t".repeat(32),
            client_id: "paired-client".into(),
            person_id: person.to_string(),
            device_id: "mac-local".into(),
        })),
        vec![registration],
        root_environment_admission(),
    )
    .await
    .unwrap();
    install_test_manifest(&open, manifest).await;
    let snapshot = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment_id = registered_assignment(&snapshot, &open).id;
    let catalog = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "floe.source.calendar",
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].availability, "available");
    let source_connection = open
        .core
        .source_service()
        .load(
            person,
            &floe_context_contract::ConnectionId::try_new(calendar_connection_id.clone()).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    let reference =
        floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
            person_id: person,
            device_id: "mac-local",
            capability: "calendar.timeline",
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: Some(&execution_owner),
            source_connections: std::slice::from_ref(&source_connection),
        })
        .unwrap()
        .remove(0);
    assert_eq!(catalog.candidates[0].candidate_id, reference.candidate_id);
    assert_eq!(
        reference.reference.connection_id.as_str(),
        calendar_connection_id
    );
    let source_service = open.core.source_service();
    let source = source_service
        .load(
            person,
            &floe_context_contract::ConnectionId::try_new(calendar_connection_id.clone()).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    let expanded = source_service
        .configure(
            person,
            source.connection_id(),
            source.revision(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    floe_context_contract::ResourceHandle::try_new("primary").unwrap(),
                    "Primary".into(),
                )
                .unwrap(),
                floe_connections::ConnectionResource::new(
                    floe_context_contract::ResourceHandle::try_new("work").unwrap(),
                    "Work".into(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
    assert_eq!(expanded.revision(), source.revision() + 1);
    let expanded_catalog = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "floe.source.calendar",
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(expanded_catalog.binding_revision, catalog.binding_revision);
    assert_eq!(expanded_catalog.candidates, catalog.candidates);

    let registry = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment = &registered_assignment(&registry, &open);
    let package = registered_installation(&registry, &open).package.clone();
    let saved_mail = floe_context_contract::SourceSelectionReference {
        connector_id: floe_context_contract::ConnectorId::try_new("mail.google").unwrap(),
        connection_id: floe_context_contract::ConnectionId::try_new("mail-account").unwrap(),
        execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new(&execution_owner)
            .unwrap(),
        capability_id: "mail.communication".into(),
        resource: floe_context_contract::ResourceHandle::try_new("mail:mail-account").unwrap(),
        contract_version: 1,
    };
    let binding = open
        .vault
        .replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id,
                package: package.clone(),
                definition_revision: 2,
                requirement_key: "floe.source.mail".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![saved_mail.clone()],
            },
        )
        .await
        .unwrap();
    let offline = crate::vault_host::expert_binding_settings::inspect(
        &open,
        person,
        "mac-local",
        assignment_id,
        "floe.source.mail",
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(offline.candidates.len(), 1);
    assert!(offline.candidates[0].selected);
    assert_eq!(offline.candidates[0].availability, "unavailable");
    let remove = crate::ExpertBindingSelectionIntent {
        assignment_id,
        package_id: package.id,
        package_version: package.version,
        definition_revision: 2,
        requirement_key: "floe.source.mail".into(),
        expected_binding_revision: binding.revision,
        candidate_ids: vec![],
    };
    let remove_operation = Uuid::new_v4();
    let removed = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "mac-local",
        remove_operation,
        &remove,
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert!(removed.candidates.is_empty());
    assert_eq!(removed.binding_revision, binding.revision + 1);
    let retry = crate::vault_host::expert_binding_settings::replace(
        &open,
        person,
        "mac-local",
        remove_operation,
        &remove,
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(retry, removed);
    let mut select_offline = remove;
    select_offline.expected_binding_revision = removed.binding_revision;
    select_offline.candidate_ids = vec![floe_context::source_candidate_id(&saved_mail).unwrap()];
    assert_eq!(
        crate::vault_host::expert_binding_settings::replace(
            &open,
            person,
            "mac-local",
            Uuid::new_v4(),
            &select_offline,
            &Cancellation::default(),
        )
        .await,
        Err(AgentFailure::CapabilityUnavailable),
    );
}

#[tokio::test]
async fn initial_shipped_setup_selects_single_native_source_only_once() {
    let person = PersonId::new();
    let registration = super::conversation_turn::expert_dispatch::shipped_registrations()
        .into_iter()
        .find(|entry| entry.manifest.package.id == "floe.builtin.focus-attention")
        .unwrap();
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    establish_attention_source(&open, person).await;
    let before = open.vault.expert_registry().await.unwrap().unwrap();
    let assignment_id = registered_assignment(&before, &open).id;
    assert_eq!(registered_assignment(&before, &open).binding.revision, 1);
    crate::vault_host::expert_binding_settings::bind_initial_defaults(
        &open,
        person,
        "mac-local",
        Uuid::new_v4(),
        &Cancellation::default(),
    )
    .await
    .unwrap();
    let first = open.vault.expert_registry().await.unwrap().unwrap();
    let binding = &first
        .assignments
        .iter()
        .find(|entry| entry.id == assignment_id)
        .unwrap()
        .binding;
    assert!(
        binding
            .entries
            .iter()
            .any(|entry| entry.capability == "attention.coarse" && entry.selected.len() == 1)
    );
    let revision = binding.revision;
    crate::vault_host::expert_binding_settings::bind_initial_defaults(
        &open,
        person,
        "other-device",
        Uuid::new_v4(),
        &Cancellation::default(),
    )
    .await
    .unwrap();
    let after = open.vault.expert_registry().await.unwrap().unwrap();
    assert_eq!(
        registered_assignment(&after, &open).binding.revision,
        revision
    );
}

#[tokio::test]
async fn shipped_descriptions_reach_context_projection_after_fresh_install() {
    for registration in super::conversation_turn::expert_dispatch::shipped_registrations() {
        let person = PersonId::new();
        let manifest = registration.manifest.clone();
        let expected = manifest.definition.clone();
        let (_root, open) =
            installed_open_without_provider(person, vec![registration], manifest).await;
        let environment = open
            .task_coordinator
            .environment(&person.to_string())
            .unwrap();
        let catalog = environment.catalog();
        assert_eq!(catalog.cards, vec![expected.clone()]);
        let active_experts = catalog
            .cards
            .iter()
            .map(|definition| definition.card.clone())
            .collect::<Vec<_>>();
        let started = floe_conversation::start_session(
            open.conversation_repository.as_ref(),
            floe_conversation::SessionRequest {
                principal: person.to_string(),
            },
        )
        .await
        .unwrap();
        let command_id = floe_agent_contract::CommandId::new();
        let admission = floe_conversation::ConversationRepository::admit_turn(
            open.conversation_repository.as_ref(),
            floe_conversation::TurnAdmissionRequest {
                expert_environment: environment.identity(),
                run_id: RunId::new(),
                command_id,
                session_id: started.session_id,
                expected_session_revision: 0,
                principal: person.to_string(),
                request_digest: [7; 32],
                mode: floe_conversation::TurnMode::New,
                retry_of: None,
                profile: floe_conversation::ProfileSelection::Auto,
                user_message: floe_agent_contract::AgentMessage {
                    message_id: command_id.as_uuid(),
                    role: floe_agent_contract::MessageRole::User,
                    text: "Review the supplied context.".into(),
                    call_id: None,
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                },
            },
        )
        .await
        .unwrap();
        let floe_conversation::TurnAdmission::Created(admitted) = admission else {
            panic!("expected Created");
        };
        assert_eq!(admitted.receipt.expert_environment, environment.identity());
        assert_eq!(
            admitted.receipt.expert_environment.revision,
            catalog.revision
        );
        assert_ne!(admitted.receipt.expert_environment.digest, [0; 32]);
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        };
        let projection =
            floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
                role: floe_context::ContextProjectionRole::Manager,
                purpose: floe_inference::CANONICAL_MODEL_PURPOSE,
                response_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT,
                correction: None,
                prompt: floe_conversation::prompts::manager_prompt(None).unwrap(),
                conversation: floe_agent_contract::ModelConversation {
                    history: vec![],
                    current_turn: vec![floe_agent_contract::ModelConversationEntry::User {
                        message_id: Uuid::new_v4(),
                        text: "Review the supplied context.".into(),
                    }],
                },
                agent_context: &context,
                catalog: &catalog,
                active_experts: &active_experts,
                authorized_history_dependencies: &[],
                input_data_classes: vec![floe_agent_contract::DataClass::Personal],
                max_output_bytes: 4096,
            })
            .unwrap();
        assert_eq!(
            projection.envelope.scoped_instructions.active_experts,
            active_experts
        );
        assert_eq!(
            projection.envelope.scoped_instructions.active_experts[0].version,
            "1.0.1"
        );
        assert_eq!(catalog.cards[0].definition_revision, 2);
    }
}

#[tokio::test]
async fn stale_shipped_manifest_blocks_activation_without_overwriting_configuration() {
    let person = PersonId::new();
    let registration = super::conversation_turn::expert_dispatch::shipped_registrations().remove(0);
    let mut stale = registration.manifest.clone();
    stale.definition.card.description = "Obsolete shipped description".into();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    vault
        .install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: vault.registry_instance_id(),
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &[stale],
            Cancellation::default(),
        )
        .await
        .unwrap();
    let before = vault.expert_registry().await.unwrap().unwrap();
    assert!(matches!(
        OpenVault::activate(
            vault,
            Arc::new(FloeCore::open(":memory:").await.unwrap()),
            Arc::new(LocalContextHost::default()),
            CurrentSavedConnectionStore::fixed(None),
            vec![registration],
            root_environment_admission()
        )
        .await,
        Err(AgentFailure::Conflict)
    ));
    let vault = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(vault.expert_registry().await.unwrap().unwrap(), before);
}

#[tokio::test]
async fn registered_runner_admission_and_manifest_mismatches_fail_closed() {
    RUNNER_CALLS.store(0, Ordering::SeqCst);
    let person = PersonId::new();
    let (_root, open) =
        installed_open_without_provider(person, vec![example_registration()], example_manifest())
            .await;
    let original = open
        .task_coordinator
        .environment(&person.to_string())
        .unwrap()
        .catalog();
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
                .environment(&person.to_string())
                .unwrap()
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
        open.task_coordinator
            .environment(&person.to_string())
            .unwrap()
            .catalog(),
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
            .environment(&person.to_string())
            .unwrap()
            .catalog()
            .cards
            .iter()
            .all(|definition| definition.card.id != "example.test.expert")
    );
    let run_id = RunId::new();
    let task_id = TaskId::new();
    assert!(
        open.task_coordinator
            .environment(&person.to_string())
            .unwrap()
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
            .environment(&person.to_string())
            .unwrap()
            .catalog()
            .cards
            .iter()
            .all(|definition| definition.card.id != "example.test.expert")
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
            .environment(&version_open.vault.person_id().to_string())
            .unwrap()
            .catalog()
            .cards
            .is_empty()
    );
}

#[tokio::test]
async fn registered_runner_extension_does_not_change_first_party_observe_policy() {
    let before = crate::first_party_observe::calendar_policy().unwrap();
    let fingerprint = crate::first_party_observe::policy_digest(&before).unwrap();
    let remote =
        crate::first_party_observe::member_policy_digest("gmail", "mail.communication").unwrap();
    let attention = crate::first_party_observe::personal_policy("attention.macos")
        .unwrap()
        .consumers;
    let attention_fingerprint =
        crate::first_party_observe::member_policy_digest("attention.macos", "attention.coarse")
            .unwrap();
    let person = PersonId::new();
    let registration = required_source_registration(required_source_runner);
    let manifest = registration.manifest.clone();
    let (_root, open) = installed_open_without_provider(person, vec![registration], manifest).await;
    assert_eq!(
        open.task_coordinator
            .environment(&person.to_string())
            .unwrap()
            .catalog()
            .cards
            .len(),
        1
    );
    let after = crate::first_party_observe::calendar_policy().unwrap();
    assert_eq!(after, before);
    assert_eq!(
        crate::first_party_observe::personal_policy("attention.macos")
            .unwrap()
            .consumers,
        attention
    );
    assert!(
        !attention
            .iter()
            .any(|consumer| consumer.identifier() == "example.test.expert")
    );
    assert_eq!(
        crate::first_party_observe::member_policy_digest("attention.macos", "attention.coarse")
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
        crate::first_party_observe::policy_digest(&after).unwrap(),
        fingerprint
    );
    assert_eq!(
        crate::first_party_observe::member_policy_digest("gmail", "mail.communication").unwrap(),
        remote
    );
}
