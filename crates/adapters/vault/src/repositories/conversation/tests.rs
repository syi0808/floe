use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

use crate::{VaultConversationAdmissionRequest, VaultKey};
use floe_agent_contract::prompts::{
    PromptAssembly, PromptComponent, PromptComponentKind, PromptRole,
};
use floe_agent_contract::{
    AllowedCatalog, AuthorizedModelProjection, BatchCursor, BoxFuture, ContextEnvelope,
    ContextManifest, ContextualData, DataClass, DelegationPort, DelegationRequest,
    DependencyCoverage, ModelPort, ModelProjectionPort, ModelProjectionRequest, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, PinnedToolRevision, ProjectionRef, RoleSpec,
    RuntimeContext, ScopedInstructions, ToolCall, ToolDescriptor, ToolPort, ToolResult,
    ValidatedModelBatch,
};
use floe_conversation::SessionStore;
use floe_conversation::{
    ConversationPorts, ConversationService, FinalPayloadValidator, ManagerConfig, TurnMode,
    TurnRequest,
};
use floe_kernel::PersonId;

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

struct TestProjector;

static PROJECTOR: TestProjector = TestProjector;

impl ModelProjectionPort for TestProjector {
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>> {
        request.validate().unwrap();
        let envelope = ContextEnvelope {
            schema_version: floe_agent_contract::AGENT_VERSION,
            stable_instructions: PromptAssembly {
                schema_version: floe_agent_contract::AGENT_VERSION,
                role: PromptRole::Manager,
                components: vec![
                    PromptComponent {
                        kind: PromptComponentKind::BehaviorKernel,
                        source: "test-kernel".into(),
                        revision: 1,
                        content: "kernel".into(),
                    },
                    PromptComponent {
                        kind: PromptComponentKind::Role,
                        source: "test-role".into(),
                        revision: 1,
                        content: "role".into(),
                    },
                    PromptComponent {
                        kind: PromptComponentKind::CapabilityProtocol,
                        source: "test-protocol".into(),
                        revision: 1,
                        content: "protocol".into(),
                    },
                ],
            },
            scoped_instructions: ScopedInstructions {
                purpose: "test-purpose".into(),
                response_contract: request.role.output_contract.clone(),
                available_capabilities: vec![],
                active_experts: vec![],
                correction: request.correction.clone(),
            },
            contextual_data: ContextualData {
                projection_version: 1,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            conversation: request.conversation.clone(),
            runtime: RuntimeContext {
                max_output_bytes: request.max_output_bytes,
            },
            manifest: ContextManifest {
                prompt_components: vec![],
                evidence: vec![],
                memories: vec![],
                agent_cards: vec![],
            },
        };
        Box::pin(async move {
            Ok(AuthorizedModelProjection {
                projection_ref: ProjectionRef::new(),
                projection_revision: 1,
                envelope,
                coverage: DependencyCoverage::Independent,
                input_data_classes: vec![DataClass::Synthetic],
            })
        })
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

#[derive(Default)]
struct FinalizingModel {
    calls: std::sync::atomic::AtomicUsize,
}

impl ModelPort for FinalizingModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: if call == 0 {
                    vec![ModelStep::CallTool {
                        tool_id: "lookup".into(),
                        definition_revision: 1,
                        input: "{}".into(),
                    }]
                } else {
                    assert_eq!(call, 1);
                    assert!(request.catalog.tools.is_empty());
                    assert!(request.catalog.cards.is_empty());
                    vec![ModelStep::Answer {
                        text: "The lookup finished, but the full request did not complete.".into(),
                        artifacts: vec![],
                    }]
                },
                usage: ModelUsage {
                    tokens: 2,
                    cost_micros: 1,
                },
            })
        })
    }
}

#[derive(Default)]
struct ReadTool {
    calls: std::sync::atomic::AtomicUsize,
}

#[derive(Default)]
struct ToolThenAnswerModel {
    calls: std::sync::atomic::AtomicUsize,
}

impl ModelPort for ToolThenAnswerModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: if call == 0 {
                    vec![ModelStep::CallTool {
                        tool_id: "actions.receipt".into(),
                        definition_revision: 1,
                        input: "{}".into(),
                    }]
                } else {
                    vec![ModelStep::Answer {
                        text: "source-derived answer".into(),
                        artifacts: vec![],
                    }]
                },
                usage: ModelUsage {
                    tokens: 2,
                    cost_micros: 1,
                },
            })
        })
    }
}

