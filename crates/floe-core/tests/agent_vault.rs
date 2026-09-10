#![cfg(unix)]

use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use chrono::{TimeZone, Utc};
use floe_agent::*;
use floe_core::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use floe_domain::PersonId;
use uuid::Uuid;

#[path = "agent_vault/calendar_sessions.rs"]
mod calendar_sessions;

#[derive(Clone, Default)]
struct Keys(Arc<KeyState>);

#[derive(Default)]
struct KeyState {
    values: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
    blocked: AtomicBool,
    inserts: AtomicUsize,
}

fn private_root() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        if self.0.blocked.load(Ordering::SeqCst) {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.0
            .values
            .lock()
            .unwrap()
            .get(&(person, vault))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        self.0.inserts.fetch_add(1, Ordering::SeqCst);
        if self.0.blocked.load(Ordering::SeqCst) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut values = self.0.values.lock().unwrap();
        if values.contains_key(&(person, vault)) {
            return Err(AgentFailure::VaultUnavailable);
        }
        values.insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

fn assert_no_plaintext(directory: &Path, markers: &[&str]) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = fs::read(path).unwrap();
            for marker in markers {
                assert!(
                    !bytes
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes()),
                    "Synthetic secret marker leaked to disk"
                );
            }
        }
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
async fn reviewed_memory_candidate_is_idempotent_ledgered_and_persistent() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let mut session = vault.create_session().await.unwrap();
    let turn_id = Uuid::new_v4();
    let correction_turn_id = Uuid::new_v4();
    session.messages = vec![
        AgentMessage::User {
            turn_id,
            text: "회의는 오전보다 오후를 선호한다고 기억해줘".into(),
        },
        AgentMessage::Assistant {
            turn_id,
            text: "검토할 기억으로 준비했어요.".into(),
        },
        AgentMessage::User {
            turn_id: correction_turn_id,
            text: "정확히는 14시 이후 회의를 선호해.".into(),
        },
        AgentMessage::Assistant {
            turn_id: correction_turn_id,
            text: "정정 내용을 별도 검토 항목으로 준비할게요.".into(),
        },
    ];
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&session, 0).await.unwrap();
    let created_at = Utc.with_ymd_and_hms(2026, 9, 10, 12, 1, 0).unwrap();
    let request = StageMemoryCandidate {
        session_id: session.id,
        expected_session_revision: session.revision,
        turn_ids: vec![turn_id],
        observation_kind: LearningObservationKind::ExplicitRemember,
        digest: "사용자가 회의 시간대 선호를 명시했다.".into(),
        value: memory_value("사용자는 회의를 오전보다 오후에 선호한다."),
        target_id: None,
        base_revision: None,
        extractor_version: "memory.fixture.v1".into(),
        prompt_version: "explicit-memory.v1".into(),
        actor: KnowledgeActor::User,
        created_at,
    };

    let mut stale_request = request.clone();
    stale_request.expected_session_revision = 0;
    assert_eq!(
        vault.stage_memory_candidate(stale_request).await,
        Err(AgentFailure::Conflict)
    );

    let candidate = vault.stage_memory_candidate(request.clone()).await.unwrap();
    assert_eq!(candidate.state, KnowledgeCandidateState::Pending);
    assert_eq!(
        vault.stage_memory_candidate(request).await.unwrap(),
        candidate
    );
    assert_eq!(
        vault
            .pending_knowledge_candidates()
            .await
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&candidate)
    );
    assert!(vault.active_personal_memories().await.unwrap().is_empty());

    let decided_at = Utc.with_ymd_and_hms(2026, 9, 10, 12, 2, 0).unwrap();
    let result = vault
        .decide_knowledge_candidate(
            candidate.id,
            KnowledgeDecisionKind::Approve,
            KnowledgeActor::User,
            decided_at,
        )
        .await
        .unwrap();
    assert_eq!(result.candidate.state, KnowledgeCandidateState::Approved);
    let revision = result.revision.unwrap();
    assert_eq!(revision.revision, 1);
    assert_eq!(revision.state, KnowledgeRevisionState::Active);
    assert_eq!(result.mutation.as_ref().unwrap().to_revision, 1);
    assert!(result.mutation.as_ref().unwrap().from_revision.is_none());
    assert!(
        vault
            .pending_knowledge_candidates()
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        vault.active_personal_memories().await.unwrap().as_slice(),
        std::slice::from_ref(&revision)
    );
    assert_eq!(
        vault.knowledge_mutations(revision.target_id).await.unwrap(),
        [result.mutation.unwrap()]
    );
    assert_eq!(
        vault
            .decide_knowledge_candidate(
                candidate.id,
                KnowledgeDecisionKind::Approve,
                KnowledgeActor::User,
                decided_at,
            )
            .await,
        Err(AgentFailure::Conflict)
    );

    let revised_candidate = vault
        .stage_memory_candidate(StageMemoryCandidate {
            session_id: session.id,
            expected_session_revision: session.revision,
            turn_ids: vec![correction_turn_id],
            observation_kind: LearningObservationKind::UserCorrection,
            digest: "사용자가 회의 선호의 구체적인 시작 시각을 정정했다.".into(),
            value: memory_value("사용자는 회의를 14시 이후에 선호한다."),
            target_id: Some(revision.target_id),
            base_revision: Some(1),
            extractor_version: "memory.fixture.v1".into(),
            prompt_version: "explicit-memory.v1".into(),
            actor: KnowledgeActor::User,
            created_at: Utc.with_ymd_and_hms(2026, 9, 10, 12, 7, 0).unwrap(),
        })
        .await
        .unwrap();
    let revised = vault
        .decide_knowledge_candidate(
            revised_candidate.id,
            KnowledgeDecisionKind::Approve,
            KnowledgeActor::User,
            Utc.with_ymd_and_hms(2026, 9, 10, 12, 8, 0).unwrap(),
        )
        .await
        .unwrap();
    let active_revision = revised.revision.unwrap();
    assert_eq!(active_revision.revision, 2);
    assert_eq!(revised.mutation.as_ref().unwrap().from_revision, Some(1));
    assert_eq!(
        revised.mutation.as_ref().unwrap().rollback_revision,
        Some(1)
    );
    assert_eq!(
        vault.active_personal_memories().await.unwrap().as_slice(),
        std::slice::from_ref(&active_revision)
    );
    let context = vault
        .personal_memory_context(Utc.with_ymd_and_hms(2026, 9, 10, 12, 9, 0).unwrap())
        .await
        .unwrap();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].target_id, active_revision.target_id);
    assert_eq!(context[0].revision, 2);
    assert_eq!(context[0].source_refs[0].turn_id, correction_turn_id);
    assert_eq!(
        context[0].statement,
        "사용자는 회의를 14시 이후에 선호한다."
    );

    drop(vault);
    let vault = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(
        vault.active_personal_memories().await.unwrap(),
        [active_revision]
    );
    assert_eq!(
        vault
            .knowledge_mutations(revision.target_id)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn memory_review_rejects_untrusted_sources_and_non_user_decisions() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys)
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

    let mut personal = vault.create_session().await.unwrap();
    personal.messages = vec![AgentMessage::User {
        turn_id,
        text: "personal preference".into(),
    }];
    personal.revision = 1;
    personal.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&personal, 0).await.unwrap();
    let candidate = vault
        .stage_memory_candidate(StageMemoryCandidate {
            session_id: personal.id,
            expected_session_revision: personal.revision,
            turn_ids: vec![turn_id],
            observation_kind: LearningObservationKind::UserCorrection,
            digest: "사용자가 선호를 정정했다.".into(),
            value: memory_value("사용자는 오후 회의를 선호한다."),
            target_id: None,
            base_revision: None,
            extractor_version: "memory.fixture.v1".into(),
            prompt_version: "correction.v1".into(),
            actor: KnowledgeActor::Learner {
                run_id: Uuid::new_v4(),
            },
            created_at: Utc.with_ymd_and_hms(2026, 9, 10, 12, 4, 0).unwrap(),
        })
        .await
        .unwrap();
    assert_eq!(
        vault
            .decide_knowledge_candidate(
                candidate.id,
                KnowledgeDecisionKind::Approve,
                KnowledgeActor::System,
                Utc.with_ymd_and_hms(2026, 9, 10, 12, 5, 0).unwrap(),
            )
            .await,
        Err(AgentFailure::PolicyDenied)
    );
    let rejected = vault
        .decide_knowledge_candidate(
            candidate.id,
            KnowledgeDecisionKind::Reject,
            KnowledgeActor::User,
            Utc.with_ymd_and_hms(2026, 9, 10, 12, 6, 0).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.candidate.state, KnowledgeCandidateState::Rejected);
    assert!(rejected.revision.is_none());
    assert!(rejected.mutation.is_none());
    assert!(vault.active_personal_memories().await.unwrap().is_empty());
}

