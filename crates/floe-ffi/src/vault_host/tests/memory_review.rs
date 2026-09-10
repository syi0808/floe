use super::*;

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