struct DependentReceiptTool {
    coverage: DependencyCoverage,
}

impl ToolPort for DependentReceiptTool {
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        let coverage = self.coverage.clone();
        Box::pin(async move {
            Ok(ToolResult {
                call_id: call.call_id,
                text: "source observation".into(),
                artifacts: vec![ContractArtifact {
                    artifact_id: Uuid::new_v4(),
                    name: "Action receipt".into(),
                    parts: vec![ContractArtifactPart::Data {
                        media_type: "application/vnd.floe.action-receipt+json".into(),
                        data: "{\"status\":\"settled\"}".into(),
                    }],
                    coverage: coverage.clone(),
                }],
                coverage,
                issue: None,
            })
        })
    }
}

impl ToolPort for ReadTool {
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            Ok(ToolResult {
                call_id: call.call_id,
                text: "encrypted lookup result".into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
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
        if (role == "manager" || role == floe_conversation::FINALIZATION_ROLE_ID)
            && !text.trim().is_empty()
        {
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
                instructions: "Answer safely.".into(),
                output_contract: "User-facing text.".into(),
            },
            purpose: "test-purpose".into(),
            max_iterations: 4,
            max_output_bytes: 16 * 1024,
            max_run_duration: std::time::Duration::from_secs(10),
            budget: floe_execution::budget::BudgetConfig::new(16_384, 1_000_000),
        },
    )
    .unwrap()
}

fn build_finalization_service(
    repository: Arc<VaultConversationRepository<Keys>>,
) -> ConversationService<VaultConversationRepository<Keys>> {
    ConversationService::new(
        repository,
        ManagerConfig {
            role_spec: RoleSpec {
                role_id: "manager".into(),
                instructions: "Answer safely.".into(),
                output_contract: "User-facing text.".into(),
            },
            purpose: "test-purpose".into(),
            max_iterations: 1,
            max_output_bytes: 16 * 1024,
            max_run_duration: std::time::Duration::from_secs(10),
            budget: floe_execution::budget::BudgetConfig::new(8_192, 100)
                .with_finalization_reserve(1_024, 10),
        },
    )
    .unwrap()
}

fn delegation_context() -> floe_agent_contract::DelegationExecutionContext {
    floe_agent_contract::DelegationExecutionContext {
        session_id: Uuid::new_v4(),
        device_id: "test-device".into(),
        agent_context: floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        max_output_bytes: 16 * 1024,
    }
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
        mode: TurnMode::New,
        retry_of: None,
        profile: floe_conversation::ProfileSelection::Auto,
        allowed_catalog: AllowedCatalog::default(),
        replay: vec![],
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
        cancellation,
        delegation_context: Some(delegation_context()),
    }
}