#[tokio::test]
async fn session_archive_search_compaction_and_recovery_survive_reopen() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let mut session = vault.create_session().await.unwrap();
    let archived_turn = Uuid::new_v4();
    let retained_turn = Uuid::new_v4();
    session.messages = vec![
        AgentMessage::User {
            turn_id: archived_turn,
            text: "제주 워케이션 숙소를 찾아줘".into(),
        },
        AgentMessage::Assistant {
            turn_id: archived_turn,
            text: "조용한 숙소를 우선할게요.".into(),
        },
        AgentMessage::User {
            turn_id: retained_turn,
            text: "다음 주 일정도 확인해줘".into(),
        },
        AgentMessage::Assistant {
            turn_id: retained_turn,
            text: "일정을 확인했어요.".into(),
        },
    ];
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&session, 0).await.unwrap();

    let live_hits = vault.search_sessions("워케이션 숙소", 10).await.unwrap();
    assert_eq!(live_hits.len(), 1);
    assert_eq!(live_hits[0].session_id, session.id);
    assert!(live_hits[0].recovery.is_none());

    let compacted = vault
        .compact_session(
            session.id,
            1,
            archived_turn,
            "사용자는 제주 워케이션에서 조용한 숙소를 원했다.".into(),
        )
        .await
        .unwrap();
    assert_eq!(compacted.session.revision, 2);
    assert_eq!(compacted.session.messages.len(), 3);
    assert!(matches!(
        &compacted.session.messages[0],
        AgentMessage::Compaction { recovery, .. } if recovery == &compacted.recovery
    ));
    assert_eq!(
        vault.recover_session(&compacted.recovery).await.unwrap(),
        session
    );
    assert_eq!(
        vault
            .compact_session(session.id, 1, retained_turn, "stale compaction".into(),)
            .await,
        Err(AgentFailure::Conflict)
    );

    drop(vault);
    let vault = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    let recovered = vault.recover_session(&compacted.recovery).await.unwrap();
    assert_eq!(recovered, session);
    let archived_hits = vault.search_sessions("제주 워케이션", 10).await.unwrap();
    assert!(archived_hits.iter().any(|hit| {
        hit.recovery.as_ref() == Some(&compacted.recovery)
            && hit.session_revision == compacted.recovery.source_revision
    }));
    assert_eq!(
        vault.load(person, session.id).await.unwrap(),
        compacted.session
    );
}

