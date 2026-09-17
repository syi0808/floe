use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use uuid::Uuid;

use crate::{EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate, KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind, KnowledgeDecisionResult, KnowledgeKind, KnowledgeMutation, KnowledgeOperation, KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState};

/// One decision the Person made about a learned memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryReviewDecision {
    pub candidate_id: Uuid,
    pub kind: KnowledgeDecisionKind,
}

/// What the Person is shown about the memories they have under review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryReviewResult {
    pub snapshot: crate::MemoryReviewSnapshot,
    pub decision: Option<KnowledgeDecisionResult>,
}

/// Show the Person their memories under review, and record one decision.
///
/// A decision counts only on a candidate that is still pending, and only under
/// the Person's own hand: nothing else may approve what was learned about them.
/// The snapshot is taken again afterwards so that what they are shown is the
/// state their decision left behind.
pub async fn review_memory(
    repository: &impl crate::ports::repository::MemoryReviewRepository,
    decision: Option<MemoryReviewDecision>,
    decided_at: DateTime<Utc>,
) -> Result<MemoryReviewResult, AgentFailure> {
    let pending = repository.memory_review_snapshot().await?;
    let Some(request) = decision else {
        return Ok(MemoryReviewResult {
            snapshot: pending,
            decision: None,
        });
    };
    if !pending
        .candidates
        .iter()
        .any(|candidate| candidate.id == request.candidate_id)
    {
        return Err(AgentFailure::NotFound);
    }
    let decision = repository
        .decide_memory_candidate(
            request.candidate_id,
            request.kind,
            KnowledgeActor::User,
            decided_at,
        )
        .await?;
    Ok(MemoryReviewResult {
        snapshot: repository.memory_review_snapshot().await?,
        decision: Some(decision),
    })
}

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

#[cfg(test)]
mod tests {
    use floe_kernel::PersonId;

    use super::*;
    use crate::{EpistemicStatus, KnowledgeKind, KnowledgePayload, LearningEvidenceRef, PersonalMemoryKind, PersonalMemoryValue};

