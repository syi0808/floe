//! Pure Knowledge decisions applied by its encrypted repository transactions.
use chrono::{DateTime, Utc};
use floe_kernel::{AgentFailure, PersonId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use crate::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryStageIdentity {
    pub observation_hash: String,
    pub candidate_key: String,
    pub source_refs: Vec<LearningEvidenceRef>,
}
pub struct MemoryStagePlan {
    pub observation: LearningObservation,
    pub candidate: KnowledgeCandidate,
}

pub fn knowledge_content_hash(value: &impl Serialize) -> Result<String, AgentFailure> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentFailure::StorageUnavailable)?;
    Ok(Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn memory_stage_identity(person_id: PersonId, request: &StageMemoryCandidate,
    snapshot: &LearningEvidenceSnapshot) -> Result<MemoryStageIdentity, AgentFailure>
{
    validate_stage_request(request)?;
    validate_learning_evidence(snapshot, person_id, request.session_id,
        request.expected_session_revision, &request.turn_ids)?;
    let source_refs = request.turn_ids.iter().map(|turn_id| LearningEvidenceRef {
        session_id: request.session_id, turn_id: *turn_id,
    }).collect::<Vec<_>>();
    let observation_hash = knowledge_content_hash(&(person_id, request.session_id, &source_refs,
        request.observation_kind, request.digest.trim(), snapshot.outcome))?;
    let candidate_key = knowledge_content_hash(&(&observation_hash, request.extractor_version.trim(),
        request.prompt_version.trim(), KnowledgeKind::Memory, request.target_id))?;
    Ok(MemoryStageIdentity { observation_hash, candidate_key, source_refs })
}

pub fn plan_memory_stage(person_id: PersonId, request: &StageMemoryCandidate,
    snapshot: &LearningEvidenceSnapshot, stored_observation: Option<LearningObservation>,
    current_revision: Option<KnowledgeRevision>) -> Result<MemoryStagePlan, AgentFailure>
{
    let identity = memory_stage_identity(person_id, request, snapshot)?;
    let observation = match stored_observation {
        Some(observation) => {
            if observation.schema_version != KNOWLEDGE_VERSION || observation.person_id != person_id
                || observation.session_id != request.session_id || observation.evidence != identity.source_refs
                || observation.outcome != LearningOutcome::Completed || observation.kind != request.observation_kind
                || observation.digest != request.digest.trim() || observation.content_hash != identity.observation_hash
                || observation.id.is_nil()
            { return Err(AgentFailure::StorageUnavailable); }
            observation
        }
        None => LearningObservation { schema_version: KNOWLEDGE_VERSION,
            id: Uuid::new_v5(&request.session_id, identity.observation_hash.as_bytes()), person_id,
            session_id: request.session_id, evidence: identity.source_refs.clone(),
            outcome: LearningOutcome::Completed, kind: request.observation_kind,
            digest: request.digest.trim().to_owned(), observed_at: request.created_at,
            content_hash: identity.observation_hash },
    };
    let (operation, before_hash) = match (request.target_id, request.base_revision, current_revision) {
        (None, None, None) => (KnowledgeOperation::Create, None),
        (Some(target_id), Some(base_revision), Some(current)) => {
            project_memory_summary(&current, person_id)?;
            if current.target_id != target_id || current.revision != base_revision {
                return Err(AgentFailure::Conflict);
            }
            (KnowledgeOperation::Revise, Some(knowledge_content_hash(&current.payload)?))
        }
        (Some(_), Some(_), None) => return Err(AgentFailure::Conflict),
        _ => return Err(AgentFailure::InvalidInput),
    };
    let payload = KnowledgePayload::Memory { value: request.value.clone() };
    let candidate = KnowledgeCandidate { schema_version: KNOWLEDGE_VERSION,
        id: Uuid::new_v5(&observation.id, identity.candidate_key.as_bytes()), person_id,
        observation_id: observation.id, idempotency_key: identity.candidate_key,
        kind: KnowledgeKind::Memory, operation, target_id: request.target_id,
        base_revision: request.base_revision, before_hash, after_hash: knowledge_content_hash(&payload)?,
        payload, source_refs: identity.source_refs, extractor_version: request.extractor_version.trim().into(),
        prompt_version: request.prompt_version.trim().into(), actor: request.actor.clone(),
        state: KnowledgeCandidateState::Pending, created_at: request.created_at };
    Ok(MemoryStagePlan { observation, candidate })
}

#[derive(Clone, Debug)]
pub struct MemoryContextFact {
    pub revision: KnowledgeRevision,
    pub evidence_independent: bool,
}
pub fn project_memory_context(person_id: PersonId, facts: Vec<MemoryContextFact>, now: DateTime<Utc>)
    -> Result<MemoryContextSnapshot, AgentFailure>
{
    let mut memories = Vec::new();
    let mut bytes = 0usize;
    let mut targets = std::collections::HashSet::new();
    for fact in facts {
        project_memory_summary(&fact.revision, person_id)?;
        if !targets.insert(fact.revision.target_id) { return Err(AgentFailure::StorageUnavailable); }
        if !fact.evidence_independent { continue; }
        let KnowledgePayload::Memory { value } = fact.revision.payload
            else { return Err(AgentFailure::StorageUnavailable); };
        if value.valid_from.is_some_and(|from| from > now) || value.valid_until.is_some_and(|until| until <= now) { continue; }
        bytes = bytes.checked_add(value.statement.len()).ok_or(AgentFailure::BudgetExceeded)?;
        if memories.len() >= MAX_CONTEXT_MEMORIES || bytes > MAX_CONTEXT_MEMORY_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        memories.push(ContextMemory { target_id: fact.revision.target_id, revision: fact.revision.revision,
            kind: value.kind, statement: value.statement, epistemic_status: value.epistemic_status,
            confidence_millis: value.confidence_millis, observed_at_unix_ms: value.observed_at.timestamp_millis(),
            valid_from_unix_ms: value.valid_from.map(|value| value.timestamp_millis()),
            valid_until_unix_ms: value.valid_until.map(|value| value.timestamp_millis()), source_refs: fact.revision.source_refs });
    }
    Ok(MemoryContextSnapshot { memories, issue: None })
}

pub fn learner_job_key(input: &LearnerReviewInput) -> Result<String, AgentFailure> {
    validate_learner_input(input, input.person_id)?;
    knowledge_content_hash(&(input.person_id, input.session_id, input.session_revision,
        &input.turn_ids, input.outcome, input.digest.trim()))
}

pub fn new_learner_job(mut input: LearnerReviewInput, available_at: DateTime<Utc>)
    -> Result<LearnerReviewJob, AgentFailure>
{
    input.digest = input.digest.trim().into();
    let idempotency_key = learner_job_key(&input)?;
    let id = Uuid::new_v5(&input.session_id, idempotency_key.as_bytes());
    input.run_id = id;
    Ok(LearnerReviewJob { schema_version: KNOWLEDGE_VERSION, id, idempotency_key, input,
        state: LearnerJobState::Queued, attempts: 0, available_at, claimed_at: None,
        finished_at: None, candidate_id: None, last_failure: None, blocked: None, claimed_device_id: None })
}