#[tokio::test]
async fn encrypted_messages_and_tool_results_survive_wal_and_checkpoint_reopen() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    assert_eq!(vault.protection(), SessionProtection::Encrypted);
    let mut session = vault.create_session().await.unwrap();
    let turn = Uuid::new_v4();
    let markers = [
        "synthetic-private-user-931ea",
        "synthetic-tool-input-7db83",
        "synthetic-tool-result-236b6",
        "synthetic-answer-111f4",
        "synthetic-provider-replay-c914e",
        "synthetic-expert-private-result-632ba",
        "synthetic-pending-output-882fb",
    ];
    session.messages = vec![
        AgentMessage::User {
            turn_id: turn,
            text: markers[0].into(),
        },
        AgentMessage::Capability {
            turn_id: turn,
            call_id: Uuid::new_v4(),
            capability_id: "fixture.read.v1".into(),
            input: markers[1].into(),
            result: Ok(markers[2].into()),
        },
        AgentMessage::Assistant {
            turn_id: turn,
            text: markers[3].into(),
        },
    ];
    let AgentMessage::Capability { call_id, .. } = session.messages[1] else {
        panic!("missing synthetic call")
    };
    session.capability_executions.push(CapabilityExecution {
        scope_id: session.id,
        result: Some(Ok(markers[2].into())),
        turn_id: turn,
        call_id,
        capability_id: "fixture.read.v1".into(),
        input: markers[1].into(),
        state: CapabilityExecutionState::Settled,
        replay: Some(ProviderReplay {
            gateway: "http://127.0.0.1:8431".into(),
            purpose: "everyday_assistance".into(),
            external: true,
            source: "a".repeat(64),
            call_ids: vec!["provider-original".into()],
            preamble: String::new(),
            provider_call_id: "provider-original".into(),
            items: serde_json::json!([{"type":"reasoning","encrypted_content":markers[4]}]),
        }),
    });
    let mut child = session.capability_executions[0].clone();
    child.scope_id = Uuid::new_v4();
    child.turn_id = child.scope_id;
    child.call_id = Uuid::new_v4();
    child.capability_id = "schedule.find_free_windows".into();
    child.result = Some(Ok(markers[5].into()));
    session.capability_executions.push(child);
    session.active_turn = Some(turn);
    session.pending_output = Some(vec![floe_agent::ModelStep::Preamble {
        text: markers[6].into(),
    }]);
    let attempt_id = Uuid::new_v4();
    session.model_attempts.push(ModelAttemptRecord {
        id: attempt_id,
        turn_id: turn,
        scope_id: session.id,
        attempt: 1,
        placement: ModelPlacement::DeviceLocal,
        state: ModelAttemptState::Accepted,
        failure: None,
        usage: ModelUsage {
            attempts: 1,
            tokens: 10,
            ..ModelUsage::default()
        },
    });
    session.usage = AgentUsage {
        model_attempts: 1,
        tokens: 10,
        ..AgentUsage::default()
    };
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&session, 0).await.unwrap();
    let directory = root.path().join(person.to_string());
    assert!(
        fs::metadata(directory.join("sessions.db-wal"))
            .unwrap()
            .len()
            > 0
    );
    assert_no_plaintext(&directory, &markers);
    assert_no_plaintext(&directory, &[&attempt_id.to_string()]);
    vault.checkpoint().await.unwrap();
    assert!(fs::metadata(directory.join("sessions.db")).unwrap().len() > 4096);
    assert_no_plaintext(&directory, &markers);
    assert_no_plaintext(&directory, &[&attempt_id.to_string()]);
    drop(vault);
    assert!(
        turso::Builder::new_local(directory.join("sessions.db").to_str().unwrap())
            .build()
            .await
            .is_err()
    );
    let vault = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(vault.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn vaults_enforce_person_revision_version_and_size_boundaries() {
    let root = private_root();
    let person = PersonId::new();
    let other = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let second = EncryptedAgentVault::create(root.path(), other, keys.clone())
        .await
        .unwrap();
    let original = vault.create_session().await.unwrap();
    assert_eq!(
        vault.load(other, original.id).await,
        Err(AgentFailure::NotFound)
    );
    assert_eq!(
        second.load(other, original.id).await,
        Err(AgentFailure::NotFound)
    );
    let mut next = original.clone();
    next.revision = 1;
    assert_eq!(
        second.compare_and_swap(&next, 0).await,
        Err(AgentFailure::NotFound)
    );
    vault.compare_and_swap(&next, 0).await.unwrap();
    assert_eq!(
        vault.compare_and_swap(&next, 0).await,
        Err(AgentFailure::Conflict)
    );
    next.revision = 3;
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::Conflict)
    );
    next.revision = 2;
    next.schema_version = 99;
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::UnsupportedVersion)
    );
    next.schema_version = AGENT_VERSION;
    next.data_classes = vec![DataClass::Credential];
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::PolicyDenied)
    );
    next.data_classes = vec![DataClass::Personal];
    next.messages.push(AgentMessage::User {
        turn_id: Uuid::new_v4(),
        text: "x".repeat(AgentBudget::default().max_session_bytes),
    });
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::BudgetExceeded)
    );
    assert_eq!(vault.load(person, original.id).await.unwrap().revision, 1);
    let values = keys.0.values.lock().unwrap();
    assert_eq!(values.len(), 2);
    let mut keys = values.values();
    assert!(keys.next() != keys.next());
}

