use std::collections::HashSet;

use floe_kernel::{AgentFailure, PersonId};

use crate::{
    EpistemicStatus, EvidenceProjectionPurpose, EvidenceReader, KNOWLEDGE_VERSION, KnowledgeActor,
    KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState, LearningEvidenceSnapshot,
    LearningOutcome, MAX_MEMORY_OVERVIEW_ITEMS, MemoryOrigin, MemorySummary, PersonalMemoryKind,
    StageMemoryCandidate,
};

const MAX_OBSERVATION_DIGEST_BYTES: usize = 4 * 1024;
const MAX_MEMORY_STATEMENT_BYTES: usize = 2 * 1024;
const MAX_EVIDENCE_REFS: usize = 32;
const MAX_VERSION_BYTES: usize = 128;

pub fn validate_memory_overview_limit(limit: usize) -> Result<(), AgentFailure> {
    if limit == 0 || limit > MAX_MEMORY_OVERVIEW_ITEMS {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

pub fn project_memory_summary(
    revision: &KnowledgeRevision,
    expected_person_id: PersonId,
) -> Result<MemorySummary, AgentFailure> {
    if revision.schema_version != KNOWLEDGE_VERSION
        || revision.person_id != expected_person_id
        || revision.kind != crate::KnowledgeKind::Memory
        || revision.state != KnowledgeRevisionState::Active
        || revision.target_id.is_nil()
        || revision.revision == 0
        || revision.source_refs.is_empty()
        || revision
            .source_refs
            .iter()
            .any(|reference| reference.session_id.is_nil() || reference.turn_id.is_nil())
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let KnowledgePayload::Memory { value } = &revision.payload else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if value.statement.trim().is_empty()
        || value.confidence_millis > 1000
        || (matches!(value.kind, PersonalMemoryKind::Inference)
            != matches!(value.epistemic_status, EpistemicStatus::Inference))
        || value
            .valid_until
            .zip(value.valid_from)
            .is_some_and(|(until, from)| until <= from)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let origin = match revision.created_by {
        KnowledgeActor::User => MemoryOrigin::UserProvided,
        KnowledgeActor::Learner { .. } => MemoryOrigin::Learned,
        KnowledgeActor::Curator | KnowledgeActor::System => {
            return Err(AgentFailure::VaultUnavailable);
        }
    };
    Ok(MemorySummary {
        target_id: revision.target_id,
        revision: revision.revision,
        statement: value.statement.clone(),
        memory_kind: value.kind,
        epistemic_status: value.epistemic_status,
        confidence_millis: value.confidence_millis,
        source_count: revision.source_refs.len(),
        origin,
        created_at: revision.created_at,
        valid_from: value.valid_from,
        valid_until: value.valid_until,
    })
}

pub fn validate_stage_request(request: &StageMemoryCandidate) -> Result<(), AgentFailure> {
    let digest = request.digest.trim();
    let statement = request.value.statement.trim();
    if request.session_id.is_nil() || request.expected_session_revision == 0
        || request.turn_ids.iter().any(uuid::Uuid::is_nil)
        || !matches!((request.target_id, request.base_revision), (None, None))
            && !matches!((request.target_id, request.base_revision), (Some(id), Some(revision)) if !id.is_nil() && revision > 0)
        || matches!(request.actor, KnowledgeActor::Learner { run_id } if run_id.is_nil())
        || digest.is_empty()
        || digest.len() > MAX_OBSERVATION_DIGEST_BYTES
        || statement.is_empty()
        || statement.len() > MAX_MEMORY_STATEMENT_BYTES
        || request.turn_ids.is_empty()
        || request.turn_ids.len() > MAX_EVIDENCE_REFS
        || request.turn_ids.iter().collect::<HashSet<_>>().len() != request.turn_ids.len()
        || !valid_version(&request.extractor_version)
        || !valid_version(&request.prompt_version)
        || request.value.confidence_millis > 1000
        || request
            .value
            .valid_until
            .zip(request.value.valid_from)
            .is_some_and(|(until, from)| until <= from)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if matches!(
        request.actor,
        KnowledgeActor::Curator | KnowledgeActor::System
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    if matches!(request.value.kind, PersonalMemoryKind::Inference)
        != matches!(request.value.epistemic_status, EpistemicStatus::Inference)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

pub fn validate_learning_evidence(
    snapshot: &LearningEvidenceSnapshot,
    expected_person_id: floe_kernel::PersonId,
    expected_session_id: uuid::Uuid,
    expected_revision: u64,
    turn_ids: &[uuid::Uuid],
) -> Result<(), AgentFailure> {
    validate_evidence_request(
        expected_person_id,
        expected_session_id,
        expected_revision,
        turn_ids,
    )?;
    if snapshot.person_id != expected_person_id
        || snapshot.session_id != expected_session_id
        || snapshot.revision != expected_revision
    {
        return Err(AgentFailure::Conflict);
    }
    snapshot
        .coverage
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if snapshot.active_turn
        || snapshot.pending_output
        || snapshot.outcome != Some(LearningOutcome::Completed)
        || !snapshot.personal
        || snapshot.purpose != EvidenceProjectionPurpose::Learning
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if turn_ids
        .iter()
        .any(|turn_id| !snapshot.turn_ids.contains(turn_id))
    {
        return Err(AgentFailure::NotFound);
    }
    if snapshot.coverage != floe_context_contract::DependencyCoverage::Independent {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

pub async fn admit_learning_evidence(
    reader: &impl EvidenceReader,
    expected_person_id: floe_kernel::PersonId,
    expected_session_id: uuid::Uuid,
    expected_revision: u64,
    turn_ids: &[uuid::Uuid],
) -> Result<LearningEvidenceSnapshot, AgentFailure> {
    validate_evidence_request(
        expected_person_id,
        expected_session_id,
        expected_revision,
        turn_ids,
    )?;
    let snapshot = reader
        .read_learning_evidence(expected_person_id, expected_session_id, turn_ids)
        .await?;
    validate_learning_evidence(
        &snapshot,
        expected_person_id,
        expected_session_id,
        expected_revision,
        turn_ids,
    )?;
    Ok(snapshot)
}

fn validate_evidence_request(
    person_id: floe_kernel::PersonId,
    session_id: uuid::Uuid,
    revision: u64,
    turn_ids: &[uuid::Uuid],
) -> Result<(), AgentFailure> {
    if !person_id.is_valid()
        || session_id.is_nil()
        || turn_ids.is_empty()
        || turn_ids.len() > MAX_EVIDENCE_REFS
        || turn_ids.iter().any(uuid::Uuid::is_nil)
        || turn_ids.iter().collect::<HashSet<_>>().len() != turn_ids.len()
    {
        return Err(AgentFailure::InvalidInput);
    }
    if revision == 0 {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

fn valid_version(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_VERSION_BYTES
}
