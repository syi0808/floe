// Focused owner-custody regressions reuse the Core scenario fixture.

use super::*;
use crate::vault::owner_custody::{ConversationRunOutputReceipt, ConversationRunOutputRequest};
use crate::vault::owner_transcript_reads::{
    OwnerResolvedTranscriptEntry, OwnerTranscriptPageBudget, OwnerTranscriptTypedEvidence,
};
use floe_agent_contract::ModelConversationEntry;
use floe_conversation::TypedAgentMessageProvenance;
use floe_conversation_core::{
    ConversationReadTarget, TranscriptEntryLookup, TranscriptReadBoundary, TranscriptReadCursor,
};

fn expert_task_fixture(
    person_id: floe_kernel::PersonId,
    executor_generation: u64,
    task_id: floe_kernel::TaskId,
    assignment_id: Uuid,
    definition_revision: u64,
    package_version: &str,
    message: &str,
) -> (TaskRecord, floe_experts::ExpertTaskConversationDraft) {
    let admission = ExpertAdmissionIdentity {
        registry_instance_id: Uuid::new_v4(),
        assignment_id,
        installation_id: Uuid::new_v4(),
        package: floe_agent_contract::PackageRef {
            kind: PackageKind::Expert,
            id: "fixture.expert".into(),
            version: package_version.into(),
        },
        definition_revision,
    };
    let key = floe_experts::ExpertConversationKey::from_admission(person_id, &admission)
        .expect("derive authenticated Expert assignment conversation key");
    let snapshot = TaskSnapshot {
        task_id,
        parent_run_id: Some(Uuid::new_v4()),
        principal: person_id.to_string(),
        agent_id: "fixture.expert".into(),
        definition_revision,
        state: TaskState::Submitted,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        issue: None,
        blockage: None,
    };
    let record = TaskRecord {
        snapshot,
        admission,
        selection: ExpertExecutionSelection::without_requirements(1)
            .expect("valid empty source selection"),
        invocation_key: floe_agent_contract::InvocationKey::new(),
        request_digest: [33; 32],
        aggregate_revision: 1,
        executor_generation,
        execution_id: Uuid::new_v4(),
        device_id: "device-expert-history-test".into(),
        catalog_revision: definition_revision,
        model_allowance: floe_execution::budget::ModelReservationCeiling {
            tokens: 128,
            cost_micros: 128,
        },
        maximum_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
        journal_revision: 0,
        journal_digest: Sha256::digest(
            serde_json::to_vec(&(
                "floe.execution-journal.sha256.v1",
                Vec::<floe_agent_contract::JournalEntry>::new(),
            ))
            .expect("encode empty Task journal"),
        )
        .into(),
        receipt: None,
    };
    let draft = floe_experts::ExpertTaskConversationDraft {
        key,
        run_id: floe_kernel::RunId::new(),
        input_message_id: MessageId::new(),
        input_command_id: CommandId::new(),
        delegated_message: message.into(),
        input_coverage: DependencyCoverage::Independent,
    };
    (record, draft)
}

async fn start_expert_task(scenario: &Scenario, record: &TaskRecord) -> TaskRecord {
    let expert_input = scenario
        .vault()
        .expert_task_admission_reference(record.execution())
        .await
        .expect("read the durable Expert admission reference");
    scenario
        .vault()
        .compare_and_swap_task(
            record.snapshot.task_id,
            record.aggregate_revision,
            record.executor_generation,
            TaskSnapshot {
                state: TaskState::Working,
                ..record.snapshot.clone()
            },
            expert_input,
        )
        .await
        .expect("start exact admitted Expert Task and append its pinned input")
}

async fn append_scripted_expert_answer(
    scenario: &Scenario,
    working: &TaskRecord,
    output: &str,
    lose_output_ack: bool,
) -> Result<(), AgentFailure> {
    append_scripted_expert_answer_with_coverage(
        scenario,
        working,
        output,
        lose_output_ack,
        DependencyCoverage::Independent,
    )
    .await
}

async fn append_scripted_expert_answer_with_coverage(
    scenario: &Scenario,
    working: &TaskRecord,
    output: &str,
    lose_output_ack: bool,
    coverage: DependencyCoverage,
) -> Result<(), AgentFailure> {
    use floe_agent_contract::{
        BatchCursor, JournalEvent, ModelBindingDigest, ModelBudgetProfile, ModelCapabilities,
        ModelSelectionCommitment, ModelStep, ModelUsage, PreparedModelPlan, ProcessingBoundary,
        ProjectionRef, ValidatedModelBatch,
    };

    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch_id = Uuid::new_v4();
    let vault = scenario.vault();
    vault
        .append_task_journal(
            working.execution(),
            "intent",
            JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: Some(working.snapshot.task_id),
                reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                    tokens: 128,
                    cost_micros: 128,
                },
                projection_ref,
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: working.snapshot.principal.clone(),
                    device_id: working.device_id.clone(),
                    purpose: "everyday_assistance".into(),
                    consumer: floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([61; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([62; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
            },
        )
        .await?;
    vault
        .append_task_journal(
            working.execution(),
            "result",
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await?;
    vault
        .append_task_journal(
            working.execution(),
            "checkpoint",
            JournalEvent::ValidatedBatch {
                batch: ValidatedModelBatch {
                    execution_id: working.execution_id,
                    attempt_id,
                    projection_ref,
                    batch_id,
                    steps: vec![ModelStep::Answer {
                        text: output.into(),
                        artifacts: vec![],
                    }],
                    catalog_revision: working.catalog_revision,
                    tool_revisions: vec![],
                    agent_revisions: vec![],
                    projection_coverage: coverage,
                    delegation_context: None,
                },
            },
        )
        .await?;
    vault
        .append_task_journal(
            working.execution(),
            "checkpoint",
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await?;
    if lose_output_ack {
        vault
            .registry_transaction_ack_loss
            .store(true, std::sync::atomic::Ordering::Release);
    }
    vault
        .append_task_journal(
            working.execution(),
            "output",
            JournalEvent::Output {
                text: output.into(),
                artifacts: vec![],
            },
        )
        .await?;
    Ok(())
}

fn next_expert_task_for(
    template: &TaskRecord,
    person_id: floe_kernel::PersonId,
    admission: floe_experts::ExpertAdmissionIdentity,
    definition_revision: u64,
    message: &str,
) -> (TaskRecord, floe_experts::ExpertTaskConversationDraft) {
    let task_id = TaskId::new();
    let mut record = template.clone();
    record.snapshot = TaskSnapshot {
        task_id,
        parent_run_id: Some(Uuid::new_v4()),
        principal: person_id.to_string(),
        agent_id: admission.package.id.clone(),
        definition_revision,
        state: TaskState::Submitted,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        issue: None,
        blockage: None,
    };
    record.admission = admission.clone();
    record.selection = ExpertExecutionSelection::without_requirements(definition_revision)
        .expect("derive a valid empty Expert selection");
    record.invocation_key = floe_agent_contract::InvocationKey::new();
    record.request_digest = [definition_revision as u8 | 1; 32];
    record.aggregate_revision = 1;
    record.execution_id = Uuid::new_v4();
    record.catalog_revision = definition_revision;
    record.journal_revision = 0;
    record.journal_digest = Sha256::digest(
        serde_json::to_vec(&(
            "floe.execution-journal.sha256.v1",
            Vec::<floe_agent_contract::JournalEntry>::new(),
        ))
        .expect("encode empty Task journal"),
    )
    .into();
    record.receipt = None;
    let draft = floe_experts::ExpertTaskConversationDraft {
        key: floe_experts::ExpertConversationKey::from_admission(person_id, &admission)
            .expect("derive verified Expert assignment key"),
        run_id: floe_kernel::RunId::new(),
        input_message_id: MessageId::new(),
        input_command_id: CommandId::new(),
        delegated_message: message.into(),
        input_coverage: DependencyCoverage::Independent,
    };
    (record, draft)
}

async fn complete_manager_turn_with_output(
    scenario: &mut Scenario,
    user_text: &str,
    answer: String,
) -> RunRecord {
    use floe_agent_contract::{EngineStep, JournalEvent};

    scenario.identity = floe_conversation::prompts::default_manager_identity(scenario.person_id)
        .expect("derive the stable default Manager identity");
    let request = scenario.owner_request(user_text);
    let run = match scenario
        .vault()
        .admit_manager_conversation_turn(request, scenario.identity.clone())
        .await
        .expect("admit Manager turn through the production Vault composer")
    {
        crate::vault::VaultManagerConversationAdmission::Created(run) => run,
        other => panic!("unexpected Manager admission: {other:?}"),
    };
    append_valid_answer_batch(scenario, &run, &answer, Vec::new()).await;
    scenario
        .vault()
        .record_manager_conversation_journal(
            run.run_id,
            "output",
            JournalEvent::Output {
                text: answer.clone(),
                artifacts: Vec::new(),
            },
        )
        .await
        .expect("atomically record Manager output and typed Core contribution");
    let terminal = floe_conversation::RunTerminal {
        state: floe_conversation::RunState::Completed,
        output: Some(answer.clone()),
        steps: vec![EngineStep::Answer {
            text: answer,
            artifacts: Vec::new(),
        }],
        coverage: DependencyCoverage::Independent,
        issue: None,
        blocked: None,
        interactions: Vec::new(),
    };
    let settlement = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: run.run_id,
            expected_aggregate_revision: run.aggregate_revision,
            terminal,
        },
    )
    .await
    .expect("settle Manager owner and recorder together");
    let CoreComposedSettlementResult::Settled { record, .. } = settlement else {
        panic!("Manager turn should settle both owner and recorder")
    };
    scenario.session_revision = record.session_revision;
    record
}

async fn append_unresolved_model_attempt(scenario: &Scenario, run: &RunRecord) -> Uuid {
    use floe_agent_contract::{
        JournalEvent, ModelBindingDigest, ModelBudgetProfile, ModelCapabilities,
        ModelSelectionCommitment, PreparedModelPlan, ProcessingBoundary, ProjectionRef,
    };

    let attempt_id = Uuid::new_v4();
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: None,
                reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                    tokens: 128,
                    cost_micros: 128,
                },
                projection_ref: ProjectionRef::new(),
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: scenario.person_id.to_string(),
                    device_id: run.device_id.clone(),
                    purpose: "everyday_assistance".into(),
                    consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([61; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([62; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
            },
        )
        .await;
    attempt_id
}

async fn append_unresolved_delegation(scenario: &Scenario, run: &RunRecord) {
    use floe_agent_contract::{
        AgentContext, BatchCursor, DelegationExecutionContext, DelegationRequest, InvocationKey,
        JournalEvent, ModelBindingDigest, ModelBudgetProfile, ModelCapabilities,
        ModelSelectionCommitment, ModelStep, PinnedAgentRevision, PreparedModelPlan,
        ProcessingBoundary, ProjectionRef, ValidatedModelBatch,
    };

    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch_id = Uuid::new_v4();
    let context = DelegationExecutionContext {
        session_id: run.session_id,
        device_id: run.device_id.clone(),
        agent_context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
        projection_coverage: DependencyCoverage::Independent,
    };
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: None,
                reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                    tokens: 128,
                    cost_micros: 128,
                },
                projection_ref,
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: scenario.person_id.to_string(),
                    device_id: run.device_id.clone(),
                    purpose: "everyday_assistance".into(),
                    consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([81; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([82; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelResult {
                attempt_id,
                usage: floe_agent_contract::ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ValidatedBatch {
                batch: ValidatedModelBatch {
                    execution_id: run.run_id.as_uuid(),
                    attempt_id,
                    projection_ref,
                    batch_id,
                    steps: vec![ModelStep::Delegate {
                        agent_id: "fixture.expert".into(),
                        definition_revision: 1,
                        message: "unresolved delegated work".into(),
                        context_refs: vec![],
                    }],
                    catalog_revision: run.expert_environment.revision,
                    tool_revisions: vec![],
                    agent_revisions: vec![PinnedAgentRevision {
                        agent_id: "fixture.expert".into(),
                        definition_revision: 1,
                    }],
                    projection_coverage: DependencyCoverage::Independent,
                    delegation_context: Some(context.clone()),
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await;
    let execution_id = run.run_id.as_uuid();
    let task_id = floe_kernel::TaskId::from_uuid(Uuid::new_v5(
        &execution_id,
        format!("{execution_id}:{batch_id}:0:task").as_bytes(),
    ))
    .expect("derive stable unresolved TaskId");
    let invocation_key = InvocationKey::from_uuid(Uuid::new_v5(
        &execution_id,
        format!("{execution_id}:{batch_id}:0:delegation").as_bytes(),
    ))
    .expect("derive stable unresolved invocation");
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::DelegationIntent {
                request: DelegationRequest {
                    task_id,
                    parent_run_id: Some(run.run_id.as_uuid()),
                    principal: scenario.person_id.to_string(),
                    invocation_key,
                    selected_agent_id: "fixture.expert".into(),
                    selected_definition_revision: 1,
                    message: "unresolved delegated work".into(),
                    context_refs: vec![],
                    execution_context: context,
                },
            },
        )
        .await;
}

async fn actual_terminal_task_for_recovery(
    scenario: &Scenario,
    run: &RunRecord,
) -> floe_agent_contract::TaskReceipt {
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate actual Task owner")
        .executor_generation;
    let (_, receipts) = scenario
        .terminal_task_receipts(run, generation, false)
        .await;
    receipts
        .into_iter()
        .next()
        .expect("one actual terminal Task")
}

async fn actual_terminal_task_with_parent_result_for_recovery(
    scenario: &Scenario,
    run: &RunRecord,
) -> floe_agent_contract::TaskReceipt {
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate actual Task owner")
        .executor_generation;
    let (_, receipts) = scenario
        .terminal_task_receipts_with_manager_history(run, generation, 2, 2, false, true)
        .await;
    receipts
        .into_iter()
        .next()
        .expect("one actual terminal Task with its owner result")
}

fn recovery_actor(scenario: &Scenario) -> floe_kernel::OwnerActor {
    floe_kernel::OwnerActor {
        person_id: scenario.person_id,
        device_id: "device-core-custody-test".into(),
        runtime_epoch: 1,
    }
}

async fn run_settlement_counts(
    scenario: &Scenario,
    run: RunId,
) -> (i64, i64, i64, i64, i64, i64, i64) {
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect for settlement counts");
    let closes = run_table_count(
        &connection,
        "agent_conversation_core_v3_close_receipts",
        scenario.person_id,
        run,
    )
    .await;
    let active = run_table_count(
        &connection,
        "agent_conversation_core_v3_active_recorders",
        scenario.person_id,
        run,
    )
    .await;
    let audits = table_count(&connection, "agent_conversation_review_audits").await;
    let interactions = table_count(&connection, "agent_conversation_interactions").await;
    let terminal_receipts = table_count(&connection, "agent_conversation_terminal_receipts").await;
    let resume_requests = table_count(&connection, "agent_conversation_resume_requests").await;
    let coverage = table_count(&connection, "agent_context_dependency_coverage").await;
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
        .await
        .expect("start per Run settlement count");
    let scoped_close = close_receipt_on(&transaction, scenario.person_id, run)
        .await
        .expect("read exact close receipt")
        .is_some();
    let scoped_active = active_recorder_on(
        &transaction,
        Scope::from_identity(
            scenario.person_id,
            &scenario.identity,
            scenario.conversation_id,
            scenario.branch_id,
        ),
    )
    .await
    .expect("read active recorder")
    .is_some_and(|fence| fence.run_id == run);
    transaction.commit().await.expect("finish count snapshot");
    assert_eq!(scoped_close, closes == 1);
    assert_eq!(scoped_active, active == 1);
    (
        closes,
        active,
        audits,
        interactions,
        terminal_receipts,
        resume_requests,
        coverage,
    )
}

async fn run_table_count(
    connection: &turso::Connection,
    table: &str,
    person_id: floe_kernel::PersonId,
    run: RunId,
) -> i64 {
    connection
        .query(
            &format!("SELECT count(*) FROM {table} WHERE person_id = ? AND run_id = ?"),
            (person_id.to_string(), run.as_uuid().to_string()),
        )
        .await
        .expect("count owner rows scoped to the exact Run")
        .next()
        .await
        .expect("read exact Run row count")
        .expect("one count row")
        .get::<i64>(0)
        .expect("read Run row count")
}

#[derive(Debug, Eq, PartialEq)]
struct SettlementCustodySnapshot {
    run: Option<RunRecord>,
    journal: Vec<super::super::super::conversations::VaultConversationJournalEntry>,
    session: (i64, String),
    input_binding: Option<OwnerInputBinding>,
    coverage: Option<(i64, String)>,
    terminal_receipt: Option<String>,
    resume_rows: Vec<(String, String, String, String, Option<String>, String)>,
    review_audits: Vec<(String, String, String, String)>,
    interactions: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
    )>,
    head: Option<(ConversationHead, String)>,
    entries: Vec<(
        i64,
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )>,
    input_receipts: Vec<(String, i64, String)>,
    open_receipt: Option<RecorderOpenReceipt>,
    active_recorder: Option<RecorderFence>,
    close_receipt: Option<RecorderCloseReceipt>,
    retirement_receipt: Option<RecorderRetirementReceipt>,
}

async fn settlement_custody_snapshot(
    scenario: &Scenario,
    run_id: RunId,
) -> SettlementCustodySnapshot {
    let run = scenario
        .vault()
        .conversation_run(run_id)
        .await
        .expect("read owner Run snapshot");
    let journal = scenario
        .vault()
        .conversation_journal(run_id)
        .await
        .expect("read owner journal snapshot");
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect for complete settlement snapshot");
    let session = session_storage_snapshot(&connection, scenario.session_id).await;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start exact settlement snapshot");
    let scope = Scope::from_identity(
        scenario.person_id,
        &scenario.identity,
        scenario.conversation_id,
        scenario.branch_id,
    );
    let input_binding = owner_input_binding_on(&transaction, scenario.person_id, run_id)
        .await
        .expect("read exact Run input binding");
    let mut rows = transaction
        .query(
            "SELECT version, payload FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (scenario.person_id.to_string(), scenario.session_id.to_string(), run_id.as_uuid().to_string()),
        )
        .await
        .expect("read exact Run coverage");
    let coverage = rows
        .next()
        .await
        .expect("read exact Run coverage row")
        .map(|row| {
            (
                row.get::<i64>(0).expect("coverage version type"),
                row.get::<String>(1).expect("coverage payload type"),
            )
        });
    assert!(
        rows.next()
            .await
            .expect("check coverage row uniqueness")
            .is_none()
    );
    let terminal_receipt =
        super::super::super::conversations::terminal_receipt_on(&transaction, run_id)
            .await
            .expect("read exact terminal receipt");

    let mut rows = transaction
        .query(
            "SELECT origin_run_id, person_id, session_id, state, child_run_id, payload FROM agent_conversation_resume_requests WHERE origin_run_id = ? ORDER BY origin_run_id",
            [run_id.as_uuid().to_string()],
        )
        .await
        .expect("read exact Resume queue row");
    let mut resume_rows = Vec::new();
    while let Some(row) = rows.next().await.expect("read Resume queue row") {
        resume_rows.push((
            row.get::<String>(0).expect("Resume origin type"),
            row.get::<String>(1).expect("Resume person type"),
            row.get::<String>(2).expect("Resume Session type"),
            row.get::<String>(3).expect("Resume state type"),
            row.get::<Option<String>>(4).expect("Resume child type"),
            row.get::<String>(5).expect("Resume payload type"),
        ));
    }
    let mut rows = transaction
        .query(
            "SELECT operation_id, run_id, person_id, payload FROM agent_conversation_review_audits WHERE run_id = ? ORDER BY operation_id",
            [run_id.as_uuid().to_string()],
        )
        .await
        .expect("read exact review audit rows");
    let mut review_audits = Vec::new();
    while let Some(row) = rows.next().await.expect("read review audit row") {
        review_audits.push((
            row.get::<String>(0).expect("audit operation type"),
            row.get::<String>(1).expect("audit Run type"),
            row.get::<String>(2).expect("audit Person type"),
            row.get::<String>(3).expect("audit payload type"),
        ));
    }
    let mut rows = transaction
        .query(
            "SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions WHERE origin_run_id = ? ORDER BY interaction_id",
            [run_id.as_uuid().to_string()],
        )
        .await
        .expect("read exact publication rows");
    let mut interactions = Vec::new();
    while let Some(row) = rows.next().await.expect("read publication row") {
        interactions.push((
            row.get::<String>(0).expect("interaction id type"),
            row.get::<String>(1).expect("interaction Session type"),
            row.get::<String>(2).expect("interaction Person type"),
            row.get::<String>(3).expect("interaction Run type"),
            row.get::<String>(4)
                .expect("interaction requirement digest type"),
            row.get::<String>(5)
                .expect("interaction target digest type"),
            row.get::<String>(6).expect("interaction state type"),
            row.get::<i64>(7).expect("interaction revision type"),
            row.get::<i64>(8).expect("interaction creation type"),
            row.get::<i64>(9).expect("interaction expiry type"),
            row.get::<String>(10).expect("interaction payload type"),
        ));
    }
    let head = load_head_on(&transaction, scope)
        .await
        .expect("read exact Core head")
        .map(|loaded| (loaded.state, loaded.identity_json));
    let mut rows = transaction
        .query(
            "SELECT sequence, message_id, message_json, message_bytes, entry_kind, producer_run_id, contribution_id, producing_task_json, prefix_digest FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? ORDER BY sequence",
            scope.sql(),
        )
        .await
        .expect("read exact Core entries");
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.expect("read Core entry") {
        entries.push((
            row.get::<i64>(0).expect("entry sequence type"),
            row.get::<String>(1).expect("entry message id type"),
            row.get::<String>(2).expect("entry message type"),
            row.get::<i64>(3).expect("entry message bytes type"),
            row.get::<String>(4).expect("entry kind type"),
            row.get::<Option<String>>(5)
                .expect("entry producer Run type"),
            row.get::<Option<String>>(6)
                .expect("entry contribution type"),
            row.get::<Option<String>>(7).expect("entry Task type"),
            row.get::<String>(8).expect("entry prefix digest type"),
        ));
    }
    let mut rows = transaction
        .query(
            "SELECT message_id, sequence, receipt_json FROM agent_conversation_core_v3_input_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? ORDER BY sequence",
            scope.sql(),
        )
        .await
        .expect("read exact Core input receipts");
    let mut input_receipts = Vec::new();
    while let Some(row) = rows.next().await.expect("read Core input receipt") {
        input_receipts.push((
            row.get::<String>(0).expect("input receipt message id type"),
            row.get::<i64>(1).expect("input receipt sequence type"),
            row.get::<String>(2).expect("input receipt payload type"),
        ));
    }
    let open_receipt = open_receipt_on(&transaction, scenario.person_id, run_id)
        .await
        .expect("read exact open receipt");
    let active_recorder = active_recorder_on(&transaction, scope)
        .await
        .expect("read exact active recorder");
    let close_receipt = close_receipt_on(&transaction, scenario.person_id, run_id)
        .await
        .expect("read exact close receipt");
    let retirement_receipt = retirement_receipt_on(&transaction, scenario.person_id, run_id)
        .await
        .expect("read exact retirement receipt");
    transaction
        .commit()
        .await
        .expect("finish exact settlement snapshot");
    SettlementCustodySnapshot {
        run,
        journal,
        session,
        input_binding,
        coverage,
        terminal_receipt,
        resume_rows,
        review_audits,
        interactions,
        head,
        entries,
        input_receipts,
        open_receipt,
        active_recorder,
        close_receipt,
        retirement_receipt,
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ComposedAdmissionSnapshot {
    run: Option<RunRecord>,
    session: (i64, String),
    table_counts: Vec<(&'static str, i64)>,
    binding: Option<OwnerInputBinding>,
    mapping_by_run: Option<OwnerTranscriptInputMapping>,
    mapping_by_reference: Option<OwnerTranscriptInputMapping>,
    input_receipt: Option<(AdmissionResult, ConversationMessage)>,
    input_entry: Option<TranscriptEntry>,
    head: Option<(ConversationHead, String)>,
    open_receipt: Option<RecorderOpenReceipt>,
    active_recorder: Option<RecorderFence>,
}

async fn composed_admission_snapshot(
    scenario: &Scenario,
    run_id: RunId,
    input: TranscriptReference,
) -> ComposedAdmissionSnapshot {
    let run = scenario
        .vault()
        .conversation_run(run_id)
        .await
        .expect("read owner Run snapshot");
    let mut connection = scenario.vault().connection().expect("connect for snapshot");
    let session = session_storage_snapshot(&connection, scenario.session_id).await;
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
        .await
        .expect("start composed evidence snapshot");
    let scope = Scope::from_identity(
        scenario.person_id,
        &scenario.identity,
        scenario.conversation_id,
        scenario.branch_id,
    );
    let binding = owner_input_binding_on(&transaction, scenario.person_id, run_id)
        .await
        .expect("read owner input binding");
    let mapping_by_run =
        owner_transcript_input_for_run_on(&transaction, scenario.person_id, run_id)
            .await
            .expect("read Run-to-input mapping");
    let mapping_by_reference =
        owner_transcript_input_for_reference_on(&transaction, scenario.person_id, input)
            .await
            .expect("read transcript-to-owner mapping");
    let input_receipt = input_receipt_on(&transaction, scope, input.message_id)
        .await
        .expect("read Core input receipt");
    let input_entry = entry_on(&transaction, scope, input.sequence)
        .await
        .expect("read Core input entry");
    let head = load_head_on(&transaction, scope)
        .await
        .expect("read Core head")
        .map(|loaded| (loaded.state, loaded.identity_json));
    let open_receipt = open_receipt_on(&transaction, scenario.person_id, run_id)
        .await
        .expect("read immutable open receipt");
    let active_recorder = active_recorder_on(&transaction, scope)
        .await
        .expect("read active recorder");
    let mut table_counts = Vec::new();
    for table in [
        "agent_conversation_runs",
        "agent_sessions",
        "agent_conversation_core_v3_heads",
        "agent_conversation_core_v3_entries",
        "agent_conversation_core_v3_input_receipts",
        "agent_conversation_core_v3_owner_bindings",
        "agent_conversation_core_v3_open_receipts",
        "agent_conversation_core_v3_active_recorders",
        "agent_conversation_owner_transcript_inputs_v1",
        "agent_conversation_owner_transcript_run_inputs_v1",
    ] {
        table_counts.push((table, table_count_in_transaction(&transaction, table).await));
    }
    transaction
        .commit()
        .await
        .expect("finish composed evidence snapshot");
    ComposedAdmissionSnapshot {
        run,
        session,
        table_counts,
        binding,
        mapping_by_run,
        mapping_by_reference,
        input_receipt,
        input_entry,
        head,
        open_receipt,
        active_recorder,
    }
}

async fn retry_composed_admission_after_busy(
    vault: &EncryptedAgentVault<TestKeys>,
    intent: CoreComposedOwnerIntent,
    core_request: MessageAdmissionRequest,
    mut result: Result<CoreComposedRunAdmission, ConversationStoreFailure>,
) -> Result<CoreComposedRunAdmission, ConversationStoreFailure> {
    for _ in 0..8 {
        if result != Err(ConversationStoreFailure::Busy) {
            return result;
        }
        tokio::task::yield_now().await;
        result = compose_owner_core_run(vault, intent.clone(), core_request.clone()).await;
    }
    assert_ne!(
        result,
        Err(ConversationStoreFailure::Busy),
        "transient Busy must be retried, not counted as a successful admission rejection"
    );
    result
}

fn output_artifact() -> floe_agent_contract::Artifact {
    floe_agent_contract::Artifact {
        artifact_id: Uuid::new_v4(),
        name: "answer.txt".into(),
        parts: vec![floe_agent_contract::ArtifactPart::Text {
            text: "attached answer".into(),
        }],
        coverage: DependencyCoverage::Independent,
    }
}

async fn append_valid_answer_batch(
    scenario: &Scenario,
    run: &RunRecord,
    text: &str,
    artifacts: Vec<floe_agent_contract::Artifact>,
) -> u64 {
    use floe_agent_contract::{
        BatchCursor, JournalEvent, ModelBindingDigest, ModelBudgetProfile, ModelCapabilities,
        ModelSelectionCommitment, ModelStep, ModelUsage, PreparedModelPlan, ProcessingBoundary,
        ValidatedModelBatch,
    };

    let attempt_id = Uuid::new_v4();
    let projection_ref = floe_agent_contract::ProjectionRef::new();
    let batch_id = Uuid::new_v4();
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: None,
                reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                    tokens: 128,
                    cost_micros: 128,
                },
                projection_ref,
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: scenario.person_id.to_string(),
                    device_id: run.device_id.clone(),
                    purpose: "everyday_assistance".into(),
                    consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([101; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([102; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ValidatedBatch {
                batch: ValidatedModelBatch {
                    execution_id: run.run_id.as_uuid(),
                    attempt_id,
                    projection_ref,
                    batch_id,
                    steps: vec![ModelStep::Answer {
                        text: text.to_owned(),
                        artifacts,
                    }],
                    catalog_revision: run.expert_environment.revision,
                    tool_revisions: vec![],
                    agent_revisions: vec![],
                    projection_coverage: DependencyCoverage::Independent,
                    delegation_context: None,
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await;
    scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read prepared owner Run")
        .expect("prepared owner Run remains present")
        .journal_revision
}

fn run_output_request(
    recorder: RecorderFence,
    text: &str,
    artifacts: Vec<floe_agent_contract::Artifact>,
) -> ConversationRunOutputRequest {
    ConversationRunOutputRequest {
        recorder,
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        event: floe_agent_contract::JournalEvent::Output {
            text: text.to_owned(),
            artifacts,
        },
    }
}

async fn compose_run_output(
    vault: &EncryptedAgentVault<TestKeys>,
    request: ConversationRunOutputRequest,
) -> Result<ConversationRunOutputReceipt, ConversationStoreFailure> {
    let mut connection = vault.connection().map_err(start_error)?;
    let (guard, transaction) = vault
        .journal_transaction(&mut connection)
        .await
        .map_err(start_error)?;
    let result = vault
        .record_conversation_run_output_on(&transaction, request)
        .await;
    vault
        .finish_conversation_core_transaction(guard, transaction, result)
        .await
}

fn owner_read_target(scenario: &Scenario) -> ConversationReadTarget {
    ConversationReadTarget {
        identity: scenario.identity.clone(),
        conversation_id: scenario.conversation_id,
        branch_id: scenario.branch_id,
    }
}

async fn owner_read_boundary(scenario: &Scenario) -> TranscriptReadBoundary {
    scenario
        .vault()
        .read_conversation_head(owner_read_target(scenario))
        .await
        .expect("read exact Core head for owner transcript page")
}

async fn record_typed_answer(
    scenario: &Scenario,
    run: &RunRecord,
    input: TranscriptReference,
    text: &str,
) -> TypedConversationRecordingRequest {
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(run, input))
        .await
        .expect("open typed owner recorder");
    let request = typed_assistant_request(open.fence, text);
    compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("record typed owner answer through accepted writer fixture");
    request
}

fn typed_message(entry: &OwnerResolvedTranscriptEntry) -> &floe_conversation::AgentMessage {
    let OwnerTranscriptTypedEvidence::Present { message, .. } = &entry.typed_evidence else {
        panic!("generated entry must have exact typed evidence");
    };
    message
}

#[tokio::test]
async fn run_output_composition_commits_journal_typed_core_and_link_together() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("atomic owner Output")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open actual owner recorder");
    let text = "answer with a sole-journal artifact";
    let artifacts = vec![output_artifact()];
    let before = append_valid_answer_batch(&scenario, &run, text, artifacts.clone()).await;
    let request = run_output_request(open.fence.clone(), text, artifacts.clone());

    let receipt = compose_run_output(scenario.vault(), request.clone())
        .await
        .expect("compose the validated journal Output with typed/Core/link custody");
    assert_eq!(receipt.journal_revision, before + 1);
    assert_eq!(receipt.recording.recorder, open.fence);
    assert_eq!(receipt.recording.contribution_id, request.contribution_id);

    let journal = scenario
        .vault()
        .conversation_journal(run.run_id)
        .await
        .expect("read validated owner journal");
    let last = journal.last().expect("composed Output is journal tail");
    assert_eq!(last.revision, receipt.journal_revision);
    let persisted = serde_json::from_str::<floe_agent_contract::JournalEvent>(&last.payload)
        .expect("decode persisted Output");
    let (
        floe_agent_contract::JournalEvent::Output {
            text: persisted_text,
            artifacts: persisted_artifacts,
        },
        floe_agent_contract::JournalEvent::Output {
            text: requested_text,
            artifacts: requested_artifacts,
        },
    ) = (&persisted, &request.event)
    else {
        panic!("the composed journal tail is the requested Output event");
    };
    assert_eq!(persisted_text, requested_text);
    assert_eq!(
        persisted_artifacts, requested_artifacts,
        "artifacts remain in the sole Run journal"
    );

    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("Output has its immutable owner link");
    assert_eq!(link.first_recording_run_id, run.run_id);
    assert_eq!(link.transcript_entry.message.text, text);
    let boundary = owner_read_boundary(&scenario).await;
    let resolved = scenario
        .vault()
        .read_owner_transcript_entry(
            owner_read_target(&scenario),
            scenario.session_id,
            boundary,
            TranscriptEntryLookup::Reference(receipt.recording.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("resolve exact owner-linked transcript entry");
    assert_eq!(
        typed_message(&resolved),
        &floe_conversation::AgentMessage::Assistant {
            turn_id: run.run_id.as_uuid(),
            text: text.into(),
        },
        "typed Assistant custody stores the derived turn and text"
    );
    assert_eq!(resolved.transcript_entry.message.text, text);
    assert_eq!(
        resolved
            .transcript_entry
            .message
            .evidence
            .as_ref()
            .map(|evidence| evidence.digest()),
        Some(link.typed_reference.digest())
    );
}

#[tokio::test]
async fn run_output_composition_rolls_back_after_journal_typed_core_and_link_writes() {
    for stage in [4, 1, 2, 3] {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario
            .admit_run_and_append_input(&format!("Output rollback stage {stage}"))
            .await;
        let open = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
            .await
            .expect("open actual owner recorder");
        let text = format!("rollback stage {stage}");
        let artifacts = vec![output_artifact()];
        let before = append_valid_answer_batch(&scenario, &run, &text, artifacts.clone()).await;
        let request = run_output_request(open.fence, &text, artifacts);
        scenario
            .vault()
            .conversation_core_typed_write_fault
            .store(stage, Ordering::Release);
        assert_eq!(
            compose_run_output(scenario.vault(), request)
                .await
                .expect_err("fault must abort the full shared transaction"),
            ConversationStoreFailure::NotCommitted,
            "fault stage {stage} reports a confirmed rollback"
        );

        let current = scenario
            .vault()
            .conversation_run(run.run_id)
            .await
            .expect("read Run after fault rollback")
            .expect("owner Run remains present");
        assert_eq!(current.journal_revision, before);
        assert_eq!(
            scenario
                .vault()
                .conversation_journal(run.run_id)
                .await
                .expect("read journal after rollback")
                .iter()
                .filter(|entry| entry.kind == "output")
                .count(),
            0,
            "a failed composition leaves no journal Output"
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after Output rollback");
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            0
        );
        assert_eq!(
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1"
            )
            .await,
            0
        );
        assert!(
            !crate::schema::typed_history_family_present(&connection)
                .await
                .expect("inspect optional typed family after rollback"),
            "typed owner family creation rolls back with every write stage"
        );
    }
}

#[tokio::test]
async fn composed_terminal_settlement_closes_completed_failed_and_cancelled_runs() {
    for terminal_kind in ["completed", "failed", "cancelled"] {
        let mut scenario = Scenario::new().await;
        let (run, _) = scenario
            .admit_run_and_append_input(&format!("composed {terminal_kind} terminal"))
            .await;
        let terminal = match terminal_kind {
            "completed" => scenario.prepare_completed_terminal(&run).await,
            "failed" => RunTerminal::from_failure(AgentFailure::BudgetExceeded),
            _ => RunTerminal::from_failure(AgentFailure::Cancelled),
        };
        let expected_state = terminal.state;
        let result = compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id: run.run_id,
                expected_aggregate_revision: run.aggregate_revision,
                terminal,
            },
        )
        .await
        .expect("owner terminal and Core close commit atomically");
        let CoreComposedSettlementResult::Settled { record, close } = result else {
            panic!("a terminal outcome must close its recorder");
        };
        assert_eq!(record.state, expected_state);
        assert_eq!(close.fence.run_id, run.run_id);
        assert_eq!(
            run_settlement_counts(&scenario, run.run_id).await,
            (1, 0, 0, 0, 1, 0, 1)
        );
    }
}

#[tokio::test]
async fn composed_cancellation_commits_owner_outcome_while_unresolved_model_keeps_custody() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("cancel with unresolved model intent")
        .await;
    let unresolved_attempt = append_unresolved_model_attempt(&scenario, &run).await;
    let request = CoreComposedSettlementRequest::Finish {
        run_id: run.run_id,
        expected_aggregate_revision: run.aggregate_revision,
        terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
    };

    let result = compose_owner_core_settlement(scenario.vault(), request.clone())
        .await
        .expect("Host cancellation commits while uncertain model evidence retains Core custody");
    let CoreComposedSettlementResult::CustodyPending { record } = result else {
        panic!("terminal owner uncertainty must retain recorder custody");
    };
    assert_eq!(record.state, RunState::Cancelled);
    assert!(record.pending_terminal.is_none());
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 1, 0, 1),
        "owner terminal evidence commits without closing or releasing the recorder"
    );

    let journal = scenario
        .vault()
        .conversation_journal(run.run_id)
        .await
        .expect("read unresolved model evidence");
    let events = journal
        .iter()
        .map(|entry| serde_json::from_str::<floe_agent_contract::JournalEvent>(&entry.payload))
        .collect::<Result<Vec<_>, _>>()
        .expect("decode unresolved model journal");
    assert!(events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::ModelIntent { attempt_id, .. }
            if *attempt_id == unresolved_attempt
    )));
    assert!(!events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::ModelResult { attempt_id, .. }
            if *attempt_id == unresolved_attempt
    )));

    let before_replay = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert_eq!(
        compose_owner_core_settlement(scenario.vault(), request)
            .await
            .expect("exact cancellation replay returns retained custody"),
        CoreComposedSettlementResult::CustodyPending { record },
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_replay,
        "exact owner settlement replay adds no rows or recorder transition"
    );
}

#[tokio::test]
async fn blocked_composed_settlement_replays_exact_publication_after_ack_loss() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("atomic blocked settlement")
        .await;
    let commit = blocked_run_commit(&scenario, &run).await;
    let replay = commit.clone();
    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Blocked(commit),
        )
        .await
        .expect_err("commit acknowledgement loss is uncertain"),
        ConversationStoreFailure::OutcomeUnknown
    );
    scenario.reopen().await;
    scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance mutable executor fence after committed settlement");
    let result = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Blocked(replay.clone()),
    )
    .await
    .expect("exact blocked request replays after reopen and fence movement");
    let CoreComposedSettlementResult::Settled { record, close } = result else {
        panic!("blocked terminal must be settled");
    };
    assert_eq!(record.state, RunState::Blocked);
    assert_eq!(close.fence.run_id, run.run_id);
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (1, 0, 1, 1, 1, 0, 1)
    );

    let mut changed_fence = replay;
    changed_fence.expected_session_revision += 1;
    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Blocked(changed_fence),
        )
        .await
        .is_err()
    );
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (1, 0, 1, 1, 1, 0, 1)
    );
}