#[tokio::test]
async fn missing_wrong_and_revoked_keys_never_create_replacement_data() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    keys.0.blocked.store(true, Ordering::SeqCst);
    assert_eq!(
        vault.load(person, session.id).await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(vault.protection(), SessionProtection::KeyUnavailable);
    keys.0.blocked.store(false, Ordering::SeqCst);
    assert_eq!(
        vault.create_session().await,
        Err(AgentFailure::VaultUnavailable)
    );
    drop(vault);
    let path = root.path().join(person.to_string()).join("sessions.db");
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, Keys::default()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    let saved = keys.0.values.lock().unwrap().clone();
    for key in keys.0.values.lock().unwrap().values_mut() {
        *key = [0; 32];
    }
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(keys.0.inserts.load(Ordering::SeqCst), 1);
    *keys.0.values.lock().unwrap() = saved;
    let reopened = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(reopened.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn failed_provisioning_and_missing_database_are_not_silently_reinitialized() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    keys.0.blocked.store(true, Ordering::SeqCst);
    assert!(matches!(
        EncryptedAgentVault::create(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    keys.0.blocked.store(false, Ordering::SeqCst);
    assert!(matches!(
        EncryptedAgentVault::create(root.path(), person, keys.clone()).await,
        Err(AgentFailure::Conflict)
    ));
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    assert_eq!(keys.0.inserts.load(Ordering::SeqCst), 1);
    assert!(
        !root
            .path()
            .join(person.to_string())
            .join("sessions.db")
            .exists()
    );
    let other = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), other, keys.clone())
        .await
        .unwrap();
    drop(vault);
    let path = root.path().join(other.to_string()).join("sessions.db");
    fs::remove_file(&path).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), other, keys.clone())
            .await
            .is_err()
    );
    assert!(!path.exists());
    fs::write(&path, []).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), other, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
}