    fn candidate() -> KnowledgeCandidate {
        KnowledgeCandidate {
            schema_version: KNOWLEDGE_VERSION,
            id: Uuid::new_v4(),
            person_id: PersonId::new(),
            observation_id: Uuid::new_v4(),
            idempotency_key: "review.fixture".into(),
            kind: KnowledgeKind::Memory,
            operation: KnowledgeOperation::Create,
            target_id: None,
            base_revision: None,
            payload: KnowledgePayload::Memory {
                value: PersonalMemoryValue {
                    kind: PersonalMemoryKind::Preference,
                    statement: "Prefers concise replies".into(),
                    epistemic_status: EpistemicStatus::Fact,
                    confidence_millis: 1000,
                    valid_from: None,
                    valid_until: None,
                    observed_at: Utc::now(),
                },
            },
            before_hash: None,
            after_hash: "payload.fixture".into(),
            source_refs: vec![LearningEvidenceRef {
                session_id: Uuid::new_v4(),
                turn_id: Uuid::new_v4(),
            }],
            extractor_version: "fixture.v1".into(),
            prompt_version: "fixture.v1".into(),
            actor: KnowledgeActor::User,
            state: KnowledgeCandidateState::Pending,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn rejection_needs_user_and_pending_candidate_but_not_source_readmission() {
        let candidate = candidate();
        let rejected = plan_review(
            candidate.clone(),
            KnowledgeDecisionKind::Reject,
            KnowledgeActor::User,
            Utc::now(),
            None,
        )
        .unwrap();
        assert_eq!(
            rejected.result.candidate.state,
            KnowledgeCandidateState::Rejected
        );
        assert!(rejected.result.revision.is_none());
        assert!(rejected.result.mutation.is_none());
        assert!(rejected.superseded.is_none());
        assert!(matches!(
            plan_review(
                candidate,
                KnowledgeDecisionKind::Reject,
                KnowledgeActor::System,
                Utc::now(),
                None,
            ),
            Err(AgentFailure::PolicyDenied)
        ));
        assert!(matches!(
            plan_review(
                rejected.result.candidate,
                KnowledgeDecisionKind::Reject,
                KnowledgeActor::User,
                Utc::now(),
                None,
            ),
            Err(AgentFailure::Conflict)
        ));
    }

    #[test]
    fn approval_requires_independent_evidence_and_unused_create_target() {
        let candidate = candidate();
        let target_id = Uuid::new_v4();
        let admission = |independent, exists| {
            Some(ReviewAdmission {
                target_id,
                target_exists: exists,
                current_revision: None,
                current_payload_hash: None,
                evidence_independent: independent,
            })
        };
        for basis in [None, admission(false, false)] {
            assert!(matches!(
                plan_review(
                    candidate.clone(),
                    KnowledgeDecisionKind::Approve,
                    KnowledgeActor::User,
                    Utc::now(),
                    basis,
                ),
                Err(AgentFailure::PolicyDenied)
            ));
        }
        assert!(matches!(
            plan_review(
                candidate.clone(),
                KnowledgeDecisionKind::Approve,
                KnowledgeActor::User,
                Utc::now(),
                admission(true, true),
            ),
            Err(AgentFailure::Conflict)
        ));
        let approved = plan_review(
            candidate,
            KnowledgeDecisionKind::Approve,
            KnowledgeActor::User,
            Utc::now(),
            admission(true, false),
        )
        .unwrap();
        assert_eq!(
            approved.result.candidate.state,
            KnowledgeCandidateState::Approved
        );
        assert_eq!(approved.result.candidate.target_id, Some(target_id));
        assert_eq!(approved.result.revision.as_ref().unwrap().revision, 1);
        assert_eq!(
            approved.result.mutation.as_ref().unwrap().rollback_revision,
            None
        );
    }

    #[test]
    fn revision_binds_current_payload_and_preserves_rollback_version() {
        let mut candidate = candidate();
        let target_id = Uuid::new_v4();
        candidate.operation = KnowledgeOperation::Revise;
        candidate.target_id = Some(target_id);
        candidate.base_revision = Some(4);
        candidate.before_hash = Some("previous.payload".into());
        let current = KnowledgeRevision {
            schema_version: KNOWLEDGE_VERSION,
            target_id,
            revision: 4,
            person_id: candidate.person_id,
            kind: candidate.kind,
            payload: candidate.payload.clone(),
            state: KnowledgeRevisionState::Active,
            source_refs: candidate.source_refs.clone(),
            created_by: KnowledgeActor::User,
            created_at: Utc::now(),
        };
        let admission = |revision, hash: &str| {
            Some(ReviewAdmission {
                target_id,
                target_exists: true,
                current_revision: Some(revision),
                current_payload_hash: Some(hash.into()),
                evidence_independent: true,
            })
        };
        let mut stale = current.clone();
        stale.revision = 5;
        let mut foreign = current.clone();
        foreign.person_id = PersonId::new();
        for basis in [
            admission(current.clone(), "different.payload"),
            admission(stale, "previous.payload"),
            admission(foreign, "previous.payload"),
        ] {
            assert!(matches!(
                plan_review(
                    candidate.clone(),
                    KnowledgeDecisionKind::Approve,
                    KnowledgeActor::User,
                    Utc::now(),
                    basis,
                ),
                Err(AgentFailure::Conflict)
            ));
        }
        let revised = plan_review(
            candidate,
            KnowledgeDecisionKind::Approve,
            KnowledgeActor::User,
            Utc::now(),
            admission(current, "previous.payload"),
        )
        .unwrap();
        let superseded = revised.superseded.unwrap();
        assert_eq!(superseded.revision, 4);
        assert_eq!(superseded.state, KnowledgeRevisionState::Superseded);
        assert_eq!(revised.result.revision.unwrap().revision, 5);
        let mutation = revised.result.mutation.unwrap();
        assert_eq!(mutation.from_revision, Some(4));
        assert_eq!(mutation.rollback_revision, Some(4));
    }
}
