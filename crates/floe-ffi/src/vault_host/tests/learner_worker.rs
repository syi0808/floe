use super::*;
use chrono::Utc;
use floe_agent::{
    AgentMessage, AgentOutcome, KNOWLEDGE_VERSION, LearnerJobState, LearnerModel,
    LearnerModelRequest, LearnerReviewInput, LearnerReviewOutput, ModelPlacement, SessionStore,
};
use std::os::unix::fs::PermissionsExt;

struct FailingModel(AgentFailure);

struct NoChangeModel;

impl LearnerModel for NoChangeModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn review(
        &self,
        _request: LearnerModelRequest,
    ) -> Result<LearnerReviewOutput, AgentFailure> {
        Ok(LearnerReviewOutput {
            schema_version: KNOWLEDGE_VERSION,
            proposal: None,
            used_tokens: 1,
            cost_micros: 0,
        })
    }
}

#[tokio::test]
async fn owner_service_discovers_claims_and_settles_without_a_candidate() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let vault = EncryptedAgentVault::create(directory.path(), PersonId::new(), Keys::default())
        .await
        .unwrap();
    let mut session = vault.create_session().await.unwrap();
    let turn_id = Uuid::new_v4();
    session.messages = vec![
        AgentMessage::User {
            turn_id,
            text: "Please remember for later that I prefer afternoon meetings.".into(),
        },
        AgentMessage::Assistant {
            turn_id,
            text: "I will prepare the preference for review.".into(),
        },
    ];
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault
        .governed_general_store(session.id)
        .compare_and_swap(&session, 0)
        .await
        .unwrap();
    let service = floe_knowledge::LearnerService {
        repository: &vault,
        model: &NoChangeModel,
    };
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert_eq!(service.run_next(cancelled).await, Ok(false));
    assert!(
        vault
            .claim_learner_review(Utc::now())
            .await
            .unwrap()
            .is_none()
    );
    let queued = vault
        .discover_explicit_learner_reviews(Utc::now(), 1)
        .await
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(service.run_next(Cancellation::default()).await, Ok(true));
    let settled = vault
        .enqueue_learner_review(queued[0].input.clone(), Utc::now())
        .await
        .unwrap();
    assert_eq!(settled.state, LearnerJobState::Completed);
    assert_eq!(settled.attempts, 1);
    assert!(
        vault
            .memory_review_snapshot()
            .await
            .unwrap()
            .candidates
            .is_empty()
    );
    assert_eq!(service.run_next(Cancellation::default()).await, Ok(false));
    assert_eq!(
        vault.load(session.person_id, session.id).await.unwrap(),
        session
    );
}

impl LearnerModel for FailingModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn review(
        &self,
        _request: LearnerModelRequest,
    ) -> Result<LearnerReviewOutput, AgentFailure> {
        Err(self.0)
    }
}

#[tokio::test]
async fn storage_failure_escapes_without_terminal_settlement_but_model_failure_is_local() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(directory.path(), person, Keys::default())
        .await
        .unwrap();
    let mut session = vault.create_session().await.unwrap();
    let turn_id = Uuid::new_v4();
    session.messages = vec![AgentMessage::User {
        turn_id,
        text: "Remember my preference".into(),
    }];
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault
        .governed_general_store(session.id)
        .compare_and_swap(&session, 0)
        .await
        .unwrap();
    for failure in [
        AgentFailure::VaultUnavailable,
        AgentFailure::StorageUnavailable,
        AgentFailure::InvalidModelOutput,
    ] {
        let now = Utc::now();
        let input = LearnerReviewInput {
            schema_version: KNOWLEDGE_VERSION,
            run_id: Uuid::new_v4(),
            person_id: person,
            session_id: session.id,
            session_revision: session.revision,
            turn_ids: vec![turn_id],
            outcome: AgentOutcome::Completed.into(),
            digest: format!("Distinct fixture for {failure:?}"),
            current_memories: vec![],
            observed_at: now,
        };
        let queued = vault
            .enqueue_learner_review(input.clone(), now)
            .await
            .unwrap();
        let claimed = vault.claim_learner_review(now).await.unwrap().unwrap();
        assert_eq!(claimed.id, queued.id);
        let result = floe_knowledge::LearnerService {
            repository: &vault,
            model: &FailingModel(failure),
        }
        .review_claimed(&claimed, Cancellation::default())
        .await;
        let stored = vault.enqueue_learner_review(input, now).await.unwrap();
        if failure == AgentFailure::InvalidModelOutput {
            assert_eq!(result, Ok(true));
            assert_eq!(stored.state, LearnerJobState::Failed);
            assert_eq!(stored.last_failure, Some(failure));
        } else {
            assert_eq!(result, Err(failure));
            assert_eq!(stored, claimed);
        }
    }
}