#[tokio::test]
async fn ciphertext_tampering_fails_closed() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    vault.create_session().await.unwrap();
    vault.checkpoint().await.unwrap();
    drop(vault);
    let path = root.path().join(person.to_string()).join("sessions.db");
    let mut ciphertext = fs::read(&path).unwrap();
    ciphertext[200] ^= 0x40;
    fs::write(&path, &ciphertext).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), ciphertext);
}

#[tokio::test]
async fn plaintext_database_is_not_opened_or_automatically_migrated() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    drop(vault);
    let source = root.path().join("synthetic-plaintext.db");
    let database = turso::Builder::new_local(source.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE TABLE fixture (payload TEXT)", ())
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO fixture VALUES ('synthetic-plaintext-marker')",
            (),
        )
        .await
        .unwrap();
    let mut rows = connection
        .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
        .await
        .unwrap();
    while rows.next().await.unwrap().is_some() {}
    drop(rows);
    drop(connection);
    drop(database);
    let destination = root.path().join(person.to_string()).join("sessions.db");
    fs::copy(&source, &destination).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::read(source).unwrap(), fs::read(destination).unwrap());
}

#[tokio::test]
async fn encrypted_identity_rejects_relabeling_even_with_a_matching_key() {
    let root = private_root();
    let person = PersonId::new();
    let other = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    vault.checkpoint().await.unwrap();
    drop(vault);
    let second = EncryptedAgentVault::create(root.path(), other, keys.clone())
        .await
        .unwrap();
    drop(second);
    let source = root.path().join(person.to_string());
    let destination = root.path().join(other.to_string());
    for name in ["vault.id", "sessions.db"] {
        fs::copy(source.join(name), destination.join(name)).unwrap();
    }
    let vault_id = Uuid::parse_str(&fs::read_to_string(source.join("vault.id")).unwrap()).unwrap();
    let key = keys.load(person, vault_id).unwrap();
    keys.insert(other, vault_id, &key).unwrap();
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), other, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    let original = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(original.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn insecure_and_symlinked_paths_are_rejected() {
    let root = private_root();
    let person = PersonId::new();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        EncryptedAgentVault::create(root.path(), person, Keys::default())
            .await
            .is_err()
    );
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    drop(vault);
    let directory = root.path().join(person.to_string());
    let lock = directory.join("host.lock");
    fs::remove_file(&lock).unwrap();
    let target = root.path().join("unrelated");
    fs::write(&target, "untouched").unwrap();
    symlink(&target, lock).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
}