#[tokio::test]
async fn pending_delegated_terminal_retains_recorder_custody_across_reopen() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("pending delegated terminal")
        .await;
    append_unresolved_delegation(&scenario, &run).await;
    let request = CoreComposedSettlementRequest::Finish {
        run_id: run.run_id,
        expected_aggregate_revision: run.aggregate_revision,
        terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
    };
    let result = compose_owner_core_settlement(scenario.vault(), request.clone())
        .await
        .expect("unresolved delegation defers terminal and commits pending state");
    let CoreComposedSettlementResult::OwnerOutcomeDeferred { record } = result else {
        panic!("pending delegation must not be reported as closed");
    };
    assert_eq!(record.state, RunState::Working);
    assert_eq!(
        record
            .pending_terminal
            .as_ref()
            .map(|pending| pending.failure),
        Some(AgentFailure::Cancelled)
    );
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 0, 0, 1)
    );

    scenario.reopen().await;
    let activation = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance mutable executor generation before pending replay");
    assert!(activation.executor_generation > run.executor_generation);
    let before_replay = settlement_custody_snapshot(&scenario, run.run_id).await;
    let result = compose_owner_core_settlement(scenario.vault(), request)
        .await
        .expect("exact deferred request replays against its retained active fence");
    assert!(matches!(
        result,
        CoreComposedSettlementResult::OwnerOutcomeDeferred { .. }
    ));
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_replay,
        "pending replay is observational after generation movement"
    );
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 0, 0, 1)
    );
}

#[tokio::test]
async fn actual_task_result_finishes_pending_current_run_and_closes_core_atomically() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("recover actual Task result")
        .await;
    let task_result = actual_terminal_task_for_recovery(&scenario, &run).await;
    assert_eq!(
        task_result.snapshot.state,
        floe_agent_contract::TaskState::Interrupted
    );
    let before_pending = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read Run after actual Task admission")
        .expect("Run remains present");
    let pending = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: run.run_id,
            expected_aggregate_revision: before_pending.aggregate_revision,
            terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
        },
    )
    .await
    .expect("unresolved Task defers the requested host cancellation");
    let CoreComposedSettlementResult::OwnerOutcomeDeferred {
        record: pending_record,
    } = pending
    else {
        panic!("unresolved actual Task keeps the Run pending");
    };
    assert_eq!(pending_record.state, RunState::Working);
    assert_eq!(
        pending_record
            .pending_terminal
            .as_ref()
            .map(|value| value.failure),
        Some(AgentFailure::Cancelled),
        "Task interruption does not replace the Host Run cancellation"
    );
    let open = scenario
        .last_core_recorder
        .as_ref()
        .expect("composed Run has its original recorder")
        .clone();
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 0, 0, 1),
        "pending Task evidence leaves the exact recorder active"
    );

    let result = scenario
        .vault()
        .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
        .await
        .expect("actual Task receipt, owner cancellation and Core close share one commit");
    let CoreComposedRecoveryResult::Closed { record, close } = result else {
        panic!("the final Task result closes current-generation custody");
    };
    assert_eq!(record.state, RunState::Cancelled);
    assert!(record.pending_terminal.is_none());
    assert_eq!(close.fence, open.fence);
    assert_eq!(
        close.settled_prefix,
        open.fence.input.sequence + 1,
        "recovery settles the input and exact authenticated Task contribution"
    );
    assert_eq!(close.owner_aggregate_revision, record.aggregate_revision);
    let task = scenario
        .vault()
        .task(task_result.task_id)
        .await
        .expect("read actual Task owner record")
        .expect("Task owner record remains durable");
    assert_eq!(
        task.snapshot.state,
        floe_agent_contract::TaskState::Interrupted
    );
    assert_eq!(
        scenario
            .vault()
            .accounted_conversation_receipt(run.run_id)
            .await
            .expect("read owner accounting from the committed transaction")
            .expect("Run receipt remains available")
            .task_refs
            .len(),
        1,
        "Task evidence is accounted once"
    );
    let journal = scenario
        .vault()
        .conversation_journal(run.run_id)
        .await
        .expect("read Run journal after custody close");
    let events = journal
        .iter()
        .map(|entry| serde_json::from_str::<floe_agent_contract::JournalEvent>(&entry.payload))
        .collect::<Result<Vec<_>, _>>()
        .expect("decode owner journal evidence");
    assert!(
        events.iter().any(|event| matches!(
            event,
            floe_agent_contract::JournalEvent::ModelResult { accounting, .. }
                if accounting.unknown_tokens && accounting.unknown_cost
        )),
        "uncertain model accounting evidence remains intact"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                floe_agent_contract::JournalEvent::DelegationResult { .. }
            ))
            .count(),
        1,
        "Core recovery does not duplicate the Task result"
    );

    let mut changed_result = task_result.clone();
    changed_result.snapshot.coverage = DependencyCoverage::Independent;
    let before_bad_replay = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, changed_result)
            .await
            .is_err(),
        "a changed Task result cannot replace the immutable owner result"
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_bad_replay,
        "changed-result rejection is read-only"
    );

    scenario.reopen().await;
    let later = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance generation after current custody already closed")
        .executor_generation;
    assert!(later > open.fence.executor_generation);
    assert_eq!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result)
            .await
            .expect("exact close replay precedes later executor-generation checks"),
        CoreComposedRecoveryResult::Closed { record, close }
    );
}

#[tokio::test]
async fn first_late_task_result_inserts_with_unresolved_model_and_keeps_current_custody() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("Task result with separate uncertain model attempt")
        .await;
    let task_generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate real Task owner for the nested-accounting fixture")
        .executor_generation;
    let (_, task_receipts) = scenario
        .terminal_task_receipts_with_manager_history(&run, task_generation, 2, 1, true, true)
        .await;
    let unresolved_attempt = match &task_receipts[0].execution {
        floe_agent_contract::TaskExecutionEvidence::Admitted(execution) => {
            execution
                .accounting
                .unresolved_attempts
                .first()
                .expect("prior Interrupted Task retains unresolved model accounting")
                .attempt_id
        }
        floe_agent_contract::TaskExecutionEvidence::Unadmitted => {
            panic!("prior Task has an authenticated execution receipt")
        }
    };
    let prior_task_id = task_receipts[0].task_id;
    let task_result = task_receipts
        .into_iter()
        .nth(1)
        .expect("the second actual Task result remains absent from the parent journal");
    let open = scenario
        .last_core_recorder
        .as_ref()
        .expect("composed Run has its original recorder")
        .clone();
    let current = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read Run before cancellation")
        .expect("Run remains present");
    let settled = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: run.run_id,
            expected_aggregate_revision: current.aggregate_revision,
            terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
        },
    )
    .await
    .expect("Host cancellation commits while the model attempt retains custody");
    let CoreComposedSettlementResult::OwnerOutcomeDeferred {
        record: owner_record,
    } = settled
    else {
        panic!("late Task result must keep cancellation pending before recovery");
    };
    assert_eq!(owner_record.state, RunState::Working);
    assert!(owner_record.pending_terminal.is_some());

    let result = scenario
        .vault()
        .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
        .await
        .expect("exact actual Task evidence replay preserves model uncertainty and Core custody");
    let CoreComposedRecoveryResult::CustodyPending { record } = result else {
        panic!("an unresolved model attempt keeps current-generation custody active");
    };
    assert_eq!(record.state, RunState::Cancelled);
    assert!(record.pending_terminal.is_none());
    let after_task_result = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert_eq!(after_task_result.active_recorder, Some(open.fence.clone()));
    assert!(after_task_result.close_receipt.is_none());
    assert!(after_task_result.retirement_receipt.is_none());
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 1, 0, 1),
        "Task evidence remains committed without releasing the original recorder"
    );
    let task = scenario
        .vault()
        .task(task_result.task_id)
        .await
        .expect("read actual Task owner record")
        .expect("terminal Task remains durable");
    assert_eq!(
        task.snapshot.state,
        floe_agent_contract::TaskState::Interrupted
    );
    let prior_task = scenario
        .vault()
        .task(prior_task_id)
        .await
        .expect("read prior Task with separate model uncertainty")
        .expect("prior Task receipt remains durable");
    let prior_task_receipt = prior_task
        .receipt
        .as_ref()
        .expect("prior Task has an authenticated terminal receipt");
    assert!(
        prior_task_receipt
            .accounting
            .unresolved_attempts
            .iter()
            .any(|attempt| attempt.attempt_id == unresolved_attempt)
    );
    assert_eq!(
        scenario
            .vault()
            .accounted_conversation_receipt(run.run_id)
            .await
            .expect("read authenticated Task accounting")
            .expect("Host Run receipt remains available")
            .task_refs
            .len(),
        2,
        "the actual terminal Task pair is accounted once each"
    );
    let journal = scenario
        .vault()
        .conversation_journal(run.run_id)
        .await
        .expect("read owner journal after Task result");
    let events = journal
        .iter()
        .map(|entry| serde_json::from_str::<floe_agent_contract::JournalEvent>(&entry.payload))
        .collect::<Result<Vec<_>, _>>()
        .expect("decode actual owner journal");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                floe_agent_contract::JournalEvent::DelegationResult { receipt }
                    if receipt.task_id == task_result.task_id
            ))
            .count(),
        1,
        "the first-inserted late Task result has exactly one journal entry"
    );
    let task_connection = scenario
        .vault()
        .connection()
        .expect("connect to read prior Task journal evidence");
    let mut task_rows = task_connection
        .query(
            "SELECT payload FROM agent_task_journal WHERE task_id = ? ORDER BY revision",
            [prior_task_id.as_uuid().to_string()],
        )
        .await
        .expect("read prior Task journal");
    let mut prior_task_events = Vec::new();
    while let Some(row) = task_rows.next().await.expect("read prior Task event") {
        let payload = row.get::<String>(0).expect("Task event payload");
        prior_task_events.push(
            serde_json::from_str::<floe_agent_contract::JournalEvent>(&payload)
                .expect("decode prior Task event"),
        );
    }
    assert!(prior_task_events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::ModelIntent { attempt_id, .. }
            if *attempt_id == unresolved_attempt
    )));
    assert!(!prior_task_events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::ModelResult { attempt_id, .. }
            if *attempt_id == unresolved_attempt
    )));
    drop(task_rows);
    drop(task_connection);

    let history_before_replay = scenario
        .vault()
        .read_manager_session_history_page(
            scenario.session_id,
            None,
            None,
            16,
            floe_conversation::MAX_SESSION_BYTES,
        )
        .await
        .expect("read first-inserted normalized Manager Task contribution");
    let recorded_late_task = history_before_replay
        .messages
        .iter()
        .filter(|item| {
            matches!(
                &item.message,
                floe_conversation::AgentMessage::Delegation { task, .. }
                    if task.task_id == task_result.task_id
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(recorded_late_task.len(), 1);
    assert_eq!(
        recorded_late_task[0].alias_id,
        task_result.task_id.as_uuid(),
        "the public stable cursor aliases the exact Task contribution"
    );

    scenario.reopen().await;
    let later = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance generation while the uncertain attempt retains custody")
        .executor_generation;
    assert!(later > open.fence.executor_generation);
    let before_replay = settlement_custody_snapshot(&scenario, run.run_id).await;
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before exact result replay");
    let task_rows = table_count(&connection, "agent_tasks").await;
    let task_journal_rows = table_count(&connection, "agent_task_journal").await;
    drop(connection);
    assert_eq!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
            .await
            .expect("exact replay returns pending custody before generation-sensitive transitions"),
        CoreComposedRecoveryResult::CustodyPending { record }
    );
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_history_page(
                scenario.session_id,
                None,
                None,
                16,
                floe_conversation::MAX_SESSION_BYTES,
            )
            .await
            .expect("read normalized history after exact replay"),
        history_before_replay,
        "exact result replay creates no duplicate typed or Core contribution"
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_replay,
        "exact replay adds no owner, accounting, or Core rows"
    );
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after exact result replay");
    assert_eq!(table_count(&connection, "agent_tasks").await, task_rows);
    assert_eq!(
        table_count(&connection, "agent_task_journal").await,
        task_journal_rows,
        "replay does not redispatch or create another Task execution"
    );
    drop(connection);
}

