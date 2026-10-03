//! Product projections of confirmed review state; storage evidence stays private.
use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, CommandId, PersonId};
use uuid::Uuid;
use crate::{EpistemicStatus, KnowledgeActor, KnowledgeCandidateState, KnowledgeDecisionKind,
    KnowledgeDecisionResult, KnowledgeOperation, KnowledgePayload, MemoryReviewSnapshot,
    PersonalMemoryKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryReviewAction { Approve, Reject }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCandidateSummary {
    pub candidate_id: Uuid,
    pub operation: KnowledgeOperation,
    pub statement: String,
    pub memory_kind: PersonalMemoryKind,
    pub epistemic_status: EpistemicStatus,
    pub confidence_millis: u16,
    pub source_count: usize,
    pub created_at: DateTime<Utc>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub allowed_actions: Vec<MemoryReviewAction>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryReviewDisplay {
    pub person_id: PersonId,
    pub candidates: Vec<MemoryCandidateSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryDecisionAcknowledgement {
    pub command_id: CommandId,
    pub candidate_id: Uuid,
    pub decision: KnowledgeDecisionKind,
    pub committed_at: DateTime<Utc>,
    pub resulting_target_id: Option<Uuid>,
    pub resulting_revision: Option<u64>,
}

pub fn project_memory_review(snapshot: &MemoryReviewSnapshot) -> Result<MemoryReviewDisplay, AgentFailure> {
    if !snapshot.person_id.is_valid() || snapshot.candidates.len() > 100 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let mut seen = std::collections::HashSet::new();
    let candidates = snapshot.candidates.iter().map(|candidate| {
        crate::validate_memory_review_candidate(candidate, snapshot.person_id)?;
        if candidate.id.is_nil() || !seen.insert(candidate.id) { return Err(AgentFailure::StorageUnavailable); }
        let KnowledgePayload::Memory { value } = &candidate.payload else { return Err(AgentFailure::StorageUnavailable); };
        let mut allowed_actions = vec![MemoryReviewAction::Reject];
        if crate::validate_approval_candidate(candidate).is_ok() {
            match candidate.operation {
                KnowledgeOperation::Create => allowed_actions.insert(0, MemoryReviewAction::Approve),
                KnowledgeOperation::Revise if candidate.target_id.is_some_and(|id| !id.is_nil())
                    && candidate.base_revision.is_some_and(|revision| revision > 0)
                    && candidate.before_hash.as_ref().is_some_and(|value| !value.is_empty()) =>
                        allowed_actions.insert(0, MemoryReviewAction::Approve),
                _ => {}
            }
        }
        Ok(MemoryCandidateSummary { candidate_id: candidate.id, operation: candidate.operation,
            statement: value.statement.clone(), memory_kind: value.kind, epistemic_status: value.epistemic_status,
            confidence_millis: value.confidence_millis, source_count: candidate.source_refs.len(),
            created_at: candidate.created_at, valid_from: value.valid_from,
            valid_until: value.valid_until, allowed_actions })
    }).collect::<Result<_, AgentFailure>>()?;
    Ok(MemoryReviewDisplay { person_id: snapshot.person_id, candidates })
}

/// The repository has already authenticated this exact immutable command receipt.
pub fn project_memory_decision(command_id: CommandId, result: &KnowledgeDecisionResult)
    -> Result<MemoryDecisionAcknowledgement, AgentFailure>
{
    if command_id.as_uuid().is_nil() || result.candidate.id.is_nil()
        || result.candidate.kind != crate::KnowledgeKind::Memory
        || result.decision.candidate_id != result.candidate.id || result.decision.actor != KnowledgeActor::User
    { return Err(AgentFailure::StorageUnavailable); }
    let (resulting_target_id, resulting_revision) = match result.decision.decision {
        KnowledgeDecisionKind::Approve => {
            let revision = result.revision.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
            crate::project_memory_summary(revision, result.candidate.person_id)?;
            let mutation = result.mutation.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
            if result.candidate.state != KnowledgeCandidateState::Approved
                || result.candidate.target_id != Some(revision.target_id)
                || mutation.candidate_id != result.candidate.id || mutation.target_id != revision.target_id
                || mutation.to_revision != revision.revision || mutation.actor != KnowledgeActor::User
                || result.decision.decided_at != revision.created_at || result.decision.decided_at != mutation.created_at
            { return Err(AgentFailure::StorageUnavailable); }
            (Some(revision.target_id), Some(revision.revision))
        }
        KnowledgeDecisionKind::Reject => {
            if result.candidate.state != KnowledgeCandidateState::Rejected
                || result.revision.is_some() || result.mutation.is_some()
            { return Err(AgentFailure::StorageUnavailable); }
            (None, None)
        }
    };
    Ok(MemoryDecisionAcknowledgement { command_id, candidate_id: result.candidate.id,
        decision: result.decision.decision, committed_at: result.decision.decided_at,
        resulting_target_id, resulting_revision })
}
