use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

use floe_agent::SessionStore;
use floe_agent_contract::{
    AllowedCatalog, BoundedContext, DelegationPort, DelegationRequest, ModelPort, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, RoleSpec, ToolCall, ToolPort, ToolResult,
};
use floe_conversation::{
    ConversationPorts, ConversationService, FinalPayloadValidator, ManagerConfig, TurnRequest,
};
use floe_core::{FloeCore, VaultConversationAdmissionRequest, VaultKey};
use floe_domain::PersonId;

use super::*;
use crate::vault_host::OpenVault;

#[derive(Clone, Default)]
struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

impl VaultKeyProvider for Keys {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person_id, vault_id))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &VaultKey,
    ) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person_id, vault_id), *key.as_bytes());
        Ok(())
    }
}

#[derive(Default)]
struct Model {
    calls: std::sync::atomic::AtomicUsize,
}

impl ModelPort for Model {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: "encrypted answer".into(),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: 2,
                    cost_micros: 1,
                },
            })
        })
    }
}

struct NoTools;
impl ToolPort for NoTools {
    fn invoke<'a>(
        &'a self,
        _: ToolCall,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }
}

struct NoDelegation;
impl DelegationPort for NoDelegation {
    fn delegate<'a>(
        &'a self,
        _: DelegationRequest,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }
}

struct Validator;
impl FinalPayloadValidator for Validator {
    fn validate(&self, role: &str, text: &str, _: &[ContractArtifact]) -> Result<(), AgentFailure> {
        if role == "manager" && !text.trim().is_empty() {
            Ok(())
        } else {
            Err(AgentFailure::InvalidModelOutput)
        }
    }
}

fn build_service(
    repository: Arc<VaultConversationRepository<Keys>>,
) -> ConversationService<VaultConversationRepository<Keys>> {
    ConversationService::new(
        repository,
        ManagerConfig {
            role_spec: RoleSpec {
                role_id: "manager".into(),
                prompt: "Answer safely.".into(),
                output_contract: "User-facing text.".into(),
            },
            max_iterations: 4,
            max_output_bytes: 16 * 1024,
            max_run_duration: std::time::Duration::from_secs(10),
            budget: floe_execution::budget::BudgetConfig::new(16_384, 1_000_000),
        },
    )
    .unwrap()
}

fn request(
    command_id: floe_agent_contract::CommandId,
    session_id: Uuid,
    cancellation: floe_execution::Cancellation,
) -> TurnRequest {
    TurnRequest {
        command_id,
        session_id,
        expected_session_revision: 0,
        principal: String::new(),
        prompt: "hello".into(),
        request_context_digest: [1; 32],
        bounded_context: BoundedContext {
            text: String::new(),
            coverage: DependencyCoverage::Independent,
        },
        allowed_catalog: AllowedCatalog::default(),
        replay: vec![],
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
        cancellation,
    }
}