#[tokio::test]
async fn manager_compaction_archives_exact_normalized_prefix_without_pruning() {
    use floe_agent_contract::{
        ArchivePointer, ArchiveReadRequest, BatchCursor, EngineStep, JournalEvent,
        ModelBindingDigest, ModelBudgetProfile, ModelCapabilities, ModelSelectionCommitment,
        ModelStep, ModelUsage, PreparedModelPlan, ProcessingBoundary, ProjectionRef,
        ValidatedModelBatch,
    };

    let mut scenario = Scenario::new().await;
    scenario.identity = floe_conversation::prompts::default_manager_identity(scenario.person_id)
        .expect("derive the stable default Manager identity");
    let text = "Archive this Manager turn";
    let request = scenario.owner_request(text);
    let user_message_id = match &request.input {
        TurnInput::NewMessage(message) => message.message_id,
        TurnInput::ExistingMessage { message_id } => *message_id,
    };
    let run = match scenario
        .vault()
        .admit_manager_conversation_turn(request, scenario.identity.clone())
        .await
        .expect("admit Manager Run with normalized input custody")
    {
        crate::vault::VaultManagerConversationAdmission::Created(run) => run,
        other => panic!("unexpected Manager admission: {other:?}"),
    };

    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch_id = Uuid::new_v4();
    let answer = "A stable archived answer.".to_owned();
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: None,
                reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                    tokens: 128,
                    cost_micros: 128,
                },
                projection_ref,
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: scenario.person_id.to_string(),
                    device_id: run.device_id.clone(),
                    purpose: "everyday_assistance".into(),
                    consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([101; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([102; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ValidatedBatch {
                batch: ValidatedModelBatch {
                    execution_id: run.run_id.as_uuid(),
                    attempt_id,
                    projection_ref,
                    batch_id,
                    steps: vec![ModelStep::Answer {
                        text: answer.clone(),
                        artifacts: Vec::new(),
                    }],
                    catalog_revision: run.expert_environment.revision,
                    tool_revisions: vec![],
                    agent_revisions: vec![],
                    projection_coverage: DependencyCoverage::Independent,
                    delegation_context: None,
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await;
    scenario
        .vault()
        .record_manager_conversation_journal(
            run.run_id,
            "output",
            JournalEvent::Output {
                text: answer.clone(),
                artifacts: Vec::new(),
            },
        )
        .await
        .expect("atomically record owner output and typed Core contribution");
    let terminal = RunTerminal {
        state: RunState::Completed,
        output: Some(answer.clone()),
        steps: vec![EngineStep::Answer {
            text: answer,
            artifacts: Vec::new(),
        }],
        coverage: DependencyCoverage::Independent,
        issue: None,
        blocked: None,
        interactions: vec![],
    };
    let settled = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: run.run_id,
            expected_aggregate_revision: run.aggregate_revision,
            terminal,
        },
    )
    .await
    .expect("settle Manager owner and recorder together");
    let CoreComposedSettlementResult::Settled { record, .. } = settled else {
        panic!("completed Manager Run did not settle its recorder")
    };

    let full_history = scenario
        .vault()
        .read_manager_session_history_page(
            scenario.session_id,
            None,
            None,
            8,
            floe_conversation::MAX_SESSION_BYTES,
        )
        .await
        .expect("preflight the complete exact Manager history");
    let user_item_bytes = full_history
        .messages
        .iter()
        .find(|message| message.alias_id == user_message_id)
        .expect("New Manager history has its exact User alias")
        .encoded_bytes;
    assert_eq!(full_history.messages.len(), 2);

    scenario
        .vault()
        .owner_transcript_entry_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_history_page(scenario.session_id, None, None, 8, 1,)
            .await,
        Err(AgentFailure::BudgetExceeded),
        "an oversized first live item fails from its preflight"
    );
    assert_eq!(
        scenario
            .vault()
            .owner_transcript_entry_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_history_page(
                scenario.session_id,
                None,
                Some(user_message_id),
                8,
                1,
            )
            .await,
        Err(AgentFailure::BudgetExceeded),
        "an oversized required User is rejected before owner entry hydration"
    );
    assert_eq!(
        scenario
            .vault()
            .owner_transcript_entry_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    let compacted = scenario
        .vault()
        .compact_manager_session(
            scenario.session_id,
            record.session_revision,
            run.run_id.as_uuid(),
            "The Manager answered the user.".into(),
        )
        .await
        .expect("commit archive manifest, exact Core checkpoint, and Session CAS");
    let pointer = ArchivePointer {
        archive_id: compacted.recovery.archive_id,
        source_revision: compacted.recovery.source_revision,
        through_turn_id: compacted.recovery.through_turn_id,
        archived_message_count: compacted.recovery.archived_message_count,
    };
    let request = ArchiveReadRequest {
        person_id: scenario.person_id,
        session_id: scenario.session_id,
        pointer: pointer.clone(),
        max_messages: 8,
        max_bytes: 32 * 1024,
    };
    scenario
        .vault()
        .owner_transcript_entry_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_history_page(scenario.session_id, None, None, 8, 1)
            .await,
        Err(AgentFailure::BudgetExceeded),
        "oversized compaction summary coverage fails before hydration"
    );
    assert_eq!(
        scenario
            .vault()
            .owner_transcript_entry_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    let mut oversized_first = request.clone();
    oversized_first.max_bytes = 2;
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_archive(&oversized_first)
            .await,
        Err(AgentFailure::BudgetExceeded),
        "oversized first archived item is rejected before hydration"
    );
    assert_eq!(
        scenario
            .vault()
            .owner_transcript_entry_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    let mut cumulative_overflow = request.clone();
    cumulative_overflow.max_bytes = 2 + user_item_bytes;
    assert_eq!(
        scenario
            .vault()
            .read_manager_session_archive(&cumulative_overflow)
            .await,
        Err(AgentFailure::BudgetExceeded),
        "the next archive item is preflighted against remaining aggregate bytes"
    );
    assert_eq!(
        scenario
            .vault()
            .owner_transcript_entry_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "only the first inbound archived entry was hydrated"
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "the typed output body did not hydrate after cumulative overflow"
    );
    scenario
        .vault()
        .owner_transcript_entry_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    let archive = scenario
        .vault()
        .read_manager_session_archive(&request)
        .await
        .expect("resolve the archive from normalized Core entries");
    archive
        .validate(&request)
        .expect("archive preserves exact turn and message count");
    assert_eq!(archive.messages.len(), 2);
    assert_eq!(
        archive.messages[0].message.text,
        "Archive this Manager turn"
    );
    assert_eq!(
        archive.messages[1].message.text,
        "A stable archived answer."
    );
    let connection = scenario
        .vault()
        .connection()
        .expect("open Vault for payload count");
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        2
    );
}

#[tokio::test]
async fn manager_compaction_preflights_the_aggregate_archive_budget() {
    let mut scenario = Scenario::new().await;
    let answer = "x".repeat(63_000);
    complete_manager_turn_with_output(&mut scenario, "first large answer", answer.clone()).await;
    let last =
        complete_manager_turn_with_output(&mut scenario, "second large answer", answer).await;

    let history = scenario
        .vault()
        .read_manager_session_history_page(
            scenario.session_id,
            None,
            None,
            8,
            floe_agent_contract::MAX_ARCHIVE_PROJECTION_BYTES * 4,
        )
        .await
        .expect("resolve each exact Manager archive contribution size");
    assert_eq!(history.messages.len(), 4);
    let item_bytes = history
        .messages
        .iter()
        .map(|message| message.encoded_bytes)
        .collect::<Vec<_>>();
    assert!(
        item_bytes
            .iter()
            .all(|bytes| *bytes < floe_agent_contract::MAX_ARCHIVE_PROJECTION_BYTES)
    );
    assert!(
        item_bytes.iter().sum::<usize>() + 2 > floe_agent_contract::MAX_ARCHIVE_PROJECTION_BYTES
    );

    let page_budget = item_bytes
        .iter()
        .copied()
        .max()
        .expect("history has entries");
    let expected_aliases = history
        .messages
        .iter()
        .map(|message| message.alias_id)
        .collect::<Vec<_>>();
    let mut before = None;
    let mut byte_limited_pages = Vec::new();
    loop {
        let page = scenario
            .vault()
            .read_manager_session_history_page(scenario.session_id, before, None, 8, page_budget)
            .await
            .expect("cursor page fits the byte limit");
        assert!(!page.messages.is_empty());
        assert!(page.encoded_bytes <= page_budget);
        let aliases = page
            .messages
            .iter()
            .map(|message| message.alias_id)
            .collect::<Vec<_>>();
        let next_before = aliases.first().copied();
        byte_limited_pages.push(aliases);
        if !page.has_earlier_messages {
            break;
        }
        assert_ne!(next_before, before, "byte cursor makes progress");
        before = next_before;
    }
    byte_limited_pages.reverse();
    let flattened_aliases = byte_limited_pages.into_iter().flatten().collect::<Vec<_>>();
    let unique_aliases = flattened_aliases
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(flattened_aliases, expected_aliases);
    assert_eq!(unique_aliases.len(), flattened_aliases.len());

    scenario
        .vault()
        .owner_transcript_entry_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .compact_manager_session(
                scenario.session_id,
                last.session_revision,
                last.run_id.as_uuid(),
                "Both large answers are retained exactly.".into(),
            )
            .await,
        Err(AgentFailure::BudgetExceeded),
        "bounded compaction refuses a cumulative projection over the archive cap"
    );
    assert!(
        scenario
            .vault()
            .owner_transcript_entry_hydrations
            .load(std::sync::atomic::Ordering::Relaxed)
            < 4,
        "the last cumulative-overflow entry is rejected before owner hydration"
    );
    assert!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed)
            < 2,
        "not all typed output bodies hydrate after aggregate overflow"
    );
    assert!(
        scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed)
            < 2,
        "not all output coverage bodies hydrate after aggregate overflow"
    );
}

#[tokio::test]
async fn manager_continue_reuses_the_original_normalized_user_entry() {
    let mut scenario = Scenario::new().await;
    scenario.identity = floe_conversation::prompts::default_manager_identity(scenario.person_id)
        .expect("derive the stable default Manager identity");
    let original_text = "Continue this exact Manager request";
    let first_request = scenario.owner_request(original_text);
    let first = match scenario
        .vault()
        .admit_manager_conversation_turn(first_request, scenario.identity.clone())
        .await
        .expect("admit the Manager New turn through the production composer")
    {
        crate::vault::VaultManagerConversationAdmission::Created(run) => run,
        other => panic!("unexpected Manager admission: {other:?}"),
    };
    let failed = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: first.run_id,
            expected_aggregate_revision: first.aggregate_revision,
            terminal: RunTerminal::from_failure(AgentFailure::BudgetExceeded),
        },
    )
    .await
    .expect("settle a Continue-eligible Manager failure and close its recorder");
    let CoreComposedSettlementResult::Settled { record: failed, .. } = failed else {
        panic!("resolved Manager failure left recorder custody open")
    };
    scenario.session_revision = failed.session_revision;
    let continuation = floe_conversation::project_run_receipt(failed.clone())
        .expect("project exact failed Manager receipt")
        .continuation()
        .expect("BudgetExceeded Manager run admits Continue");
    let mode = TurnMode::Continue(continuation);
    let command_id = CommandId::new();
    let intent = CanonicalTurnIntent {
        session_id: scenario.session_id,
        expected_revision: failed.session_revision,
        text: original_text.to_owned(),
        mode: mode.clone(),
        retry_of: None,
    };
    let principal = scenario.person_id.to_string();
    let continued_request = TurnAdmissionRequest {
        expert_environment: failed.expert_environment,
        run_id: RunId::new(),
        command_id,
        session_id: scenario.session_id,
        expected_session_revision: failed.session_revision,
        principal: principal.clone(),
        device_id: failed.device_id.clone(),
        request_digest: intent
            .digest(&principal)
            .expect("digest exact Continue intent"),
        mode,
        retry_of: None,
        input: TurnInput::ExistingMessage {
            message_id: failed.user_message_id,
        },
    };
    let continued = match scenario
        .vault()
        .admit_manager_conversation_turn(continued_request, scenario.identity.clone())
        .await
        .expect("admit Manager Continue through the production composer")
    {
        crate::vault::VaultManagerConversationAdmission::Created(run) => run,
        other => panic!("unexpected Manager Continue admission: {other:?}"),
    };
    assert_ne!(continued.run_id, failed.run_id);
    assert_eq!(continued.user_message_id, failed.user_message_id);
    let connection = scenario
        .vault()
        .connection()
        .expect("connect for Core input count");
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        1,
        "Manager Continue binds the existing input and appends no User duplicate"
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
        1,
        "the original New mapping remains the only owner input mapping"
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_run_inputs_v1"
        )
        .await,
        2,
        "New and Continue Runs both link to the original input mapping"
    );
}

#[tokio::test]
async fn stale_working_recovery_keeps_custody_until_task_evidence_then_retires_and_replays() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("recover interrupted old Run")
        .await;
    let task_result = actual_terminal_task_for_recovery(&scenario, &run).await;
    let open = scenario
        .last_core_recorder
        .as_ref()
        .expect("composed Run has its original recorder")
        .clone();
    scenario.reopen().await;
    let later = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("durably fence the abandoned Host Run generation")
        .executor_generation;
    assert!(later > run.executor_generation);

    let actor = recovery_actor(&scenario);
    let before_denied_actor = settlement_custody_snapshot(&scenario, run.run_id).await;
    for denied_actor in [
        floe_kernel::OwnerActor {
            person_id: floe_kernel::PersonId::new(),
            device_id: actor.device_id.clone(),
            runtime_epoch: actor.runtime_epoch,
        },
        floe_kernel::OwnerActor {
            person_id: actor.person_id,
            device_id: "another-device".into(),
            runtime_epoch: actor.runtime_epoch,
        },
    ] {
        assert!(
            scenario
                .vault()
                .settle_pending_conversation_terminal_with_core(&denied_actor, run.run_id)
                .await
                .is_err(),
            "Person and device checks remain mandatory"
        );
        assert_eq!(
            settlement_custody_snapshot(&scenario, run.run_id).await,
            before_denied_actor,
            "rejected recovery actor cannot mutate owner or Core state"
        );
    }
    let pending = scenario
        .vault()
        .settle_pending_conversation_terminal_with_core(&actor, run.run_id)
        .await
        .expect("interrupt old Working Run but preserve unresolved Task custody");
    let CoreComposedRecoveryResult::CustodyPending { record } = pending else {
        panic!("generation movement alone cannot retire unresolved work");
    };
    assert_eq!(record.state, RunState::Working);
    assert_eq!(
        record.pending_terminal.as_ref().map(|value| value.failure),
        Some(AgentFailure::Interrupted)
    );
    let before_task_result = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert_eq!(before_task_result.active_recorder, Some(open.fence.clone()));
    assert!(before_task_result.retirement_receipt.is_none());

    scenario.vault().conversation_recovery_fault_stage.store(
        CONVERSATION_RECOVERY_FAULT_AFTER_CORE_CUSTODY,
        Ordering::Release,
    );
    assert_eq!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
            .await,
        Err(ConversationStoreFailure::NotCommitted),
        "rollback after retirement receipt also removes the owner Task result and terminal writes"
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_task_result,
        "retirement rollback preserves Pending, accounting and the original active fence"
    );

    let settled = scenario
        .vault()
        .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
        .await
        .expect(
            "final actual Task result settles the interrupted Host Run and retires old custody",
        );
    let CoreComposedRecoveryResult::Retired { record, retirement } = settled else {
        panic!("an old-generation recorder uses retirement rather than close");
    };
    assert_eq!(record.state, RunState::Interrupted);
    assert!(record.pending_terminal.is_none());
    assert_eq!(retirement.fence, open.fence);
    assert_eq!(
        retirement.generation_fence.old_generation,
        run.executor_generation
    );
    assert_eq!(retirement.generation_fence.current_generation, later);
    assert_eq!(retirement.owner_evidence_digest.len(), 32);
    let task = scenario
        .vault()
        .task(task_result.task_id)
        .await
        .expect("read Task owner result")
        .expect("actual Task persists");
    assert_eq!(
        task.snapshot.state,
        floe_agent_contract::TaskState::Interrupted
    );
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await.1,
        0,
        "retirement releases only the recorder after terminal Task evidence"
    );

    scenario.reopen().await;
    let newest = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance beyond the generation captured by retirement")
        .executor_generation;
    assert!(newest > later);
    assert_eq!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
            .await
            .expect("exact retirement replay validates stored owner and generation evidence"),
        CoreComposedRecoveryResult::Retired {
            record,
            retirement: retirement.clone()
        }
    );

    let mut bad_fence = retirement.clone();
    bad_fence.fence.recorder_epoch = bad_fence.fence.recorder_epoch.saturating_add(1);
    let mut connection = scenario
        .vault()
        .connection()
        .expect("tamper recovery receipt fixture");
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .expect("start contradictory retirement receipt fixture");
    transaction
        .execute(
            "UPDATE agent_conversation_core_v3_retirement_receipts SET receipt_json = ? WHERE person_id = ? AND run_id = ?",
            (
                serde_json::to_string(&bad_fence).expect("encode changed fence fixture"),
                scenario.person_id.to_string(),
                run.run_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("change only the stored retirement fence for rejection test");
    transaction
        .commit()
        .await
        .expect("commit contradictory fixture");
    drop(connection);
    let before_contradiction = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result)
            .await
            .is_err(),
        "a changed stored fence cannot replay the original retirement"
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_contradiction,
        "contradictory retirement evidence fails without owner mutation"
    );
}

#[tokio::test]
async fn stale_working_recovery_keeps_old_custody_for_unresolved_model_after_task_result() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("old Run with separate uncertain model attempt")
        .await;
    let task_result = actual_terminal_task_with_parent_result_for_recovery(&scenario, &run).await;
    scenario
        .append_owner_event(
            run.run_id,
            floe_agent_contract::JournalEvent::Checkpoint { iteration: 1 },
        )
        .await;
    let unresolved_attempt = append_unresolved_model_attempt(&scenario, &run).await;
    let open = scenario
        .last_core_recorder
        .as_ref()
        .expect("composed Run has its original recorder")
        .clone();
    scenario.reopen().await;
    let later = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("durably fence the old Host Run generation")
        .executor_generation;
    assert!(later > run.executor_generation);

    let pending = scenario
        .vault()
        .settle_pending_conversation_terminal_with_core(&recovery_actor(&scenario), run.run_id)
        .await
        .expect("old Working Run becomes interrupted while Task evidence remains pending");
    let CoreComposedRecoveryResult::CustodyPending {
        record: pending_record,
    } = pending
    else {
        panic!("stale Host recovery must retain custody for the unresolved model attempt");
    };
    assert_eq!(pending_record.state, RunState::Interrupted);
    assert!(pending_record.pending_terminal.is_none());
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id)
            .await
            .active_recorder,
        Some(open.fence.clone())
    );

    let settled = scenario
        .vault()
        .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
        .await
        .expect("exact Task result replay leaves old recorder custody for model uncertainty");
    let CoreComposedRecoveryResult::CustodyPending { record } = settled else {
        panic!("unresolved model evidence must prevent old-generation retirement");
    };
    assert_eq!(record.state, RunState::Interrupted);
    assert!(record.pending_terminal.is_none());
    let after_task_result = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert_eq!(after_task_result.active_recorder, Some(open.fence.clone()));
    assert!(after_task_result.close_receipt.is_none());
    assert!(after_task_result.retirement_receipt.is_none());
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 1, 0, 1),
        "old custody remains active until all owner uncertainty has proof"
    );
    assert_eq!(
        scenario
            .vault()
            .task(task_result.task_id)
            .await
            .expect("read terminal Task")
            .expect("Task owner record persists")
            .snapshot
            .state,
        floe_agent_contract::TaskState::Interrupted
    );
    assert_eq!(
        scenario
            .vault()
            .accounted_conversation_receipt(run.run_id)
            .await
            .expect("read committed owner accounting")
            .expect("Host Run remains accounted")
            .task_refs
            .len(),
        2,
        "both authentic terminal Tasks are accounted once"
    );
    let journal = scenario
        .vault()
        .conversation_journal(run.run_id)
        .await
        .expect("read Host Run journal after Task result");
    let events = journal
        .iter()
        .map(|entry| serde_json::from_str::<floe_agent_contract::JournalEvent>(&entry.payload))
        .collect::<Result<Vec<_>, _>>()
        .expect("decode Host Run journal");
    assert!(events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::ModelIntent { attempt_id, .. }
            if *attempt_id == unresolved_attempt
    )));
    assert!(!events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::ModelResult { attempt_id, .. }
            if *attempt_id == unresolved_attempt
    )));
    assert!(
        events.iter().any(|event| matches!(
            event,
            floe_agent_contract::JournalEvent::ModelResult { accounting, .. }
                if accounting.unknown_tokens && accounting.unknown_cost
        )),
        "unknown model accounting evidence remains durable"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                floe_agent_contract::JournalEvent::DelegationResult { receipt }
                    if receipt.task_id == task_result.task_id
            ))
            .count(),
        1,
        "the replayed Task result has exactly one journal entry"
    );

    scenario.reopen().await;
    let newest = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance beyond the previously fenced generation")
        .executor_generation;
    assert!(newest > later);
    let before_replay = settlement_custody_snapshot(&scenario, run.run_id).await;
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before exact old-generation replay");
    let task_rows = table_count(&connection, "agent_tasks").await;
    let task_journal_rows = table_count(&connection, "agent_task_journal").await;
    drop(connection);
    assert_eq!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result)
            .await
            .expect("exact replay preserves retained old custody after newer fencing"),
        CoreComposedRecoveryResult::CustodyPending { record }
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_replay,
        "exact replay does not add owner rows or fabricate retirement evidence"
    );
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after exact old-generation replay");
    assert_eq!(table_count(&connection, "agent_tasks").await, task_rows);
    assert_eq!(
        table_count(&connection, "agent_task_journal").await,
        task_journal_rows,
        "replay cannot redispatch or settle the actual Task a second time"
    );
    drop(connection);
}

#[tokio::test]
async fn recovery_replay_does_not_retrofit_absent_core_custody_receipt() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("owner-only result must not retrofit Core")
        .await;
    let task_result = actual_terminal_task_for_recovery(&scenario, &run).await;
    let current = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read Run before deferred owner terminal")
        .expect("Run remains present");
    compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: run.run_id,
            expected_aggregate_revision: current.aggregate_revision,
            terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
        },
    )
    .await
    .expect("unresolved actual Task stores pending owner outcome");
    scenario
        .vault()
        .reconcile_conversation_delegation(run.run_id, task_result.clone())
        .await
        .expect("existing owner-only recovery path remains available before caller cutover");
    let before_replay = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(before_replay.close_receipt.is_none());
    assert!(before_replay.retirement_receipt.is_none());
    assert!(before_replay.active_recorder.is_some());
    assert!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result)
            .await
            .is_err(),
        "an exact owner-only Task replay cannot manufacture missing Core close evidence"
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before_replay,
        "absent custody evidence remains absent"
    );
}

#[tokio::test]
async fn recovery_missing_active_recorder_rolls_back_task_and_terminal_writes() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("recovery must reject missing active recorder")
        .await;
    let task_result = actual_terminal_task_for_recovery(&scenario, &run).await;
    let current = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read Run before pending settlement")
        .expect("Run remains present");
    let result = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Finish {
            run_id: run.run_id,
            expected_aggregate_revision: current.aggregate_revision,
            terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
        },
    )
    .await
    .expect("pending Task keeps the owner outcome deferred");
    assert!(matches!(
        result,
        CoreComposedSettlementResult::OwnerOutcomeDeferred { .. }
    ));
    let open = scenario
        .last_core_recorder
        .as_ref()
        .expect("Run has its original Core recorder")
        .clone();
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to remove active recorder evidence");
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .expect("start missing active-recorder fixture");
    transaction
        .execute(
            "DELETE FROM agent_conversation_core_v3_active_recorders WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND run_id = ?",
            (
                scenario.person_id.to_string(),
                open.fence.conversation_id.as_uuid().to_string(),
                open.fence.branch_id.as_uuid().to_string(),
                run.run_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("remove only the Run's active recorder row");
    transaction
        .commit()
        .await
        .expect("commit deliberately missing recorder evidence");
    drop(connection);
    let before = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(before.active_recorder.is_none());
    assert!(
        scenario
            .vault()
            .reconcile_conversation_delegation_with_core(run.run_id, task_result)
            .await
            .is_err(),
        "missing active Core custody rejects recovery"
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before,
        "a missing active recorder cannot partially commit Task or Host Run mutations"
    );
}

#[tokio::test]
async fn recovery_rolls_back_journal_terminal_accounting_and_close_at_each_injected_stage() {
    for stage in [
        CONVERSATION_RECOVERY_FAULT_AFTER_RESULT_JOURNAL,
        CONVERSATION_RECOVERY_FAULT_AFTER_OWNER_TERMINAL,
        CONVERSATION_RECOVERY_FAULT_AFTER_CORE_CUSTODY,
    ] {
        let mut scenario = Scenario::new().await;
        let (run, _) = scenario
            .admit_run_and_append_input(&format!("rollback recovery stage {stage}"))
            .await;
        let task_result = actual_terminal_task_for_recovery(&scenario, &run).await;
        let current = scenario
            .vault()
            .conversation_run(run.run_id)
            .await
            .expect("read Run before pending settlement")
            .expect("Run remains present");
        let pending = compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id: run.run_id,
                expected_aggregate_revision: current.aggregate_revision,
                terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
            },
        )
        .await
        .expect("retain the unresolved Task under a pending cancellation");
        assert!(matches!(
            pending,
            CoreComposedSettlementResult::OwnerOutcomeDeferred { .. }
        ));
        let before = settlement_custody_snapshot(&scenario, run.run_id).await;
        scenario
            .vault()
            .conversation_recovery_fault_stage
            .store(stage, Ordering::Release);
        assert_eq!(
            scenario
                .vault()
                .reconcile_conversation_delegation_with_core(run.run_id, task_result.clone())
                .await,
            Err(ConversationStoreFailure::NotCommitted),
            "injected recovery stage {stage} proves the transaction rolls back"
        );
        assert_eq!(
            settlement_custody_snapshot(&scenario, run.run_id).await,
            before,
            "stage {stage} leaves journal, Session accounting, terminal and Core state unchanged"
        );
        assert!(matches!(
            scenario
                .vault()
                .reconcile_conversation_delegation_with_core(run.run_id, task_result)
                .await
                .expect("retry after confirmed rollback commits once"),
            CoreComposedRecoveryResult::Closed { .. }
        ));
    }
}

#[tokio::test]
async fn settlement_rejects_missing_active_recorder_without_mutating_owner_or_core() {
    let mut scenario = Scenario::new().await;
    let (admitted, _) = scenario
        .admit_run_and_append_input("missing active recorder before settlement")
        .await;
    append_unresolved_delegation(&scenario, &admitted).await;
    let run = scenario
        .vault()
        .conversation_run(admitted.run_id)
        .await
        .expect("read Run with unresolved delegated work")
        .expect("Run remains stored");
    assert!(run.pending_terminal.is_none());
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to remove active recorder evidence");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start active recorder corruption fixture");
    let changed = transaction
        .execute(
            "DELETE FROM agent_conversation_core_v3_active_recorders WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (
                scenario.person_id.to_string(),
                scenario.conversation_id.as_uuid().to_string(),
                scenario.branch_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("remove active recorder row");
    assert_eq!(changed, 1);
    transaction
        .commit()
        .await
        .expect("commit missing active recorder fixture");
    let before = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(before.open_receipt.is_some());
    assert!(before.active_recorder.is_none());

    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id: run.run_id,
                expected_aggregate_revision: run.aggregate_revision,
                terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
            },
        )
        .await
        .is_err()
    );
    let after = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(
        after
            .run
            .as_ref()
            .is_some_and(|record| record.pending_terminal.is_none())
    );
    assert_eq!(
        after, before,
        "missing active custody cannot defer the terminal or write Session, coverage, or Core evidence"
    );
}

#[tokio::test]
async fn settlement_rejects_mismatched_active_recorder_without_mutating_owner_or_core() {
    let mut scenario = Scenario::new().await;
    let (admitted, _) = scenario
        .admit_run_and_append_input("mismatched active recorder before settlement")
        .await;
    append_unresolved_delegation(&scenario, &admitted).await;
    let run = scenario
        .vault()
        .conversation_run(admitted.run_id)
        .await
        .expect("read Run with unresolved delegated work")
        .expect("Run remains stored");
    assert!(run.pending_terminal.is_none());
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to alter active recorder evidence");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start mismatched active recorder fixture");
    let scope = Scope::from_identity(
        scenario.person_id,
        &scenario.identity,
        scenario.conversation_id,
        scenario.branch_id,
    );
    let mut fence = active_recorder_on(&transaction, scope)
        .await
        .expect("read active recorder")
        .expect("active recorder exists");
    fence.recorder_epoch += 1;
    let changed = transaction
        .execute(
            "UPDATE agent_conversation_core_v3_active_recorders SET fence_json = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (
                serde_json::to_string(&fence).expect("encode altered active fence"),
                scenario.person_id.to_string(),
                scenario.conversation_id.as_uuid().to_string(),
                scenario.branch_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("alter active recorder fence");
    assert_eq!(changed, 1);
    transaction
        .commit()
        .await
        .expect("commit mismatched active recorder fixture");
    let before = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(before.open_receipt.is_some());
    assert!(before.active_recorder.is_some());
    assert_ne!(
        before.open_receipt.as_ref().map(|receipt| &receipt.fence),
        before.active_recorder.as_ref()
    );

    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id: run.run_id,
                expected_aggregate_revision: run.aggregate_revision,
                terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
            },
        )
        .await
        .is_err()
    );
    let after = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(
        after
            .run
            .as_ref()
            .is_some_and(|record| record.pending_terminal.is_none())
    );
    assert_eq!(
        after, before,
        "mismatched active custody cannot defer the terminal or write Session, coverage, or Core evidence"
    );
}

