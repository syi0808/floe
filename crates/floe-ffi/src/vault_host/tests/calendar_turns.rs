use chrono::{Datelike, TimeZone};
use floe_agent::{
    AgentEventKind, AgentOutcome, CalendarExpertSetup, RegistryConfiguration,
    RegistryConfigurationTarget,
};
use floe_domain::{CalendarProvider, CalendarRange};

use super::*;

struct Fixture {
    _directory: tempfile::TempDir,
    worker: Worker,
    core: Arc<FloeCore>,
    person: PersonId,
    session: AgentSession,
    request: AgentCalendarTurnRequestDto,
}

fn fixture() -> Fixture {
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
    let vault = runtime
        .block_on(EncryptedAgentVault::create(&root, person, keys.clone()))
        .unwrap();
    let setup = runtime
        .block_on(vault.install_calendar_expert(
            CalendarExpertSetup {
                instance_id: vault.registry_instance_id(),
                expected_revision: 0,
                setup_id: Uuid::new_v4(),
                provider: CalendarProvider::Fixture,
                calendar_ids: vec!["calendar-a".into()],
            },
            Cancellation::default(),
        ))
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
        let registry = runtime
            .block_on(vault.registry_overview())
            .unwrap()
            .unwrap();
        runtime
            .block_on(vault.configure_registry(
                RegistryConfiguration {
                    instance_id: registry.instance_id,
                    expected_revision: registry.revision,
                    target,
                },
                Cancellation::default(),
            ))
            .unwrap();
    }
    let now = chrono::Utc::now();
    let tomorrow = now.date_naive().succ_opt().unwrap();
    let day = CalendarRange {
        start_date: tomorrow,
        end_date_exclusive: tomorrow.succ_opt().unwrap(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    };
    runtime
        .block_on(core.select_calendar(
            person,
            CalendarProvider::Fixture,
            "calendar-a".into(),
            "Fixture".into(),
        ))
        .unwrap();
    let selected = runtime
        .block_on(core.calendar_connection(person))
        .unwrap()
        .unwrap();
    runtime
        .block_on(core.import_calendar(person, selected.revision, day.clone(), vec![], now))
        .unwrap();
    let connection = runtime
        .block_on(core.calendar_connection(person))
        .unwrap()
        .unwrap();
    let session = runtime
        .block_on(vault.create_calendar_session(setup.setup_id, Cancellation::default()))
        .unwrap();
    drop(vault);
    let starts_at = chrono::Utc
        .with_ymd_and_hms(tomorrow.year(), tomorrow.month(), tomorrow.day(), 9, 0, 0)
        .unwrap();
    let request = AgentCalendarTurnRequestDto {
        session_id: session.id.to_string(),
        expected_revision: session.revision,
        prompt: AgentCalendarPromptDto::ProposeFocus { focus_minutes: 60 },
        model: AgentCalendarModelDto::DeterministicFixture,
        day,
        starts_at,
        ends_at: starts_at + chrono::Duration::hours(8),
        destination: Some(AgentCalendarDestinationDto {
            provider: CalendarProvider::Fixture,
            calendar_id: "calendar-a".into(),
            connection_revision: connection.revision,
            timezone: "UTC".into(),
        }),
        remote_route: None,
    };
    let worker = Worker::with_core(root, keys, core.clone()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    Fixture {
        _directory: directory,
        worker,
        core,
        person,
        session,
        request,
    }
}

#[test]
fn calendar_turn_job_streams_and_waits_for_exactly_one_prepared_action() {
    let fixture = fixture();
    let action = AgentVaultActionDto::CalendarTurn {
        request: fixture.request.clone(),
    };
    let job = Uuid::new_v4();
    fixture
        .worker
        .request(
            fixture.person,
            job,
            AgentVaultOperationDto::Submit {
                action: action.clone(),
            },
        )
        .unwrap();
    let completed = wait(&fixture.worker, fixture.person, job);
    assert_eq!(
        fixture
            .worker
            .request(
                fixture.person,
                job,
                AgentVaultOperationDto::Submit { action }
            )
            .unwrap(),
        completed
    );
    assert!(completed.done && completed.failure.is_none());
    assert!(completed.events.iter().any(|event| matches!(
        event.event,
        AgentEventKind::Finished {
            outcome: AgentOutcome::Completed,
            ..
        }
    )));
    let session = completed.session.as_ref().unwrap();
    assert_eq!(session.id, fixture.session.id);
    assert_eq!(session.last_outcome, Some(AgentOutcome::Completed));
    let result = completed.calendar_turn.as_ref().unwrap();
    assert_eq!(result.person_id, fixture.person.to_string());
    assert_eq!(result.session_id, fixture.session.id.to_string());
    assert_eq!(result.model, AgentCalendarModelDto::DeterministicFixture);
    assert_eq!(result.proposals.len(), 1);
    assert!(result.proposals[0].action.is_some());
    assert!(result.proposals[0].failure.is_none());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        runtime
            .block_on(fixture.core.calendar_actions(fixture.person))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn calendar_turn_rejects_invalid_bounds_destination_revision_and_disconnected_source() {
    let fixture = fixture();
    let mut invalid_day = fixture.request.clone();
    invalid_day.model = AgentCalendarModelDto::FoundationModels;
    invalid_day.day.end_date_exclusive = invalid_day.day.end_date_exclusive.succ_opt().unwrap();
    assert_eq!(
        perform(
            &fixture.worker,
            fixture.person,
            AgentVaultActionDto::CalendarTurn {
                request: invalid_day
            }
        )
        .failure,
        Some(AgentFailure::InvalidInput)
    );
    let mut wrong_destination = fixture.request.clone();
    wrong_destination
        .destination
        .as_mut()
        .unwrap()
        .connection_revision += 1;
    assert_eq!(
        perform(
            &fixture.worker,
            fixture.person,
            AgentVaultActionDto::CalendarTurn {
                request: wrong_destination
            }
        )
        .failure,
        Some(AgentFailure::CapabilityDenied)
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let revision = runtime
        .block_on(fixture.core.calendar_connection(fixture.person))
        .unwrap()
        .unwrap()
        .revision;
    runtime
        .block_on(fixture.core.disconnect_calendar(fixture.person, revision))
        .unwrap();
    let mut disconnected = fixture.request;
    disconnected.destination = None;
    assert_eq!(
        perform(
            &fixture.worker,
            fixture.person,
            AgentVaultActionDto::CalendarTurn {
                request: disconnected
            }
        )
        .failure,
        Some(AgentFailure::StaleContext)
    );
}
