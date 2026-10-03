//! Claimed Knowledge evidence projected through the common Context path.
use std::{collections::BTreeSet, sync::Arc};
use floe_access::{AccessClock, DependencyAuthorization, DependencyResolver};
use floe_agent_contract::{AgentContext, AgentFailure, AllowedCatalog, DataClass, ModelConversation, ModelConversationEntry, ModelProjectionOutcome};
use floe_context_contract::DependencyCoverage;
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::OwnerActor;
use floe_knowledge::{LearnerEvidenceRepository, LearnerProjectionPort, LearnerProjectionRequest};
use crate::{ContextProjectionInput, ContextProjectionRole, EvidenceReader, assemble_context_projection};

pub struct ContextLearnerProjection {
    actor: OwnerActor,
    repository: Arc<dyn LearnerEvidenceRepository>,
    evidence: Arc<dyn EvidenceReader>,
    resolver: Arc<dyn DependencyResolver>,
    clock: Arc<dyn AccessClock>,
}
impl ContextLearnerProjection {
    pub fn new(actor: OwnerActor, repository: Arc<dyn LearnerEvidenceRepository>, evidence: Arc<dyn EvidenceReader>, resolver: Arc<dyn DependencyResolver>, clock: Arc<dyn AccessClock>) -> Result<Self, AgentFailure> {
        actor.validate()?; Ok(Self { actor, repository, evidence, resolver, clock })
    }
}
impl LearnerProjectionPort for ContextLearnerProjection {
    fn project<'a>(&'a self, request: LearnerProjectionRequest, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>> {
        Box::pin(async move {
            request.validate()?; check(&request, scope, &self.actor, self.clock.now())?;
            let input = scope.run(self.repository.read_claim(&request.actor, request.claim, scope)).await?;
            floe_knowledge::validate_learner_input(&input, request.actor.person_id)?;
            floe_knowledge::validate_learner_memory_time(&input, self.clock.now())?;
            if input.run_id != request.claim.job_id || input.observed_at > self.clock.now() || serde_json::to_vec(&input).map_err(|_| AgentFailure::InvalidInput)?.len() > request.bounds.max_input_bytes { return Err(AgentFailure::PolicyDenied); }
            let expected_refs = input.turn_ids.iter().map(|turn_id| floe_knowledge::LearningEvidenceRef { session_id: input.session_id, turn_id: *turn_id }).collect::<Vec<_>>();
            if request.evidence_refs != expected_refs { return Err(AgentFailure::PolicyDenied); }
            let mut references = BTreeSet::new();
            for reference in request.evidence_refs.iter().chain(input.current_memories.iter().flat_map(|memory| memory.source_refs.iter())) {
                if reference.session_id.is_nil() || reference.turn_id.is_nil() { return Err(AgentFailure::PolicyDenied); }
                references.insert((reference.session_id, reference.turn_id));
            }
            let mut coverage = DependencyCoverage::Independent;
            for (session_id, turn_id) in references {
                let observed = scope.run(self.evidence.read_turn_coverage(session_id, turn_id)).await?;
                observed.validate().map_err(|_| AgentFailure::StorageUnavailable)?;
                coverage = coverage.merge(&observed).map_err(|_| AgentFailure::PolicyDenied)?;
            }
            let dependencies = match coverage {
                DependencyCoverage::Independent => Vec::new(),
                DependencyCoverage::Dependent { dependencies } => dependencies,
                DependencyCoverage::Unknown => return Err(AgentFailure::PolicyDenied),
            };
            let authorization = DependencyAuthorization { deadline: scope.deadline(), cancellation: scope.cancellation().clone() };
            let mut input_classes = vec![DataClass::Personal];
            for dependency in &dependencies {
                if dependency.person_id() != request.actor.person_id { return Err(AgentFailure::PolicyDenied); }
                scope.run(self.resolver.authorize(dependency, &authorization)).await?;
                if dependency.source().connector().as_str() == floe_access::WELLBEING_CONNECTOR {
                    dependency.validate_health_transform(&request.actor.device_id, self.clock.now())?;
                    input_classes.push(DataClass::HighlySensitive);
                }
            }
            input_classes.sort(); input_classes.dedup();
            let context = AgentContext { projection_version: 1, persona: None, memories: input.current_memories.clone(), optional_context_issues: vec![], evidence: vec![] };
            let conversation = ModelConversation { history: vec![], current_turn: vec![ModelConversationEntry::User { message_id: *input.turn_ids.last().ok_or(AgentFailure::InvalidInput)?, text: input.digest.clone() }] };
            let catalog = AllowedCatalog { cards: vec![], tools: vec![], revision: 1 };
            let outcome = assemble_context_projection(ContextProjectionInput { role: ContextProjectionRole::Learner, plan: &request.plan, projection_operation_id: request.projection_operation_id, purpose: floe_knowledge::LEARNER_INFERENCE_PURPOSE, response_contract: "One JSON object with schema_version 1 and a proposals array containing zero or one bounded memory proposal.", output_format: &floe_knowledge::prompts::learner_output_format()?, correction: request.correction.clone(), prompt: floe_knowledge::prompts::learner_prompt(), conversation, agent_context: &context, catalog: &catalog, expert_environment: None, authorized_history_dependencies: &dependencies, input_data_classes: input_classes, max_output_bytes: request.bounds.max_output_bytes })?;
            if let ModelProjectionOutcome::Ready(projection) = &outcome {
                if serde_json::to_vec(&projection.envelope).map_err(|_| AgentFailure::InvalidInput)?.len() > request.bounds.max_input_bytes { return Err(AgentFailure::BudgetExceeded); }
            }
            // A claim that settled/replaced while evidence was reauthorized may
            // not publish a projection under its old input.
            let current = scope.run(self.repository.read_claim(&request.actor, request.claim, scope)).await?;
            if current != input { return Err(AgentFailure::Conflict); }
            floe_knowledge::validate_learner_memory_time(&current, self.clock.now())?;
            check(&request, scope, &self.actor, self.clock.now())?; Ok(outcome)
        })
    }
}
fn check(request: &LearnerProjectionRequest, scope: &ExecutionScope, expected: &OwnerActor, now: chrono::DateTime<chrono::Utc>) -> Result<(), AgentFailure> {
    if &request.actor != expected { return Err(AgentFailure::PolicyDenied); }
    if scope.cancellation().is_cancelled() { return Err(AgentFailure::Cancelled); }
    if tokio::time::Instant::now() >= scope.deadline() || request.expires_at <= now { return Err(AgentFailure::DeadlineExceeded); } Ok(())
}