#[tokio::test]
async fn settlement_rejects_altered_open_receipt_without_mutating_owner_or_core() {
    let mut scenario = Scenario::new().await;
    let (admitted, _) = scenario
        .admit_run_and_append_input("altered open receipt before settlement")
        .await;
    append_unresolved_delegation(&scenario, &admitted).await;
    let run = scenario
        .vault()
        .conversation_run(admitted.run_id)
        .await
        .expect("read Run with unresolved delegated work")
        .expect("Run remains stored");
    assert!(run.pending_terminal.is_none());
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to alter immutable open receipt");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start altered open receipt fixture");
    let mut open = open_receipt_on(&transaction, scenario.person_id, run.run_id)
        .await
        .expect("read open receipt")
        .expect("open receipt exists");
    open.opened_head_revision += 1;
    let changed = transaction
        .execute(
            "UPDATE agent_conversation_core_v3_open_receipts SET receipt_json = ? WHERE person_id = ? AND run_id = ?",
            (
                serde_json::to_string(&open).expect("encode altered open receipt"),
                scenario.person_id.to_string(),
                run.run_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("alter immutable open receipt payload");
    assert_eq!(changed, 1);
    transaction
        .commit()
        .await
        .expect("commit altered open receipt fixture");
    let before = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(before.open_receipt.as_ref().is_some_and(|receipt| {
        receipt.opened_head_revision
            > before
                .head
                .as_ref()
                .expect("Core head exists")
                .0
                .head_revision
    }));
    assert_eq!(
        before.active_recorder.as_ref(),
        before.open_receipt.as_ref().map(|receipt| &receipt.fence)
    );

    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id: run.run_id,
                expected_aggregate_revision: run.aggregate_revision,
                terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
            },
        )
        .await
        .is_err()
    );
    let after = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(
        after
            .run
            .as_ref()
            .is_some_and(|record| record.pending_terminal.is_none())
    );
    assert_eq!(
        after, before,
        "altered open evidence cannot defer the terminal or write Session, coverage, or Core evidence"
    );
}

#[tokio::test]
async fn normal_settlement_ack_loss_replays_exact_request_before_mutable_fences() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("settlement acknowledgement loss")
        .await;
    let request = CoreComposedSettlementRequest::Finish {
        run_id: run.run_id,
        expected_aggregate_revision: run.aggregate_revision,
        terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
    };
    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_settlement(scenario.vault(), request.clone())
            .await
            .expect_err("committed settlement acknowledgement is lost"),
        ConversationStoreFailure::OutcomeUnknown
    );
    scenario.reopen().await;
    scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance current executor fence before exact replay");
    let result = compose_owner_core_settlement(scenario.vault(), request.clone())
        .await
        .expect("exact terminal request and close receipt replay despite generation movement");
    assert!(matches!(
        result,
        CoreComposedSettlementResult::Settled { .. }
    ));
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (1, 0, 0, 0, 1, 0, 1)
    );

    let CoreComposedSettlementRequest::Finish {
        run_id,
        expected_aggregate_revision,
        terminal,
    } = request
    else {
        unreachable!()
    };
    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id,
                expected_aggregate_revision,
                terminal: RunTerminal::from_failure(AgentFailure::BudgetExceeded),
            },
        )
        .await
        .is_err()
    );
    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id,
                expected_aggregate_revision: expected_aggregate_revision + 1,
                terminal,
            },
        )
        .await
        .is_err()
    );
    assert_eq!(
        run_settlement_counts(&scenario, run_id).await,
        (1, 0, 0, 0, 1, 0, 1)
    );
}

#[tokio::test]
async fn composed_settlement_never_repairs_a_missing_close_receipt() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("missing immutable close evidence")
        .await;
    let terminal = RunTerminal::from_failure(AgentFailure::Cancelled);
    scenario
        .vault()
        .finish_conversation_run(run.run_id, run.aggregate_revision, terminal.clone())
        .await
        .expect("write the separately settled owner receipt fixture");
    assert!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Finish {
                run_id: run.run_id,
                expected_aggregate_revision: run.aggregate_revision,
                terminal,
            },
        )
        .await
        .is_err()
    );
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 1, 0, 1)
    );
}

#[tokio::test]
async fn composed_settlement_rolls_back_owner_terminal_before_core_close() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("rollback between owner and Core settlement")
        .await;
    let before_connection = scenario
        .vault()
        .connection()
        .expect("connect before injected settlement fault");
    let before_session = session_storage_snapshot(&before_connection, scenario.session_id).await;
    drop(before_connection);
    scenario
        .vault()
        .conversation_core_fault_after_owner_settlement
        .store(true, Ordering::Release);
    let request = CoreComposedSettlementRequest::Finish {
        run_id: run.run_id,
        expected_aggregate_revision: run.aggregate_revision,
        terminal: RunTerminal::from_failure(AgentFailure::Cancelled),
    };
    assert_eq!(
        compose_owner_core_settlement(scenario.vault(), request.clone())
            .await
            .expect_err("injected boundary fault aborts the shared transaction"),
        ConversationStoreFailure::NotCommitted
    );
    let after_rollback = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read owner after rollback")
        .expect("Run remains stored");
    assert_eq!(after_rollback.state, RunState::Working);
    assert!(after_rollback.pending_terminal.is_none());
    let after_connection = scenario
        .vault()
        .connection()
        .expect("connect after injected settlement fault");
    assert_eq!(
        session_storage_snapshot(&after_connection, scenario.session_id).await,
        before_session,
        "Session changes roll back with Run, coverage and terminal receipt"
    );
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (0, 1, 0, 0, 0, 0, 1)
    );

    assert!(matches!(
        compose_owner_core_settlement(scenario.vault(), request)
            .await
            .expect("retry commits complete settlement"),
        CoreComposedSettlementResult::Settled { .. }
    ));
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (1, 0, 0, 0, 1, 0, 1)
    );
}

#[tokio::test]
async fn blocked_settlement_rolls_back_publication_and_owner_state_before_core_close() {
    let mut scenario = Scenario::new().await;
    let (run, _) = scenario
        .admit_run_and_append_input("blocked owner transaction rollback")
        .await;
    let commit = blocked_run_commit(&scenario, &run).await;
    let retry = commit.clone();
    let before = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert!(before.review_audits.is_empty());
    assert!(before.interactions.is_empty());
    assert!(before.terminal_receipt.is_none());
    assert!(before.resume_rows.is_empty());

    scenario
        .vault()
        .conversation_core_fault_after_owner_settlement
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_settlement(
            scenario.vault(),
            CoreComposedSettlementRequest::Blocked(commit),
        )
        .await
        .expect_err("fault after blocked owner settlement aborts the shared transaction"),
        ConversationStoreFailure::NotCommitted
    );
    assert_eq!(
        settlement_custody_snapshot(&scenario, run.run_id).await,
        before,
        "publication, audit, Run, Session, coverage, terminal receipt, Resume queue, and Core all roll back"
    );

    let result = compose_owner_core_settlement(
        scenario.vault(),
        CoreComposedSettlementRequest::Blocked(retry),
    )
    .await
    .expect("retry shared blocked owner and Core settlement");
    let CoreComposedSettlementResult::Settled { record, close } = result else {
        panic!("blocked terminal must close after the owner transaction commits");
    };
    assert_eq!(record.state, RunState::Blocked);
    assert_eq!(close.fence.run_id, run.run_id);
    let after_retry = settlement_custody_snapshot(&scenario, run.run_id).await;
    assert_eq!(after_retry.run.as_ref(), Some(&record));
    assert_eq!(after_retry.review_audits.len(), 1);
    assert_eq!(after_retry.interactions.len(), 1);
    assert!(after_retry.terminal_receipt.is_some());
    assert!(after_retry.close_receipt.is_some());
    assert!(after_retry.active_recorder.is_none());
    assert!(after_retry.resume_rows.is_empty());
    assert_eq!(
        run_settlement_counts(&scenario, run.run_id).await,
        (1, 0, 1, 1, 1, 0, 1)
    );
}

#[tokio::test]
async fn run_output_ack_loss_reopens_and_replays_after_session_and_generation_movement() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("Output ACK replay")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open actual owner recorder");
    let text = "persist once across movement";
    let artifacts = vec![output_artifact()];
    let before = append_valid_answer_batch(&scenario, &run, text, artifacts.clone()).await;
    let request = run_output_request(open.fence.clone(), text, artifacts);

    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_run_output(scenario.vault(), request.clone()).await,
        Err(ConversationStoreFailure::OutcomeUnknown),
        "journal, typed payload, Core receipt, and link commit before the lost ACK"
    );

    scenario.reopen().await;
    let mut moved_session = scenario
        .vault()
        .load(scenario.person_id, run.session_id)
        .await
        .expect("load the original owner Session after reopen");
    let previous_session_revision = moved_session.revision;
    moved_session.revision = previous_session_revision + 1;
    scenario
        .vault()
        .compare_and_swap(&moved_session, previous_session_revision)
        .await
        .expect("advance the same owner Session through its CAS API");
    assert!(moved_session.revision > run.session_revision);
    scenario.session_revision = moved_session.revision;
    let next_generation = scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("move the active executor fence after the committed output")
        .executor_generation;
    assert!(next_generation > request.recorder.executor_generation);

    let replay = compose_run_output(scenario.vault(), request.clone())
        .await
        .expect("recover exact original Output after reopen and mutable state movement");
    assert_eq!(replay.journal_revision, before + 1);
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("original immutable link remains available");
    assert_eq!(link.first_recording_run_id, run.run_id);
    assert_eq!(replay.recording.recorder, request.recorder);
    assert_eq!(replay.recording.contribution_id, request.contribution_id);
    assert_eq!(replay.recording.transcript, link.transcript_entry.reference);
    assert_eq!(
        scenario
            .vault()
            .conversation_journal(run.run_id)
            .await
            .expect("read recovered journal")
            .iter()
            .filter(|entry| entry.kind == "output")
            .count(),
        1,
        "exact ACK replay never appends a second Output"
    );
}

#[tokio::test]
async fn run_output_replay_compares_text_artifacts_and_contribution_identity() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("Output conflict replay")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open actual owner recorder");
    let text = "same text, exact artifacts";
    let artifacts = vec![output_artifact()];
    let before = append_valid_answer_batch(&scenario, &run, text, artifacts.clone()).await;
    let request = run_output_request(open.fence, text, artifacts);
    let original = compose_run_output(scenario.vault(), request.clone())
        .await
        .expect("compose original Output");

    let mut changed_text = request.clone();
    let floe_agent_contract::JournalEvent::Output { text, .. } = &mut changed_text.event else {
        panic!("request carries Output");
    };
    text.push_str(" changed");
    assert_eq!(
        compose_run_output(scenario.vault(), changed_text).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::MessageIdConflict
        )),
        "same contribution with different text conflicts"
    );

    let mut changed_artifacts = request.clone();
    let floe_agent_contract::JournalEvent::Output { artifacts, .. } = &mut changed_artifacts.event
    else {
        panic!("request carries Output");
    };
    artifacts.push(output_artifact());
    assert_eq!(
        compose_run_output(scenario.vault(), changed_artifacts).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::MessageIdConflict
        )),
        "same contribution and text with different artifacts conflicts"
    );

    let mut changed_contribution = request.clone();
    changed_contribution.contribution_id = LogicalContributionId::new();
    assert_eq!(
        compose_run_output(scenario.vault(), changed_contribution).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a different contribution cannot claim the existing Run Output"
    );

    assert_eq!(original.journal_revision, before + 1);
    let current = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read Run after conflict replays")
        .expect("Run remains present");
    assert_eq!(current.journal_revision, original.journal_revision);
}

#[tokio::test]
async fn run_output_continue_child_cannot_replay_a_parent_core_link_or_revision() {
    let mut scenario = Scenario::new().await;
    let (parent, input) = scenario
        .admit_run_and_append_input("parent contribution for Continue")
        .await;
    let parent_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&parent, input.receipt.transcript))
        .await
        .expect("open parent owner recorder");
    let parent_request =
        typed_assistant_request(parent_open.fence.clone(), "parent typed contribution");
    let parent_receipt = compose_typed_recording(scenario.vault(), parent_request.clone())
        .await
        .expect("record parent typed contribution through the existing writer");
    let failed_parent = scenario.budget_exceeded_owner_run(&parent).await;
    scenario
        .vault()
        .close_conversation_recorder(parent_open.fence)
        .await
        .expect("close parent recorder after the terminal budget failure");
    assert_eq!(parent_receipt.recorder.run_id, parent.run_id);
    assert_eq!(failed_parent.journal_revision, 0);

    let (child, child_input) = scenario
        .admit_owner_run_reusing_input(&failed_parent, "continue parent contribution")
        .await;
    let child_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&child, child_input.receipt.transcript))
        .await
        .expect("open child owner recorder");
    let text = "child answer with matching contribution id";
    let artifacts = vec![output_artifact()];
    let before = append_valid_answer_batch(&scenario, &child, text, artifacts.clone()).await;
    let child_output = floe_agent_contract::JournalEvent::Output {
        text: text.into(),
        artifacts: artifacts.clone(),
    };
    scenario
        .append_owner_event(child.run_id, child_output.clone())
        .await;
    let output_revision = before + 1;
    let request = ConversationRunOutputRequest {
        recorder: child_open.fence,
        message_id: parent_request.message_id,
        command_id: parent_request.command_id,
        contribution_id: parent_request.contribution_id,
        typed_entry_id: parent_request.typed_entry_id,
        event: child_output,
    };

    assert_eq!(
        compose_run_output(scenario.vault(), request).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the child Output cannot claim the parent's typed/Core/link proof"
    );
    assert_ne!(parent_receipt.recorder.run_id, child.run_id);
    assert_eq!(
        scenario
            .vault()
            .conversation_run(child.run_id)
            .await
            .expect("read Continue child after replay rejection")
            .expect("child remains present")
            .journal_revision,
        output_revision,
        "no parent journal revision is returned or appended to the child"
    );
}

#[tokio::test]
async fn run_output_composition_rejects_partial_journal_or_core_link_evidence() {
    {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario
            .admit_run_and_append_input("unbound journal Output")
            .await;
        let open = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
            .await
            .expect("open actual owner recorder");
        let text = "journal output without owner proof";
        let artifacts = vec![output_artifact()];
        append_valid_answer_batch(&scenario, &run, text, artifacts.clone()).await;
        let request = run_output_request(open.fence, text, artifacts.clone());
        scenario
            .append_owner_event(run.run_id, request.event.clone())
            .await;
        let output_revision = scenario
            .vault()
            .conversation_run(run.run_id)
            .await
            .expect("read unbound journal Run")
            .expect("Run remains present")
            .journal_revision;
        assert_eq!(
            compose_run_output(scenario.vault(), request).await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "existing Output with no typed/Core/link proof is not retrofitted"
        );
        assert_eq!(
            scenario
                .vault()
                .conversation_run(run.run_id)
                .await
                .expect("read Run after unbound Output rejection")
                .expect("Run remains present")
                .journal_revision,
            output_revision,
            "the unbound Output is not appended again"
        );
    }

    {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario
            .admit_run_and_append_input("unbound typed Output")
            .await;
        let open = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
            .await
            .expect("open actual owner recorder");
        let text = "owner proof without journal output";
        let artifacts = vec![output_artifact()];
        let before = append_valid_answer_batch(&scenario, &run, text, artifacts.clone()).await;
        let typed = typed_assistant_request(open.fence.clone(), text);
        compose_typed_recording(scenario.vault(), typed.clone())
            .await
            .expect("create the existing typed/Core/link evidence through its writer");
        let request = ConversationRunOutputRequest {
            recorder: typed.recorder,
            message_id: typed.message_id,
            command_id: typed.command_id,
            contribution_id: typed.contribution_id,
            typed_entry_id: typed.typed_entry_id,
            event: floe_agent_contract::JournalEvent::Output {
                text: text.into(),
                artifacts,
            },
        };
        assert_eq!(
            compose_run_output(scenario.vault(), request).await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "existing Core/link evidence with no journal Output is not adopted"
        );
        assert_eq!(
            scenario
                .vault()
                .conversation_run(run.run_id)
                .await
                .expect("read Run after unbound typed evidence rejection")
                .expect("Run remains present")
                .journal_revision,
            before,
            "the composer does not append around pre-existing Core/link evidence"
        );
    }
}

#[tokio::test]
async fn run_output_new_write_requires_the_current_executor_generation() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("stale new Output")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open actual owner recorder");
    let text = "new output must use the current executor";
    let artifacts = vec![output_artifact()];
    let before = append_valid_answer_batch(&scenario, &run, text, artifacts.clone()).await;
    let request = run_output_request(open.fence, text, artifacts);
    scenario
        .vault()
        .activate_conversation_executor()
        .await
        .expect("advance the actual executor generation");

    assert_eq!(
        compose_run_output(scenario.vault(), request).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::WrongWriter
        )),
        "a genuinely new Output is denied under a stale executor fence"
    );
    let current = scenario
        .vault()
        .conversation_run(run.run_id)
        .await
        .expect("read stale Run after denial")
        .expect("Run remains present");
    assert_eq!(current.journal_revision, before);
    assert_eq!(
        scenario
            .vault()
            .conversation_journal(run.run_id)
            .await
            .expect("read journal after stale new write")
            .iter()
            .filter(|entry| entry.kind == "output")
            .count(),
        0
    );
}

fn large_owner_coverage(person_id: PersonId) -> DependencyCoverage {
    use chrono::{Duration, Utc};
    use floe_access::{
        ConnectionId, ContextDependency, ExecutionOwnerId, GrantAuthority, GrantConsumer,
        GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantSourceBinding,
        ProcessingRestriction, ResourceHandle, SourceAuthority,
    };

    let connection_id = ConnectionId::try_new("fixture.connection").expect("valid connection ID");
    let source = GrantSourceBinding::try_new(
        person_id,
        connection_id,
        floe_access::ConnectorId::try_new("fixture.connector").expect("valid connector ID"),
        ExecutionOwnerId::try_new("fixture.owner").expect("valid execution owner"),
    )
    .expect("valid context source binding");
    let now = Utc::now();
    DependencyCoverage::Dependent {
        dependencies: vec![
            ContextDependency::try_new(
                person_id,
                GrantId::new(),
                GrantAuthority::new(),
                source,
                vec![ResourceHandle::try_new("fixture.resource").expect("valid resource")],
                SourceAuthority::new(),
                vec![
                    ResourceHandle::try_new("fixture.source-resource")
                        .expect("valid source resource"),
                ],
                vec![GrantDataCategory::Content],
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin("manager").expect("valid built-in consumer"),
                ProcessingRestriction::DeviceOnly,
                Uuid::new_v4(),
                vec![b'x'; 4_096],
                Uuid::new_v4(),
                Uuid::new_v4(),
                now,
                now + Duration::minutes(5),
            )
            .expect("valid large owner dependency"),
        ],
    }
}

async fn store_owner_coverage(scenario: &Scenario, turn_id: Uuid, coverage: DependencyCoverage) {
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to write live coverage fixture");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start live coverage write transaction");
    crate::vault::context_dependencies::merge_context_dependency_coverage(
        &transaction,
        scenario.person_id,
        scenario.session_id,
        turn_id,
        coverage,
    )
    .await
    .expect("store coverage under the exact Person/Session/turn key");
    transaction
        .commit()
        .await
        .expect("commit live coverage fixture");
}

async fn remove_owner_coverage(scenario: &Scenario, turn_id: Uuid) {
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to remove optional coverage fixture");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start optional coverage removal transaction");
    transaction
        .execute(
            "DELETE FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (scenario.person_id.to_string(), scenario.session_id.to_string(), turn_id.to_string()),
        )
        .await
        .expect("remove optional live coverage row");
    transaction
        .commit()
        .await
        .expect("commit optional coverage removal");
}

async fn schema_tables(connection: &turso::Connection) -> Vec<String> {
    let mut rows = connection
        .query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'agent_conversation_%' ORDER BY name",
            (),
        )
        .await
        .expect("inspect conversation table names");
    let mut names = Vec::new();
    while let Some(row) = rows.next().await.expect("read conversation table name") {
        names.push(row.get::<String>(0).expect("conversation table name"));
    }
    names
}

fn typed_interaction_request(
    recorder: RecorderFence,
    turn_id: Uuid,
    interaction_id: Uuid,
    interaction_kind: floe_agent_contract::UserInteractionKind,
) -> TypedConversationRecordingRequest {
    TypedConversationRecordingRequest {
        recorder,
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Interaction {
            turn_id,
            interaction_id,
            interaction_kind,
        },
    }
}

fn pending_navigation_interaction(
    run: &RunRecord,
    execution: floe_agent_contract::TaskExecutionReceiptRef,
) -> floe_conversation::ConversationInteraction {
    use floe_context_contract::{
        GrantConsumer, GrantOperation, GrantPurpose, SourceAccessRequirement,
        SourceAccessRequirementKind,
    };
    use floe_conversation::{
        BlockedReviewEvidence, ConversationInteraction, InteractionOrigin, InteractionRequirement,
        InteractionState, NavigationDestination, NavigationOnlyTarget, ReviewAuditRecord,
        ReviewedTarget, canonical_requirement_digest, canonical_target_digest,
        interaction_publication_id,
    };

    let source = SourceAccessRequirement::try_new(
        "floe.source.calendar",
        None,
        None,
        GrantOperation::Read,
        GrantConsumer::builtin("manager").expect("valid owner publication consumer"),
        GrantPurpose::Assistant,
        Vec::new(),
        None,
        SourceAccessRequirementKind::SelectResource,
        None,
        None,
        false,
    )
    .expect("valid navigation source requirement");
    let requirement = InteractionRequirement::from_source(&source, false);
    let target = NavigationOnlyTarget {
        destination: NavigationDestination::ResourcePicker,
        source_id: source.source_id().to_owned(),
        connection_id: source
            .connection_id()
            .map(|connection| connection.as_str().to_owned()),
        consumer: source.consumer().identifier().to_owned(),
        purpose: requirement.purpose.clone(),
    };
    let target = ReviewedTarget::NavigationOnly(target);
    let origin = InteractionOrigin::Task {
        execution: execution.clone(),
        capability_call_id: None,
    };
    let audit = ReviewAuditRecord {
        person_id: run.person_id,
        device_id: run.device_id.clone(),
        session_id: run.session_id,
        run_id: run.run_id,
        executor_generation: run.executor_generation,
        operation_id: Uuid::new_v4(),
        evidence: BlockedReviewEvidence::Navigation {
            execution,
            requirement: source,
            target: match &target {
                ReviewedTarget::NavigationOnly(target) => target.clone(),
                _ => unreachable!(),
            },
        },
    };
    let requirement_digest =
        canonical_requirement_digest(&requirement).expect("canonical navigation requirement");
    let target_digest = canonical_target_digest(&target).expect("canonical navigation target");
    let id = interaction_publication_id(run.run_id, &origin, &requirement_digest, &target_digest)
        .expect("canonical owner publication ID");
    let interaction = ConversationInteraction {
        id,
        person_id: run.person_id,
        session_id: run.session_id,
        origin_run_id: run.run_id,
        origin_turn_id: run.run_id.as_uuid(),
        origin,
        audit,
        kind: floe_agent_contract::UserInteractionKind::SourceAccess,
        requirement,
        requirement_digest,
        target,
        target_digest,
        state: InteractionState::Pending,
        revision: 1,
        created_at_unix_ms: 1,
        expires_at_unix_ms: 1 + floe_conversation::INTERACTION_PENDING_LIFETIME_MS,
    };
    interaction
        .validate()
        .expect("valid owner interaction publication fixture");
    interaction
}

async fn publish_interaction_with_owner_audit(
    scenario: &Scenario,
    run: &RunRecord,
    interaction: floe_conversation::ConversationInteraction,
) {
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to owner Interaction publication fixture");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start owner Interaction publication transaction");
    let current = scenario
        .vault()
        .conversation_run_on(&transaction, run.run_id)
        .await
        .expect("load Interaction owner Run")
        .expect("Interaction owner Run exists");
    assert_eq!(current.state, floe_conversation::RunState::Working);
    scenario
        .vault()
        .check_interaction_origin_on(&transaction, &interaction)
        .await
        .expect("validate actual Task and Run journal origin");
    scenario
        .vault()
        .store_review_audit_on(&transaction, &current, &interaction.audit, false)
        .await
        .expect("store the exact owner publication audit and evidence");
    super::super::super::conversation_interactions::insert_interaction(&transaction, &interaction)
        .await
        .expect("publish owner-validated ConversationInteraction row");
    transaction
        .commit()
        .await
        .expect("commit owner Interaction publication transaction");
}

