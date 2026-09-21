use chrono::TimeZone;
use floe_actions::{ExpertCalendarDestination, ExpertCalendarRequest};
use floe_agent_contract::{
    DataClass, ExpertFocusProposal, ExpertInsight, ExpertResult, PackageKind, PackageRef,
};
use floe_context_contract::CalendarProvider;
use floe_conversation::AgentMessage;
use floe_day::CalendarRange;
use floe_experts::{
    AgentRegistry, CalendarExpertSetup, RegistryConfiguration, RegistryConfigurationTarget,
};

use super::expert_evidence::delegation_message;
use super::*;

fn fixture_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.with_ymd_and_hms(2050, 1, 15, 9, 0, 0).unwrap()
}

async fn seed(
    root: &std::path::Path,
    person: PersonId,
    keys: Keys,
    core: &FloeCore,
) -> (AgentSession, ExpertResult) {
    let vault = EncryptedAgentVault::create(root, person, keys)
        .await
        .unwrap();
    let setup = vault
        .install_calendar_expert(
            CalendarExpertSetup {
                instance_id: vault.registry_instance_id(),
                expected_revision: 0,
                setup_id: Uuid::new_v4(),
                provider: CalendarProvider::Fixture,
                device_id: "test-device".into(),
                calendar_ids: vec!["test-calendar".into()],
                connection_scope: floe_context_contract::CalendarScope::Selected,
                connection_revision: 1,
                source_authority: None,
                reviewed_native_subject_fingerprint: None,
            },
            &crate::vault_host::schedule_packaging(),
            Cancellation::default(),
        )
        .await
        .unwrap()
        .setup;
    for target in [
        RegistryConfigurationTarget::Installation {
            id: setup.tool_installation_id,
            enabled: true,
        },
        RegistryConfigurationTarget::Installation {
            id: setup.expert_installation_id,
            enabled: true,
        },
        RegistryConfigurationTarget::Assignment {
            id: setup.tool_assignment_id,
            enabled: true,
        },
        RegistryConfigurationTarget::Assignment {
            id: setup.expert_assignment_id,
            enabled: true,
        },
        RegistryConfigurationTarget::CalendarView {
            id: setup.view_handle,
            enabled: true,
        },
    ] {
        let registry = vault.registry_overview().await.unwrap().unwrap();
        vault
            .configure_registry(
                RegistryConfiguration {
                    instance_id: registry.instance_id,
                    expected_revision: registry.revision,
                    target,
                },
                Cancellation::default(),
            )
            .await
            .unwrap();
    }
    let now = fixture_now();
    let day = CalendarRange {
        start_date: now.date_naive(),
        end_date_exclusive: (now + chrono::Duration::days(1)).date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    };
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "test-calendar".into(),
        "Synthetic".into(),
    )
    .await
    .unwrap();
    let revision = core
        .calendar_connection(person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    core.import_calendar(person, revision, day.clone(), vec![], now)
        .await
        .unwrap();
    let revision = core
        .calendar_connection(person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    let snapshot = vault.expert_registry().await.unwrap().unwrap();
    let mut registry = AgentRegistry::restore(snapshot, vault.registry_instance_id()).unwrap();
    let registry_revision = registry.revision();
    let card = registry
        .expert_card(
            person,
            setup.expert_assignment_id,
            registry_revision,
            setup.view_handle,
        )
        .unwrap();
    let evidence = ExpertResult {
        schema_version: 1,
        invocation_id: Uuid::new_v4(),
        instance_id: vault.registry_instance_id(),
        person_id: person,
        assignment_id: setup.expert_assignment_id,
        package: PackageRef {
            kind: PackageKind::Expert,
            id: card.id,
            version: card.version,
        },
        view_handle: setup.view_handle,
        source_handle: format!("calendar.timeline:{}:{revision}", setup.view_handle),
        data_class: DataClass::Synthetic,
        expires_at_unix_ms: (now + chrono::Duration::minutes(2)).timestamp_millis() as u64,
        insights: vec![ExpertInsight::FocusWindow {
            starts_at_unix_ms: (now + chrono::Duration::minutes(5)).timestamp_millis() as u64,
            ends_at_unix_ms: (now + chrono::Duration::minutes(65)).timestamp_millis() as u64,
        }],
        action_proposals: vec![ExpertFocusProposal {
            starts_at_unix_ms: (now + chrono::Duration::minutes(5)).timestamp_millis() as u64,
            ends_at_unix_ms: (now + chrono::Duration::minutes(65)).timestamp_millis() as u64,
            view_handle: setup.view_handle,
        }],
        summary: Some("Synthetic proposal recorded.".into()),
        model_calls: 2,
        state_revision: 1,
        view_calls: 1,
    };
    registry
        .record_result(registry_revision, &evidence)
        .unwrap();
    let mut session = vault.create_sample_session().await.unwrap();
    let turn_id = Uuid::new_v4();
    session.active_turn = Some(turn_id);
    session.revision = 1;
    session.messages.push(AgentMessage::User {
        turn_id,
        text: "Synthetic focus request".into(),
    });
    vault.compare_and_swap(&session, 0).await.unwrap();
    session.revision = 2;
    session
        .messages
        .push(delegation_message(turn_id, &evidence));
    vault
        .commit_expert_session(&session, 1, registry_revision, &registry.snapshot())
        .await
        .unwrap();
    session.revision = 3;
    session.active_turn = None;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&session, 2).await.unwrap();
    (session, evidence)
}