fn archive_dependency(person_id: PersonId) -> floe_context::ContextDependency {
    use chrono::{Duration, Utc};
    use floe_access::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };

    let now = Utc::now();
    floe_context::ContextDependency::try_new(
        person_id,
        GrantId::new(),
        GrantAuthority::new(),
        GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("archive-connection").unwrap(),
            ConnectorId::try_new("archive-connector").unwrap(),
            ExecutionOwnerId::try_new("archive-owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap(),
        vec![ResourceHandle::try_new("action/receipt").unwrap()],
        vec![GrantDataCategory::Metadata],
        GrantOperation::Read,
        GrantPurpose::Scheduling,
        GrantConsumer::builtin("manager").unwrap(),
        ProcessingRestriction::LocalOnly,
        ConsumerPolicyAuthority::new(),
        Uuid::new_v4(),
        b"archive-query".to_vec(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        now,
        now + Duration::minutes(5),
    )
    .unwrap()
}

#[tokio::test]
async fn session_management_uses_conversation_owner_and_rejects_foreign_or_sample_reads() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = Arc::new(
        EncryptedAgentVault::create(root.path(), person_id, Keys::default())
            .await
            .unwrap(),
    );
    let repository = VaultConversationRepository::new(Arc::clone(&vault));
    let started = floe_conversation::start_session(
        &repository,
        floe_conversation::SessionRequest {
            principal: person_id.to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(started.session_revision, 0);
    assert_eq!(
        floe_conversation::get_session(
            &repository,
            floe_conversation::SessionReadRequest {
                principal: person_id.to_string(),
                session_id: started.session_id,
            },
        )
        .await
        .unwrap(),
        started
    );
    assert_eq!(
        floe_conversation::resume_session(
            &repository,
            floe_conversation::SessionRequest {
                principal: person_id.to_string(),
            },
        )
        .await
        .unwrap(),
        started
    );
    assert_eq!(
        floe_conversation::get_session(
            &repository,
            floe_conversation::SessionReadRequest {
                principal: PersonId::new().to_string(),
                session_id: started.session_id,
            },
        )
        .await,
        Err(AgentFailure::CapabilityDenied)
    );
    let sample = vault.create_sample_session().await.unwrap();
    assert_eq!(
        floe_conversation::get_session(
            &repository,
            floe_conversation::SessionReadRequest {
                principal: person_id.to_string(),
                session_id: sample.id,
            },
        )
        .await,
        Err(AgentFailure::PolicyDenied)
    );
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
                projection: &PROJECTOR,
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
    assert_eq!(
        floe_conversation::get_command(
            repository.as_ref(),
            floe_conversation::CommandQuery {
                principal: person_id.to_string(),
                command_id,
            },
        )
        .await
        .unwrap(),
        Some(receipt.clone())
    );
    assert_eq!(
        floe_conversation::get_run(
            repository.as_ref(),
            floe_conversation::RunQuery {
                principal: person_id.to_string(),
                run_id: receipt.run_id,
            },
        )
        .await
        .unwrap(),
        Some(receipt.clone())
    );
    assert_eq!(
        floe_conversation::get_run(
            repository.as_ref(),
            floe_conversation::RunQuery {
                principal: PersonId::new().to_string(),
                run_id: receipt.run_id,
            },
        )
        .await,
        Err(AgentFailure::CapabilityDenied)
    );
    let stored = vault
        .conversation_run(receipt.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.journal_revision, 5);
    let journal = repository.load_journal(receipt.run_id).await.unwrap();
    assert_eq!(journal.len(), 5);
    assert!(matches!(journal[0].event, JournalEvent::ModelIntent { .. }));
    assert!(matches!(journal[1].event, JournalEvent::ModelResult { .. }));
    assert!(matches!(
        journal[2].event,
        JournalEvent::ValidatedBatch { .. }
    ));
    assert!(matches!(
        journal[3].event,
        JournalEvent::BatchProgress { .. }
    ));
    assert!(matches!(journal[4].event, JournalEvent::Output { .. }));
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
                projection: &PROJECTOR,
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
async fn t28_compaction_preserves_recovery_and_provenance() {
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
    let dependency = archive_dependency(person_id);
    let tools = DependentReceiptTool {
        coverage: DependencyCoverage::dependent(dependency.clone()).unwrap(),
    };
    let mut first_request = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        floe_execution::Cancellation::default(),
    );
    first_request.principal = person_id.to_string();
    first_request.prompt = "find the source and retain its action receipt".into();
    first_request.allowed_catalog = AllowedCatalog {
        cards: vec![],
        tools: vec![ToolDescriptor {
            id: "actions.receipt".into(),
            definition_revision: 1,
            description: "Read a source-bound action receipt.".into(),
            input_schema: "{\"type\":\"object\"}".into(),
            output_data_class: "personal".into(),
        }],
        revision: 1,
    };
    let first = service
        .run_turn(
            first_request,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &ToolThenAnswerModel::default(),
                tools: &tools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.state, RunState::Completed);
    assert!(matches!(
        first.coverage,
        DependencyCoverage::Dependent { .. }
    ));
    let journal_before = vault.conversation_journal(first.run_id).await.unwrap();
    assert!(journal_before.iter().any(|entry| {
        entry.kind == "result"
            && entry
                .payload
                .contains("application/vnd.floe.action-receipt+json")
    }));

    let mut second_request = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        floe_execution::Cancellation::default(),
    );
    second_request.principal = person_id.to_string();
    second_request.expected_session_revision = first.session_revision;
    second_request.prompt = "keep this later turn".into();
    let second = service
        .run_turn(
            second_request,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &Model::default(),
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.state, RunState::Completed);

    let compacted = service
        .compact_session(floe_conversation::CompactionRequest {
            session_id: session.id,
            expected_session_revision: second.session_revision,
            principal: person_id.to_string(),
            through_turn_id: first.run_id.as_uuid(),
            summary: "source-derived compacted summary".into(),
        })
        .await
        .unwrap();
    assert_eq!(compacted.session_revision, second.session_revision + 1);
    assert!(matches!(
        compacted.summary.coverage,
        DependencyCoverage::Dependent { .. }
    ));
    let live = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(live.messages.len(), 3);
    assert!(matches!(
        &live.messages[0],
        AgentMessage::Compaction { summary, .. }
            if summary == "source-derived compacted summary"
    ));
    assert!(
        live.messages[1..]
            .iter()
            .all(|message| message.turn_id() == second.run_id.as_uuid())
    );
    assert_eq!(
        vault.conversation_journal(first.run_id).await.unwrap(),
        journal_before
    );

    let archive_request = floe_agent_contract::ArchiveReadRequest {
        person_id,
        session_id: session.id,
        pointer: compacted.pointer.clone(),
        max_messages: 8,
        max_bytes: 4 * 1024,
    };
    let denied = service
        .read_archive(&archive_request, |candidate| {
            let expected = dependency.clone();
            async move { Ok(candidate != expected) }
        })
        .await
        .unwrap();
    assert!(denied.messages.is_empty());
    assert_eq!(
        service
            .read_archive(&archive_request, |_| async {
                Err(AgentFailure::VaultUnavailable)
            })
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    let recovered = service
        .read_archive(&archive_request, |_| async { Ok(true) })
        .await
        .unwrap();
    assert_eq!(recovered.messages.len(), 2);
    assert!(
        recovered
            .messages
            .iter()
            .all(|message| message.turn_id == first.run_id.as_uuid())
    );
    assert!(
        serde_json::to_vec(
            &recovered
                .messages
                .iter()
                .map(|message| &message.message)
                .collect::<Vec<_>>()
        )
        .unwrap()
        .len()
            <= archive_request.max_bytes
    );

    drop(service);
    drop(repository);
    drop(vault);
    let reopened = Arc::new(
        EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap(),
    );
    reopened.activate_conversation_executor().await.unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&reopened)));
    let service = build_service(Arc::clone(&repository));
    assert_eq!(
        reopened.conversation_journal(first.run_id).await.unwrap(),
        journal_before
    );
    let recovered = service
        .read_archive(&archive_request, |_| async { Ok(true) })
        .await
        .unwrap();
    assert_eq!(recovered.messages.len(), 2);

    let mut third_request = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        floe_execution::Cancellation::default(),
    );
    third_request.principal = person_id.to_string();
    third_request.expected_session_revision = compacted.session_revision;
    third_request.prompt = "continue after compaction".into();
    let third = service
        .run_turn(
            third_request,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &Model::default(),
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(third.state, RunState::Completed);
    assert_eq!(third.session_revision, compacted.session_revision + 2);
}