async fn unadmitted_terminal_task_receipt(
    scenario: &Scenario,
    run: &RunRecord,
) -> floe_agent_contract::TaskReceipt {
    use floe_agent_contract::{
        AgentContext, DelegationExecutionContext, DelegationRequest, JournalEvent,
        ModelBindingDigest, ModelBudgetProfile, ModelCapabilities, ModelSelectionCommitment,
        PinnedAgentRevision, PreparedModelPlan, ProcessingBoundary, ProjectionRef,
        TaskExecutionEvidence, TaskSnapshot, TaskState, ValidatedModelBatch,
    };

    let batch_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();
    let task_id = TaskId::from_uuid(Uuid::new_v5(
        &run.run_id.as_uuid(),
        format!("{}:{batch_id}:0:task", run.run_id.as_uuid()).as_bytes(),
    ))
    .expect("derive owner journal's stable Task ID");
    let invocation_key = floe_agent_contract::InvocationKey::from_uuid(Uuid::new_v5(
        &run.run_id.as_uuid(),
        format!("{}:{batch_id}:0:delegation", run.run_id.as_uuid()).as_bytes(),
    ))
    .expect("derive owner journal's stable delegation invocation");
    let projection_ref = ProjectionRef::new();
    let execution_context = DelegationExecutionContext {
        session_id: run.session_id,
        device_id: run.device_id.clone(),
        agent_context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
        projection_coverage: DependencyCoverage::Independent,
    };
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: None,
                reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                    tokens: 128,
                    cost_micros: 128,
                },
                projection_ref,
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: scenario.person_id.to_string(),
                    device_id: run.device_id.clone(),
                    purpose: "everyday_assistance".into(),
                    consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([91; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([92; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ModelResult {
                attempt_id,
                usage: floe_agent_contract::ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::ValidatedBatch {
                batch: ValidatedModelBatch {
                    execution_id: run.run_id.as_uuid(),
                    attempt_id,
                    projection_ref,
                    batch_id,
                    steps: vec![floe_agent_contract::ModelStep::Delegate {
                        agent_id: "fixture.expert".into(),
                        definition_revision: 1,
                        message: "unadmitted terminal failure".into(),
                        context_refs: vec![],
                    }],
                    catalog_revision: run.expert_environment.revision,
                    tool_revisions: vec![],
                    agent_revisions: vec![PinnedAgentRevision {
                        agent_id: "fixture.expert".into(),
                        definition_revision: 1,
                    }],
                    projection_coverage: DependencyCoverage::Independent,
                    delegation_context: Some(execution_context.clone()),
                },
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::BatchProgress {
                cursor: floe_agent_contract::BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await;
    let request = DelegationRequest {
        task_id,
        parent_run_id: Some(run.run_id.as_uuid()),
        principal: scenario.person_id.to_string(),
        invocation_key,
        selected_agent_id: "fixture.expert".into(),
        selected_definition_revision: 1,
        message: "unadmitted terminal failure".into(),
        context_refs: vec![],
        execution_context,
    };
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::DelegationIntent {
                request: request.clone(),
            },
        )
        .await;
    let receipt = floe_agent_contract::TaskReceipt {
        task_id,
        snapshot: TaskSnapshot {
            task_id,
            parent_run_id: request.parent_run_id,
            principal: request.principal,
            agent_id: request.selected_agent_id,
            definition_revision: request.selected_definition_revision,
            state: TaskState::Rejected,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            issue: Some(AgentFailure::Interrupted),
            blockage: None,
        },
        replay: None,
        execution: TaskExecutionEvidence::Unadmitted,
    };
    receipt
        .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
        .expect("valid owner Unadmitted terminal failure receipt");
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::DelegationResult {
                receipt: Box::new(receipt.clone()),
            },
        )
        .await;
    scenario
        .append_owner_event(
            run.run_id,
            JournalEvent::BatchProgress {
                cursor: floe_agent_contract::BatchCursor {
                    batch_id,
                    next_step_index: 1,
                },
            },
        )
        .await;
    receipt
}

#[tokio::test]
async fn owner_input_reverse_lookup_uses_exact_mapping_keys_with_unrelated_runs_present() {
    let mut scenario = Scenario::new().await;
    let (first_run, first_input) = scenario
        .admit_run_and_append_input("indexed owner input A")
        .await;
    scenario.cancel_owner_run(&first_run).await;

    let mut second_core_request = scenario.core_input_request("unrelated owner input B");
    second_core_request.target = AdmissionTarget::AppendToExisting {
        reference: ConversationReference {
            identity: scenario.identity.clone(),
            conversation_id: scenario.conversation_id,
            branch_id: scenario.branch_id,
            head_revision: first_input.receipt.transcript.sequence,
        },
    };
    let (second_run, second_input) = scenario
        .admit_run_and_bind_input(
            scenario.owner_request("unrelated owner input B"),
            second_core_request,
        )
        .await;
    assert_ne!(first_run.run_id, second_run.run_id);
    assert_ne!(
        first_input.receipt.transcript,
        second_input.receipt.transcript
    );

    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to indexed mapping fixture");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start exact mapping lookup transaction");
    let mapping = owner_transcript_input_for_reference_on(
        &transaction,
        scenario.person_id,
        first_input.receipt.transcript,
    )
    .await
    .expect("point lookup exact original Core input")
    .expect("first input mapping remains present");
    assert_eq!(mapping.original_owner_run_id, first_run.run_id);
    assert_eq!(mapping.owner_user_message_id, first_run.user_message_id);
    assert_eq!(mapping.input, first_input.receipt.transcript);

    let mut reverse_plan = transaction
        .query(
            "EXPLAIN QUERY PLAN SELECT mapping_json FROM agent_conversation_owner_transcript_inputs_v1 INDEXED BY agent_conversation_owner_transcript_inputs_core_key_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND input_sequence = ? AND input_message_id = ? LIMIT 2",
            (
                scenario.person_id.to_string(),
                first_input
                    .receipt
                    .transcript
                    .conversation_id
                    .as_uuid()
                    .to_string(),
                first_input
                    .receipt
                    .transcript
                    .branch_id
                    .as_uuid()
                    .to_string(),
                integer(first_input.receipt.transcript.sequence).expect("positive sequence"),
                first_input
                    .receipt
                    .transcript
                    .message_id
                    .as_uuid()
                    .to_string(),
            ),
        )
        .await
        .expect("explain exact Core reverse lookup");
    let reverse_detail = reverse_plan
        .next()
        .await
        .expect("read reverse lookup plan")
        .expect("reverse lookup has a plan row")
        .get::<String>(3)
        .expect("reverse lookup plan detail");
    assert!(
        reverse_detail.contains("agent_conversation_owner_transcript_inputs_core_key_v1"),
        "Core reverse lookup must use the exact immutable mapping primary key: {reverse_detail}"
    );

    let mut owner_plan = transaction
        .query(
            "EXPLAIN QUERY PLAN SELECT mapping_json FROM agent_conversation_owner_transcript_inputs_v1 INDEXED BY agent_conversation_owner_transcript_inputs_owner_key_v1 WHERE person_id = ? AND session_id = ? AND owner_user_message_id = ? LIMIT 2",
            (
                scenario.person_id.to_string(),
                scenario.session_id.to_string(),
                first_run.user_message_id.to_string(),
            ),
        )
        .await
        .expect("explain unique owner-key lookup");
    let owner_detail = owner_plan
        .next()
        .await
        .expect("read owner-key lookup plan")
        .expect("owner-key lookup has a plan row")
        .get::<String>(3)
        .expect("owner-key lookup plan detail");
    assert!(
        owner_detail.contains("agent_conversation_owner_transcript_inputs_owner_key_v1"),
        "Continue lookup must use its explicit unique owner key: {owner_detail}"
    );
    assert_eq!(
        owner_transcript_input_for_owner_message_on(
            &transaction,
            scenario.person_id,
            scenario.session_id,
            first_run.user_message_id,
        )
        .await
        .expect("resolve Continue owner key"),
        Some(mapping),
    );
    transaction
        .commit()
        .await
        .expect("finish exact mapping lookup transaction");
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        2,
        "two distinct Runs remain in the old Run-binding family"
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
        2
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_run_inputs_v1"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn failed_owner_mapping_write_rolls_back_the_new_optional_family() {
    let scenario = Scenario::new().await;
    let before_session = session_storage_snapshot(
        &scenario
            .vault()
            .connection()
            .expect("connect before injected mapping failure"),
        scenario.session_id,
    )
    .await;
    scenario
        .vault()
        .conversation_core_fault_after_input_mapping
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(scenario.owner_request("rollback mapping")),
            scenario.core_input_request("rollback mapping"),
        )
        .await,
        Err(ConversationStoreFailure::NotCommitted)
    );

    let connection = scenario
        .vault()
        .connection()
        .expect("connect after injected mapping failure");
    assert_eq!(
        table_count(&connection, "agent_conversation_runs").await,
        0,
        "owner admission rolls back with the Core mapping"
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_open_receipts").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_active_recorders").await,
        0
    );
    assert!(
        !crate::schema::conversation_owner_custody_family_v2_present(&connection)
            .await
            .expect("inspect optional family without creating it")
    );
    assert_eq!(
        session_storage_snapshot(&connection, scenario.session_id).await,
        before_session
    );
}

#[tokio::test]
async fn fault_after_composed_recorder_open_rolls_back_every_owner_and_core_write() {
    let scenario = Scenario::new().await;
    let before_session = session_storage_snapshot(
        &scenario
            .vault()
            .connection()
            .expect("connect before injected open failure"),
        scenario.session_id,
    )
    .await;
    scenario
        .vault()
        .conversation_core_fault_after_recorder_open
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(scenario.owner_request("rollback after recorder open")),
            scenario.core_input_request("rollback after recorder open"),
        )
        .await,
        Err(ConversationStoreFailure::NotCommitted)
    );

    let connection = scenario
        .vault()
        .connection()
        .expect("connect after injected open failure");
    for table in [
        "agent_conversation_runs",
        "agent_conversation_core_v3_entries",
        "agent_conversation_core_v3_input_receipts",
        "agent_conversation_core_v3_owner_bindings",
        "agent_conversation_core_v3_open_receipts",
        "agent_conversation_core_v3_active_recorders",
    ] {
        assert_eq!(
            table_count(&connection, table).await,
            0,
            "rollback removes every {table} write"
        );
    }
    assert!(
        !crate::schema::conversation_owner_custody_family_v2_present(&connection)
            .await
            .expect("inspect owner mapping family without creating it"),
        "rollback removes the Run-to-input mapping family"
    );
    assert_eq!(
        session_storage_snapshot(&connection, scenario.session_id).await,
        before_session,
        "the owner Session revision and User entry roll back with Core custody"
    );
}

#[tokio::test]
async fn owner_custody_reads_and_open_do_not_repair_optional_schema() {
    let mut scenario = Scenario::new().await;
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before optional-family reads");
    let before = live_catalog_snapshot(&connection).await;
    drop(connection);
    assert!(
        !before
            .iter()
            .any(|(_, name, _)| name.contains("owner_transcript"))
    );
    let absent_reference = TranscriptReference {
        conversation_id: ConversationId::new(),
        branch_id: ConversationBranchId::new(),
        sequence: 1,
        message_id: MessageId::new(),
    };
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect before absent-family reads");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start absent-family read transaction");
    assert_eq!(
        owner_transcript_input_for_reference_on(
            &transaction,
            scenario.person_id,
            absent_reference,
        )
        .await
        .expect("read missing owner input mapping"),
        None
    );
    assert_eq!(
        typed_transcript_link_for_entry_on(&transaction, scenario.person_id, absent_reference,)
            .await
            .expect("read missing typed transcript link"),
        None
    );
    transaction
        .commit()
        .await
        .expect("finish absent-family read transaction");
    scenario
        .vault()
        .validate_conversation_core_store()
        .await
        .expect("validate absent optional Core family");
    scenario.reopen().await;
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after no-repair reopen");
    let after = live_catalog_snapshot(&connection).await;
    assert_eq!(after, before, "open/read leave catalog bytes unchanged");
}

#[tokio::test]
async fn owner_admission_and_core_input_rollback_as_one_transaction() {
    let scenario = Scenario::new().await;
    let initial_revision = scenario.session_revision;
    let owner_request = scenario.owner_request("atomic admission");
    let core_request = scenario.core_input_request("atomic admission");
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to test Vault");
    let (mut guard, transaction) = scenario
        .vault()
        .journal_transaction(&mut connection)
        .await
        .expect("start composed transaction");
    let mut replay_checked = false;
    let mut prior_command = false;
    scenario
        .vault()
        .admit_conversation_run_with_core_input_on(
            &transaction,
            CoreComposedOwnerIntent::Turn(owner_request),
            core_request,
            &mut replay_checked,
            &mut prior_command,
        )
        .await
        .expect("write bound owner Run and Core input before deliberate rollback");
    transaction
        .rollback()
        .await
        .expect("roll back owner and Core rows together");
    guard.settled();

    let connection = scenario
        .vault()
        .connection()
        .expect("reconnect after rollback");
    assert_eq!(table_count(&connection, "agent_conversation_runs").await, 0);
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_heads").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_sessions").await,
        1,
        "the initial owner Session remains"
    );
    let session = SessionStore::load(scenario.vault(), scenario.person_id, scenario.session_id)
        .await
        .expect("reload Session after rollback");
    assert_eq!(session.revision, initial_revision);
    assert!(session.active_turn.is_none());
}

#[tokio::test]
async fn owner_input_mapping_ack_loss_replays_exact_proof_after_reopen() {
    let mut scenario = Scenario::new().await;
    let owner_request = scenario.owner_request("owner input mapping ACK replay");
    let core_request = scenario.core_input_request("owner input mapping ACK replay");
    let expected_run = owner_request.run_id;
    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(owner_request.clone()),
            core_request.clone(),
        )
        .await,
        Err(ConversationStoreFailure::OutcomeUnknown),
        "mapping and owner/Core admission commit before the lost acknowledgement"
    );

    scenario.reopen().await;
    let replay = compose_owner_core_run(
        scenario.vault(),
        CoreComposedOwnerIntent::Turn(owner_request),
        core_request,
    )
    .await
    .expect("replay validates the immutable input mapping after reopen");
    let CoreComposedRunAdmission::Admitted { record, input, .. } = replay else {
        panic!("exact New retry did not replay its original admission");
    };
    assert_eq!(record.run_id, expected_run);
    assert_eq!(input.disposition, AdmissionDisposition::Replayed);
    assert_eq!(
        stored_run_input_mapping(scenario.vault(), scenario.person_id, record.run_id)
            .await
            .expect("reopened owner input mapping")
            .original_owner_run_id,
        expected_run
    );
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after exact admission replay");
    assert_eq!(table_count(&connection, "agent_conversation_runs").await, 1);
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
        1
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_run_inputs_v1"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn typed_composer_rejects_wrong_turn_and_user_kind_and_scopes_digest() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed owner output")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed output recorder");

    let mut wrong_turn = typed_assistant_request(open.fence.clone(), "wrong turn");
    wrong_turn.typed_message = floe_conversation::AgentMessage::Assistant {
        turn_id: Uuid::new_v4(),
        text: "wrong turn".into(),
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), wrong_turn).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a new typed output must belong to its admitted owner Run"
    );

    let mut user_payload = typed_assistant_request(open.fence.clone(), "not generated");
    user_payload.typed_message = floe_conversation::AgentMessage::User {
        turn_id: run.run_id.as_uuid(),
        message_id: Uuid::new_v4(),
        text: "User content cannot be a generated output".into(),
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), user_payload).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "User payload cannot masquerade as a generated Core output"
    );

    let unlinked = typed_assistant_request(open.fence.clone(), "unrelated same-Session row");
    let unlinked_reference = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to insert an unlinked owner payload");
        let (guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start explicit unlinked payload fixture write");
        let result = scenario
            .vault()
            .insert_typed_agent_message_on(
                &transaction,
                scenario.session_id,
                unlinked.typed_entry_id,
                TypedAgentMessageProvenance::OwnerRecorded,
                &unlinked.typed_message,
            )
            .await
            .map_err(owner_error);
        scenario
            .vault()
            .finish_conversation_core_transaction(guard, transaction, result)
            .await
            .expect("store an unrelated same-Session typed row")
    };
    assert_eq!(unlinked_reference.session_id(), scenario.session_id);
    let mut adopt_unrelated = unlinked.clone();
    adopt_unrelated.message_id = MessageId::new();
    adopt_unrelated.command_id = CommandId::new();
    adopt_unrelated.contribution_id = LogicalContributionId::new();
    assert_eq!(
        compose_typed_recording(scenario.vault(), adopt_unrelated).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "an unrelated same-Session digest cannot be retroactively attached"
    );

    let request = typed_assistant_request(open.fence.clone(), "the typed answer");
    let receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("derive and atomically link the exact typed output");
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("typed output has its immutable owner link");
    let mut link_connection = scenario
        .vault()
        .connection()
        .expect("connect to point-read exact typed link");
    let link_transaction = link_connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start exact transcript link read transaction");
    assert_eq!(
        typed_transcript_link_for_entry_on(
            &link_transaction,
            scenario.person_id,
            link.transcript_entry.reference,
        )
        .await
        .expect("resolve exact transcript entry owner proof"),
        Some(link.clone()),
        "the reader helper is an exact transcript-key lookup"
    );
    link_transaction
        .commit()
        .await
        .expect("finish exact transcript link read transaction");
    assert_eq!(link.owner_input, input.receipt.transcript);
    assert_eq!(link.owner_session_id, scenario.session_id);
    assert_eq!(link.owner_turn_id, run.run_id.as_uuid());
    assert_eq!(link.first_recording_run_id, run.run_id);
    assert_eq!(link.transcript_entry.producer_run, Some(run.run_id));
    assert_eq!(
        link.transcript_entry.contribution_id,
        Some(request.contribution_id)
    );
    assert_eq!(
        link.transcript_entry
            .message
            .evidence
            .as_ref()
            .expect("Core retains opaque typed digest")
            .digest(),
        link.typed_reference.digest(),
    );
    assert_eq!(receipt.transcript, link.transcript_entry.reference);

    let other_session = match scenario
        .vault()
        .start_conversation_session(StartSessionRequest {
            principal: scenario.person_id.to_string(),
            command_id: CommandId::new(),
        })
        .await
        .expect("create another owner Session for digest scope check")
    {
        SessionStartAdmission::Started(receipt) => receipt.session_id,
        other => panic!("unexpected second Session admission: {other:?}"),
    };
    let foreign_reference = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to foreign Session");
        let (guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start foreign typed payload transaction");
        let result = scenario
            .vault()
            .insert_typed_agent_message_on(
                &transaction,
                other_session,
                request.typed_entry_id,
                TypedAgentMessageProvenance::OwnerRecorded,
                &request.typed_message,
            )
            .await
            .map_err(owner_error);
        scenario
            .vault()
            .finish_conversation_core_transaction(guard, transaction, result)
            .await
            .expect("store foreign-session typed fixture without a Core link")
    };
    assert_ne!(foreign_reference.digest(), link.typed_reference.digest());
    let mut cross_session_link = link.clone();
    cross_session_link.typed_reference = foreign_reference;
    assert_eq!(
        cross_session_link.validate(scenario.person_id),
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a same-payload digest scoped to another Session cannot prove this transcript row"
    );
}

#[tokio::test]
async fn typed_writer_faults_roll_back_payload_core_receipt_and_link_together() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("atomic typed writer")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed writer recorder");
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before typed writer fault injection");
    assert!(
        !crate::schema::typed_history_family_present(&connection)
            .await
            .expect("check optional typed payload family")
    );
    drop(connection);

    for stage in [1, 2, 3] {
        scenario
            .vault()
            .conversation_core_typed_write_fault
            .store(stage, Ordering::Release);
        let request = typed_assistant_request(open.fence.clone(), &format!("fault stage {stage}"));
        assert_eq!(
            compose_typed_recording(scenario.vault(), request).await,
            Err(ConversationStoreFailure::NotCommitted),
            "injected stage {stage} failure aborts the single composed write"
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after typed writer rollback");
        assert!(
            !crate::schema::typed_history_family_present(&connection)
                .await
                .expect("inspect rolled-back optional typed family"),
            "typed payload family creation is part of the write transaction"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1,
            "only the admitted inbound entry remains"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            0
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_output_receipts_v2").await,
            0
        );
        assert_eq!(
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1"
            )
            .await,
            0
        );
    }
}

#[tokio::test]
async fn typed_contribution_ack_loss_replays_exact_link_after_reopen() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed ACK replay")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed ACK recorder");
    let request = typed_assistant_request(open.fence.clone(), "persist once");
    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_typed_recording(scenario.vault(), request.clone()).await,
        Err(ConversationStoreFailure::OutcomeUnknown),
        "the payload, Core receipt, and link committed before the lost ACK"
    );

    scenario.reopen().await;
    let replay = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("exact typed contribution replays after reopen");
    assert_eq!(replay.recorder, open.fence);
    assert_eq!(replay.contribution_id, request.contribution_id);
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("replayed typed contribution retains its link");
    assert_eq!(link.first_recording_run_id, run.run_id);
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after exact replay");
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        2
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
        1
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_evidence_v1"
        )
        .await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_typed_history_entries").await,
        1
    );
}

#[tokio::test]
async fn typed_delegation_requires_actual_task_snapshot_receipt_and_owner_journal() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed Task proof")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed Task recorder");
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task evidence owner")
        .executor_generation;
    let (task_receipts, _) = scenario
        .terminal_task_receipts(&run, generation, true)
        .await;
    let accepted_reference = task_receipts[0].clone();
    let task = scenario
        .vault()
        .task(accepted_reference.execution.task_id)
        .await
        .expect("read actual Task snapshot")
        .expect("fixture Task exists");
    let typed_message = floe_conversation::AgentMessage::Delegation {
        turn_id: run.run_id.as_uuid(),
        task: task.snapshot.clone(),
        execution_receipt: Some(accepted_reference.clone()),
    };
    let valid_request = TypedConversationRecordingRequest {
        recorder: open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: typed_message.clone(),
    };

    let mut unadmitted_request = valid_request.clone();
    if let floe_conversation::AgentMessage::Delegation {
        execution_receipt, ..
    } = &mut unadmitted_request.typed_message
    {
        *execution_receipt = None;
    }
    unadmitted_request.contribution_id = LogicalContributionId::new();
    unadmitted_request.typed_entry_id = Uuid::new_v4();
    assert_eq!(
        compose_typed_recording(scenario.vault(), unadmitted_request).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "an admitted Task result cannot be presented as Unadmitted"
    );

    let mut wrong_parent_snapshot = task.snapshot.clone();
    wrong_parent_snapshot.parent_run_id = Some(Uuid::new_v4());
    let wrong_parent = TypedConversationRecordingRequest {
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: wrong_parent_snapshot,
            execution_receipt: Some(accepted_reference.clone()),
        },
        ..valid_request.clone()
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), wrong_parent).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the Delegation snapshot must be the Task admitted under this owner Run"
    );

    let mut wrong_receipt = accepted_reference.clone();
    wrong_receipt.task_revision += 1;
    let mismatched_receipt = TypedConversationRecordingRequest {
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: task.snapshot.clone(),
            execution_receipt: Some(wrong_receipt),
        },
        ..valid_request.clone()
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), mismatched_receipt).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the supplied Task receipt must match its actual stored execution receipt"
    );

    let receipt = compose_typed_recording(scenario.vault(), valid_request.clone())
        .await
        .expect("valid Task snapshot and owner journal proof compose atomically");
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        valid_request.contribution_id,
    )
    .await
    .expect("valid Delegation output has an owner transcript link");
    assert_eq!(link.original_task_receipt, Some(accepted_reference.clone()));
    assert_eq!(
        receipt.producing_task,
        Some(task_evidence_reference(&accepted_reference).expect("derive Task evidence"))
    );
    assert_eq!(
        link.transcript_entry.message.task_id,
        Some(accepted_reference.execution.task_id)
    );
}

#[tokio::test]
async fn typed_unadmitted_delegation_requires_exact_owner_pair_and_replays_original_producer() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed unadmitted Task proof")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open Unadmitted terminal recorder");

    let unproven = floe_agent_contract::TaskSnapshot {
        task_id: TaskId::new(),
        parent_run_id: Some(run.run_id.as_uuid()),
        principal: scenario.person_id.to_string(),
        agent_id: "fixture.expert".into(),
        definition_revision: 1,
        state: floe_agent_contract::TaskState::Rejected,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Independent,
        issue: Some(AgentFailure::Interrupted),
        blockage: None,
    };
    let unproven_request = TypedConversationRecordingRequest {
        recorder: open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: unproven.clone(),
            execution_receipt: None,
        },
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), unproven_request).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "None execution evidence without the owning journal pair is not proof"
    );

    let terminal_receipt = unadmitted_terminal_task_receipt(&scenario, &run).await;
    assert!(
        scenario
            .vault()
            .task(terminal_receipt.task_id)
            .await
            .expect("inspect Task owner after Unadmitted result")
            .is_none(),
        "the Unadmitted result has no admitted Task owner record"
    );
    let mut mismatched_snapshot = terminal_receipt.snapshot.clone();
    mismatched_snapshot.agent_id = "counterfeit.agent".into();
    let mismatched_request = TypedConversationRecordingRequest {
        recorder: open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: mismatched_snapshot,
            execution_receipt: None,
        },
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), mismatched_request).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the typed Task snapshot must equal the validated journal receipt"
    );

    let mut wrong_task_id = terminal_receipt.snapshot.clone();
    wrong_task_id.task_id = TaskId::new();
    let mismatched_journal_result = TypedConversationRecordingRequest {
        recorder: open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: wrong_task_id,
            execution_receipt: None,
        },
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), mismatched_journal_result).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a journal result for another Task ID cannot prove this output"
    );

    let request = TypedConversationRecordingRequest {
        recorder: open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: terminal_receipt.snapshot.clone(),
            execution_receipt: None,
        },
    };
    let original_receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("compose validated owner Unadmitted terminal failure");
    assert_eq!(original_receipt.producing_task, None);
    let original_link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("load linked Unadmitted Task output");
    assert_eq!(original_link.first_recording_run_id, run.run_id);
    assert_eq!(original_link.original_task_receipt, None);
    assert_eq!(
        original_link.transcript_entry.producer_run,
        Some(run.run_id)
    );
    assert_eq!(original_link.transcript_entry.producing_task, None);
    assert_eq!(
        original_link.transcript_entry.message.origin,
        MessageOrigin::Host
    );
    assert_eq!(original_link.transcript_entry.message.task_id, None);
    let resolved = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to typed Task output");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("start typed Task payload read");
        let resolved = scenario
            .vault()
            .resolve_typed_agent_message_on(
                &transaction,
                &original_link.typed_reference,
                floe_conversation::MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .expect("resolve original typed Unadmitted Task");
        transaction
            .commit()
            .await
            .expect("finish typed Task payload read");
        resolved.message
    };
    assert_eq!(resolved, request.typed_message);

    let terminal = scenario.budget_exceeded_owner_run(&run).await;
    scenario
        .vault()
        .close_conversation_recorder(open.fence)
        .await
        .expect("close original Unadmitted Task recorder");
    let (continued, _) = scenario
        .admit_owner_run_reusing_input(&terminal, "Continue after Unadmitted failure")
        .await;
    let continued_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&continued, input.receipt.transcript))
        .await
        .expect("open Continue recorder for Unadmitted contribution replay");
    let mut replay_request = request.clone();
    replay_request.recorder = continued_open.fence;
    let replay = compose_typed_recording(scenario.vault(), replay_request)
        .await
        .expect("Continue validates the original Unadmitted journal producer");
    assert_eq!(replay, original_receipt);
    assert_eq!(replay.recorder.run_id, run.run_id);
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id,
        )
        .await,
        Some(original_link.clone()),
        "Continue replay retains the first journal producer and typed Task snapshot"
    );

    let boundary = owner_read_boundary(&scenario).await;
    let owner_resolved = scenario
        .vault()
        .read_owner_transcript_entry(
            owner_read_target(&scenario),
            scenario.session_id,
            boundary,
            TranscriptEntryLookup::Reference(original_link.transcript_entry.reference),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("owner read keeps Unadmitted Delegation as typed evidence");
    assert_eq!(owner_resolved.original_owner_run_id, run.run_id);
    assert_eq!(
        owner_resolved.transcript_entry.producer_run,
        Some(run.run_id)
    );
    match &owner_resolved.typed_evidence {
        OwnerTranscriptTypedEvidence::Present {
            message,
            first_recording_run_id,
            original_task_receipt,
            ..
        } => {
            assert_eq!(message, &request.typed_message);
            assert_eq!(*first_recording_run_id, run.run_id);
            assert_eq!(*original_task_receipt, None);
        }
        OwnerTranscriptTypedEvidence::Absent => {
            panic!("Unadmitted Delegation has its exact typed owner record")
        }
    }
}