#[tokio::test]
async fn a_second_handle_or_process_cannot_steal_a_live_vault() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::Conflict)
    ));
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "vault_lock_child"])
        .env("FLOE_TEST_VAULT_ROOT", root.path())
        .env("FLOE_TEST_VAULT_PERSON", person.to_string())
        .status()
        .unwrap();
    assert!(status.success());
    drop(vault);
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn vault_lock_child() {
    let Ok(root) = std::env::var("FLOE_TEST_VAULT_ROOT") else {
        return;
    };
    let person =
        PersonId(Uuid::parse_str(&std::env::var("FLOE_TEST_VAULT_PERSON").unwrap()).unwrap());
    assert!(matches!(
        EncryptedAgentVault::open(Path::new(&root), person, Keys::default()).await,
        Err(AgentFailure::Conflict)
    ));
}

struct LocalModel {
    keys: Keys,
    revoke: bool,
    calls: AtomicUsize,
}

impl ModelRunner for LocalModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn generate(&self, _: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.revoke {
            self.keys.0.blocked.store(true, Ordering::SeqCst);
        }
        Ok(ModelResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            output: vec![ModelStep::Answer {
                text: "synthetic-private-answer".into(),
            }],
            used_tokens: 10,
            cost_micros: 0,
        })
    }
}

struct NoCapabilities;

impl CapabilityHost for NoCapabilities {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        vec![]
    }

    async fn invoke(&self, _: CapabilityInvocation) -> Result<String, AgentFailure> {
        panic!("No capability may be replayed during vault recovery")
    }
}

fn local_policy() -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "synthetic-vault-test".into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![ModelPlacement::DeviceLocal],
        performance_class: "fixture".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

#[tokio::test]
async fn runtime_fails_closed_on_key_loss_and_recovers_without_model_replay() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    let model = LocalModel {
        keys: keys.clone(),
        revoke: true,
        calls: AtomicUsize::new(0),
    };
    let policy = local_policy();
    let command = AgentCommand {
        schema_version: AGENT_VERSION,
        person_id: person,
        session_id: session.id,
        expected_revision: 0,
        text: "synthetic-private-question".into(),
    };
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        evidence: vec![],
    };
    let mut events = vec![];
    let runtime = AgentRuntime {
        store: &vault,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    assert_eq!(
        runtime
            .run_turn(
                command.clone(),
                context.clone(),
                Cancellation::default(),
                |event| events.push(event)
            )
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert!(!events.iter().any(|event| matches!(
        event.event,
        AgentEventKind::Finished {
            outcome: AgentOutcome::Completed,
            ..
        }
    )));
    assert!(!events.iter().any(|event| matches!(
        event.event,
        AgentEventKind::MessageCommitted {
            message: AgentMessage::Assistant { .. },
            ..
        }
    )));
    assert_eq!(
        runtime
            .run_turn(
                command.clone(),
                context.clone(),
                Cancellation::default(),
                |_| {}
            )
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    drop(vault);
    keys.0.blocked.store(false, Ordering::SeqCst);
    let reopened = EncryptedAgentVault::open(root.path(), person, keys.clone())
        .await
        .unwrap();
    let interrupted = reopened.load(person, session.id).await.unwrap();
    assert!(interrupted.active_turn.is_some());
    assert_eq!(interrupted.messages.len(), 1);
    let runtime = AgentRuntime {
        store: &reopened,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    let recovered = runtime
        .recover_interrupted(person, session.id, interrupted.revision)
        .await
        .unwrap();
    assert_eq!(recovered.messages, interrupted.messages);
    assert_eq!(
        recovered.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Interrupted
        })
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(reopened.load(person, session.id).await.unwrap(), recovered);
    let model = LocalModel {
        keys,
        revoke: false,
        calls: AtomicUsize::new(0),
    };
    let runtime = AgentRuntime {
        store: &reopened,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    let completed = runtime
        .run_turn(
            AgentCommand {
                expected_revision: recovered.revision,
                ..command
            },
            context,
            Cancellation::default(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(completed.last_outcome, Some(AgentOutcome::Completed));
    assert_eq!(completed.messages.len(), 3);
}