#[tokio::test]
async fn finalization_commits_reply_while_encrypted_run_remains_failed() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = Arc::new(
        EncryptedAgentVault::create(root.path(), person_id, Keys::default())
            .await
            .unwrap(),
    );
    let session = vault.create_session().await.unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
    let service = build_finalization_service(Arc::clone(&repository));
    let model = FinalizingModel::default();
    let tools = ReadTool::default();
    let mut turn = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        floe_execution::Cancellation::default(),
    );
    turn.principal = person_id.to_string();
    turn.allowed_catalog = AllowedCatalog {
        cards: vec![],
        tools: vec![ToolDescriptor {
            id: "lookup".into(),
            definition_revision: 1,
            description: "Read an independent value.".into(),
            input_schema: "{\"type\":\"object\"}".into(),
            output_data_class: "public".into(),
        }],
        revision: 1,
    };

    let receipt = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &tools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::Stalled));
    assert_eq!(
        receipt.output.as_deref(),
        Some("The lookup finished, but the full request did not complete.")
    );
    assert!(receipt.continuation().is_none());
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(tools.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let stored_session = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(stored_session.continuation, None);
    assert_eq!(
        stored_session.last_outcome,
        Some(floe_conversation::AgentOutcome::Halted {
            reason: AgentFailure::Stalled
        })
    );
    assert!(matches!(
        stored_session.messages.as_slice(),
        [AgentMessage::User { .. }, AgentMessage::Assistant { text, .. }]
            if text == "The lookup finished, but the full request did not complete."
    ));
    // Work: intent, result, batch, cursor, tool intent/result, cursor,
    // checkpoint. Finalization: intent, result, batch, cursor, output.
    assert_eq!(
        repository.load_journal(receipt.run_id).await.unwrap().len(),
        13
    );
}

