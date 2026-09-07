use floe_agent::{AgentFailure, AgentMessage, AgentOutcome};
use floe_core::{AgentFixturePrompt, FloeCore};
use floe_domain::PersonId;

#[tokio::test]
async fn fixture_conversation_survives_reopen_and_keeps_person_and_revision_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent.db");
    let person = PersonId::new();
    let core = FloeCore::open(&path).await.unwrap();
    let initial = core.start_agent_fixture(person).await.unwrap();
    let result = core
        .run_agent_fixture(
            person,
            initial.id,
            initial.revision,
            AgentFixturePrompt::Today,
        )
        .await
        .unwrap();
    assert_eq!(result.session.last_outcome, Some(AgentOutcome::Completed));
    assert_eq!(result.session.messages.len(), 3);
    assert!(matches!(
        result.session.messages[1],
        AgentMessage::Capability { .. }
    ));
    assert!(!result.events.is_empty());
    assert_eq!(
        core.agent_fixture_session(PersonId::new(), initial.id)
            .await,
        Err(AgentFailure::NotFound)
    );
    assert!(matches!(
        core.run_agent_fixture(person, initial.id, 0, AgentFixturePrompt::Today)
            .await,
        Err(AgentFailure::Conflict)
    ));
    drop(core);
    let core = FloeCore::open(&path).await.unwrap();
    let restored = core
        .agent_fixture_session(person, initial.id)
        .await
        .unwrap();
    assert_eq!(restored, result.session);
    let next = core
        .run_agent_fixture(
            person,
            restored.id,
            restored.revision,
            AgentFixturePrompt::FollowUp,
        )
        .await
        .unwrap();
    assert_eq!(next.session.messages.len(), 5);
    assert_eq!(&next.session.messages[..3], restored.messages.as_slice());
}

#[tokio::test]
async fn fixture_stall_and_retry_are_durable_without_external_actions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stall.db");
    let person = PersonId::new();
    let core = FloeCore::open(&path).await.unwrap();
    let session = core.start_agent_fixture(person).await.unwrap();
    let stalled = core
        .run_agent_fixture(person, session.id, 0, AgentFixturePrompt::RepeatedCall)
        .await
        .unwrap()
        .session;
    assert_eq!(
        stalled.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Stalled
        })
    );
    assert_eq!(stalled.messages.len(), 3);
    drop(core);
    let core = FloeCore::open(&path).await.unwrap();
    assert_eq!(
        core.agent_fixture_session(person, session.id)
            .await
            .unwrap(),
        stalled
    );
    let retry = core
        .run_agent_fixture(
            person,
            session.id,
            stalled.revision,
            AgentFixturePrompt::Today,
        )
        .await
        .unwrap()
        .session;
    assert_eq!(retry.last_outcome, Some(AgentOutcome::Completed));
    assert_eq!(core.calendar_actions(person).await.unwrap().len(), 0);
}
