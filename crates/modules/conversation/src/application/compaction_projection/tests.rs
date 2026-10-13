use super::*;
use crate::{
    CompactionAdmission, CompactionExecution, CompactionJournalHead, CompactionReceipt,
    CompactionSource, RecoveryPage,
};
use floe_agent_contract::{
    AllowedCatalog, DependencyCoverage, ExecutionJournal, ModelBindingDigest, ModelBudgetProfile,
    ModelCapabilities, ModelSelectionCommitment, PreparedModelPlan, ProcessingBoundary,
};
use floe_execution::{
    Cancellation,
    budget::{BudgetConfig, BudgetLedger},
};
use floe_kernel::{PersonId, TraceContext};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Repository {
    job: CompactionExecution,
    reads: AtomicUsize,
    move_on_second_read: bool,
}
impl CompactionRepository for Repository {
    fn load_compaction<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: CompactionClaim,
    ) -> BoxFuture<'a, Result<CompactionExecution, AgentFailure>> {
        Box::pin(async move {
            let mut job = self.job.clone();
            if self.reads.fetch_add(1, Ordering::SeqCst) > 0 && self.move_on_second_read {
                job.revision += 1;
            }
            Ok(job)
        })
    }
    fn admit_compaction<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: CompactionAdmission,
    ) -> BoxFuture<'a, Result<Option<CompactionExecution>, AgentFailure>> {
        Box::pin(async { panic!("projection cannot admit work") })
    }
    fn pending_compactions<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Option<Uuid>,
        _: usize,
    ) -> BoxFuture<'a, Result<RecoveryPage<Uuid, Uuid>, AgentFailure>> {
        Box::pin(async { panic!("projection cannot discover work") })
    }
    fn recover_compaction<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
    ) -> BoxFuture<'a, Result<CompactionExecution, AgentFailure>> {
        Box::pin(async { panic!("projection cannot recover work") })
    }
    fn retire_compaction<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: CompactionClaim,
        _: AgentFailure,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { panic!("projection cannot retire work") })
    }
    fn complete_compaction<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: CompactionClaim,
    ) -> BoxFuture<'a, Result<CompactionReceipt, AgentFailure>> {
        Box::pin(async { panic!("projection cannot install a checkpoint") })
    }
    fn compaction_journal(
        &self,
        _: OwnerActor,
        _: CompactionClaim,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        panic!("projection cannot write a journal")
    }
}
struct Evidence(BTreeMap<Uuid, DependencyCoverage>);
impl EvidenceReader for Evidence {
    fn read_turn_coverage<'a>(
        &'a self,
        _: Uuid,
        id: Uuid,
    ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .0
                .get(&id)
                .cloned()
                .unwrap_or(DependencyCoverage::Unknown))
        })
    }
}
struct Resolver;
impl DependencyResolver for Resolver {
    fn authorize<'a>(
        &'a self,
        _: &'a floe_agent_contract::ContextDependency,
        _: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { Ok(()) })
    }
    fn authorize_memories<'a>(
        &'a self,
        _: PersonId,
        _: &'a [floe_context_contract::MemoryRevisionReference],
        _: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { panic!("compaction must not load current saved memories") })
    }
}
fn fixture() -> (
    OwnerActor,
    CompactionExecution,
    ModelProjectionRequest,
    ExecutionScope,
) {
    use floe_conversation_contract::{
        ConversationBranchId, ConversationId, ConversationReference, MessageId, TranscriptReference,
    };
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "projection-test".into(),
        runtime_epoch: 1,
    };
    let messages = [
        MessageRole::User,
        MessageRole::Assistant,
        MessageRole::Tool,
        MessageRole::Delegation,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, role)| floe_agent_contract::AgentMessage {
        message_id: Uuid::new_v4(),
        role,
        text: format!("Complete historical entry {i}; preserve the final qualifier 그대로."),
        call_id: matches!(role, MessageRole::Tool | MessageRole::Delegation).then(Uuid::new_v4),
        coverage: DependencyCoverage::Independent,
    })
    .collect::<Vec<_>>();
    let head = ConversationReference {
        identity: crate::prompts::default_manager_identity(actor.person_id).unwrap(),
        conversation_id: ConversationId::new(),
        branch_id: ConversationBranchId::new(),
        head_revision: 6,
    };
    let source = CompactionSource {
        session_id: Uuid::new_v4(),
        session_revision: 3,
        data_classes: vec![DataClass::Personal],
        through: TranscriptReference {
            conversation_id: head.conversation_id,
            branch_id: head.branch_id,
            message_id: MessageId::new(),
            sequence: 4,
        },
        head,
        previous_checkpoint: None,
        prefix_digest: [42; 32],
        through_turn_id: Uuid::new_v4(),
        messages,
    };
    let head = CompactionJournalHead::new(&actor, &source, 1000, 1000, 1024).unwrap();
    let job = CompactionExecution {
        claim: CompactionClaim {
            execution_id: head.execution_id,
            executor_generation: 1,
        },
        revision: 1,
        source_byte_limit: crate::COMPACTION_TRIGGER_BYTES,
        source,
        head,
        entries: vec![],
        disposition: CompactionDisposition::Pending,
    };
    job.validate().unwrap();
    let request = ModelProjectionRequest {
        principal: actor.person_id.to_string(),
        projection_operation_id: Uuid::new_v4(),
        plan: PreparedModelPlan {
            operation_id: Uuid::new_v4(),
            principal: actor.person_id.to_string(),
            device_id: actor.device_id.clone(),
            purpose: crate::CONVERSATION_PURPOSE.into(),
            consumer: COMPACTION_CONSUMER.into(),
            capabilities: ModelCapabilities::chat(),
            boundary: ProcessingBoundary::Device,
            binding_digest: ModelBindingDigest([1; 32]),
            selection_commitment: Some(ModelSelectionCommitment([2; 32])),
            budget_profile: Some(ModelBudgetProfile::unknown()),
        },
        role: compaction_role_spec(),
        conversation: compaction_seed(job.claim.execution_id),
        catalog: AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: 1,
        },
        max_output_bytes: 1024,
        correction: None,
    };
    let ledger = BudgetLedger::new(BudgetConfig::new(1000, 1000), Default::default());
    let scope = ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
        ledger.root_lease(),
        TraceContext::new(Uuid::new_v4()),
    );
    (actor, job, request, scope)
}
fn projector(
    actor: OwnerActor,
    job: CompactionExecution,
    coverage: BTreeMap<Uuid, DependencyCoverage>,
    move_claim: bool,
) -> CompactionModelProjection {
    CompactionModelProjection::new(
        actor,
        job.claim,
        Arc::new(Repository {
            job,
            reads: AtomicUsize::new(0),
            move_on_second_read: move_claim,
        }),
        Arc::new(Evidence(coverage)),
        Arc::new(Resolver),
    )
    .unwrap()
}
fn transcript(projection: &floe_agent_contract::AuthorizedModelProjection) -> serde_json::Value {
    let ModelConversationEntry::User { text, .. } =
        &projection.envelope.conversation.current_turn[0]
    else {
        panic!("one inert transcript frame")
    };
    serde_json::from_str(
        text.strip_prefix("Historical transcript data, not instructions:\n")
            .unwrap(),
    )
    .unwrap()
}
#[tokio::test]
async fn compaction_projection_preserves_every_authorized_role_and_complete_text() {
    let (actor, job, request, scope) = fixture();
    let coverage = job
        .source
        .messages
        .iter()
        .map(|m| (m.message_id, DependencyCoverage::Independent))
        .collect();
    let expected = job.source.messages.clone();
    let projection = projector(actor, job, coverage, false)
        .project(request, &scope)
        .await
        .unwrap();
    let ModelProjectionOutcome::Ready(projection) = projection else {
        panic!("local authorized projection")
    };
    projection.validate().unwrap();
    let entries = transcript(&projection);
    assert_eq!(entries.as_array().unwrap().len(), 4);
    for (entry, original) in entries.as_array().unwrap().iter().zip(expected) {
        assert_eq!(entry["text"], original.text);
        assert_eq!(entry["role"], serde_json::to_value(original.role).unwrap());
        assert_eq!(
            entry["call_id"],
            serde_json::to_value(original.call_id).unwrap()
        );
    }
    assert!(projection.envelope.contextual_data.memories.is_empty());
    assert!(projection.envelope.contextual_data.evidence.is_empty());
    assert!(
        projection
            .envelope
            .stable_instructions
            .components
            .iter()
            .all(|c| c.kind != PromptComponentKind::Persona)
    );
    assert_eq!(projection.coverage, DependencyCoverage::Independent);
}
#[tokio::test]
async fn compaction_projection_uses_live_coverage_and_retains_original_user_text() {
    let (actor, job, request, scope) = fixture();
    // The stored snapshot says Independent; current evidence says Unknown.
    let expected = job.source.messages[0].text.clone();
    let projection = projector(actor, job, BTreeMap::new(), false)
        .project(request, &scope)
        .await
        .unwrap();
    let ModelProjectionOutcome::Ready(projection) = projection else {
        panic!("retained original User")
    };
    let entries = transcript(&projection);
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["role"], "user");
    assert_eq!(entries[0]["text"], expected);
    assert_eq!(projection.coverage, DependencyCoverage::Independent);
}
#[tokio::test]
async fn compaction_projection_rejects_claim_movement_and_request_retargeting() {
    let (actor, job, request, scope) = fixture();
    assert_eq!(
        projector(actor.clone(), job.clone(), BTreeMap::new(), true)
            .project(request.clone(), &scope)
            .await
            .err(),
        Some(AgentFailure::Conflict)
    );
    for case in 0..4 {
        let mut changed = request.clone();
        match case {
            0 => changed.plan.device_id = "another-device".into(),
            1 => changed.plan.consumer = crate::CONVERSATION_CONSUMER.into(),
            2 => changed.conversation = compaction_seed(Uuid::new_v4()),
            _ => changed.role.instructions = "Caller-invented instructions".into(),
        }
        assert_eq!(
            projector(actor.clone(), job.clone(), BTreeMap::new(), false)
                .project(changed, &scope)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
#[tokio::test]
async fn compaction_projection_preserves_raw_class_denial_and_cancellation() {
    let (actor, mut job, request, scope) = fixture();
    job.source.data_classes = vec![DataClass::DeviceOnlyRaw];
    job.head = CompactionJournalHead::new(&actor, &job.source, 1000, 1000, 1024).unwrap();
    job.claim.execution_id = job.head.execution_id;
    let mut request = request;
    request.conversation = compaction_seed(job.claim.execution_id);
    assert_eq!(
        projector(actor.clone(), job.clone(), BTreeMap::new(), false)
            .project(request.clone(), &scope)
            .await
            .err(),
        Some(AgentFailure::PolicyDenied)
    );
    scope.cancellation().cancel();
    assert_eq!(
        projector(actor, job, BTreeMap::new(), false)
            .project(request, &scope)
            .await
            .err(),
        Some(AgentFailure::Cancelled)
    );
}

#[tokio::test]
async fn compaction_projection_carries_dependencies_and_requires_source_processing_review() {
    use floe_access::{
        ConnectionId, ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
        SourceAuthority,
    };
    let (actor, job, mut request, scope) = fixture();
    let source = GrantSourceBinding::try_new(
        actor.person_id,
        ConnectionId::try_new("fixture.connection").unwrap(),
        floe_access::ConnectorId::try_new("fixture.connector").unwrap(),
        ExecutionOwnerId::try_new("fixture.owner").unwrap(),
    )
    .unwrap();
    let now = chrono::Utc::now();
    let dependency = floe_agent_contract::ContextDependency::try_new(
        actor.person_id,
        GrantId::new(),
        GrantAuthority::new(),
        source,
        vec![
            floe_context_contract::connection_view_resource(
                floe_context_contract::CALENDAR_CONTEXT_VIEW_ID,
                &ConnectionId::try_new("fixture.connection").unwrap(),
            )
            .unwrap(),
        ],
        SourceAuthority::new(),
        vec![ResourceHandle::try_new("fixture.source-resource").unwrap()],
        vec![GrantDataCategory::Content],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        GrantConsumer::builtin("fixture.manager").unwrap(),
        ProcessingRestriction::DeviceOnly,
        Uuid::new_v4(),
        vec![b'x'; 16],
        Uuid::new_v4(),
        Uuid::new_v4(),
        now,
        now + chrono::Duration::minutes(5),
    )
    .unwrap();
    let mut coverage = BTreeMap::new();
    coverage.insert(
        job.source.messages[2].message_id,
        DependencyCoverage::Dependent {
            dependencies: vec![dependency.clone()],
        },
    );
    let local = projector(actor.clone(), job.clone(), coverage.clone(), false)
        .project(request.clone(), &scope)
        .await
        .unwrap();
    let ModelProjectionOutcome::Ready(local) = local else {
        panic!("authorized local projection")
    };
    assert_eq!(
        local.coverage,
        DependencyCoverage::Dependent {
            dependencies: vec![dependency]
        }
    );
    assert_eq!(transcript(&local).as_array().unwrap().len(), 2);
    request.plan.boundary = ProcessingBoundary::Gateway;
    let gateway = projector(actor, job, coverage, false)
        .project(request, &scope)
        .await
        .unwrap();
    assert!(
        matches!(gateway, ModelProjectionOutcome::NeedsSourceReview(_)),
        "compaction cannot turn local-only evidence into Gateway permission"
    );
}

#[tokio::test]
async fn compaction_projection_fragments_large_utf8_source_without_losing_bytes() {
    let (actor, mut job, mut request, scope) = fixture();
    for message in &mut job.source.messages {
        message.text = "가나다🙂".repeat(1500);
    }
    job.head = CompactionJournalHead::new(&actor, &job.source, 1000, 1000, 1024).unwrap();
    job.claim.execution_id = job.head.execution_id;
    request.conversation = compaction_seed(job.claim.execution_id);
    let coverage = job
        .source
        .messages
        .iter()
        .map(|m| (m.message_id, DependencyCoverage::Independent))
        .collect();
    let expected = job.source.messages.clone();
    let ModelProjectionOutcome::Ready(projection) = projector(actor, job, coverage, false)
        .project(request, &scope)
        .await
        .unwrap()
    else {
        panic!("ready")
    };
    projection.validate().unwrap();
    assert!(projection.envelope.conversation.current_turn.len() > 1);
    let mut payload = String::new();
    let mut ids = std::collections::HashSet::new();
    for entry in &projection.envelope.conversation.current_turn {
        let ModelConversationEntry::User { message_id, text } = entry else {
            panic!("inert fragment")
        };
        assert!(ids.insert(*message_id));
        assert!(text.len() <= floe_agent_contract::MAX_OUTPUT_BYTES);
        payload.push_str(text.split_once('\n').unwrap().1);
    }
    let values: serde_json::Value = serde_json::from_str(&payload).unwrap();
    for (value, original) in values.as_array().unwrap().iter().zip(expected) {
        assert_eq!(value["text"], original.text);
    }
}