#[tokio::test]
async fn service_commits_encrypted_run_and_replays_after_vault_reopen() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let vault = Arc::new(
        EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap(),
    );
    let session = vault.create_session().await.unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
    let service = build_service(Arc::clone(&repository));
    let model = Model::default();
    let command_id = floe_agent_contract::CommandId::new();
    let mut turn = request(
        command_id,
        session.id,
        floe_execution::Cancellation::default(),
    );
    turn.principal = person_id.to_string();
    let receipt = service
        .run_turn(
            turn.clone(),
            ConversationPorts {
                model: &model,
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(receipt.state, RunState::Completed);
    assert_eq!(receipt.session_revision, 2);
    let stored = vault
        .conversation_run(receipt.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.journal_revision, 3);
    let journal = repository.load_journal(receipt.run_id).await.unwrap();
    assert_eq!(journal.len(), 3);
    assert!(matches!(journal[0].event, JournalEvent::ModelIntent { .. }));
    assert!(matches!(journal[1].event, JournalEvent::ModelResult { .. }));
    assert!(matches!(journal[2].event, JournalEvent::Output { .. }));
    let legacy = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(legacy.active_turn, None);
    assert!(matches!(
        legacy.messages.as_slice(),
        [AgentMessage::User { .. }, AgentMessage::Assistant { text, .. }] if text == "encrypted answer"
    ));

    drop(service);
    drop(repository);
    drop(vault);
    let reopened = Arc::new(
        EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap(),
    );
    let activation = reopened.activate_conversation_executor().await.unwrap();
    assert!(activation.interrupted.is_empty());
    let reopened_repository = Arc::new(VaultConversationRepository::new(reopened));
    let reopened_service = build_service(reopened_repository);
    turn.deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
    let replay = reopened_service
        .run_turn(
            turn,
            ConversationPorts {
                model: &model,
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn encrypted_journal_projects_only_settled_continuation_work() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = Arc::new(
        EncryptedAgentVault::create(root.path(), person_id, Keys::default())
            .await
            .unwrap(),
    );
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    vault
        .admit_conversation_turn(VaultConversationAdmissionRequest {
            run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id: session.id,
            person_id,
            expected_session_revision: 0,
            request_digest: [9; 32],
            text: "continue safely".into(),
        })
        .await
        .unwrap();
    let attempt_id = Uuid::new_v4();
    let call = ToolCall {
        call_id: Uuid::new_v4(),
        invocation_key: floe_agent_contract::InvocationKey::new(),
        tool_id: "read.context".into(),
        definition_revision: 1,
        input: "{}".into(),
    };
    let result = ToolResult {
        call_id: call.call_id,
        text: "settled observation".into(),
        artifacts: vec![],
        coverage: DependencyCoverage::Independent,
        issue: None,
    };
    for (kind, event) in [
        ("intent", JournalEvent::ModelIntent { attempt_id }),
        (
            "result",
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
        ),
        ("intent", JournalEvent::ToolIntent { call: call.clone() }),
        (
            "result",
            JournalEvent::ToolResult {
                result: result.clone(),
            },
        ),
        ("checkpoint", JournalEvent::Checkpoint { iteration: 1 }),
    ] {
        vault
            .append_conversation_journal(run_id, kind, &serde_json::to_string(&event).unwrap())
            .await
            .unwrap();
    }
    vault
        .finish_conversation_run(
            run_id,
            1,
            floe_core::VaultConversationTerminal {
                state: floe_core::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let repository = VaultConversationRepository::new(vault);
    let continuation = floe_conversation::continuation(&repository, run_id, &person_id.to_string())
        .await
        .unwrap();
    assert_eq!(continuation.completed_iterations, 1);
    assert_eq!(continuation.messages.len(), 2);
    assert_eq!(continuation.replay.len(), 1);
    assert_eq!(continuation.replay[0].call_id, call.call_id);
    assert_eq!(continuation.replay[0].result, result.text);
}

#[tokio::test]
async fn open_vault_activation_interrupts_an_unfinished_conversation_run() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    let command_id = floe_agent_contract::CommandId::new();
    vault
        .admit_conversation_turn(VaultConversationAdmissionRequest {
            run_id,
            command_id,
            session_id: session.id,
            person_id,
            expected_session_revision: 0,
            request_digest: [9; 32],
            text: "unfinished".into(),
        })
        .await
        .unwrap();
    drop(vault);

    let opened = OpenVault::activate(
        EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap(),
        Arc::new(FloeCore::open(":memory:").await.unwrap()),
        Arc::new(crate::local_context::LocalContextStore::default()),
    )
    .await
    .unwrap();
    assert_eq!(opened._recovered_conversation_runs.len(), 1);
    let recovered = &opened._recovered_conversation_runs[0];
    assert_eq!(recovered.run_id, run_id);
    assert_eq!(recovered.command_id, command_id);
    assert_eq!(recovered.state, VaultConversationRunState::Interrupted);
    assert_eq!(recovered.issue, Some(AgentFailure::Interrupted));
    assert_eq!(recovered.executor_generation, 2);
    let recovered_session = opened.load(person_id, session.id).await.unwrap();
    assert_eq!(recovered_session.active_turn, None);
    assert_eq!(
        opened.conversation_run(run_id).await.unwrap().unwrap(),
        *recovered
    );
}