#[tokio::test]
async fn encrypted_journal_projects_cumulative_settled_continuation_work() {
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
            continuation: None,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    let attempt_id = Uuid::new_v4();
    let batch = ValidatedModelBatch {
        execution_id: Uuid::new_v4(),
        attempt_id,
        projection_ref: ProjectionRef::new(),
        batch_id: Uuid::new_v4(),
        steps: vec![ModelStep::CallTool {
            tool_id: "read.context".into(),
            definition_revision: 1,
            input: "{}".into(),
        }],
        catalog_revision: 1,
        tool_revisions: vec![PinnedToolRevision {
            tool_id: "read.context".into(),
            definition_revision: 1,
        }],
        agent_revisions: vec![],
        projection_coverage: DependencyCoverage::Independent,
        delegation_context: None,
    };
    let call = ToolCall {
        call_id: floe_agent_runtime::stable_call_id(batch.execution_id, batch.batch_id, 0),
        invocation_key: floe_agent_runtime::stable_invocation_key(
            batch.execution_id,
            batch.batch_id,
            0,
            floe_agent_runtime::InvocationKind::Tool,
        ),
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
        (
            "intent",
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: batch.projection_ref,
            },
        ),
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
        (
            "checkpoint",
            JournalEvent::ValidatedBatch {
                batch: batch.clone(),
            },
        ),
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: batch.batch_id,
                    next_step_index: 0,
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
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: batch.batch_id,
                    next_step_index: 1,
                },
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
            crate::VaultConversationTerminal {
                state: crate::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
    let continuation =
        floe_conversation::continuation(repository.as_ref(), run_id, &person_id.to_string())
            .await
            .unwrap();
    assert_eq!(continuation.completed_iterations, 1);
    assert_eq!(continuation.model_conversation.len(), 2);
    assert_eq!(continuation.replay.len(), 1);
    assert_eq!(continuation.replay[0].call_id, call.call_id);
    assert_eq!(continuation.replay[0].result, result.text);

    let second_run_id = RunId::new();
    vault
        .admit_conversation_turn(VaultConversationAdmissionRequest {
            run_id: second_run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id: session.id,
            person_id,
            expected_session_revision: continuation.session_revision,
            request_digest: [8; 32],
            text: "continue safely".into(),
            continuation: Some(crate::VaultConversationContinuationRef {
                run_id: continuation.reference.run_id,
                executor_generation: continuation.reference.executor_generation,
                level: continuation.reference.level,
            }),
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    let second_attempt_id = Uuid::new_v4();
    let second_batch = ValidatedModelBatch {
        execution_id: Uuid::new_v4(),
        attempt_id: second_attempt_id,
        projection_ref: ProjectionRef::new(),
        batch_id: Uuid::new_v4(),
        steps: vec![ModelStep::CallTool {
            tool_id: "read.more-context".into(),
            definition_revision: 1,
            input: "{}".into(),
        }],
        catalog_revision: 1,
        tool_revisions: vec![PinnedToolRevision {
            tool_id: "read.more-context".into(),
            definition_revision: 1,
        }],
        agent_revisions: vec![],
        projection_coverage: DependencyCoverage::Independent,
        delegation_context: None,
    };
    let second_call = ToolCall {
        call_id: floe_agent_runtime::stable_call_id(
            second_batch.execution_id,
            second_batch.batch_id,
            0,
        ),
        invocation_key: floe_agent_runtime::stable_invocation_key(
            second_batch.execution_id,
            second_batch.batch_id,
            0,
            floe_agent_runtime::InvocationKind::Tool,
        ),
        tool_id: "read.more-context".into(),
        definition_revision: 1,
        input: "{}".into(),
    };
    let second_result = ToolResult {
        call_id: second_call.call_id,
        text: "second settled observation".into(),
        artifacts: vec![],
        coverage: DependencyCoverage::Independent,
        issue: None,
    };
    for (kind, event) in [
        (
            "intent",
            JournalEvent::ModelIntent {
                attempt_id: second_attempt_id,
                projection_ref: second_batch.projection_ref,
            },
        ),
        (
            "result",
            JournalEvent::ModelResult {
                attempt_id: second_attempt_id,
                usage: ModelUsage {
                    tokens: 2,
                    cost_micros: 1,
                },
            },
        ),
        (
            "checkpoint",
            JournalEvent::ValidatedBatch {
                batch: second_batch.clone(),
            },
        ),
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: second_batch.batch_id,
                    next_step_index: 0,
                },
            },
        ),
        (
            "intent",
            JournalEvent::ToolIntent {
                call: second_call.clone(),
            },
        ),
        (
            "result",
            JournalEvent::ToolResult {
                result: second_result.clone(),
            },
        ),
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: second_batch.batch_id,
                    next_step_index: 1,
                },
            },
        ),
        ("checkpoint", JournalEvent::Checkpoint { iteration: 1 }),
    ] {
        vault
            .append_conversation_journal(
                second_run_id,
                kind,
                &serde_json::to_string(&event).unwrap(),
            )
            .await
            .unwrap();
    }
    vault
        .finish_conversation_run(
            second_run_id,
            1,
            crate::VaultConversationTerminal {
                state: crate::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let continuation =
        floe_conversation::continuation(repository.as_ref(), second_run_id, &person_id.to_string())
            .await
            .unwrap();
    assert_eq!(continuation.completed_iterations, 2);
    assert_eq!(continuation.usage.attempts, 2);
    assert_eq!(continuation.usage.tokens, 3);
    assert_eq!(continuation.model_conversation.len(), 3);
    assert_eq!(continuation.replay.len(), 2);
    assert_eq!(continuation.replay[0].call_id, call.call_id);
    assert_eq!(continuation.replay[1].call_id, second_call.call_id);

    let service = build_service(Arc::clone(&repository));
    let model = Model::default();
    let mut turn = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        floe_execution::Cancellation::default(),
    );
    turn.principal = person_id.to_string();
    turn.expected_session_revision = continuation.session_revision;
    turn.prompt = "continue safely".into();
    turn.mode = TurnMode::Continue(continuation.reference);
    let completed = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.state, RunState::Completed);
    assert_eq!(completed.continuation_of, Some(second_run_id));
    assert_eq!(completed.continuation_level, 2);
    let session = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(
        session
            .messages
            .iter()
            .filter(|message| matches!(message, AgentMessage::User { .. }))
            .count(),
        1
    );
    assert_eq!(session.continuation, None);
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
            continuation: None,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    drop(vault);

    let opened = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    let activation = opened.activate_conversation_executor().await.unwrap();
    assert_eq!(activation.interrupted.len(), 1);
    let recovered = &activation.interrupted[0];
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