#[tokio::test]
async fn old_calendar_receipt_cannot_be_published_against_a_new_connection_revision() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let core = FloeCore::open(directory.path().join("core.db"))
        .await
        .unwrap();
    let (session, evidence) = seed(&root, person, keys.clone(), &core).await;
    let vault = EncryptedAgentVault::open(&root, person, keys)
        .await
        .unwrap();
    let connection = core.calendar_connection(person).await.unwrap().unwrap();
    core.import_calendar(
        person,
        connection.revision,
        CalendarRange {
            start_date: fixture_now().date_naive(),
            end_date_exclusive: (fixture_now() + chrono::Duration::days(1)).date_naive(),
            timezone_offset_seconds: 0,
            end_timezone_offset_seconds: None,
        },
        vec![],
        fixture_now(),
    )
    .await
    .unwrap();
    let revision = core
        .calendar_connection(person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    assert_eq!(revision, connection.revision + 1);
    assert_eq!(
        core.prepare_expert_calendar_action(
            &vault,
            ExpertCalendarRequest {
                reference: ExpertProposalReference {
                    person_id: person,
                    session_id: session.id,
                    invocation_id: evidence.invocation_id,
                },
                destination: ExpertCalendarDestination {
                    provider: CalendarProvider::Fixture,
                    calendar_id: "test-calendar".into(),
                    connection_revision: revision,
                    timezone: "Asia/Seoul".into(),
                },
                cancellation: Cancellation::default(),
                deadline: tokio::time::Instant::now() + Duration::from_secs(1),
            },
            fixture_now,
        )
        .await,
        Err(AgentFailure::StaleContext),
    );
    assert_eq!(vault.load(person, session.id).await.unwrap(), session);
    assert!(
        core.actions()
            .calendar_actions(person)
            .await
            .unwrap()
            .is_empty()
    );
}