#[tokio::test]
async fn typed_interaction_requires_current_valid_owner_row_and_stays_textless() {
    let mut scenario = Scenario::new().await;
    let original_session_revision = scenario.session_revision;
    let task_generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task owner for real publication fixtures")
        .executor_generation;
    let (other_session_id, other_session_revision) = match scenario
        .vault()
        .start_conversation_session(StartSessionRequest {
            principal: scenario.person_id.to_string(),
            command_id: CommandId::new(),
        })
        .await
        .expect("create foreign Session in the same owner Vault")
    {
        SessionStartAdmission::Started(receipt) => (receipt.session_id, receipt.session_revision),
        other => panic!("unexpected foreign Session admission: {other:?}"),
    };
    let foreign_run = scenario
        .admit_unbound_owner_run(scenario.owner_request_for_session(
            other_session_id,
            other_session_revision,
            "owner-published foreign Session Interaction",
        ))
        .await;
    let foreign_task = scenario
        .terminal_task_receipts(&foreign_run, task_generation, true)
        .await
        .0
        .into_iter()
        .next()
        .expect("foreign Run has a real owner Task receipt");
    let foreign_interaction = pending_navigation_interaction(&foreign_run, foreign_task);
    publish_interaction_with_owner_audit(&scenario, &foreign_run, foreign_interaction.clone())
        .await;
    scenario
        .vault()
        .finish_conversation_run(
            foreign_run.run_id,
            foreign_run.aggregate_revision,
            floe_conversation::RunTerminal::from_failure(AgentFailure::BudgetExceeded),
        )
        .await
        .expect("settle the unbound foreign Session through its existing owner");
    scenario.session_revision = original_session_revision;

    let (run, input) = scenario
        .admit_run_and_append_input("typed Interaction terminal")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open terminal Interaction recorder");

    let missing = typed_interaction_request(
        open.fence.clone(),
        run.run_id.as_uuid(),
        Uuid::new_v4(),
        floe_agent_contract::UserInteractionKind::SourceAccess,
    );
    assert_eq!(
        compose_typed_recording(scenario.vault(), missing).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a typed historical reference without a ConversationInteraction row is rejected"
    );

    let owner_task = scenario
        .terminal_task_receipts(&run, task_generation, true)
        .await
        .0
        .into_iter()
        .next()
        .expect("origin Run has a real owner Task receipt");
    let interaction = pending_navigation_interaction(&run, owner_task);
    publish_interaction_with_owner_audit(&scenario, &run, interaction.clone()).await;

    let wrong_kind = typed_interaction_request(
        open.fence.clone(),
        run.run_id.as_uuid(),
        interaction.id,
        floe_agent_contract::UserInteractionKind::ExpertBinding,
    );
    assert_eq!(
        compose_typed_recording(scenario.vault(), wrong_kind).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the typed Interaction kind must match the current owner row"
    );

    let request = typed_interaction_request(
        open.fence.clone(),
        run.run_id.as_uuid(),
        interaction.id,
        interaction.kind,
    );
    let original_receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("compose the exact owner-published textless Interaction");
    assert_eq!(original_receipt.producing_task, None);
    let original_link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("load Interaction evidence link");
    assert_eq!(original_link.first_recording_run_id, run.run_id);
    assert_eq!(
        original_link.transcript_entry.producer_run,
        Some(run.run_id)
    );
    assert_eq!(original_link.transcript_entry.producing_task, None);
    assert_eq!(original_link.original_task_receipt, None);
    assert_eq!(
        original_link.transcript_entry.message.origin,
        MessageOrigin::Host
    );
    assert!(original_link.transcript_entry.message.text.is_empty());
    assert_eq!(original_link.transcript_entry.message.task_id, None);
    assert_eq!(
        original_link
            .transcript_entry
            .message
            .evidence
            .as_ref()
            .expect("neutral Core entry preserves typed digest")
            .digest(),
        original_link.typed_reference.digest(),
    );
    let resolved = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to exact typed Interaction payload");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("start Interaction payload lookup");
        let resolved = scenario
            .vault()
            .resolve_typed_agent_message_on(
                &transaction,
                &original_link.typed_reference,
                floe_conversation::MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .expect("resolve exact Interaction reference");
        transaction
            .commit()
            .await
            .expect("finish Interaction payload lookup");
        resolved.message
    };
    assert_eq!(resolved, request.typed_message);

    let boundary = owner_read_boundary(&scenario).await;
    let owner_resolved = scenario
        .vault()
        .read_owner_transcript_entry(
            owner_read_target(&scenario),
            scenario.session_id,
            boundary,
            TranscriptEntryLookup::Reference(original_link.transcript_entry.reference),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("owner read preserves the typed textless Interaction");
    assert!(owner_resolved.transcript_entry.message.text.is_empty());
    assert_eq!(typed_message(&owner_resolved), &request.typed_message);
    assert!(matches!(
        owner_resolved.typed_evidence,
        OwnerTranscriptTypedEvidence::Present { .. }
    ));

    let foreign_reference = typed_interaction_request(
        open.fence.clone(),
        run.run_id.as_uuid(),
        foreign_interaction.id,
        foreign_interaction.kind,
    );
    assert_eq!(
        compose_typed_recording(scenario.vault(), foreign_reference).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "an owner-published interaction from another Session cannot be attached"
    );

    let expired = scenario
        .vault()
        .expire_conversation_interaction(floe_conversation::ExpireInteraction {
            interaction_id: interaction.id,
            person_id: scenario.person_id,
            now_unix_ms: interaction.expires_at_unix_ms,
        })
        .await
        .expect("advance current Interaction status through owner expiry");
    assert!(matches!(
        expired,
        floe_conversation::ExpireOutcome::Expired(_)
    ));
    let same_run_replay = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("historical Interaction replay observes the current Expired row");
    assert_eq!(same_run_replay, original_receipt);
    let stale_decision = scenario
        .vault()
        .record_conversation_interaction_decision(floe_conversation::InteractionDecision {
            command_id: Uuid::new_v4(),
            interaction_id: interaction.id,
            interaction_revision: interaction.revision,
            kind: floe_conversation::InteractionDecisionKind::Approve,
            target_digest: interaction.target_digest,
            principal: scenario.person_id.to_string(),
            decided_at_unix_ms: interaction.expires_at_unix_ms - 1,
        })
        .await;
    assert!(
        matches!(
            stale_decision,
            Err(floe_kernel::CommandFailure::NotApplied(
                AgentFailure::Conflict
            ))
        ),
        "historical typed output does not authorize a decision against today's expired row"
    );
    assert!(matches!(
        scenario
            .vault()
            .conversation_interaction(interaction.id)
            .await
            .expect("read authoritative current Interaction status")
            .expect("Interaction remains in owner storage")
            .state,
        floe_conversation::InteractionState::Expired
    ));

    let terminal = scenario.budget_exceeded_owner_run(&run).await;
    scenario.session_revision = terminal.session_revision;

    scenario
        .vault()
        .close_conversation_recorder(open.fence)
        .await
        .expect("close the terminal Interaction recorder");
    let (continued, _) = scenario
        .admit_owner_run_reusing_input(&terminal, "Continue with Interaction history")
        .await;
    let continued_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&continued, input.receipt.transcript))
        .await
        .expect("open Continue recorder for Interaction replay");
    let mut replay_request = request.clone();
    replay_request.recorder = continued_open.fence.clone();
    let replay = compose_typed_recording(scenario.vault(), replay_request)
        .await
        .expect("Continue revalidates and replays the original Interaction producer");
    assert_eq!(replay, original_receipt);
    assert_eq!(replay.recorder.run_id, run.run_id);
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id,
        )
        .await,
        Some(original_link),
        "Continue replay preserves the original typed Interaction and producer"
    );

    let foreign_run = typed_interaction_request(
        continued_open.fence,
        continued.run_id.as_uuid(),
        interaction.id,
        interaction.kind,
    );
    assert_eq!(
        compose_typed_recording(scenario.vault(), foreign_run).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a different Run cannot attach the original Run's Interaction"
    );
}

#[tokio::test]
async fn typed_contribution_replay_keeps_original_producer_after_continue() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed producer replay")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open first contribution recorder");
    let request = typed_assistant_request(open.fence.clone(), "first recorder owns this");
    let original_receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("record typed contribution under first Run");
    let original_link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("load original producer proof");

    let terminal = scenario.budget_exceeded_owner_run(&run).await;
    scenario
        .vault()
        .close_conversation_recorder(open.fence.clone())
        .await
        .expect("close first producer after owner Run ends");
    let (continued, _) = scenario
        .admit_owner_run_reusing_input(&terminal, "continue against retained input")
        .await;
    let continued_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&continued, input.receipt.transcript))
        .await
        .expect("open new recorder for the same owner input");
    let mut replay_request = request.clone();
    replay_request.recorder = continued_open.fence;
    let replay = compose_typed_recording(scenario.vault(), replay_request)
        .await
        .expect("replay the same typed contribution from the Continue recorder");
    assert_eq!(replay, original_receipt);
    assert_eq!(replay.recorder.run_id, run.run_id);
    assert_eq!(replay.recorder.run_id, original_link.first_recording_run_id);
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id
        )
        .await,
        Some(original_link),
        "re-recorded replay keeps original link and producer identity"
    );
}

#[tokio::test]
async fn delegated_contribution_replay_after_continue_preserves_original_task_proof() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("Delegation producer replay")
        .await;
    let original_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open original Delegation recorder");
    let task_generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task evidence owner")
        .executor_generation;
    let original_task_receipt = scenario
        .terminal_task_receipts(&run, task_generation, true)
        .await
        .0
        .into_iter()
        .next()
        .expect("first owner Run Task receipt");
    let original_task = scenario
        .vault()
        .task(original_task_receipt.execution.task_id)
        .await
        .expect("read original Task")
        .expect("original Task exists");
    let request = TypedConversationRecordingRequest {
        recorder: original_open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: original_task.snapshot,
            execution_receipt: Some(original_task_receipt.clone()),
        },
    };
    let original_receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("record Delegation with the original owner Task proof");
    let original_link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("read immutable Delegation link");
    assert_eq!(original_link.first_recording_run_id, run.run_id);
    assert_eq!(
        original_link.original_task_receipt,
        Some(original_task_receipt.clone())
    );

    let terminal = scenario.budget_exceeded_owner_run(&run).await;
    scenario
        .vault()
        .close_conversation_recorder(original_open.fence)
        .await
        .expect("close original recorder after terminal Run");
    let (continued, _) = scenario
        .admit_owner_run_reusing_input(&terminal, "Continue on the retained input")
        .await;
    let continued_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&continued, input.receipt.transcript))
        .await
        .expect("open Continue recorder for the same owner input");
    let current_task_receipt = scenario
        .terminal_task_receipts(&continued, task_generation, true)
        .await
        .0
        .into_iter()
        .next()
        .expect("current recorder Task receipt");
    let current_task = scenario
        .vault()
        .task(current_task_receipt.execution.task_id)
        .await
        .expect("read current recorder Task")
        .expect("current recorder Task exists");

    let before_counts = {
        let connection = scenario
            .vault()
            .connection()
            .expect("connect before replay");
        (
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            table_count(&connection, "agent_conversation_core_output_receipts_v2").await,
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1",
            )
            .await,
            table_count(&connection, "agent_conversation_typed_history_entries").await,
        )
    };

    let substituted_current_task = TypedConversationRecordingRequest {
        recorder: continued_open.fence.clone(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: continued.run_id.as_uuid(),
            task: current_task.snapshot,
            execution_receipt: Some(current_task_receipt),
        },
        ..request.clone()
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), substituted_current_task).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a new Task produced by the Continue Run cannot replace the original contribution proof"
    );
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id,
        )
        .await,
        Some(original_link.clone()),
        "rejection leaves the original immutable Task link intact"
    );

    let mut replay_request = request.clone();
    replay_request.recorder = continued_open.fence;
    let replay_receipt = compose_typed_recording(scenario.vault(), replay_request)
        .await
        .expect("Continue replay revalidates the original Task under its producer Run");
    assert_eq!(replay_receipt, original_receipt);
    assert_eq!(replay_receipt.recorder.run_id, run.run_id);
    assert_eq!(
        replay_receipt.transcript,
        original_link.transcript_entry.reference
    );
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id,
        )
        .await,
        Some(original_link.clone()),
        "accepted replay preserves the first producer and original Task receipt"
    );

    let boundary = owner_read_boundary(&scenario).await;
    let owner_resolved = scenario
        .vault()
        .read_owner_transcript_entry(
            owner_read_target(&scenario),
            scenario.session_id,
            boundary,
            TranscriptEntryLookup::Reference(original_link.transcript_entry.reference),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("owner read resolves the original Task proof after Continue");
    assert_eq!(owner_resolved.original_owner_run_id, run.run_id);
    assert_eq!(
        owner_resolved.transcript_entry.producer_run,
        Some(run.run_id)
    );
    match &owner_resolved.typed_evidence {
        OwnerTranscriptTypedEvidence::Present {
            message,
            first_recording_run_id,
            original_task_receipt: Some(actual_task_receipt),
            ..
        } => {
            assert_eq!(message, &request.typed_message);
            assert_eq!(*first_recording_run_id, run.run_id);
            assert_eq!(actual_task_receipt, &original_task_receipt);
        }
        OwnerTranscriptTypedEvidence::Present {
            original_task_receipt: None,
            ..
        }
        | OwnerTranscriptTypedEvidence::Absent => {
            panic!("admitted Delegation retains its original exact Task evidence")
        }
    }

    let stored_payload = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to retained typed payload");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("start typed payload read");
        let resolved = scenario
            .vault()
            .resolve_typed_agent_message_on(
                &transaction,
                &original_link.typed_reference,
                floe_conversation::MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .expect("resolve unchanged original typed payload");
        transaction
            .commit()
            .await
            .expect("commit typed payload read");
        resolved.message
    };
    assert_eq!(stored_payload, request.typed_message);
    let after_counts = {
        let connection = scenario.vault().connection().expect("connect after replay");
        (
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            table_count(&connection, "agent_conversation_core_output_receipts_v2").await,
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1",
            )
            .await,
            table_count(&connection, "agent_conversation_typed_history_entries").await,
        )
    };
    assert_eq!(
        after_counts, before_counts,
        "replay creates no second contribution"
    );
}

#[tokio::test]
async fn owner_exact_input_read_returns_explicit_absent_and_checks_owner_session() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("owner input has no typed Session row")
        .await;
    let boundary = owner_read_boundary(&scenario).await;
    let target = owner_read_target(&scenario);
    let resolved = scenario
        .vault()
        .read_owner_transcript_entry(
            target.clone(),
            scenario.session_id,
            boundary.clone(),
            TranscriptEntryLookup::Reference(input.receipt.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("resolve exact owner input mapping without inventing typed evidence");
    assert_eq!(
        resolved.typed_evidence,
        OwnerTranscriptTypedEvidence::Absent
    );
    assert_eq!(resolved.owner_input, input.receipt.transcript);
    assert_eq!(resolved.session_id, scenario.session_id);
    assert_eq!(resolved.owner_identity, scenario.identity);
    assert_eq!(resolved.original_owner_run_id, run.run_id);
    assert_ne!(
        resolved.owner_user_message_id,
        input.receipt.transcript.message_id.as_uuid(),
        "owner and Core identifiers remain distinct persisted values"
    );
    assert_eq!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                target.clone(),
                Uuid::new_v4(),
                boundary.clone(),
                TranscriptEntryLookup::Reference(input.receipt.transcript),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a different Session cannot consume the exact mapping"
    );

    let mut foreign_person = target.clone();
    foreign_person.identity.person_id = PersonId::new();
    assert_eq!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                foreign_person,
                scenario.session_id,
                boundary.clone(),
                TranscriptEntryLookup::Reference(input.receipt.transcript),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::AgentMismatch
        )),
    );
    let mut foreign_identity = target;
    foreign_identity.identity.definition_revision += 1;
    assert!(matches!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                foreign_identity,
                scenario.session_id,
                boundary,
                TranscriptEntryLookup::Reference(input.receipt.transcript),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::InvalidTranscriptBoundary)
    ));
}

#[tokio::test]
async fn owner_reverse_page_counts_cumulative_envelopes_and_preserves_cursor() {
    let mut scenario = Scenario::new().await;
    let (older_run, older_input) = scenario
        .admit_run_and_append_input("page owner history one")
        .await;
    let older_open = scenario
        .vault()
        .open_conversation_recorder(
            scenario.start_request(&older_run, older_input.receipt.transcript),
        )
        .await
        .expect("open first page fixture recorder");
    let older_request = typed_assistant_request(older_open.fence, "typed answer one");
    let older_receipt = compose_typed_recording(scenario.vault(), older_request.clone())
        .await
        .expect("record first page output");
    scenario.complete_owner_run(&older_run).await;
    let (newer_run, newer_input) = scenario
        .admit_run_and_append_input("page owner history two")
        .await;
    let newer_open = scenario
        .vault()
        .open_conversation_recorder(
            scenario.start_request(&newer_run, newer_input.receipt.transcript),
        )
        .await
        .expect("open second page fixture recorder");
    let newer_request = typed_assistant_request(newer_open.fence, "typed answer two");
    let newer_receipt = compose_typed_recording(scenario.vault(), newer_request.clone())
        .await
        .expect("record second page output");
    remove_owner_coverage(&scenario, older_run.run_id.as_uuid()).await;
    remove_owner_coverage(&scenario, newer_run.run_id.as_uuid()).await;
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to normalized learning history");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("begin normalized learning history read");
    let learning_history = scenario
        .vault()
        .read_manager_learning_history_on(&transaction, scenario.session_id, 256, 512 * 1024)
        .await
        .expect("read bounded Manager learning suffix from normalized history");
    transaction
        .commit()
        .await
        .expect("commit normalized learning history read");
    assert_eq!(learning_history.len(), 4);
    assert!(matches!(
        learning_history
            .iter()
            .map(|item| &item.message)
            .collect::<Vec<_>>()
            .as_slice(),
        [
            floe_conversation::AgentMessage::User { .. },
            floe_conversation::AgentMessage::Assistant { .. },
            floe_conversation::AgentMessage::User { .. },
            floe_conversation::AgentMessage::Assistant { .. },
        ]
    ));
    let boundary = owner_read_boundary(&scenario).await;
    let target = owner_read_target(&scenario);
    let older = scenario
        .vault()
        .read_owner_transcript_entry(
            target.clone(),
            scenario.session_id,
            boundary.clone(),
            TranscriptEntryLookup::Reference(older_receipt.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("compute exact older entry cost");
    let older_input_page = scenario
        .vault()
        .read_owner_transcript_entry(
            target.clone(),
            scenario.session_id,
            boundary.clone(),
            TranscriptEntryLookup::Reference(older_input.receipt.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("compute exact first User input cost");
    let newer_input_page = scenario
        .vault()
        .read_owner_transcript_entry(
            target.clone(),
            scenario.session_id,
            boundary.clone(),
            TranscriptEntryLookup::Reference(newer_input.receipt.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("compute exact second User input cost");
    let newer = scenario
        .vault()
        .read_owner_transcript_entry(
            target.clone(),
            scenario.session_id,
            boundary.clone(),
            TranscriptEntryLookup::Reference(newer_receipt.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await
        .expect("compute exact newer entry cost");
    assert_eq!(typed_message(&older), &older_request.typed_message);
    match &older.typed_evidence {
        OwnerTranscriptTypedEvidence::Present {
            message,
            coverage,
            first_recording_run_id,
            original_task_receipt,
            ..
        } => {
            assert_eq!(message, &older_request.typed_message);
            assert_eq!(coverage, &DependencyCoverage::Unknown);
            assert_eq!(*first_recording_run_id, older_run.run_id);
            assert_eq!(*original_task_receipt, None);
        }
        OwnerTranscriptTypedEvidence::Absent => panic!("generated output has typed evidence"),
    }
    let exact_total = older_input_page.encoded_bytes
        + older.encoded_bytes
        + newer_input_page.encoded_bytes
        + newer.encoded_bytes;
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    let exact_fit = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target.clone(),
            scenario.session_id,
            TranscriptReadCursor::start(boundary.clone()),
            OwnerTranscriptPageBudget {
                max_entries: 4,
                max_bytes: exact_total,
            },
        )
        .await
        .expect("exact cumulative byte fit includes both generated outputs");
    assert_eq!(exact_fit.encoded_bytes, exact_total);
    assert_eq!(exact_fit.entries.len(), 4);
    assert!(
        !exact_fit.has_more,
        "the exact page contains the whole two-turn prefix"
    );
    assert_eq!(
        exact_fit.entries[0].transcript_entry.reference,
        older_input.receipt.transcript
    );
    assert_eq!(
        exact_fit.entries[1].transcript_entry.reference,
        older_receipt.transcript
    );
    assert_eq!(
        exact_fit.entries[2].transcript_entry.reference,
        newer_input.receipt.transcript
    );
    assert_eq!(
        exact_fit.entries[3].transcript_entry.reference,
        newer_receipt.transcript
    );
    assert_eq!(exact_fit.next_cursor, None);
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        2
    );
    assert_eq!(
        scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "missing coverage remains Unknown without a body SELECT"
    );
    assert!(
        exact_fit
            .entries
            .iter()
            .filter(|entry| { entry.transcript_entry.kind == TranscriptEntryKind::GeneratedOutput })
            .all(|entry| matches!(
                &entry.typed_evidence,
                OwnerTranscriptTypedEvidence::Present {
                    coverage: DependencyCoverage::Unknown,
                    ..
                }
            ))
    );

    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    let one_short = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target.clone(),
            scenario.session_id,
            TranscriptReadCursor::start(boundary.clone()),
            OwnerTranscriptPageBudget {
                max_entries: 4,
                max_bytes: newer_input_page.encoded_bytes
                    + newer.encoded_bytes
                    + older.encoded_bytes
                    - 1,
            },
        )
        .await
        .expect("a later typed record outside the cumulative budget ends the page");
    assert_eq!(
        one_short.encoded_bytes,
        newer_input_page.encoded_bytes + newer.encoded_bytes
    );
    assert_eq!(one_short.entries.len(), 2);
    assert_eq!(
        one_short.entries[0].transcript_entry.reference,
        newer_input.receipt.transcript
    );
    assert_eq!(
        one_short.entries[1].transcript_entry.reference,
        newer_receipt.transcript
    );
    let next_cursor = one_short
        .next_cursor
        .clone()
        .expect("older history remains behind the exact cursor");
    assert_eq!(next_cursor.before, Some(newer_input.receipt.transcript));
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "the later oversized payload was preflighted but not fetched"
    );

    let prior_page = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target.clone(),
            scenario.session_id,
            next_cursor,
            OwnerTranscriptPageBudget {
                max_entries: 4,
                max_bytes: older_input_page.encoded_bytes + older.encoded_bytes,
            },
        )
        .await
        .expect("continue with the exact exclusive cursor without duplicates");
    assert_eq!(prior_page.entries.len(), 2);
    assert_eq!(
        prior_page.entries[0].transcript_entry.reference,
        older_input.receipt.transcript
    );
    assert_eq!(
        prior_page.entries[1].transcript_entry.reference,
        older_receipt.transcript
    );
    assert_eq!(
        prior_page.encoded_bytes,
        older_input_page.encoded_bytes + older.encoded_bytes
    );

    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .read_previous_owner_transcript_page(
                target.clone(),
                scenario.session_id,
                TranscriptReadCursor::start(boundary.clone()),
                OwnerTranscriptPageBudget {
                    max_entries: 2,
                    max_bytes: newer.encoded_bytes - 1,
                },
            )
            .await,
        Err(ConversationStoreFailure::PageItemExceedsBudget),
        "a first record that does not fit returns an error"
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert!(matches!(
        scenario
            .vault()
            .read_previous_owner_transcript_page(
                target.clone(),
                scenario.session_id,
                TranscriptReadCursor::start(boundary.clone()),
                OwnerTranscriptPageBudget {
                    max_entries: 0,
                    max_bytes: 1
                },
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::InvalidInput
        ))
    ));
    assert!(matches!(
        scenario
            .vault()
            .read_previous_owner_transcript_page(
                target.clone(),
                scenario.session_id,
                TranscriptReadCursor::start(boundary),
                OwnerTranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 0
                },
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::InvalidInput
        ))
    ));
}

#[tokio::test]
async fn owner_reverse_cursor_survives_later_append_and_reopen() {
    let mut scenario = Scenario::new().await;
    let (first_run, first_input) = scenario
        .admit_run_and_append_input("first pinned cursor input")
        .await;
    scenario.budget_exceeded_owner_run(&first_run).await;
    let (boundary_run, boundary_input) = scenario
        .admit_run_and_append_input("second pinned cursor input")
        .await;
    let boundary = owner_read_boundary(&scenario).await;
    assert_eq!(
        boundary.head_revision,
        boundary_input.receipt.transcript.sequence
    );
    scenario.budget_exceeded_owner_run(&boundary_run).await;
    let (_, later_input) = scenario
        .admit_run_and_append_input("input appended after pinned head")
        .await;
    let target = owner_read_target(&scenario);
    let first_page = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target.clone(),
            scenario.session_id,
            TranscriptReadCursor::start(boundary.clone()),
            OwnerTranscriptPageBudget {
                max_entries: 1,
                max_bytes: floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            },
        )
        .await
        .expect("page remains pinned after a later append");
    assert_eq!(first_page.entries.len(), 1);
    assert_eq!(
        first_page.entries[0].transcript_entry.reference,
        boundary_input.receipt.transcript
    );
    assert_ne!(
        first_page.entries[0].transcript_entry.reference,
        later_input.receipt.transcript
    );
    let cursor = first_page.next_cursor.expect("older inbound entry remains");
    scenario.reopen().await;
    let replayed = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target.clone(),
            scenario.session_id,
            TranscriptReadCursor::start(boundary.clone()),
            OwnerTranscriptPageBudget {
                max_entries: 1,
                max_bytes: floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            },
        )
        .await
        .expect("replaying pinned page after reopen returns the same first page");
    assert_eq!(replayed.entries, first_page.entries);
    assert_eq!(replayed.next_cursor, Some(cursor.clone()));
    let previous_page = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target,
            scenario.session_id,
            cursor,
            OwnerTranscriptPageBudget {
                max_entries: 2,
                max_bytes: floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            },
        )
        .await
        .expect("exclusive-before cursor returns the older pinned inbound entry");
    assert_eq!(previous_page.entries.len(), 1);
    assert_eq!(
        previous_page.entries[0].transcript_entry.reference,
        first_input.receipt.transcript
    );
    assert_eq!(
        previous_page.entries[0].typed_evidence,
        OwnerTranscriptTypedEvidence::Absent
    );
    assert!(!previous_page.has_more);
    assert!(previous_page.next_cursor.is_none());
}

