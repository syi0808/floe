use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use uuid::Uuid;

use crate::{
    EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate,
    KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind, KnowledgeDecisionResult,
    KnowledgeKind, KnowledgeMutation, KnowledgeOperation, KnowledgePayload, KnowledgeRevision,
    KnowledgeRevisionState,
};

pub struct ReviewAdmission {
    pub target_id: Uuid,
    pub target_exists: bool,
    pub current_revision: Option<KnowledgeRevision>,
    pub current_payload_hash: Option<String>,
    pub evidence_independent: bool,
}

pub struct ReviewPlan {
    pub result: KnowledgeDecisionResult,
    pub superseded: Option<KnowledgeRevision>,
}

pub fn validate_review_actor(actor: &KnowledgeActor) -> Result<(), AgentFailure> {
    if *actor != KnowledgeActor::User {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

pub fn validate_review_candidate(candidate: &KnowledgeCandidate) -> Result<(), AgentFailure> {
    if candidate.state != KnowledgeCandidateState::Pending {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

pub fn validate_memory_review_candidate(
    candidate: &KnowledgeCandidate,
    expected_person_id: floe_kernel::PersonId,
) -> Result<(), AgentFailure> {
    if candidate.schema_version != KNOWLEDGE_VERSION
        || candidate.person_id != expected_person_id
        || candidate.state != KnowledgeCandidateState::Pending
        || candidate.kind != KnowledgeKind::Memory
        || candidate.source_refs.is_empty()
        || candidate
            .source_refs
            .iter()
            .any(|reference| reference.session_id.is_nil() || reference.turn_id.is_nil())
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let KnowledgePayload::Memory { value } = &candidate.payload else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if value.confidence_millis > 1000
        || value.statement.trim().is_empty()
        || (matches!(value.kind, crate::PersonalMemoryKind::Inference)
            != matches!(value.epistemic_status, EpistemicStatus::Inference))
        || value
            .valid_until
            .zip(value.valid_from)
            .is_some_and(|(until, from)| until <= from)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

pub fn validate_approval_candidate(candidate: &KnowledgeCandidate) -> Result<(), AgentFailure> {
    match candidate.operation {
        KnowledgeOperation::Create
            if candidate.target_id.is_some() || candidate.base_revision.is_some() =>
        {
            Err(AgentFailure::VaultUnavailable)
        }
        KnowledgeOperation::Retire => Err(AgentFailure::InvalidInput),
        _ => Ok(()),
    }
}

pub fn plan_review(
    mut candidate: KnowledgeCandidate,
    decision_kind: KnowledgeDecisionKind,
    actor: KnowledgeActor,
    decided_at: DateTime<Utc>,
    admission: Option<ReviewAdmission>,
) -> Result<ReviewPlan, AgentFailure> {
    validate_review_actor(&actor)?;
    validate_review_candidate(&candidate)?;
    let decision = KnowledgeDecision {
        schema_version: KNOWLEDGE_VERSION,
        id: Uuid::new_v4(),
        candidate_id: candidate.id,
        decision: decision_kind,
        actor: actor.clone(),
        decided_at,
    };
    let mut superseded = None;
    let (revision, mutation) = match decision_kind {
        KnowledgeDecisionKind::Reject => {
            candidate.state = KnowledgeCandidateState::Rejected;
            (None, None)
        }
        KnowledgeDecisionKind::Approve => {
            let admission = admission.ok_or(AgentFailure::PolicyDenied)?;
            if candidate.source_refs.is_empty() || !admission.evidence_independent {
                return Err(AgentFailure::PolicyDenied);
            }
            validate_approval_candidate(&candidate)?;
            let (from_revision, revision_number) = match candidate.operation {
                KnowledgeOperation::Create => {
                    if admission.target_exists || admission.current_revision.is_some() {
                        return Err(AgentFailure::Conflict);
                    }
                    (None, 1)
                }
                KnowledgeOperation::Revise => {
                    let mut current = admission.current_revision.ok_or(AgentFailure::Conflict)?;
                    if candidate.target_id != Some(admission.target_id)
                        || current.target_id != admission.target_id
                        || current.person_id != candidate.person_id
                        || current.kind != candidate.kind
                        || current.state != KnowledgeRevisionState::Active
                        || Some(current.revision) != candidate.base_revision
                        || candidate.before_hash.is_none()
                        || candidate.before_hash != admission.current_payload_hash
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    let next = current
                        .revision
                        .checked_add(1)
                        .ok_or(AgentFailure::Conflict)?;
                    let previous = current.revision;
                    current.state = KnowledgeRevisionState::Superseded;
                    superseded = Some(current);
                    (Some(previous), next)
                }
                KnowledgeOperation::Retire => return Err(AgentFailure::InvalidInput),
            };
            let revision = KnowledgeRevision {
                schema_version: KNOWLEDGE_VERSION,
                target_id: admission.target_id,
                revision: revision_number,
                person_id: candidate.person_id,
                kind: candidate.kind,
                payload: candidate.payload.clone(),
                state: KnowledgeRevisionState::Active,
                source_refs: candidate.source_refs.clone(),
                created_by: actor.clone(),
                created_at: decided_at,
            };
            let mutation = KnowledgeMutation {
                schema_version: KNOWLEDGE_VERSION,
                id: Uuid::new_v4(),
                candidate_id: candidate.id,
                target_id: admission.target_id,
                from_revision,
                to_revision: revision_number,
                actor,
                operation: candidate.operation,
                before_hash: candidate.before_hash.clone(),
                after_hash: candidate.after_hash.clone(),
                rollback_revision: from_revision,
                created_at: decided_at,
            };
            candidate.target_id = Some(admission.target_id);
            candidate.state = KnowledgeCandidateState::Approved;
            (Some(revision), Some(mutation))
        }
    };
    Ok(ReviewPlan {
        result: KnowledgeDecisionResult {
            candidate,
            decision,
            revision,
            mutation,
        },
        superseded,
    })
}

/// The public memory decision route admits only an actual pending Memory candidate.
pub fn plan_memory_review(
    candidate: KnowledgeCandidate,
    expected_person_id: floe_kernel::PersonId,
    decision: KnowledgeDecisionKind,
    decided_at: DateTime<Utc>,
    admission: Option<ReviewAdmission>,
) -> Result<ReviewPlan, AgentFailure> {
    validate_memory_review_candidate(&candidate, expected_person_id)?;
    plan_review(
        candidate,
        decision,
        KnowledgeActor::User,
        decided_at,
        admission,
    )
}
