use std::collections::HashSet;

use floe_kernel::AgentFailure;

use crate::{
    EpistemicStatus, KnowledgeActor, LearningEvidenceSnapshot, PersonalMemoryKind,
    StageMemoryCandidate,
};

const MAX_OBSERVATION_DIGEST_BYTES: usize = 4 * 1024;
const MAX_MEMORY_STATEMENT_BYTES: usize = 2 * 1024;
const MAX_EVIDENCE_REFS: usize = 32;
const MAX_VERSION_BYTES: usize = 128;

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
    if snapshot.person_id != expected_person_id
        || snapshot.session_id != expected_session_id
        || snapshot.revision != expected_revision
    {
        return Err(AgentFailure::Conflict);
    }
    if snapshot.active_turn || snapshot.pending_output || !snapshot.completed || !snapshot.personal
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if turn_ids
        .iter()
        .any(|turn_id| !snapshot.turn_ids.contains(turn_id))
    {
        return Err(AgentFailure::NotFound);
    }
    Ok(())
}

fn valid_version(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_VERSION_BYTES
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use floe_kernel::PersonId;
    use uuid::Uuid;

    use super::*;
    use crate::{LearningObservationKind, PersonalMemoryValue};

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
            completed: true,
            personal: true,
            active_turn: false,
            pending_output: false,
            turn_ids: request.turn_ids.clone(),
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
        snapshot.completed = false;
        assert_eq!(validate(&snapshot), Err(AgentFailure::PolicyDenied));
        snapshot.completed = true;
        snapshot.turn_ids.clear();
        assert_eq!(validate(&snapshot), Err(AgentFailure::NotFound));
    }
}