#[tokio::test]
async fn owner_budget_preflights_large_coverage_and_counts_missing_unknown() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario.admit_run_and_append_input("coverage budget").await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open coverage budget recorder");
    let request = typed_assistant_request(open.fence, "tiny");
    let receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("record small typed payload");
    let coverage = large_owner_coverage(scenario.person_id);
    let coverage_bytes = coverage
        .as_persisted_bytes()
        .expect("encode large valid coverage")
        .len();
    store_owner_coverage(&scenario, run.run_id.as_uuid(), coverage.clone()).await;

    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("read exact typed link");
    let typed_bytes = usize::try_from(link.typed_reference.encoded_byte_length()).unwrap()
        + link.typed_reference.encoded_reference_bytes().unwrap();
    let core_bytes = serde_json::to_vec(&link.transcript_entry).unwrap().len();
    let expected_bytes = core_bytes + typed_bytes + coverage_bytes;
    assert!(
        coverage_bytes > typed_bytes,
        "coverage is larger than typed payload and reference"
    );

    let ordinary_count = std::sync::atomic::AtomicU64::new(0);
    let connection = scenario
        .vault()
        .connection()
        .expect("connect for ordinary single-query coverage regression");
    assert_eq!(
        crate::vault::context_dependencies::read_context_dependency_coverage_counted(
            &connection,
            scenario.person_id,
            scenario.session_id,
            run.run_id.as_uuid(),
            &ordinary_count,
        )
        .await
        .expect("ordinary one-query coverage read"),
        coverage
    );
    assert_eq!(ordinary_count.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(
        scenario
            .vault()
            .read_turn_coverage(scenario.session_id, run.run_id.as_uuid())
            .await
            .expect("learning gate still uses one plain-Connection coverage query"),
        coverage
    );
    drop(connection);

    let boundary = owner_read_boundary(&scenario).await;
    let target = owner_read_target(&scenario);
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .read_previous_owner_transcript_page(
                target.clone(),
                scenario.session_id,
                TranscriptReadCursor::start(boundary.clone()),
                OwnerTranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: expected_bytes - 1,
                },
            )
            .await,
        Err(ConversationStoreFailure::PageItemExceedsBudget),
        "large coverage prevents a small typed payload from slipping over budget"
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "typed payload body was not prefetched before the total budget fit"
    );
    assert_eq!(
        scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "coverage body was not prefetched before the total budget fit"
    );

    let exact_fit = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target,
            scenario.session_id,
            TranscriptReadCursor::start(boundary),
            OwnerTranscriptPageBudget {
                max_entries: 1,
                max_bytes: expected_bytes,
            },
        )
        .await
        .expect("exact total fit hydrates both evidence bodies");
    assert_eq!(exact_fit.encoded_bytes, expected_bytes);
    assert_eq!(
        exact_fit.entries[0].transcript_entry.reference,
        receipt.transcript
    );
    assert_eq!(typed_message(&exact_fit.entries[0]), &request.typed_message);
    assert!(matches!(
        &exact_fit.entries[0].typed_evidence,
        OwnerTranscriptTypedEvidence::Present {
            coverage: actual,
            ..
        } if actual == &coverage
    ));
    assert!(coverage.as_persisted_bytes().unwrap().len() <= coverage_bytes);
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(
        scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn owner_reads_reject_wrong_proof_scopes_and_missing_generated_links() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("proof scope checks")
        .await;
    let request =
        record_typed_answer(&scenario, &run, input.receipt.transcript, "linked output").await;
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("read exact generated link");
    let boundary = owner_read_boundary(&scenario).await;
    let target = owner_read_target(&scenario);

    assert_eq!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                target.clone(),
                Uuid::new_v4(),
                boundary.clone(),
                TranscriptEntryLookup::Reference(link.transcript_entry.reference),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a same-Person but wrong Session cannot reuse the link"
    );
    let mut wrong_person = target.clone();
    wrong_person.identity.person_id = PersonId::new();
    assert_eq!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                wrong_person,
                scenario.session_id,
                boundary.clone(),
                TranscriptEntryLookup::Reference(link.transcript_entry.reference),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::AgentMismatch
        ))
    );

    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to corrupt generated entry kind");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start wrong-kind proof fixture");
    transaction
        .execute(
            "UPDATE agent_conversation_owner_transcript_evidence_v1 SET transcript_entry_json = replace(transcript_entry_json, '\"kind\":\"generated_output\"', '\"kind\":\"inbound\"') WHERE person_id = ? AND contribution_id = ?",
            (scenario.person_id.to_string(), request.contribution_id.as_uuid().to_string()),
        )
        .await
        .expect("change the persisted link's generated kind");
    transaction
        .commit()
        .await
        .expect("commit wrong-kind proof fixture");
    assert_eq!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                target.clone(),
                scenario.session_id,
                boundary.clone(),
                TranscriptEntryLookup::Reference(link.transcript_entry.reference),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Unavailable),
        "a generated owner link cannot be relabeled as an inbound record"
    );

    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to remove exact proof");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start exact proof deletion fixture");
    transaction
        .execute(
            "DELETE FROM agent_conversation_owner_transcript_evidence_v1 WHERE person_id = ? AND contribution_id = ?",
            (scenario.person_id.to_string(), request.contribution_id.as_uuid().to_string()),
        )
        .await
        .expect("remove generated entry's exact typed link");
    transaction
        .commit()
        .await
        .expect("commit missing-link fixture");
    assert_eq!(
        scenario
            .vault()
            .read_owner_transcript_entry(
                target,
                scenario.session_id,
                boundary,
                TranscriptEntryLookup::Reference(link.transcript_entry.reference),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a generated Core text projection is not accepted without its exact typed link"
    );
}

#[tokio::test]
async fn owner_read_metadata_body_and_coverage_corruption_are_errors() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("metadata integrity")
        .await;
    let request = record_typed_answer(
        &scenario,
        &run,
        input.receipt.transcript,
        "body corrupt probe",
    )
    .await;
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("read exact typed proof");
    let boundary = owner_read_boundary(&scenario).await;
    let target = owner_read_target(&scenario);
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to corrupt typed metadata");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start typed metadata corruption fixture");
    transaction
        .execute(
            "UPDATE agent_conversation_typed_history_entries SET turn_id = ? WHERE person_id = ? AND session_id = ? AND entry_id = ?",
            (Uuid::new_v4().to_string(), scenario.person_id.to_string(), scenario.session_id.to_string(), link.typed_reference.entry_id().to_string()),
        )
        .await
        .expect("corrupt typed turn metadata");
    transaction
        .commit()
        .await
        .expect("commit typed metadata corruption");
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .read_previous_owner_transcript_page(
                target,
                scenario.session_id,
                TranscriptReadCursor::start(boundary),
                OwnerTranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 1
                },
            )
            .await,
        Err(ConversationStoreFailure::Unavailable),
        "malformed typed metadata is an error rather than a byte-boundary stop"
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    let mut body_scenario = Scenario::new().await;
    let (body_run, body_input) = body_scenario
        .admit_run_and_append_input("typed body corruption")
        .await;
    let body_request = record_typed_answer(
        &body_scenario,
        &body_run,
        body_input.receipt.transcript,
        "body corrupt probe",
    )
    .await;
    let body_link = stored_typed_link(
        body_scenario.vault(),
        body_scenario.person_id,
        body_request.contribution_id,
    )
    .await
    .expect("load body corruption reference");
    let mut connection = body_scenario
        .vault()
        .connection()
        .expect("connect to corrupt typed body");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start typed body corruption fixture");
    let mut rows = transaction
        .query(
            "SELECT payload FROM agent_conversation_typed_history_entries WHERE person_id = ? AND session_id = ? AND entry_id = ?",
            (body_scenario.person_id.to_string(), body_scenario.session_id.to_string(), body_link.typed_reference.entry_id().to_string()),
        )
        .await
        .expect("read payload for same-length corruption");
    let payload = rows
        .next()
        .await
        .expect("read typed payload row")
        .expect("typed payload exists")
        .get::<String>(0)
        .expect("typed payload text");
    let corrupted_payload = payload.replace("body corrupt probe", "Body corrupt probe");
    assert_ne!(corrupted_payload, payload);
    assert_eq!(corrupted_payload.len(), payload.len());
    drop(rows);
    transaction
        .execute(
            "UPDATE agent_conversation_typed_history_entries SET payload = ? WHERE person_id = ? AND session_id = ? AND entry_id = ?",
            (corrupted_payload, body_scenario.person_id.to_string(), body_scenario.session_id.to_string(), body_link.typed_reference.entry_id().to_string()),
        )
        .await
        .expect("corrupt typed body without changing its length");
    transaction
        .commit()
        .await
        .expect("commit typed body corruption");
    body_scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        body_scenario
            .vault()
            .read_owner_transcript_entry(
                owner_read_target(&body_scenario),
                body_scenario.session_id,
                owner_read_boundary(&body_scenario).await,
                TranscriptEntryLookup::Reference(body_link.transcript_entry.reference),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Unavailable),
        "the payload digest is checked after the body fetch"
    );
    assert_eq!(
        body_scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    let mut coverage_scenario = Scenario::new().await;
    let (coverage_run, coverage_input) = coverage_scenario
        .admit_run_and_append_input("coverage body corruption")
        .await;
    let coverage_request = record_typed_answer(
        &coverage_scenario,
        &coverage_run,
        coverage_input.receipt.transcript,
        "coverage body",
    )
    .await;
    store_owner_coverage(
        &coverage_scenario,
        coverage_run.run_id.as_uuid(),
        DependencyCoverage::Unknown,
    )
    .await;
    let coverage_link = stored_typed_link(
        coverage_scenario.vault(),
        coverage_scenario.person_id,
        coverage_request.contribution_id,
    )
    .await
    .expect("load coverage-corruption typed link");
    let mut connection = coverage_scenario
        .vault()
        .connection()
        .expect("connect to corrupt coverage body");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start coverage body corruption fixture");
    transaction
        .execute(
            "UPDATE agent_context_dependency_coverage SET payload = 'x' WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (coverage_scenario.person_id.to_string(), coverage_scenario.session_id.to_string(), coverage_run.run_id.as_uuid().to_string()),
        )
        .await
        .expect("corrupt coverage payload body");
    transaction
        .commit()
        .await
        .expect("commit coverage body corruption");
    coverage_scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    coverage_scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        coverage_scenario
            .vault()
            .read_owner_transcript_entry(
                owner_read_target(&coverage_scenario),
                coverage_scenario.session_id,
                owner_read_boundary(&coverage_scenario).await,
                TranscriptEntryLookup::Reference(coverage_link.transcript_entry.reference),
                floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
            )
            .await,
        Err(ConversationStoreFailure::Unavailable)
    );
    assert_eq!(
        coverage_scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(
        coverage_scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn owner_coverage_metadata_corruption_is_not_a_page_boundary() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("coverage metadata")
        .await;
    let request = record_typed_answer(
        &scenario,
        &run,
        input.receipt.transcript,
        "coverage metadata",
    )
    .await;
    store_owner_coverage(
        &scenario,
        run.run_id.as_uuid(),
        DependencyCoverage::Independent,
    )
    .await;
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("load coverage metadata link");
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to corrupt coverage metadata");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .expect("start coverage metadata corruption fixture");
    transaction
        .execute(
            "UPDATE agent_context_dependency_coverage SET payload = '' WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (scenario.person_id.to_string(), scenario.session_id.to_string(), run.run_id.as_uuid().to_string()),
        )
        .await
        .expect("corrupt coverage length metadata");
    transaction
        .commit()
        .await
        .expect("commit coverage metadata corruption");
    scenario
        .vault()
        .typed_history_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    scenario
        .vault()
        .context_coverage_payload_hydrations
        .store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        scenario
            .vault()
            .read_previous_owner_transcript_page(
                owner_read_target(&scenario),
                scenario.session_id,
                TranscriptReadCursor::start(owner_read_boundary(&scenario).await),
                OwnerTranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 1
                },
            )
            .await,
        Err(ConversationStoreFailure::Unavailable),
        "invalid coverage length metadata is reported even when the item cannot fit"
    );
    assert_eq!(
        scenario
            .vault()
            .typed_history_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        scenario
            .vault()
            .context_coverage_payload_hydrations
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    let _ = link;
}

#[tokio::test]
async fn owner_transcript_read_does_not_mutate_schema() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario.admit_run_and_append_input("schema freeze").await;
    let _request =
        record_typed_answer(&scenario, &run, input.receipt.transcript, "schema read").await;
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect before owner read");
    let before = schema_tables(&connection).await;
    let result = scenario
        .vault()
        .read_owner_transcript_entry(
            owner_read_target(&scenario),
            scenario.session_id,
            owner_read_boundary(&scenario).await,
            TranscriptEntryLookup::Reference(input.receipt.transcript),
            floe_conversation_core::MAX_TRANSCRIPT_PAGE_BYTES,
        )
        .await;
    assert!(result.is_ok());
    let after = schema_tables(&connection).await;
    assert_eq!(
        after, before,
        "read-only composition performs no schema DDL"
    );
}

#[tokio::test]
async fn owner_read_on_absent_core_family_does_not_initialize_it() {
    let mut scenario = Scenario::new().await;
    let target = owner_read_target(&scenario);
    let boundary = TranscriptReadBoundary {
        target: target.clone(),
        head_revision: 0,
        through: None,
    };
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before absent-family read");
    let before = schema_tables(&connection).await;
    let result = scenario
        .vault()
        .read_previous_owner_transcript_page(
            target,
            scenario.session_id,
            TranscriptReadCursor::start(boundary),
            OwnerTranscriptPageBudget {
                max_entries: 1,
                max_bytes: 1,
            },
        )
        .await;
    assert!(result.is_err());
    assert_eq!(schema_tables(&connection).await, before);
}

#[tokio::test]
async fn existing_composed_admission_without_open_receipt_fails_closed_without_repair() {
    let mut scenario = Scenario::new().await;
    let text = "do not repair missing recorder custody";
    let owner_request = scenario.owner_request(text);
    let core_request = scenario.core_input_request(text);
    let run_id = owner_request.run_id;
    let (record, input) = scenario
        .admit_run_and_bind_input(owner_request.clone(), core_request.clone())
        .await;
    assert_eq!(record.state, RunState::Working);
    assert!(scenario.last_core_recorder.is_some());

    let mut connection = scenario.vault().connection().expect("connect to Vault");
    let (mut guard, transaction) = scenario
        .vault()
        .journal_transaction(&mut connection)
        .await
        .expect("start fixture transaction to model missing committed open evidence");
    assert_eq!(
        transaction
            .execute(
                "DELETE FROM agent_conversation_core_v3_active_recorders WHERE person_id = ? AND run_id = ?",
                (scenario.person_id.to_string(), run_id.as_uuid().to_string()),
            )
            .await
            .expect("remove active index for missing-receipt fixture"),
        1
    );
    assert_eq!(
        transaction
            .execute(
                "DELETE FROM agent_conversation_core_v3_open_receipts WHERE person_id = ? AND run_id = ?",
                (scenario.person_id.to_string(), run_id.as_uuid().to_string()),
            )
            .await
            .expect("remove immutable open receipt for partial-evidence fixture"),
        1
    );
    transaction
        .commit()
        .await
        .expect("commit missing-open evidence fixture");
    guard.settled();
    drop(guard);
    drop(connection);

    scenario.reopen().await;
    let before = composed_admission_snapshot(&scenario, run_id, input.receipt.transcript).await;
    assert_eq!(
        before.run.as_ref().map(|run| run.state),
        Some(RunState::Working)
    );
    assert_eq!(
        before.head.as_ref().map(|(head, _)| head.recorder_epoch),
        Some(1),
        "the still-Working same-generation Run had an open epoch before its receipt was lost"
    );
    assert!(before.binding.is_some());
    assert!(before.mapping_by_run.is_some());
    assert!(before.mapping_by_reference.is_some());
    assert!(before.input_receipt.is_some());
    assert!(before.input_entry.is_some());
    assert!(before.open_receipt.is_none());
    assert!(before.active_recorder.is_none());

    assert_eq!(
        compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(owner_request),
            core_request,
        )
        .await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "an exact owner replay cannot retrofit a missing immutable recorder receipt"
    );
    scenario.reopen().await;
    let after = composed_admission_snapshot(&scenario, run_id, input.receipt.transcript).await;
    assert_eq!(
        after, before,
        "missing receipt replay leaves all custody unchanged"
    );
}

#[tokio::test]
async fn concurrent_composed_new_admissions_on_one_transcript_commit_at_most_one() {
    let scenario = Scenario::new().await;
    let second_session = match scenario
        .vault()
        .start_conversation_session(StartSessionRequest {
            principal: scenario.person_id.to_string(),
            command_id: CommandId::new(),
        })
        .await
        .expect("start second independent owner Session")
    {
        SessionStartAdmission::Started(receipt) => receipt,
        other => panic!("unexpected second Session admission: {other:?}"),
    };
    assert_ne!(scenario.session_id, second_session.session_id);
    let first_owner = scenario.owner_request_for_session(
        scenario.session_id,
        scenario.session_revision,
        "concurrent first Session turn",
    );
    let second_owner = scenario.owner_request_for_session(
        second_session.session_id,
        second_session.session_revision,
        "concurrent second Session turn",
    );
    let first_core = scenario.core_input_request("concurrent first Session turn");
    let second_core = scenario.core_input_request("concurrent second Session turn");
    let first_run_id = first_owner.run_id;
    let second_run_id = second_owner.run_id;
    let first_message_id = first_core.message.message_id;
    let second_message_id = second_core.message.message_id;
    let first_session_before = session_storage_snapshot(
        &scenario.vault().connection().expect("connect to Vault"),
        scenario.session_id,
    )
    .await;
    let second_session_before = session_storage_snapshot(
        &scenario.vault().connection().expect("connect to Vault"),
        second_session.session_id,
    )
    .await;
    let start = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let first_gate = start.clone();
    let second_gate = start.clone();
    let vault = scenario.vault();
    let first_intent = CoreComposedOwnerIntent::Turn(first_owner.clone());
    let second_intent = CoreComposedOwnerIntent::Turn(second_owner.clone());
    let (first, second) = tokio::join!(
        async {
            first_gate.wait().await;
            compose_owner_core_run(vault, first_intent.clone(), first_core.clone()).await
        },
        async {
            second_gate.wait().await;
            compose_owner_core_run(vault, second_intent.clone(), second_core.clone()).await
        },
    );
    let first = retry_composed_admission_after_busy(vault, first_intent, first_core, first).await;
    let second =
        retry_composed_admission_after_busy(vault, second_intent, second_core, second).await;

    let (winner, loser_run_id, loser_message_id, loser_session_id, loser_session_before) =
        match (first, second) {
            (
                Ok(CoreComposedRunAdmission::Admitted {
                    record,
                    input,
                    recorder,
                }),
                Err(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationConflict,
                )),
            ) => (
                (record, input, recorder),
                second_run_id,
                second_message_id,
                second_session.session_id,
                second_session_before,
            ),
            (
                Err(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationConflict,
                )),
                Ok(CoreComposedRunAdmission::Admitted {
                    record,
                    input,
                    recorder,
                }),
            ) => (
                (record, input, recorder),
                first_run_id,
                first_message_id,
                scenario.session_id,
                first_session_before,
            ),
            other => panic!("concurrent New admissions did not resolve to one commit: {other:?}"),
        };
    let (winner_record, winner_input, winner_recorder) = winner;
    assert_eq!(winner_record.state, RunState::Working);
    assert_eq!(winner_recorder.fence.run_id, winner_record.run_id);
    assert_eq!(winner_recorder.fence.input, winner_input.receipt.transcript);

    assert_eq!(
        scenario
            .vault()
            .conversation_run(loser_run_id)
            .await
            .expect("check losing Run after concurrent admission"),
        None
    );
    let mut connection = scenario.vault().connection().expect("connect after race");
    assert_eq!(table_count(&connection, "agent_conversation_runs").await, 1);
    assert_eq!(table_count(&connection, "agent_sessions").await, 2);
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_heads").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_input_receipts").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_open_receipts").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_active_recorders").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
        1
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_run_inputs_v1"
        )
        .await,
        1
    );
    assert_eq!(
        session_storage_snapshot(&connection, loser_session_id).await,
        loser_session_before,
        "losing Session state rolls back with its partial owner/Core writes"
    );

    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
        .await
        .expect("start loser-evidence read transaction");
    let scope = Scope::from_identity(
        scenario.person_id,
        &scenario.identity,
        scenario.conversation_id,
        scenario.branch_id,
    );
    assert!(
        owner_input_binding_on(&transaction, scenario.person_id, loser_run_id)
            .await
            .expect("check losing binding rollback")
            .is_none()
    );
    assert!(
        owner_transcript_input_for_run_on(&transaction, scenario.person_id, loser_run_id)
            .await
            .expect("check losing mapping rollback")
            .is_none()
    );
    assert!(
        input_receipt_on(&transaction, scope, loser_message_id)
            .await
            .expect("check losing Core input rollback")
            .is_none()
    );
    assert!(
        open_receipt_on(&transaction, scenario.person_id, loser_run_id)
            .await
            .expect("check losing open receipt rollback")
            .is_none()
    );
    assert_eq!(
        active_recorder_on(&transaction, scope)
            .await
            .expect("read single winning active recorder"),
        Some(winner_recorder.fence.clone())
    );
    transaction
        .commit()
        .await
        .expect("finish loser-evidence read transaction");
}

