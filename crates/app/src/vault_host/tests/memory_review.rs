use super::*;
use chrono::Utc;
use floe_conversation::{AgentMessage, AgentOutcome};
use floe_knowledge::{
    EpistemicStatus, LearningObservationKind, PersonalMemoryKind, PersonalMemoryValue,
    StageMemoryCandidate,
};

#[test]
fn memory_review_requires_an_unlocked_vault_and_returns_pending_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let inspect = AgentVaultActionDto::MemoryReview { decision: None };

    assert_eq!(
        perform(&worker, person, inspect.clone()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    perform(&worker, person, AgentVaultActionDto::Create {});

    let overview = perform(&worker, person, inspect).memory_review.unwrap();
    assert_eq!(overview.schema_version, PROTOCOL_VERSION);
    assert_eq!(overview.person_id, person.to_string());
    assert!(overview.candidates.is_empty());
    assert!(overview.decision.is_none());
}

#[test]
fn memory_review_rejects_invalid_or_unknown_candidate_ids() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});

    for (candidate_id, failure) in [
        ("invalid".to_owned(), AgentFailure::InvalidInput),
        (Uuid::new_v4().to_string(), AgentFailure::NotFound),
    ] {
        let result = perform(
            &worker,
            person,
            AgentVaultActionDto::MemoryReview {
                decision: Some(AgentMemoryReviewDecisionDto {
                    candidate_id,
                    decision: AgentMemoryReviewDecisionKindDto::Approve,
                }),
            },
        );
        assert_eq!(result.failure, Some(failure));
        assert!(result.memory_review.is_none());
    }
}

#[test]
fn memory_overview_requires_an_unlocked_vault_and_is_initially_empty() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let inspect = AgentVaultActionDto::Memory {};

    assert_eq!(
        perform(&worker, person, inspect.clone()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    perform(&worker, person, AgentVaultActionDto::Create {});

    let overview = perform(&worker, person, inspect).memory.unwrap();
    assert_eq!(overview.schema_version, PROTOCOL_VERSION);
    assert_eq!(overview.person_id, person.to_string());
    assert_eq!(overview.saved_count, 0);
    assert_eq!(overview.pending_count, 0);
    assert!(overview.memories.is_empty());
}

#[test]
fn memory_snapshots_follow_review_decisions_and_survive_unlock() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let keys = Keys::default();
    let person = PersonId::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let candidates = runtime.block_on(async {
        let vault = EncryptedAgentVault::create(&root, person, keys.clone())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn_id = Uuid::new_v4();
        session.messages = vec![AgentMessage::User {
            turn_id,
            text: "Preference fixture".into(),
        }];
        session.revision = 1;
        session.last_outcome = Some(AgentOutcome::Completed);
        vault
            .governed_general_store(session.id)
            .compare_and_swap(&session, 0)
            .await
            .unwrap();
        let mut candidates = Vec::new();
        for statement in ["Prefers focused mornings", "Prefers afternoon meetings"] {
            let now = Utc::now();
            candidates.push(
                vault
                    .stage_memory_candidate(StageMemoryCandidate {
                        session_id: session.id,
                        expected_session_revision: session.revision,
                        turn_ids: vec![turn_id],
                        observation_kind: LearningObservationKind::ExplicitRemember,
                        digest: statement.into(),
                        value: PersonalMemoryValue {
                            kind: PersonalMemoryKind::Preference,
                            statement: statement.into(),
                            epistemic_status: EpistemicStatus::Fact,
                            confidence_millis: 1000,
                            valid_from: None,
                            valid_until: None,
                            observed_at: now,
                        },
                        target_id: None,
                        base_revision: None,
                        extractor_version: "fixture.v1".into(),
                        prompt_version: "fixture.v1".into(),
                        actor: KnowledgeActor::User,
                        created_at: now,
                    })
                    .await
                    .unwrap(),
            );
        }
        candidates
    });
    let worker = Worker::new(root, keys).unwrap();
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Unlock {}).failure,
        None
    );
    let initial = perform(&worker, person, AgentVaultActionDto::Memory {})
        .memory
        .unwrap();
    assert_eq!((initial.saved_count, initial.pending_count), (0, 2));
    let pending = perform(
        &worker,
        person,
        AgentVaultActionDto::MemoryReview { decision: None },
    )
    .memory_review
    .unwrap();
    assert_eq!(pending.candidates.len(), 2);
    for (candidate, decision) in candidates.iter().zip([
        AgentMemoryReviewDecisionKindDto::Approve,
        AgentMemoryReviewDecisionKindDto::Reject,
    ]) {
        let reviewed = perform(
            &worker,
            person,
            AgentVaultActionDto::MemoryReview {
                decision: Some(AgentMemoryReviewDecisionDto {
                    candidate_id: candidate.id.to_string(),
                    decision,
                }),
            },
        )
        .memory_review
        .unwrap();
        assert!(reviewed.decision.is_some());
        assert!(
            !reviewed
                .candidates
                .iter()
                .any(|pending| pending.0["id"] == candidate.id.to_string())
        );
    }
    let saved = perform(&worker, person, AgentVaultActionDto::Memory {})
        .memory
        .unwrap();
    assert_eq!((saved.saved_count, saved.pending_count), (1, 0));
    assert_eq!(saved.person_id, person.to_string());
    assert_eq!(saved.memories.len(), 1);
    let summary = &saved.memories[0];
    assert_eq!(summary.revision, 1);
    assert_eq!(summary.statement, "Prefers focused mornings");
    assert_eq!(summary.origin, AgentMemoryOriginDto::UserProvided);
    assert_eq!(summary.source_count, 1);
    assert_eq!(summary.confidence_millis, 1000);
    assert_eq!(summary.memory_kind.0, json!("preference"));
    assert_eq!(summary.epistemic_status.0, json!("fact"));
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Lock {}).failure,
        None
    );
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Unlock {}).failure,
        None
    );
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Memory {})
            .memory
            .unwrap(),
        saved
    );
}