#[tokio::test]
async fn child_crash_before_resume_takeover_preserves_parent_pending() {
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
    // Parent times out with a pending answer batch at cursor 0.
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
            continuation: None,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch = ValidatedModelBatch {
        execution_id: Uuid::new_v4(),
        attempt_id,
        projection_ref,
        batch_id: Uuid::new_v4(),
        steps: vec![ModelStep::Answer {
            text: "done".into(),
            artifacts: vec![],
        }],
        catalog_revision: 1,
        tool_revisions: vec![],
        agent_revisions: vec![],
        projection_coverage: DependencyCoverage::Independent,
        delegation_context: None,
    };
    for (kind, event) in [
        (
            "intent",
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref,
            },
        ),
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
        (
            "checkpoint",
            JournalEvent::ValidatedBatch {
                batch: batch.clone(),
            },
        ),
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: batch.batch_id,
                    next_step_index: 0,
                },
            },
        ),
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
            crate::VaultConversationTerminal {
                state: crate::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
    let parent =
        floe_conversation::continuation(repository.as_ref(), run_id, &person_id.to_string())
            .await
            .unwrap();
    assert_eq!(parent.pending_batch.as_ref(), Some(&batch));
    assert_eq!(
        parent.batch_cursor,
        Some(BatchCursor {
            batch_id: batch.batch_id,
            next_step_index: 0,
        })
    );
    // The child is admitted but crashes before durably re-recording the
    // resume batch and cursor: its journal stays empty.
    let child_run_id = RunId::new();
    vault
        .admit_conversation_turn(VaultConversationAdmissionRequest {
            run_id: child_run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id: session.id,
            person_id,
            expected_session_revision: parent.session_revision,
            request_digest: [8; 32],
            text: "continue safely".into(),
            continuation: Some(crate::VaultConversationContinuationRef {
                run_id: parent.reference.run_id,
                executor_generation: parent.reference.executor_generation,
                level: parent.reference.level,
            }),
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    assert!(
        repository
            .load_journal(child_run_id)
            .await
            .unwrap()
            .is_empty()
    );
    vault
        .finish_conversation_run(
            child_run_id,
            1,
            crate::VaultConversationTerminal {
                state: crate::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    // Takeover never happened, so the parent pending batch stays
    // authoritative for the next continuation.
    let child =
        floe_conversation::continuation(repository.as_ref(), child_run_id, &person_id.to_string())
            .await
            .unwrap();
    assert_eq!(child.pending_batch.as_ref(), Some(&batch));
    assert_eq!(
        child.batch_cursor,
        Some(BatchCursor {
            batch_id: batch.batch_id,
            next_step_index: 0,
        })
    );
    // The next Engine resume executes the parent batch from its cursor
    // without a model call.
    let service = build_service(Arc::clone(&repository));
    let model = Model::default();
    let mut turn = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        floe_execution::Cancellation::default(),
    );
    turn.principal = person_id.to_string();
    turn.expected_session_revision = child.session_revision;
    turn.prompt = "continue safely".into();
    turn.mode = TurnMode::Continue(child.reference);
    let completed = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.state, RunState::Completed);
    assert_eq!(completed.output.as_deref(), Some("done"));
    assert_eq!(completed.continuation_of, Some(child_run_id));
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn child_resume_batch_mismatch_is_storage_fault() {
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
            continuation: None,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch = ValidatedModelBatch {
        execution_id: Uuid::new_v4(),
        attempt_id,
        projection_ref,
        batch_id: Uuid::new_v4(),
        steps: vec![ModelStep::Answer {
            text: "done".into(),
            artifacts: vec![],
        }],
        catalog_revision: 1,
        tool_revisions: vec![],
        agent_revisions: vec![],
        projection_coverage: DependencyCoverage::Independent,
        delegation_context: None,
    };
    for (kind, event) in [
        (
            "intent",
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref,
            },
        ),
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
        (
            "checkpoint",
            JournalEvent::ValidatedBatch {
                batch: batch.clone(),
            },
        ),
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: batch.batch_id,
                    next_step_index: 0,
                },
            },
        ),
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
            crate::VaultConversationTerminal {
                state: crate::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
    let parent =
        floe_conversation::continuation(repository.as_ref(), run_id, &person_id.to_string())
            .await
            .unwrap();
    assert_eq!(parent.pending_batch.as_ref(), Some(&batch));
    // The child re-records a different batch: recovery must fail closed.
    let child_run_id = RunId::new();
    vault
        .admit_conversation_turn(VaultConversationAdmissionRequest {
            run_id: child_run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id: session.id,
            person_id,
            expected_session_revision: parent.session_revision,
            request_digest: [8; 32],
            text: "continue safely".into(),
            continuation: Some(crate::VaultConversationContinuationRef {
                run_id: parent.reference.run_id,
                executor_generation: parent.reference.executor_generation,
                level: parent.reference.level,
            }),
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        })
        .await
        .unwrap();
    let other = ValidatedModelBatch {
        execution_id: Uuid::new_v4(),
        attempt_id: Uuid::new_v4(),
        projection_ref: ProjectionRef::new(),
        batch_id: Uuid::new_v4(),
        steps: vec![ModelStep::Answer {
            text: "a different plan".into(),
            artifacts: vec![],
        }],
        catalog_revision: 1,
        tool_revisions: vec![],
        agent_revisions: vec![],
        projection_coverage: DependencyCoverage::Independent,
        delegation_context: None,
    };
    assert_ne!(other.batch_id, batch.batch_id);
    for (kind, event) in [
        (
            "checkpoint",
            JournalEvent::ValidatedBatch {
                batch: other.clone(),
            },
        ),
        (
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: other.batch_id,
                    next_step_index: 0,
                },
            },
        ),
    ] {
        vault
            .append_conversation_journal(
                child_run_id,
                kind,
                &serde_json::to_string(&event).unwrap(),
            )
            .await
            .unwrap();
    }
    vault
        .finish_conversation_run(
            child_run_id,
            1,
            crate::VaultConversationTerminal {
                state: crate::VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        floe_conversation::continuation(repository.as_ref(), child_run_id, &person_id.to_string())
            .await,
        Err(AgentFailure::StorageUnavailable)
    ));
}