#[tokio::test]
async fn expert_task_runs_continue_the_pinned_assignment_conversation() {
    use floe_agent_contract::ModelConversationEntry;

    let mut scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate the Task owner");
    let assignment_id = Uuid::new_v4();
    let (first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "first delegated request",
    );
    let first_conversation_key = first_draft.key.clone();
    let first_replay_draft = first_draft.clone();
    let first = match scenario
        .vault()
        .admit_task(first, first_draft)
        .await
        .expect("admit one fresh Task with a pinned Expert history head")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let first_working = start_expert_task(&scenario, &first).await;
    let first_view = scenario
        .vault()
        .expert_task_conversation(first_working.execution())
        .await
        .expect("hydrate the empty pinned Expert history");
    assert!(first_view.conversation.history.is_empty());
    assert!(matches!(
        first_view.conversation.current_turn.as_slice(),
        [ModelConversationEntry::User { text, .. }] if text == "first delegated request"
    ));

    append_scripted_expert_answer(&scenario, &first_working, "first answer", false)
        .await
        .expect("compose Task output, journal and typed/Core contribution");
    let working = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("read the exact Working Task")
        .expect("Task remains present");
    let terminal = TaskSnapshot {
        state: TaskState::Completed,
        result: Some("first answer".into()),
        artifacts: vec![],
        coverage: DependencyCoverage::Independent,
        issue: None,
        blockage: None,
        ..working.snapshot.clone()
    };
    let terminal_commit = TaskExecutionCommit {
        execution: working.execution(),
        expected_task_revision: working.aggregate_revision,
        expected_journal_revision: working.journal_revision,
        terminal,
        settlement: None,
    };
    scenario
        .vault()
        .registry_transaction_ack_loss
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        scenario
            .vault()
            .settle_task_execution(terminal_commit.clone())
            .await,
        Err(AgentFailure::StorageUnavailable),
        "the terminal Task receipt and Core close commit before their acknowledgement is lost"
    );
    scenario.reopen().await;
    let recovered_generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("advance the durable Task generation after terminal acknowledgement loss");
    let first_receipt = scenario
        .vault()
        .settle_task_execution(terminal_commit)
        .await
        .expect("terminal replay returns the immutable committed receipt");
    let first_journal = scenario
        .vault()
        .load_task_journal(first.execution())
        .await
        .expect("read terminal Task journal after generation fence");
    let output_event = first_journal
        .iter()
        .find_map(|entry| match &entry.event {
            JournalEvent::Output { .. } => Some(entry.event.clone()),
            _ => None,
        })
        .expect("first Task has its committed output event");
    let output_revision = first_journal
        .iter()
        .find(|entry| matches!(&entry.event, JournalEvent::Output { .. }))
        .expect("find committed output revision")
        .revision;
    assert_eq!(
        scenario
            .vault()
            .append_task_journal(first.execution(), "output", output_event.clone())
            .await
            .expect("exact output replay verifies its Core and typed proof after fence"),
        output_revision
    );
    let JournalEvent::Output { text, artifacts } = output_event else {
        unreachable!()
    };
    assert_eq!(
        scenario
            .vault()
            .append_task_journal(
                first.execution(),
                "output",
                JournalEvent::Output {
                    text: format!("{text} altered"),
                    artifacts,
                },
            )
            .await,
        Err(AgentFailure::Conflict),
        "an event identity cannot be rebound to a different terminal payload"
    );

    let (mut second, mut second_draft) = expert_task_fixture(
        scenario.person_id,
        recovered_generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "second delegated request",
    );
    second.admission = first.admission.clone();
    second_draft.key = first_conversation_key;
    let second = match scenario
        .vault()
        .admit_task(second, second_draft)
        .await
        .expect("admit a later Task for the same assignment")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected continuation admission: {other:?}"),
    };
    let second_working = start_expert_task(&scenario, &second).await;
    let second_view = scenario
        .vault()
        .expert_task_conversation(second_working.execution())
        .await
        .expect("resolve the second Task's exact pinned prefix");
    assert_eq!(second_view.conversation.history.len(), 2);
    assert!(matches!(
        &second_view.conversation.history[0],
        ModelConversationEntry::User { text, .. } if text == "first delegated request"
    ));
    assert!(matches!(
        &second_view.conversation.history[1],
        ModelConversationEntry::Assistant { text, .. } if text == "first answer"
    ));
    assert!(matches!(
        second_view.conversation.current_turn.as_slice(),
        [ModelConversationEntry::User { text, .. }] if text == "second delegated request"
    ));
    let replayed_first = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("replay exact Task after the conversation head advanced")
        .expect("first Task remains immutable");
    assert_eq!(replayed_first.receipt.as_ref(), Some(&first_receipt));
    assert_eq!(first_receipt.reference.execution, first.execution());
    let exact_replay = scenario
        .vault()
        .admit_task(first.clone(), first_replay_draft)
        .await
        .expect("exact Task replay remains available after its head advances");
    let floe_experts::TaskAdmission::Existing {
        record: exact_replay,
        ..
    } = exact_replay
    else {
        panic!("exact Task replay cannot create or repin a second execution")
    };
    assert_eq!(exact_replay.receipt.as_ref(), Some(&first_receipt));
    let connection = scenario
        .vault()
        .connection()
        .expect("inspect immutable first Task history pin");
    let mut rows = connection
        .query(
            "SELECT input_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ?",
            (
                scenario.person_id.to_string(),
                first.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("read exact persisted history pin");
    let input: floe_experts::ExpertTaskConversationInput = serde_json::from_str(
        &rows
            .next()
            .await
            .expect("read pinned history row")
            .expect("first Task mapping remains present")
            .get::<String>(0)
            .expect("read canonical Task input"),
    )
    .expect("decode canonical Task input");
    assert_eq!(input.history_pin.head_revision, 0);
    assert_eq!(input.input_reference.unwrap().sequence, 1);
    drop(rows);
    connection
        .execute(
            "UPDATE agent_expert_task_conversation_entries_v1 SET evidence_json = ?, evidence_bytes = ? WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
            (
                "{",
                floe_agent_contract::MAX_MODEL_CONVERSATION_BYTES as i64,
                scenario.person_id.to_string(),
                first.snapshot.task_id.as_uuid().to_string(),
                first.execution_id.to_string(),
                first.executor_generation as i64,
            ),
        )
        .await
        .expect("make the old typed payload exceed the next request's byte budget");
    assert_eq!(
        scenario
            .vault()
            .expert_task_conversation(second_working.execution())
            .await,
        Err(AgentFailure::StorageUnavailable),
        "SQL rejects incorrect byte metadata before typed payload hydration"
    );
}

#[tokio::test]
async fn expert_admission_receipt_rejects_mutable_input_coverage_and_pin_changes() {
    let mut scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate the Task owner");
    let (proposed, draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        Uuid::new_v4(),
        1,
        "1.0.0",
        "immutable committed host input",
    );
    let draft_for_replay = draft.clone();
    let (record, receipt) = match scenario
        .vault()
        .admit_task(proposed.clone(), draft)
        .await
        .expect("admit Task with immutable owner input evidence")
    {
        floe_experts::TaskAdmission::Created {
            record,
            expert_input,
        } => (record, expert_input),
        other => panic!("unexpected Task admission: {other:?}"),
    };
    assert_eq!(receipt.execution, record.execution());
    assert_eq!(
        scenario
            .vault()
            .expert_task_admission_reference(record.execution())
            .await
            .expect("lost-ACK readback returns the canonical owner receipt"),
        receipt
    );

    let connection = scenario
        .vault()
        .connection()
        .expect("inspect admission evidence");
    let mut rows = connection
        .query(
            "SELECT reference_json FROM agent_expert_task_conversation_admissions_v1 WHERE person_id = ? AND task_id = ?",
            (
                scenario.person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("read immutable owner admission receipt");
    let owner_receipt = rows
        .next()
        .await
        .expect("read admission receipt row")
        .expect("canonical receipt exists")
        .get::<String>(0)
        .expect("read canonical owner evidence");
    drop(rows);
    let mut rows = connection
        .query(
            "SELECT input_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ?",
            (
                scenario.person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("read mutable input projection");
    let input_json = rows
        .next()
        .await
        .expect("read Task input row")
        .expect("Task input exists")
        .get::<String>(0)
        .expect("read input JSON");
    drop(rows);
    let input: floe_experts::ExpertTaskConversationInput =
        serde_json::from_str(&input_json).expect("decode Task input");

    let mut changed_coverage = input.clone();
    changed_coverage.input_coverage = DependencyCoverage::Unknown;
    connection
        .execute(
            "UPDATE agent_expert_task_conversation_runs_v1 SET input_json = ? WHERE person_id = ? AND task_id = ?",
            (
                serde_json::to_string(&changed_coverage).expect("encode changed coverage"),
                scenario.person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("tamper only the mutable input coverage copy");
    assert_eq!(
        scenario
            .vault()
            .compare_and_swap_task(
                record.snapshot.task_id,
                record.aggregate_revision,
                record.executor_generation,
                TaskSnapshot {
                    state: TaskState::Working,
                    ..record.snapshot.clone()
                },
                receipt.clone(),
            )
            .await,
        Err(AgentFailure::StorageUnavailable),
        "tampered coverage cannot activate the exact Task"
    );
    let mut rows = connection
        .query(
            "SELECT reference_json FROM agent_expert_task_conversation_admissions_v1 WHERE person_id = ? AND task_id = ?",
            (
                scenario.person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("verify owner receipt remains unchanged");
    assert_eq!(
        rows.next()
            .await
            .expect("read owner evidence")
            .expect("owner receipt remains present")
            .get::<String>(0)
            .expect("read canonical owner evidence"),
        owner_receipt
    );
    drop(rows);

    connection
        .execute(
            "UPDATE agent_expert_task_conversation_runs_v1 SET input_json = ? WHERE person_id = ? AND task_id = ?",
            (
                input_json,
                scenario.person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("restore the original committed input before the pin regression");
    drop(connection);
    scenario.reopen().await;

    let mut changed_pin = input;
    changed_pin.history_pin.head_revision = 1;
    let connection = scenario
        .vault()
        .connection()
        .expect("tamper the mutable history pin copy");
    connection
        .execute(
            "UPDATE agent_expert_task_conversation_runs_v1 SET input_json = ? WHERE person_id = ? AND task_id = ?",
            (
                serde_json::to_string(&changed_pin).expect("encode changed history pin"),
                scenario.person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
            ),
        )
        .await
        .expect("tamper only the mutable history pin copy");
    assert_eq!(
        scenario
            .vault()
            .expert_task_admission_reference(record.execution())
            .await,
        Err(AgentFailure::StorageUnavailable),
        "the canonical receipt never repins to a changed history head"
    );
    drop(connection);
    scenario.reopen().await;
    assert_eq!(
        scenario
            .vault()
            .admit_task(proposed, draft_for_replay)
            .await,
        Err(AgentFailure::StorageUnavailable),
        "exact Task replay fails closed when its stored history pin changes"
    );
}

#[tokio::test]
async fn expert_history_rejects_typed_coverage_that_disagrees_with_terminal_task() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor");
    let assignment_id = Uuid::new_v4();
    let (first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "source-backed request",
    );
    let conversation_key = first_draft.key.clone();
    let first = match scenario
        .vault()
        .admit_task(first, first_draft.clone())
        .await
        .expect("admit first Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let working = start_expert_task(&scenario, &first).await;
    append_scripted_expert_answer(&scenario, &working, "source-backed answer", false)
        .await
        .expect("record owner-projected answer with independent coverage");
    let current = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("read first Task")
        .expect("first Task exists");
    let terminal = TaskSnapshot {
        state: TaskState::Completed,
        result: Some("source-backed answer".into()),
        artifacts: vec![],
        coverage: DependencyCoverage::Independent,
        issue: None,
        blockage: None,
        ..current.snapshot.clone()
    };
    scenario
        .vault()
        .settle_task_execution(TaskExecutionCommit {
            execution: current.execution(),
            expected_task_revision: current.aggregate_revision,
            expected_journal_revision: current.journal_revision,
            terminal,
            settlement: None,
        })
        .await
        .expect("settle the source-backed Expert Task");

    let (mut second, mut second_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "follow-up request",
    );
    second.admission = first.admission.clone();
    second_draft.key = conversation_key;
    let second = match scenario
        .vault()
        .admit_task(second, second_draft)
        .await
        .expect("admit later Task for same assignment")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let second_working = start_expert_task(&scenario, &second).await;
    let valid = scenario
        .vault()
        .expert_task_conversation(second_working.execution())
        .await
        .expect("valid owner-projected dependent history is readable");
    assert_eq!(valid.history_coverage.len(), 2);
    assert_eq!(valid.history_coverage[1], DependencyCoverage::Independent);

    let connection = scenario
        .vault()
        .connection()
        .expect("tamper the typed projection independently of Task journal");
    let mut rows = connection
        .query(
            "SELECT sequence, evidence_json FROM agent_expert_task_conversation_entries_v1 WHERE person_id = ? AND task_id = ? AND executor_generation = ?",
            (
                scenario.person_id.to_string(),
                first.snapshot.task_id.as_uuid().to_string(),
                first.executor_generation as i64,
            ),
        )
        .await
        .expect("read source-backed typed evidence");
    let row = rows
        .next()
        .await
        .expect("read typed row")
        .expect("source-backed output row exists");
    let sequence = row.get::<i64>(0).expect("read transcript sequence");
    let evidence: floe_experts::ExpertConversationEvidence =
        serde_json::from_str(&row.get::<String>(1).expect("read typed evidence"))
            .expect("decode original typed evidence");
    drop(rows);
    let changed_evidence = match evidence {
        floe_experts::ExpertConversationEvidence::ModelEntry { entry, .. } => {
            floe_experts::ExpertConversationEvidence::ModelEntry {
                entry,
                coverage: DependencyCoverage::Unknown,
            }
        }
        other => panic!("expected model evidence, got {other:?}"),
    };
    let changed_evidence_json =
        serde_json::to_string(&changed_evidence).expect("encode changed coverage");
    let changed_coverage_json =
        serde_json::to_string(&DependencyCoverage::Unknown).expect("encode changed owner coverage");
    connection
        .execute(
            "UPDATE agent_expert_task_conversation_entries_v1 SET evidence_json = ?, evidence_bytes = ?, coverage_json = ? WHERE person_id = ? AND task_id = ? AND sequence = ?",
            (
                changed_evidence_json.clone(),
                changed_evidence_json.len() as i64,
                changed_coverage_json,
                scenario.person_id.to_string(),
                first.snapshot.task_id.as_uuid().to_string(),
                sequence,
            ),
        )
        .await
        .expect("tamper both mutable typed coverage copies");
    assert_eq!(
        scenario
            .vault()
            .expert_task_conversation(second_working.execution())
            .await,
        Err(AgentFailure::StorageUnavailable),
        "matching typed-row copies cannot override the terminal Task's journaled coverage"
    );
}

#[tokio::test]
async fn expert_history_uses_the_longest_newest_suffix_within_the_byte_budget() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor");
    let assignment_id = Uuid::new_v4();
    let large_input = "u".repeat(58_000);
    let large_output = "a".repeat(58_000);
    let (first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        &large_input,
    );
    let key = first_draft.key.clone();
    let first = match scenario
        .vault()
        .admit_task(first, first_draft)
        .await
        .expect("admit large first Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let first_working = start_expert_task(&scenario, &first).await;
    append_scripted_expert_answer(&scenario, &first_working, &large_output, false)
        .await
        .expect("commit large but individually bounded output");
    let current = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("read first Task")
        .expect("first Task exists");
    scenario
        .vault()
        .settle_task_execution(TaskExecutionCommit {
            execution: current.execution(),
            expected_task_revision: current.aggregate_revision,
            expected_journal_revision: current.journal_revision,
            terminal: TaskSnapshot {
                state: TaskState::Completed,
                result: Some(large_output.clone()),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
                blockage: None,
                ..current.snapshot.clone()
            },
            settlement: None,
        })
        .await
        .expect("settle first Task");

    let (mut second, mut second_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "small follow-up",
    );
    second.admission = first.admission.clone();
    second_draft.key = key;
    let second = match scenario
        .vault()
        .admit_task(second, second_draft)
        .await
        .expect("admit follow-up Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let second_working = start_expert_task(&scenario, &second).await;
    let conversation = scenario
        .vault()
        .expert_task_conversation(second_working.execution())
        .await
        .expect("load the maximal newest suffix that fits both bounds");
    assert!(matches!(
        conversation.conversation.history.as_slice(),
        [ModelConversationEntry::Assistant { text, .. }] if text == &large_output
    ));
    assert!(matches!(
        conversation.conversation.current_turn.as_slice(),
        [ModelConversationEntry::User { text, .. }] if text == "small follow-up"
    ));
}

#[tokio::test]
async fn unstarted_expert_task_recovery_releases_only_its_reservation() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate the Task owner");
    let assignment_id = Uuid::new_v4();
    let (first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "never started",
    );
    let first_key = first_draft.key.clone();
    let first = match scenario
        .vault()
        .admit_task(first, first_draft)
        .await
        .expect("reserve the exact assignment conversation")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let (mut competing, mut competing_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "concurrent request",
    );
    competing.admission = first.admission.clone();
    competing_draft.key = first_key.clone();
    assert_eq!(
        scenario
            .vault()
            .admit_task(competing.clone(), competing_draft.clone())
            .await,
        Err(AgentFailure::Conflict),
        "one Submitted or Working Task reserves the assignment's writer slot"
    );

    let recovered = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("fence the previous executor and release its never-started reservation");
    assert_eq!(recovered.interrupted.len(), 1);
    assert_eq!(
        recovered.interrupted[0].snapshot.state,
        TaskState::Interrupted
    );
    assert!(recovered.interrupted[0].receipt.is_some());
    assert_eq!(
        table_count(
            &scenario
                .vault()
                .connection()
                .expect("read untouched Core history"),
            "agent_conversation_core_v3_entries"
        )
        .await,
        0,
        "recovery of Submitted work must not append a Core input or output"
    );

    competing.executor_generation = recovered.executor_generation;
    competing_draft.run_id = floe_kernel::RunId::new();
    competing_draft.input_message_id = MessageId::new();
    competing_draft.input_command_id = CommandId::new();
    let competing = match scenario
        .vault()
        .admit_task(competing, competing_draft)
        .await
        .expect("new Task can reserve the assignment after proven unstarted recovery")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected post-recovery Task admission: {other:?}"),
    };
    let working = start_expert_task(&scenario, &competing).await;
    let view = scenario
        .vault()
        .expert_task_conversation(working.execution())
        .await
        .expect("the new run begins from the unchanged empty prefix");
    assert!(view.conversation.history.is_empty());
    assert!(matches!(
        view.conversation.current_turn.first(),
        Some(ModelConversationEntry::User { text, .. }) if text == "concurrent request"
    ));
    let original = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("read original interrupted Task")
        .expect("original Task stays terminal");
    assert_eq!(original.snapshot.state, TaskState::Interrupted);
}

#[tokio::test]
async fn lost_expert_task_admission_ack_replays_the_stored_pin_before_recovery() {
    let mut scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor");
    let (proposed, draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        Uuid::new_v4(),
        1,
        "1.0.0",
        "admission acknowledgement loss",
    );
    scenario
        .vault()
        .registry_transaction_ack_loss
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        scenario
            .vault()
            .admit_task(proposed.clone(), draft.clone())
            .await,
        Err(AgentFailure::StorageUnavailable),
        "the test fault loses the acknowledgement after admission commits"
    );

    scenario.reopen().await;
    let recovery = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("fence the possibly admitted Submitted execution");
    assert_eq!(recovery.interrupted.len(), 1);
    assert_eq!(
        recovery.interrupted[0].snapshot.state,
        TaskState::Interrupted
    );
    assert_eq!(recovery.interrupted[0].journal_revision, 0);
    let replay = scenario
        .vault()
        .admit_task(proposed, draft)
        .await
        .expect("exact retry reads the stored immutable Task admission");
    let floe_experts::TaskAdmission::Existing { record: replay, .. } = replay else {
        panic!("a lost admission acknowledgement cannot admit a second Task")
    };
    assert_eq!(replay.snapshot.state, TaskState::Interrupted);
    assert_eq!(replay.receipt, recovery.interrupted[0].receipt);
    assert_eq!(
        table_count(
            &scenario.vault().connection().expect("read Core transcript"),
            "agent_conversation_core_v3_entries"
        )
        .await,
        0,
        "unstarted recovery appends no input or output"
    );
}

#[tokio::test]
async fn lost_expert_task_working_ack_fences_before_any_dispatch_or_continuation() {
    let mut scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor");
    let (first, draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        Uuid::new_v4(),
        1,
        "1.0.0",
        "working acknowledgement loss",
    );
    let key = draft.key.clone();
    let admission = first.admission.clone();
    let first = match scenario
        .vault()
        .admit_task(first, draft)
        .await
        .expect("admit the Submitted Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let expert_input = scenario
        .vault()
        .expert_task_admission_reference(first.execution())
        .await
        .expect("read the durable Expert admission reference");
    scenario
        .vault()
        .registry_transaction_ack_loss
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        scenario
            .vault()
            .compare_and_swap_task(
                first.snapshot.task_id,
                first.aggregate_revision,
                first.executor_generation,
                TaskSnapshot {
                    state: TaskState::Working,
                    ..first.snapshot.clone()
                },
                expert_input,
            )
            .await,
        Err(AgentFailure::StorageUnavailable),
        "the Working input/open commits before its acknowledgement is lost"
    );

    scenario.reopen().await;
    let recovery = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("generation fence settles the uncertain Working owner");
    assert_eq!(recovery.interrupted.len(), 1);
    let interrupted = &recovery.interrupted[0];
    assert_eq!(interrupted.snapshot.state, TaskState::Interrupted);
    assert_eq!(interrupted.journal_revision, 0);
    assert_eq!(
        table_count(
            &scenario.vault().connection().expect("read Core transcript"),
            "agent_conversation_core_v3_entries"
        )
        .await,
        1,
        "the acknowledged owner transition contains one delegated input only"
    );

    let (mut later, mut later_draft) = expert_task_fixture(
        scenario.person_id,
        recovery.executor_generation,
        TaskId::new(),
        admission.assignment_id,
        1,
        "1.0.0",
        "later request after fence",
    );
    later.admission = admission;
    later_draft.key = key;
    let later = match scenario
        .vault()
        .admit_task(later, later_draft)
        .await
        .expect("new Task may continue only after the old generation is fenced")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected continuation admission: {other:?}"),
    };
    let later_working = start_expert_task(&scenario, &later).await;
    let conversation = scenario
        .vault()
        .expert_task_conversation(later_working.execution())
        .await
        .expect("hydrate the exact prior Person input after fence settlement");
    assert!(matches!(
        conversation.conversation.history.as_slice(),
        [ModelConversationEntry::User { text, .. }]
            if text == "working acknowledgement loss"
    ));
    assert!(matches!(
        conversation.conversation.current_turn.as_slice(),
        [ModelConversationEntry::User { text, .. }]
            if text == "later request after fence"
    ));
}

#[tokio::test]
async fn lost_expert_task_output_ack_recovers_one_typed_contribution_after_fence() {
    use floe_agent_contract::ModelConversationEntry;

    let mut scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor");
    let assignment_id = Uuid::new_v4();
    let (first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "output acknowledgement loss",
    );
    let key = first_draft.key.clone();
    let admission = first.admission.clone();
    let first = match scenario
        .vault()
        .admit_task(first, first_draft)
        .await
        .expect("admit the first Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let working = start_expert_task(&scenario, &first).await;
    assert_eq!(
        append_scripted_expert_answer(&scenario, &working, "one committed answer", true).await,
        Err(AgentFailure::StorageUnavailable),
        "the output's Task journal, typed evidence and Core contribution commit atomically"
    );

    scenario.reopen().await;
    let recovery = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("generation-fenced recovery settles the uncertain output owner");
    assert_eq!(recovery.interrupted.len(), 1);
    let recovered = &recovery.interrupted[0];
    assert_eq!(recovered.snapshot.state, TaskState::Interrupted);
    assert_eq!(recovered.journal_revision, 5);
    assert_eq!(
        recovered
            .receipt
            .as_ref()
            .unwrap()
            .reference
            .journal_revision,
        5
    );

    let (mut later, mut later_draft) = expert_task_fixture(
        scenario.person_id,
        recovery.executor_generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "request after uncertain output",
    );
    later.admission = admission;
    later_draft.key = key;
    let later = match scenario
        .vault()
        .admit_task(later, later_draft)
        .await
        .expect("continue only after generation-fenced settlement")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected continuation admission: {other:?}"),
    };
    let later_working = start_expert_task(&scenario, &later).await;
    let conversation = scenario
        .vault()
        .expert_task_conversation(later_working.execution())
        .await
        .expect("read one authenticated output contribution after recovery");
    assert!(matches!(
        conversation.conversation.history.as_slice(),
        [
            ModelConversationEntry::User { text: user, .. },
            ModelConversationEntry::Assistant { text: answer, .. }
        ] if user == "output acknowledgement loss" && answer == "one committed answer"
    ));
}

#[tokio::test]
async fn concurrent_expert_task_admissions_claim_one_assignment_slot() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor")
        .executor_generation;
    let assignment_id = Uuid::new_v4();
    let (left, left_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "concurrent left",
    );
    let (mut right, mut right_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "concurrent right",
    );
    right.admission = left.admission.clone();
    right_draft.key = left_draft.key.clone();

    let vault = scenario.vault();
    let (left_result, right_result) = tokio::join!(
        vault.admit_task(left, left_draft),
        vault.admit_task(right, right_draft),
    );
    let results = [left_result, right_result];
    let admitted = results
        .iter()
        .filter(|result| matches!(result, Ok(floe_experts::TaskAdmission::Created { .. })))
        .count();
    assert_eq!(admitted, 1, "only one Task pins the assignment head");
    for result in results {
        if let Err(failure) = result {
            assert_eq!(failure, AgentFailure::Conflict);
        }
    }
    let connection = scenario
        .vault()
        .connection()
        .expect("inspect exact assignment reservation");
    assert_eq!(table_count(&connection, "agent_tasks").await, 1);
    assert_eq!(
        table_count(
            &connection,
            "agent_expert_task_conversation_reservations_v1"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn expert_conversation_bindings_isolate_assignment_person_and_definition() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor")
        .executor_generation;
    let (base, base_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        Uuid::new_v4(),
        1,
        "1.0.0",
        "base definition",
    );
    let base_key = base_draft.key.clone();
    let base = match scenario
        .vault()
        .admit_task(base.clone(), base_draft)
        .await
        .expect("admit base assignment")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected base Task admission: {other:?}"),
    };

    let mut another_assignment = base.admission.clone();
    another_assignment.assignment_id = Uuid::new_v4();
    let (assignment_task, assignment_draft) = next_expert_task_for(
        &base,
        scenario.person_id,
        another_assignment,
        1,
        "different assignment",
    );
    let assignment_task = scenario
        .vault()
        .admit_task(assignment_task, assignment_draft)
        .await
        .expect("separate assignment admits independently");
    assert!(matches!(
        assignment_task,
        floe_experts::TaskAdmission::Created { .. }
    ));

    let mut changed_definition = base.admission.clone();
    changed_definition.installation_id = Uuid::new_v4();
    changed_definition.package.version = "2.0.0".into();
    changed_definition.definition_revision = 2;
    let (definition_task, definition_draft) = next_expert_task_for(
        &base,
        scenario.person_id,
        changed_definition,
        2,
        "changed definition",
    );
    let definition_task = scenario
        .vault()
        .admit_task(definition_task, definition_draft)
        .await
        .expect("changed installed definition admits into separate custody");
    assert!(matches!(
        definition_task,
        floe_experts::TaskAdmission::Created { .. }
    ));

    let connection = scenario
        .vault()
        .connection()
        .expect("inspect isolated assignment bindings");
    let mut rows = connection
        .query(
            "SELECT conversation_id FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? ORDER BY task_id",
            [scenario.person_id.to_string()],
        )
        .await
        .expect("read all three distinct conversations");
    let mut conversation_ids = Vec::new();
    while let Some(row) = rows.next().await.expect("read conversation row") {
        conversation_ids.push(row.get::<String>(0).expect("read ConversationId"));
    }
    assert_eq!(conversation_ids.len(), 3);
    conversation_ids.sort();
    conversation_ids.dedup();
    assert_eq!(conversation_ids.len(), 3);

    let other_person = Scenario::new().await;
    let other_generation = other_person
        .vault()
        .activate_task_executor()
        .await
        .expect("activate another Person's Task executor")
        .executor_generation;
    let (mut other_task, mut other_draft) = expert_task_fixture(
        other_person.person_id,
        other_generation,
        TaskId::new(),
        base.admission.assignment_id,
        1,
        "1.0.0",
        "same installed Expert, other Person",
    );
    other_task.admission = base.admission.clone();
    other_draft.key = floe_experts::ExpertConversationKey::from_admission(
        other_person.person_id,
        &base.admission,
    )
    .expect("derive the same installation under another Person scope");
    assert_ne!(base_key, other_draft.key);
    assert_ne!(
        base_key.core_identity().expect("base Core identity"),
        other_draft
            .key
            .core_identity()
            .expect("other Person Core identity")
    );
    let other_task = other_person
        .vault()
        .admit_task(other_task, other_draft)
        .await
        .expect("same assignment ID under another Person remains isolated");
    assert!(matches!(
        other_task,
        floe_experts::TaskAdmission::Created { .. }
    ));
}

#[tokio::test]
async fn blocked_expert_task_stays_terminal_and_a_later_task_opens_a_fresh_run() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor")
        .executor_generation;
    let (mut first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        Uuid::new_v4(),
        1,
        "1.0.0",
        "request blocked on source binding",
    );
    let requirement = floe_experts::AdmittedRequirementSelection {
        key: "calendar.read".into(),
        capability: "calendar.read".into(),
        contract_version: 1,
        minimum_sources: 1,
        maximum_sources: 1,
        selected: vec![],
    };
    let requirements = vec![requirement];
    let digest = Sha256::digest(
        serde_json::to_vec(&(
            "floe.expert-execution-selection.sha256.v1",
            1_u64,
            &requirements,
        ))
        .expect("encode missing-source selection"),
    )
    .into();
    first.selection = ExpertExecutionSelection {
        schema_version: floe_experts::EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION,
        binding_revision: 1,
        requirements,
        digest,
    };
    let first_key = first_draft.key.clone();
    let first_run_id = first_draft.run_id;
    let first = match scenario
        .vault()
        .admit_task(first, first_draft)
        .await
        .expect("admit the assignment Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let working = start_expert_task(&scenario, &first).await;
    let terminal = scenario
        .vault()
        .settle_task_execution(TaskExecutionCommit {
            execution: working.execution(),
            expected_task_revision: working.aggregate_revision,
            expected_journal_revision: working.journal_revision,
            terminal: TaskSnapshot {
                state: TaskState::Blocked,
                result: None,
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
                blockage: Some(floe_agent_contract::TaskBlockage::Binding {
                    requirement_keys: vec!["calendar.read".into()],
                }),
                ..working.snapshot.clone()
            },
            settlement: None,
        })
        .await
        .expect("persist Blocked receipt, typed evidence, close and release atomically");
    assert_eq!(terminal.snapshot.state, TaskState::Blocked);

    let (mut later, mut later_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        first.admission.assignment_id,
        1,
        "1.0.0",
        "later request after blocked Task",
    );
    later.admission = first.admission.clone();
    later.selection = first.selection.clone();
    later_draft.key = first_key;
    let later_run_id = later_draft.run_id;
    let later = match scenario
        .vault()
        .admit_task(later, later_draft)
        .await
        .expect("new Task continues assignment after the Blocked receipt")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected continuation admission: {other:?}"),
    };
    let later_working = start_expert_task(&scenario, &later).await;
    let view = scenario
        .vault()
        .expert_task_conversation(later_working.execution())
        .await
        .expect("resolve later Task's pinned assignment history");
    assert_ne!(first_run_id, later_run_id);
    assert_eq!(view.conversation.history.len(), 1);
    assert!(matches!(
        &view.conversation.history[0],
        ModelConversationEntry::User { text, .. } if text == "request blocked on source binding"
    ));
    assert!(matches!(
        view.conversation.current_turn.as_slice(),
        [ModelConversationEntry::User { text, .. }] if text == "later request after blocked Task"
    ));
    let persisted = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("read terminal Blocked Task after continuation")
        .expect("Blocked Task remains immutable");
    assert_eq!(persisted.snapshot.state, TaskState::Blocked);
    assert_eq!(persisted.receipt, Some(terminal));
    let connection = scenario
        .vault()
        .connection()
        .expect("verify Experts did not write Manager learning records");
    assert_eq!(table_count(&connection, "learner_journal_heads").await, 0);
}

#[tokio::test]
async fn cancelled_expert_task_closes_its_run_and_later_task_starts_a_fresh_run() {
    let scenario = Scenario::new().await;
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task executor")
        .executor_generation;
    let assignment_id = Uuid::new_v4();
    let (first, first_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "request cancelled while working",
    );
    let first_key = first_draft.key.clone();
    let first_run_id = first_draft.run_id;
    let first = match scenario
        .vault()
        .admit_task(first, first_draft)
        .await
        .expect("admit the assignment Task")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected Task admission: {other:?}"),
    };
    let working = start_expert_task(&scenario, &first).await;
    let cancelled = scenario
        .vault()
        .settle_task_execution(TaskExecutionCommit {
            execution: working.execution(),
            expected_task_revision: working.aggregate_revision,
            expected_journal_revision: working.journal_revision,
            terminal: TaskSnapshot {
                state: TaskState::Cancelled,
                result: None,
                artifacts: vec![],
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::Cancelled),
                blockage: None,
                ..working.snapshot.clone()
            },
            settlement: None,
        })
        .await
        .expect("compose cancelled Task receipt and Core close at the owner boundary");
    assert_eq!(cancelled.snapshot.state, TaskState::Cancelled);
    assert!(cancelled.snapshot.result.is_none());

    let (mut later, mut later_draft) = expert_task_fixture(
        scenario.person_id,
        generation,
        TaskId::new(),
        assignment_id,
        1,
        "1.0.0",
        "request after cancellation",
    );
    later.admission = first.admission.clone();
    later_draft.key = first_key;
    let later_run_id = later_draft.run_id;
    let later = match scenario
        .vault()
        .admit_task(later, later_draft)
        .await
        .expect("new Task continues the settled assignment")
    {
        floe_experts::TaskAdmission::Created { record, .. } => record,
        other => panic!("unexpected continuation admission: {other:?}"),
    };
    let later_working = start_expert_task(&scenario, &later).await;
    let view = scenario
        .vault()
        .expert_task_conversation(later_working.execution())
        .await
        .expect("resolve the later Task's pinned conversation head");
    assert_ne!(first_run_id, later_run_id);
    assert_eq!(view.conversation.history.len(), 1);
    assert!(matches!(
        &view.conversation.history[0],
        ModelConversationEntry::User { text, .. } if text == "request cancelled while working"
    ));
    assert!(matches!(
        view.conversation.current_turn.as_slice(),
        [ModelConversationEntry::User { text, .. }] if text == "request after cancellation"
    ));
    let persisted = scenario
        .vault()
        .task(first.snapshot.task_id)
        .await
        .expect("read terminal cancelled Task after continuation")
        .expect("cancelled Task remains present");
    assert_eq!(persisted.snapshot.state, TaskState::Cancelled);
    assert_eq!(persisted.receipt, Some(cancelled));
}