#[test]
fn proposal_jobs_read_absent_and_published_actions_without_republishing_after_revocation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let core = Arc::new(
        runtime
            .block_on(FloeCore::open(directory.path().join("core.db")))
            .unwrap(),
    );
    let (session, evidence) = runtime.block_on(seed(&root, person, keys.clone(), &core));
    let reference = ExpertProposalReference {
        person_id: person,
        session_id: session.id,
        invocation_id: evidence.invocation_id,
    };
    let inspect = || WorkerAction::InspectProposal {
        session_id: session.id,
        invocation_id: evidence.invocation_id,
    };
    let worker = Worker::with_core(
        root.clone(),
        keys.clone(),
        core.clone(),
        Arc::new(LocalContextHost::default()),
        Arc::new(crate::events::AppEventBuffer::default()),
    )
    .unwrap();
    assert_eq!(
        perform(&worker, person, inspect()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    perform(&worker, person, WorkerAction::Unlock);
    let absent = perform(&worker, person, inspect());
    assert_eq!(absent.state, Some(VaultState::Ready));
    assert!(absent.proposal.unwrap().action.is_none());
    assert!(
        runtime
            .block_on(core.actions().calendar_actions(person))
            .unwrap()
            .is_empty()
    );
    perform(&worker, person, WorkerAction::Lock);
    let publication = runtime.block_on(async {
        let vault = EncryptedAgentVault::open(&root, person, keys.clone())
            .await
            .unwrap();
        let connection = core.calendar_connection(person).await.unwrap().unwrap();
        core.prepare_expert_calendar_action(
            &vault,
            ExpertCalendarRequest {
                reference,
                destination: ExpertCalendarDestination {
                    provider: CalendarProvider::Fixture,
                    calendar_id: "test-calendar".into(),
                    connection_revision: connection.revision,
                    timezone: "Asia/Seoul".into(),
                },
                cancellation: Cancellation::default(),
                deadline: tokio::time::Instant::now() + Duration::from_secs(5),
            },
            fixture_now,
        )
        .await
    });
    assert_eq!(publication, Err(AgentFailure::PolicyDenied));
    perform(&worker, person, WorkerAction::Unlock);
    let overview = perform(&worker, person, WorkerAction::Registry { change: None })
        .registry
        .unwrap();
    perform(
        &worker,
        person,
        WorkerAction::Registry {
            change: Some(RegistryConfiguration {
                instance_id: overview.instance_id,
                expected_revision: overview.revision,
                target: RegistryConfigurationTarget::Assignment {
                    id: evidence.assignment_id,
                    enabled: false,
                },
            }),
        },
    );
    let id = Uuid::new_v4();
    let submit = || WorkerOperation::Submit {
        action: Box::new(inspect()),
    };
    worker.request(person, id, submit()).unwrap();
    let result = wait(&worker, person, id);
    let replayed = worker.request(person, id, submit()).unwrap();
    assert_eq!(
        (replayed.request_id, replayed.stage.clone(), replayed.done),
        (result.request_id, result.stage.clone(), result.done)
    );
    assert!(result.failure.is_none() && result.events.is_empty() && result.session.is_none());
    let projection = result.proposal.unwrap();
    assert_eq!(projection.session_id, session.id);
    assert!(projection.action.is_none());
    worker
        .request(person, id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(
        perform(&worker, PersonId::new(), inspect()).failure,
        Some(AgentFailure::NotFound)
    );
    let saved = perform(
        &worker,
        person,
        WorkerAction::Session {
            operation: FixtureOperation::Get {
                session_id: session.id,
            },
        },
    )
    .session
    .unwrap();
    assert_eq!(saved, session);
    assert_eq!(
        runtime
            .block_on(core.actions().calendar_actions(person))
            .unwrap(),
        Vec::<floe_actions::CalendarAction>::new()
    );
    keys.0.unavailable.store(true, Ordering::Release);
    let unavailable = perform(&worker, person, inspect());
    assert_eq!(unavailable.failure, Some(AgentFailure::VaultUnavailable));
    assert!(unavailable.proposal.is_none());
}

#[test]
fn stopped_proposal_inspection_retains_the_owned_job_until_key_access_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let keys = Keys::default();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), keys.clone()).unwrap();
    perform(&worker, person, WorkerAction::Create);
    keys.0.entered.store(false, Ordering::Release);
    *keys.0.paused.lock().unwrap() = true;
    let id = Uuid::new_v4();
    let inspect = || WorkerAction::InspectProposal {
        session_id: Uuid::new_v4(),
        invocation_id: Uuid::new_v4(),
    };
    worker
        .request(
            person,
            id,
            WorkerOperation::Submit {
                action: Box::new(inspect()),
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !keys.0.entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !worker
            .request(person, id, WorkerOperation::Stop)
            .unwrap()
            .done
    );
    assert_eq!(
        worker
            .request(person, id, WorkerOperation::Release)
            .unwrap_err(),
        AgentFailure::Conflict
    );
    *keys.0.paused.lock().unwrap() = false;
    keys.0.wake.notify_all();
    let result = wait(&worker, person, id);
    assert_eq!(result.failure, Some(AgentFailure::Cancelled));
    assert!(result.proposal.is_none());
}
