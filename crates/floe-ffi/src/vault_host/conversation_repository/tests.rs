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
use floe_core::VaultKey;
use floe_domain::PersonId;

use super::*;

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
