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
                // Approval does not change the originating candidate author.
                created_by: candidate.actor.clone(),
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

#[cfg(test)]
mod origin_tests {
    use super::*;
    use crate::{LearningEvidenceRef, MemoryOrigin, PersonalMemoryKind, PersonalMemoryValue};
    use floe_kernel::PersonId;

    fn candidate(actor: KnowledgeActor) -> KnowledgeCandidate {
        let now = Utc::now();
        KnowledgeCandidate {
            schema_version: KNOWLEDGE_VERSION,
            id: Uuid::new_v4(),
            person_id: PersonId::new(),
            observation_id: Uuid::new_v4(),
            idempotency_key: "origin-regression".into(),
            kind: KnowledgeKind::Memory,
            operation: KnowledgeOperation::Create,
            target_id: None,
            base_revision: None,
            payload: KnowledgePayload::Memory {
                value: PersonalMemoryValue {
                    kind: PersonalMemoryKind::Preference,
                    statement: "Prefers tea".into(),
                    epistemic_status: EpistemicStatus::Fact,
                    confidence_millis: 900,
                    valid_from: None,
                    valid_until: None,
                    observed_at: now,
                },
            },
            before_hash: None,
            after_hash: "synthetic-content-hash".into(),
            source_refs: vec![LearningEvidenceRef {
                session_id: Uuid::new_v4(),
                turn_id: Uuid::new_v4(),
            }],
            extractor_version: "test".into(),
            prompt_version: "test".into(),
            actor,
            state: KnowledgeCandidateState::Pending,
            created_at: now,
        }
    }

    fn admit_create() -> ReviewAdmission {
        ReviewAdmission {
            target_id: Uuid::new_v4(),
            target_exists: false,
            current_revision: None,
            current_payload_hash: None,
            evidence_independent: true,
        }
    }

    #[test]
    fn approving_a_memory_preserves_its_author_separately_from_the_reviewer() {
        for (author, origin) in [
            (KnowledgeActor::User, MemoryOrigin::UserProvided),
            (
                KnowledgeActor::Learner {
                    run_id: Uuid::new_v4(),
                },
                MemoryOrigin::Learned,
            ),
        ] {
            let proposed = candidate(author.clone());
            let person = proposed.person_id;
            let source_refs = proposed.source_refs.clone();
            let approved = plan_memory_review(
                proposed,
                person,
                KnowledgeDecisionKind::Approve,
                Utc::now(),
                Some(admit_create()),
            )
            .unwrap();
            let revision = approved.result.revision.unwrap();
            assert_eq!(revision.created_by, author);
            assert_eq!(revision.source_refs, source_refs);
            assert_eq!(approved.result.decision.actor, KnowledgeActor::User);
            assert_eq!(
                approved.result.mutation.unwrap().actor,
                KnowledgeActor::User
            );
            assert_eq!(
                crate::project_memory_summary(&revision, person)
                    .unwrap()
                    .origin,
                origin
            );
        }
    }

    #[test]
    fn a_learned_revision_keeps_its_new_author_and_the_previous_revision_evidence() {
        let original = candidate(KnowledgeActor::User);
        let person = original.person_id;
        let approved = plan_memory_review(
            original.clone(),
            person,
            KnowledgeDecisionKind::Approve,
            Utc::now(),
            Some(admit_create()),
        )
        .unwrap();
        let prior = approved.result.revision.unwrap();
        let mut revised = candidate(KnowledgeActor::Learner {
            run_id: Uuid::new_v4(),
        });
        revised.person_id = person;
        revised.target_id = Some(prior.target_id);
        revised.base_revision = Some(1);
        revised.before_hash = Some(original.after_hash.clone());
        revised.operation = KnowledgeOperation::Revise;
        let approved = plan_memory_review(
            revised.clone(),
            person,
            KnowledgeDecisionKind::Approve,
            Utc::now(),
            Some(ReviewAdmission {
                target_id: prior.target_id,
                target_exists: true,
                current_revision: Some(prior.clone()),
                current_payload_hash: Some(original.after_hash),
                evidence_independent: true,
            }),
        )
        .unwrap();
        let old = approved.superseded.unwrap();
        assert_eq!(old.created_by, KnowledgeActor::User);
        assert_eq!(old.source_refs, prior.source_refs);
        assert_eq!(old.state, KnowledgeRevisionState::Superseded);
        let next = approved.result.revision.unwrap();
        assert_eq!(next.created_by, revised.actor);
        assert_eq!(next.source_refs, revised.source_refs);
        assert_eq!(next.revision, 2);
        let stored = serde_json::to_vec(&next).unwrap();
        let reopened: KnowledgeRevision = serde_json::from_slice(&stored).unwrap();
        assert_eq!(reopened, next);
        assert_eq!(
            crate::project_memory_summary(&reopened, person)
                .unwrap()
                .origin,
            MemoryOrigin::Learned
        );
        assert_eq!(approved.result.decision.actor, KnowledgeActor::User);
        assert_eq!(
            approved.result.mutation.unwrap().actor,
            KnowledgeActor::User
        );
    }

    #[test]
    fn rejecting_learned_memory_never_creates_a_revision_and_learner_cannot_approve() {
        let proposed = candidate(KnowledgeActor::Learner {
            run_id: Uuid::new_v4(),
        });
        let rejected = plan_memory_review(
            proposed.clone(),
            proposed.person_id,
            KnowledgeDecisionKind::Reject,
            Utc::now(),
            None,
        )
        .unwrap();
        assert!(rejected.result.revision.is_none());
        assert!(rejected.result.mutation.is_none());
        assert_eq!(rejected.result.decision.actor, KnowledgeActor::User);
        assert_eq!(rejected.result.candidate.actor, proposed.actor);
        assert!(matches!(
            plan_review(
                proposed.clone(),
                KnowledgeDecisionKind::Approve,
                proposed.actor,
                Utc::now(),
                None
            ),
            Err(AgentFailure::PolicyDenied)
        ));
    }
}
