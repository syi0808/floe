use super::*;
use chrono::Utc;
use floe_agent::{
    AgentMessage, AgentOutcome, KNOWLEDGE_VERSION, LearnerJobState, LearnerModel,
    LearnerModelRequest, LearnerReviewInput, LearnerReviewOutput, ModelPlacement, SessionStore,
};
use std::os::unix::fs::PermissionsExt;

struct FailingModel(AgentFailure);

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
            outcome: AgentOutcome::Completed,
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
        let result = crate::vault_host::learner_worker::review_claimed(
            &vault,
            &claimed,
            &FailingModel(failure),
            Cancellation::default(),
        )
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
