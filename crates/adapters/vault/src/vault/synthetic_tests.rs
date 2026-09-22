use super::*;
use chrono::{TimeZone, Utc};
use floe_context_contract::{EpistemicStatus, PersonalMemoryKind};
use floe_conversation::{AgentMessage, AgentOutcome, SessionStore};
use floe_knowledge::{
    KnowledgeActor, LearningObservationKind, PersonalMemoryValue, StageMemoryCandidate,
};
use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, sync::Mutex};

#[derive(Default)]
struct Keys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person, vault))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }
    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

fn memory_value(statement: &str) -> PersonalMemoryValue {
    PersonalMemoryValue {
        kind: PersonalMemoryKind::Preference,
        statement: statement.into(),
        epistemic_status: EpistemicStatus::Fact,
        confidence_millis: 1000,
        valid_from: None,
        valid_until: None,
        observed_at: Utc.with_ymd_and_hms(2026, 9, 10, 12, 0, 0).unwrap(),
    }
}

#[tokio::test]
async fn synthetic_evidence_cannot_be_promoted_to_personal_memory() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let mut sample = vault.create_sample_session().await.unwrap();
    let turn_id = Uuid::new_v4();
    sample.messages = vec![AgentMessage::User {
        turn_id,
        text: "fixture preference".into(),
    }];
    sample.revision = 1;
    sample.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&sample, 0).await.unwrap();
    let request = StageMemoryCandidate {
        session_id: sample.id,
        expected_session_revision: sample.revision,
        turn_ids: vec![turn_id],
        observation_kind: LearningObservationKind::UserCorrection,
        digest: "fixture evidence".into(),
        value: memory_value("fixture-derived memory must not persist"),
        target_id: None,
        base_revision: None,
        extractor_version: "memory.fixture.v1".into(),
        prompt_version: "correction.v1".into(),
        actor: KnowledgeActor::Learner {
            run_id: Uuid::new_v4(),
        },
        created_at: Utc.with_ymd_and_hms(2026, 9, 10, 12, 3, 0).unwrap(),
    };
    assert_eq!(
        vault.stage_memory_candidate(request).await,
        Err(AgentFailure::PolicyDenied)
    );
}