struct KeyRevokingModel {
    keys: Keys,
    calls: std::sync::atomic::AtomicUsize,
}

impl ModelPort for KeyRevokingModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.keys.0.lock().unwrap().clear();
        Box::pin(async move {
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: "must not publish after key loss".into(),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: 10,
                    cost_micros: 0,
                },
            })
        })
    }
}

#[tokio::test]
async fn canonical_runtime_key_loss_recovers_confirmed_history_without_model_replay() {
    use std::sync::atomic::Ordering;
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = Arc::new(
        EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap(),
    );
    let original_keys = keys.0.lock().unwrap().clone();
    let session = vault.create_session().await.unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
    let service = build_service(Arc::clone(&repository));
    let model = KeyRevokingModel {
        keys: keys.clone(),
        calls: Default::default(),
    };
    let mut turn = request(
        floe_agent_contract::CommandId::new(),
        session.id,
        Default::default(),
    );
    turn.principal = person.to_string();
    for _ in 0..2 {
        assert_eq!(
            service
                .run_turn(
                    turn.clone(),
                    ConversationPorts {
                        projection: &PROJECTOR,
                        model: &model,
                        tools: &NoTools,
                        delegation: &NoDelegation,
                        validator: &Validator,
                    }
                )
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    }
    drop(service);
    drop(repository);
    drop(vault);
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    *keys.0.lock().unwrap() = original_keys.clone();
    let reopened = Arc::new(
        EncryptedAgentVault::open(root.path(), person, keys.clone())
            .await
            .unwrap(),
    );
    let interrupted = reopened.load(person, session.id).await.unwrap();
    assert!(interrupted.active_turn.is_some());
    assert!(matches!(
        interrupted.messages.as_slice(),
        [AgentMessage::User { .. }]
    ));
    reopened.activate_conversation_executor().await.unwrap();
    let repository = Arc::new(VaultConversationRepository::new(Arc::clone(&reopened)));
    let interrupted = reopened.load(person, session.id).await.unwrap();
    let recovered = floe_conversation::recovered_session(
        repository.as_ref(),
        reopened.as_ref(),
        person,
        floe_conversation::RecoveryRequest {
            principal: person.to_string(),
            session_id: session.id,
            expected_session_revision: interrupted.revision,
        },
    )
    .await
    .unwrap();
    assert_eq!(recovered.messages, interrupted.messages);
    assert_eq!(
        recovered.last_outcome,
        Some(floe_conversation::AgentOutcome::Halted {
            reason: AgentFailure::Interrupted
        })
    );
    assert!(recovered.active_turn.is_none());
    assert_eq!(reopened.load(person, session.id).await.unwrap(), recovered);
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(*keys.0.lock().unwrap(), original_keys);
    let service = build_service(repository);
    let model = Model::default();
    turn.command_id = floe_agent_contract::CommandId::new();
    turn.expected_session_revision = recovered.revision;
    let completed = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &NoTools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.state, RunState::Completed);
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        reopened
            .load(person, session.id)
            .await
            .unwrap()
            .messages
            .len(),
        3
    );
}
