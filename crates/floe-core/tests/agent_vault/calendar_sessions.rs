use floe_core::{AgentFixturePrompt, AgentFixtureTurn, recover_agent_sample};
use floe_domain::CalendarProvider;

use super::*;

async fn setup(vault: &EncryptedAgentVault<Keys>, provider: CalendarProvider) -> Uuid {
    let current = vault.calendar_expert_overview().await.unwrap();
    let setup_id = Uuid::new_v4();
    vault
        .install_calendar_expert(
            CalendarExpertSetup {
                instance_id: current.registry.instance_id,
                expected_revision: current.registry.revision,
                setup_id,
                provider,
                calendar_ids: vec![format!("synthetic-calendar-{setup_id}")],
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    setup_id
}

#[tokio::test]
async fn calendar_sessions_resume_by_immutable_setup_without_claiming_legacy_sessions() {
    let root = private_root();
    let keys = Keys::default();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let sample = vault.create_sample_session().await.unwrap();
    let first_setup = setup(&vault, CalendarProvider::Fixture).await;
    let second_setup = setup(&vault, CalendarProvider::EventKit).await;
    let registry = vault.expert_registry().await.unwrap();
    let first = vault
        .resume_calendar_session(first_setup, Cancellation::default())
        .await
        .unwrap();
    let second = vault
        .create_calendar_session(second_setup, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(
        first.scope,
        Some(AgentSessionScope::Calendar {
            setup_id: first_setup,
            provider: CalendarProvider::Fixture
        })
    );
    assert_eq!(first.data_classes, [DataClass::Synthetic]);
    assert_eq!(second.data_classes, [DataClass::Personal]);
    assert_eq!(vault.resume_sample_session().await.unwrap(), sample);
    assert_eq!(
        vault
            .resume_calendar_session(first_setup, Cancellation::default())
            .await
            .unwrap(),
        first
    );
    assert_eq!(
        vault
            .resume_calendar_session(second_setup, Cancellation::default())
            .await
            .unwrap(),
        second
    );
    assert_eq!(vault.expert_registry().await.unwrap(), registry);
    assert_eq!(
        vault.calendar_session(sample.id).await,
        Err(AgentFailure::PolicyDenied)
    );
    let legacy = vault.create_session().await.unwrap();
    assert_eq!(
        vault.calendar_session(legacy.id).await,
        Err(AgentFailure::PolicyDenied)
    );
    let latest = vault
        .create_calendar_session(first_setup, Cancellation::default())
        .await
        .unwrap();
    assert_ne!(latest.id, first.id);
    assert_eq!(vault.calendar_session(first.id).await.unwrap(), first);
    vault.checkpoint().await.unwrap();
    drop(vault);
    let reopened = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(
        reopened
            .resume_calendar_session(first_setup, Cancellation::default())
            .await
            .unwrap(),
        latest
    );
    assert_eq!(
        reopened
            .resume_calendar_session(second_setup, Cancellation::default())
            .await
            .unwrap(),
        second
    );
    assert_eq!(reopened.expert_registry().await.unwrap(), registry);
}

#[tokio::test]
async fn sample_turn_and_recovery_cannot_adopt_a_calendar_conversation() {
    let root = private_root();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let setup_id = setup(&vault, CalendarProvider::Fixture).await;
    let session = vault
        .create_calendar_session(setup_id, Cancellation::default())
        .await
        .unwrap();
    let registry = vault.expert_registry().await.unwrap();
    assert_eq!(
        vault
            .run_persisted_agent_sample(
                AgentFixtureTurn {
                    person_id: person,
                    session_id: session.id,
                    expected_revision: 0,
                    prompt: AgentFixturePrompt::Today,
                },
                Cancellation::default(),
                std::time::Duration::ZERO,
                |_| panic!("Sample dispatch must be denied")
            )
            .await,
        Err(AgentFailure::PolicyDenied)
    );
    assert_eq!(
        recover_agent_sample(&vault, person, session.id, 0).await,
        Err(AgentFailure::PolicyDenied)
    );
    assert_eq!(vault.load(person, session.id).await.unwrap(), session);
    assert_eq!(vault.expert_registry().await.unwrap(), registry);
    let sample = vault.resume_sample_session().await.unwrap();
    assert!(sample.scope.is_none());
    assert_ne!(sample.id, session.id);
}

#[tokio::test]
async fn calendar_scope_and_classification_cannot_be_removed_retargeted_or_added_to_legacy() {
    let root = private_root();
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
        .await
        .unwrap();
    let setup_id = setup(&vault, CalendarProvider::Fixture).await;
    let session = vault
        .create_calendar_session(setup_id, Cancellation::default())
        .await
        .unwrap();
    for scope in [
        None,
        Some(AgentSessionScope::Calendar {
            setup_id: Uuid::new_v4(),
            provider: CalendarProvider::Fixture,
        }),
    ] {
        let mut changed = session.clone();
        changed.revision += 1;
        changed.scope = scope;
        assert_eq!(
            vault.compare_and_swap(&changed, 0).await,
            Err(AgentFailure::Conflict)
        );
    }
    let mut widened = session.clone();
    widened.revision += 1;
    widened.data_classes.push(DataClass::Personal);
    assert_eq!(
        vault.compare_and_swap(&widened, 0).await,
        Err(AgentFailure::PolicyDenied)
    );
    let mut legacy = vault.create_sample_session().await.unwrap();
    legacy.scope = session.scope;
    legacy.revision += 1;
    assert_eq!(
        vault.compare_and_swap(&legacy, 0).await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(vault.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn calendar_recovery_preserves_committed_messages_and_never_invokes_models_or_restores_grants()
 {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let setup_id = setup(&vault, CalendarProvider::EventKit).await;
    let mut session = vault
        .create_calendar_session(setup_id, Cancellation::default())
        .await
        .unwrap();
    let turn = Uuid::new_v4();
    session.revision = 1;
    session.active_turn = Some(turn);
    session.messages.push(AgentMessage::User {
        turn_id: turn,
        text: "synthetic-calendar-private-recovery-marker".into(),
    });
    vault.compare_and_swap(&session, 0).await.unwrap();
    let registry = vault.expert_registry().await.unwrap();
    vault.checkpoint().await.unwrap();
    drop(vault);
    let vault = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    let interrupted = vault
        .resume_calendar_session(setup_id, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(interrupted, session);
    assert_eq!(
        vault
            .recover_calendar_session(session.id, 0, Cancellation::default())
            .await,
        Err(AgentFailure::Conflict)
    );
    let recovered = vault
        .recover_calendar_session(session.id, 1, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(recovered.revision, 2);
    assert_eq!(recovered.active_turn, None);
    assert_eq!(recovered.messages, session.messages);
    assert_eq!(recovered.scope, session.scope);
    assert_eq!(
        recovered.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Interrupted
        })
    );
    assert_eq!(
        vault
            .recover_calendar_session(session.id, 2, Cancellation::default())
            .await
            .unwrap(),
        recovered
    );
    assert_eq!(vault.expert_registry().await.unwrap(), registry);
    vault.checkpoint().await.unwrap();
    assert_no_plaintext(
        &root.path().join(person.to_string()),
        &[
            "synthetic-calendar-private-recovery-marker",
            &setup_id.to_string(),
        ],
    );
}

#[tokio::test]
async fn missing_setup_cancelled_and_key_unavailable_requests_do_not_adopt_or_create_sessions() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let sample = vault.create_sample_session().await.unwrap();
    assert_eq!(
        vault
            .create_calendar_session(Uuid::new_v4(), Cancellation::default())
            .await,
        Err(AgentFailure::NotFound)
    );
    assert!(vault.expert_registry().await.unwrap().is_none());
    let setup_id = setup(&vault, CalendarProvider::Fixture).await;
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        vault.resume_calendar_session(setup_id, cancellation).await,
        Err(AgentFailure::Cancelled)
    );
    assert_eq!(vault.resume_sample_session().await.unwrap(), sample);
    keys.0.blocked.store(true, Ordering::SeqCst);
    assert_eq!(
        vault
            .create_calendar_session(setup_id, Cancellation::default())
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    keys.0.blocked.store(false, Ordering::SeqCst);
    assert_eq!(
        vault
            .resume_calendar_session(setup_id, Cancellation::default())
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
}
