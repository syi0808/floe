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
    if digest.is_empty()
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

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use chrono::Utc;
    use floe_kernel::PersonId;
    use uuid::Uuid;

    use super::*;
    use crate::{
        KnowledgeKind, KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState,
        LearningEvidenceRef, LearningObservationKind, PersonalMemoryValue,
    };

    fn request() -> StageMemoryCandidate {
        StageMemoryCandidate {
            session_id: Uuid::new_v4(),
            expected_session_revision: 1,
            turn_ids: vec![Uuid::new_v4()],
            observation_kind: LearningObservationKind::ExplicitRemember,
            digest: "Explicit preference".into(),
            value: PersonalMemoryValue {
                kind: PersonalMemoryKind::Preference,
                statement: "Prefers concise responses".into(),
                epistemic_status: EpistemicStatus::Fact,
                confidence_millis: 1000,
                valid_from: None,
                valid_until: None,
                observed_at: Utc::now(),
            },
            target_id: None,
            base_revision: None,
            extractor_version: "fixture.v1".into(),
            prompt_version: "fixture.v1".into(),
            actor: KnowledgeActor::User,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn staging_retains_actor_classification_and_duplicate_evidence_guards() {
        let mut candidate = request();
        assert_eq!(validate_stage_request(&candidate), Ok(()));
        candidate.actor = KnowledgeActor::System;
        assert_eq!(
            validate_stage_request(&candidate),
            Err(AgentFailure::PolicyDenied)
        );
        candidate.actor = KnowledgeActor::User;
        candidate.value.kind = PersonalMemoryKind::Inference;
        assert_eq!(
            validate_stage_request(&candidate),
            Err(AgentFailure::InvalidInput)
        );
        candidate.value.epistemic_status = EpistemicStatus::Inference;
        assert_eq!(validate_stage_request(&candidate), Ok(()));
        candidate.turn_ids.push(candidate.turn_ids[0]);
        assert_eq!(
            validate_stage_request(&candidate),
            Err(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn evidence_requires_current_completed_personal_session_and_actual_turn() {
        let request = request();
        let person_id = PersonId::new();
        let mut snapshot = LearningEvidenceSnapshot {
            person_id,
            session_id: request.session_id,
            revision: request.expected_session_revision,
            outcome: Some(LearningOutcome::Completed),
            personal: true,
            active_turn: false,
            pending_output: false,
            turn_ids: request.turn_ids.clone(),
            coverage: floe_context_contract::DependencyCoverage::Independent,
            purpose: EvidenceProjectionPurpose::Learning,
        };
        let validate = |snapshot: &LearningEvidenceSnapshot| {
            validate_learning_evidence(
                snapshot,
                person_id,
                request.session_id,
                request.expected_session_revision,
                &request.turn_ids,
            )
        };
        assert_eq!(validate(&snapshot), Ok(()));
        snapshot.revision += 1;
        assert_eq!(validate(&snapshot), Err(AgentFailure::Conflict));
        snapshot.revision -= 1;
        snapshot.pending_output = true;
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.pending_output = false;
        snapshot.personal = false;
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.personal = true;
        snapshot.outcome = None;
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.outcome = Some(LearningOutcome::Halted {
            reason: AgentFailure::Cancelled,
        });
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.outcome = Some(LearningOutcome::Completed);
        snapshot.coverage = floe_context_contract::DependencyCoverage::Unknown;
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.coverage = floe_context_contract::DependencyCoverage::Dependent {
            dependencies: Vec::new(),
        };
        assert_eq!(validate(&snapshot), Err(AgentFailure::VaultUnavailable));
        snapshot.coverage = floe_context_contract::DependencyCoverage::Independent;
        snapshot.purpose = EvidenceProjectionPurpose::Context;
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.purpose = EvidenceProjectionPurpose::Learning;
        snapshot.turn_ids.clear();
        assert_eq!(validate(&snapshot), Err(AgentFailure::NotFound));
    }

    #[test]
    fn memory_summary_projection_validates_owner_and_origin() {
        let person_id = PersonId::new();
        let request = request();
        let revision = KnowledgeRevision {
            schema_version: KNOWLEDGE_VERSION,
            target_id: Uuid::new_v4(),
            revision: 1,
            person_id,
            kind: KnowledgeKind::Memory,
            payload: KnowledgePayload::Memory {
                value: request.value,
            },
            state: KnowledgeRevisionState::Active,
            source_refs: vec![LearningEvidenceRef {
                session_id: request.session_id,
                turn_id: request.turn_ids[0],
            }],
            created_by: KnowledgeActor::User,
            created_at: Utc::now(),
        };
        let summary = project_memory_summary(&revision, person_id).unwrap();
        assert_eq!(summary.origin, MemoryOrigin::UserProvided);
        assert_eq!(summary.source_count, 1);
        assert_eq!(
            validate_memory_overview_limit(0),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            validate_memory_overview_limit(MAX_MEMORY_OVERVIEW_ITEMS + 1),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            project_memory_summary(&revision, PersonId::new()),
            Err(AgentFailure::VaultUnavailable)
        );
    }

    struct SequenceEvidenceReader {
        snapshots: Mutex<VecDeque<LearningEvidenceSnapshot>>,
        requests: Mutex<Vec<(PersonId, Uuid, Vec<Uuid>)>>,
    }

    impl crate::EvidenceReader for SequenceEvidenceReader {
        fn read_learning_evidence(
            &self,
            person_id: PersonId,
            session_id: Uuid,
            turn_ids: &[Uuid],
        ) -> impl std::future::Future<Output = Result<LearningEvidenceSnapshot, AgentFailure>> + Send
        {
            self.requests
                .lock()
                .unwrap()
                .push((person_id, session_id, turn_ids.to_vec()));
            let snapshot = self.snapshots.lock().unwrap().pop_front();
            async move { snapshot.ok_or(AgentFailure::VaultUnavailable) }
        }
    }

    #[tokio::test]
    async fn admission_reads_current_snapshot_and_preserves_request_identity() {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let request_turns = vec![turn_id];
        let snapshot = LearningEvidenceSnapshot {
            person_id,
            session_id,
            revision: 4,
            outcome: Some(LearningOutcome::Completed),
            personal: true,
            active_turn: false,
            pending_output: false,
            turn_ids: request_turns.clone(),
            coverage: floe_context_contract::DependencyCoverage::Independent,
            purpose: EvidenceProjectionPurpose::Learning,
        };
        let mut changed_revision = snapshot.clone();
        changed_revision.revision += 1;
        let mut changed_coverage = snapshot.clone();
        changed_coverage.coverage = floe_context_contract::DependencyCoverage::Unknown;
        let reader = SequenceEvidenceReader {
            snapshots: Mutex::new(VecDeque::from([
                snapshot,
                changed_revision,
                changed_coverage,
            ])),
            requests: Mutex::new(Vec::new()),
        };

        assert!(
            admit_learning_evidence(&reader, person_id, session_id, 4, &request_turns,)
                .await
                .is_ok()
        );
        assert_eq!(
            admit_learning_evidence(&reader, person_id, session_id, 4, &request_turns).await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            admit_learning_evidence(&reader, person_id, session_id, 4, &request_turns).await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            *reader.requests.lock().unwrap(),
            vec![
                (person_id, session_id, request_turns.clone()),
                (person_id, session_id, request_turns.clone()),
                (person_id, session_id, request_turns),
            ]
        );
    }

    #[tokio::test]
    async fn admission_rejects_malformed_owner_request_before_read() {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let reader = SequenceEvidenceReader {
            snapshots: Mutex::new(VecDeque::new()),
            requests: Mutex::new(Vec::new()),
        };
        for (person_id, session_id, revision, turn_ids) in [
            (PersonId(Uuid::nil()), session_id, 1, vec![turn_id]),
            (person_id, Uuid::nil(), 1, vec![turn_id]),
            (person_id, session_id, 1, Vec::new()),
            (person_id, session_id, 1, vec![Uuid::nil()]),
            (person_id, session_id, 1, vec![turn_id, turn_id]),
        ] {
            assert_eq!(
                admit_learning_evidence(&reader, person_id, session_id, revision, &turn_ids).await,
                Err(AgentFailure::InvalidInput)
            );
        }
        let too_many_turns = (0..33).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        assert_eq!(
            admit_learning_evidence(&reader, person_id, session_id, 1, &too_many_turns).await,
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            admit_learning_evidence(&reader, person_id, session_id, 0, &[turn_id]).await,
            Err(AgentFailure::Conflict)
        );
        assert!(reader.requests.lock().unwrap().is_empty());
    }
}
