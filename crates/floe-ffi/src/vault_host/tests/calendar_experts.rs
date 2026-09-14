use floe_agent::{
    AgentMessage, CalendarAccessChange, CalendarAccessConfiguration, CalendarExpertSetup,
    RegistryConfiguration, RegistryConfigurationTarget,
};
use std::os::unix::fs::PermissionsExt;

use super::*;

#[test]
fn native_grants_capture_authority_only_on_explicit_review() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let fixture_library = std::env::current_exe().ok().and_then(|path| {
        path.parent()
            .map(|parent| parent.join("Frameworks/libfloe_eventkit.dylib"))
    });
    if !fixture_library.is_some_and(|path| path.exists()) {
        return;
    }
    use floe_agent::{CalendarAccessChange, CalendarAccessConfiguration};
    use floe_domain::{CalendarFailure, CalendarProvider, CalendarScope, CalendarSelection};
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId(
        Uuid::parse_str(floe_infra::native_calendar::LOCAL_PERSON)
            .expect("native test person is a valid UUID"),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let core = Arc::new(
        runtime
            .block_on(FloeCore::open(directory.path().join("core.db")))
            .unwrap(),
    );
    let setup_id = Uuid::new_v4();
    runtime
        .block_on(core.set_calendar_scope(
            person,
            setup_id.to_string(),
            1,
            "iphone".into(),
            CalendarProvider::EventKit,
            vec![CalendarSelection {
                calendar_id: "home".into(),
                calendar_name: "Home".into(),
            }],
            CalendarScope::Selected,
        ))
        .unwrap();
    let authority = runtime
        .block_on(core.calendar_connection(person))
        .unwrap()
        .unwrap()
        .source_authority;
    let worker = Worker::with_core(
        directory.path().join("vaults"),
        Keys::default(),
        core.clone(),
        Arc::new(LocalContextStore::default()),
    )
    .unwrap();
    assert!(
        perform(&worker, person, AgentVaultActionDto::Create {})
            .failure
            .is_none()
    );
    let inspect = AgentVaultActionDto::CalendarExperts { setup: None };
    let empty = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    let request = CalendarExpertSetup {
        instance_id: empty.registry.instance_id,
        expected_revision: empty.registry.revision,
        setup_id,
        provider: CalendarProvider::EventKit,
        device_id: "iphone".into(),
        calendar_ids: vec!["home".into()],
        connection_scope: CalendarScope::Selected,
        connection_revision: 1,
        source_authority: Some(authority),
        reviewed_native_subject_fingerprint: Some("a".repeat(64)),
    };
    let mut invalid = request.clone();
    invalid.calendar_ids = vec!["ungranted".into()];
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarExperts {
                setup: Some(encode_contract(&invalid).unwrap())
            }
        )
        .failure,
        Some(AgentFailure::Conflict)
    );
    let action = AgentVaultActionDto::CalendarExperts {
        setup: Some(encode_contract(&request).unwrap()),
    };
    let installed = perform(&worker, person, action.clone())
        .calendar_experts
        .unwrap();
    assert_eq!(installed.views[0].source_authority, Some(authority));
    assert_eq!(installed.setups[0].source_authority, Some(authority));
    let initial_connection = runtime
        .block_on(core.calendar_connection(person))
        .unwrap()
        .unwrap();
    runtime
        .block_on(core.set_calendar_scope(
            person,
            initial_connection.connection_id.clone(),
            initial_connection.revision + 100,
            initial_connection.device_id.clone(),
            initial_connection.provider,
            initial_connection.calendars.clone(),
            initial_connection.scope,
        ))
        .unwrap();
    let drift_retry = perform(&worker, person, action.clone());
    assert_eq!(drift_retry.failure, None);
    runtime
        .block_on(core.record_calendar_failure(
            person,
            initial_connection.revision + 100,
            CalendarFailure::PermissionDenied,
            chrono::Utc::now(),
        ))
        .unwrap();
    let connection = runtime
        .block_on(core.calendar_connection(person))
        .unwrap()
        .unwrap();
    assert_ne!(connection.source_authority, authority);
    let retry = perform(&worker, person, action);
    assert_eq!(retry.failure, Some(AgentFailure::AccessReviewRequired));
    let changed = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarAccess {
            change: encode_contract(&CalendarAccessConfiguration {
                instance_id: installed.registry.instance_id,
                expected_revision: installed.registry.revision,
                setup_id,
                change: CalendarAccessChange::SetScope {
                    provider: CalendarProvider::EventKit,
                    device_id: "iphone".into(),
                    calendar_ids: vec!["home".into()],
                    connection_scope: CalendarScope::Selected,
                    connection_revision: connection.revision,
                    source_authority: Some(authority),
                    reviewed_native_subject_fingerprint: Some("a".repeat(64)),
                },
            })
            .unwrap(),
        },
    );
    assert_eq!(changed.failure, Some(AgentFailure::AccessReviewRequired));
    let changed = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarAccess {
            change: encode_contract(&CalendarAccessConfiguration {
                instance_id: installed.registry.instance_id,
                expected_revision: installed.registry.revision,
                setup_id,
                change: CalendarAccessChange::SetScope {
                    provider: CalendarProvider::EventKit,
                    device_id: "iphone".into(),
                    calendar_ids: vec!["home".into()],
                    connection_scope: CalendarScope::Selected,
                    connection_revision: connection.revision,
                    source_authority: Some(connection.source_authority),
                    reviewed_native_subject_fingerprint: Some("a".repeat(64)),
                },
            })
            .unwrap(),
        },
    );
    assert_eq!(changed.failure, None);
    let reviewed = changed.calendar_experts.unwrap();
    assert_eq!(
        reviewed.views[0].source_authority,
        Some(connection.source_authority)
    );
    let enabled = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarAccess {
            change: encode_contract(&CalendarAccessConfiguration {
                instance_id: reviewed.registry.instance_id,
                expected_revision: reviewed.registry.revision,
                setup_id,
                change: CalendarAccessChange::SetEnabled { enabled: true },
            })
            .unwrap(),
        },
    );
    assert_eq!(enabled.failure, None);
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    );
    assert_eq!(session.failure, None);
    let session = session.session.unwrap();
    let (mut route, server) = answer_server(vec![
        floe_agent::ModelStep::Answer {
            text: "Hello!".into(),
        },
        floe_agent::ModelStep::Delegate {
            agent_id: floe_agent::BuiltinExpertKind::Schedule.package_id().into(),
            message: "Read my calendar".into(),
        },
        floe_agent::ModelStep::Call {
            capability_id: "schedule.find_free_windows".into(),
            input: serde_json::json!({
                "minimum_minutes": 60,
                "range_start_unix_ms": chrono::Utc::now().timestamp_millis(),
                "range_end_unix_ms": chrono::Utc::now().timestamp_millis() + 7_200_000,
            })
            .to_string(),
        },
        floe_agent::ModelStep::Answer {
            text: "Your calendar is clear.".into(),
        },
        floe_agent::ModelStep::Answer {
            text: "Your calendar is clear.".into(),
        },
    ]);
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "calendar-expert-test".into(),
        person_id: person.to_string(),
        device_id: "iphone".into(),
    });
    let run = |session: &AgentSession, text: &str| {
        perform(
            &worker,
            person,
            AgentVaultActionDto::ConversationTurn {
                request: floe_protocol::AgentConversationTurnRequestDto {
                    session_id: session.id.to_string(),
                    expected_revision: session.revision,
                    text: text.into(),
                    device_id: "iphone".into(),
                    continuation: false,
                    remote_route: Some(route.clone()),
                },
            },
        )
    };
    let greeting = run(&session, "Hello");
    assert_eq!(greeting.failure, None);
    let greeting = greeting.session.unwrap();
    assert_eq!(
        greeting.last_outcome,
        Some(floe_agent::AgentOutcome::Completed)
    );
    let calendar = run(&greeting, "Read my calendar");
    assert_eq!(calendar.failure, None);
    let calendar = calendar.session.unwrap();
    let requests = server.join().unwrap();
    assert_eq!(
        calendar.last_outcome,
        Some(floe_agent::AgentOutcome::Completed),
        "session: {calendar:?}; requests: {requests:?}"
    );
    assert!(calendar.messages.iter().any(|message| matches!(
        message,
        AgentMessage::Delegation { task, .. }
            if task.state == floe_agent::A2ATaskState::Completed
    )));
    assert_eq!(requests.len(), 5);
    assert!(
        requests
            .iter()
            .all(|request| request.starts_with("POST /v1/agent "))
    );
}

#[test]
fn fixture_schedule_runs_through_the_durable_registered_task() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
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
    let setup_id = Uuid::new_v4();
    runtime
        .block_on(core.set_calendar_scope(
            person,
            setup_id.to_string(),
            1,
            "mac-local".into(),
            floe_domain::CalendarProvider::Fixture,
            vec![floe_domain::CalendarSelection {
                calendar_id: "fixture-calendar".into(),
                calendar_name: "Fixture".into(),
            }],
            floe_domain::CalendarScope::Selected,
        ))
        .unwrap();
    let local = chrono::Local::now();
    runtime
        .block_on(core.import_calendar(
            person,
            1,
            floe_domain::CalendarRange {
                start_date: local.date_naive(),
                end_date_exclusive: local.date_naive() + chrono::Duration::days(1),
                timezone_offset_seconds: local.offset().local_minus_utc(),
                end_timezone_offset_seconds: None,
            },
            vec![],
            chrono::Utc::now(),
        ))
        .unwrap();
    let worker = Worker::with_core(
        root.clone(),
        keys.clone(),
        Arc::clone(&core),
        Arc::new(LocalContextStore::default()),
    )
    .unwrap();
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Create {}).failure,
        None
    );
    let empty = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts { setup: None },
    )
    .calendar_experts
    .unwrap();
    let installed = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts {
            setup: Some(
                encode_contract(&CalendarExpertSetup {
                    instance_id: empty.registry.instance_id,
                    expected_revision: empty.registry.revision,
                    setup_id,
                    provider: floe_domain::CalendarProvider::Fixture,
                    device_id: "mac-local".into(),
                    calendar_ids: vec!["fixture-calendar".into()],
                    connection_scope: floe_domain::CalendarScope::Selected,
                    connection_revision: 2,
                    source_authority: None,
                    reviewed_native_subject_fingerprint: None,
                })
                .unwrap(),
            ),
        },
    )
    .calendar_experts
    .unwrap();
    let enabled = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarAccess {
            change: encode_contract(&CalendarAccessConfiguration {
                instance_id: installed.registry.instance_id,
                expected_revision: installed.registry.revision,
                setup_id,
                change: CalendarAccessChange::SetEnabled { enabled: true },
            })
            .unwrap(),
        },
    );
    assert_eq!(enabled.failure, None);
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let (mut route, server) = answer_server(vec![
        floe_agent::ModelStep::Delegate {
            agent_id: floe_agent::BuiltinExpertKind::Schedule.package_id().into(),
            message: "Find an open hour".into(),
        },
        floe_agent::ModelStep::Call {
            capability_id: "schedule.find_free_windows".into(),
            input: serde_json::json!({
                "minimum_minutes": 60,
                "range_start_unix_ms": now,
                "range_end_unix_ms": now + 7_200_000,
            })
            .to_string(),
        },
        floe_agent::ModelStep::Answer {
            text: "The fixture calendar has an open hour.".into(),
        },
        floe_agent::ModelStep::Answer {
            text: "You have an open hour.".into(),
        },
    ]);
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "calendar-expert-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let result = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationTurn {
            request: floe_protocol::AgentConversationTurnRequestDto {
                session_id: session.id.to_string(),
                expected_revision: session.revision,
                text: "Find an open hour".into(),
                device_id: "mac-local".into(),
                continuation: false,
                remote_route: Some(route),
            },
        },
    );
    assert_eq!(result.failure, None, "result: {result:?}");
    let session = result.session.unwrap();
    assert_eq!(
        session.last_outcome,
        Some(floe_agent::AgentOutcome::Completed),
        "session: {session:?}"
    );
    let task_id = session
        .messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::Delegation { task, .. }
                if task.state == floe_agent::A2ATaskState::Completed =>
            {
                Some(task.id)
            }
            _ => None,
        })
        .expect("completed Schedule delegation");
    assert_eq!(server.join().unwrap().len(), 4);
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Lock {}).failure,
        None
    );
    let reopened = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    let task = runtime
        .block_on(reopened.task(floe_agent_contract::TaskId::from_uuid(task_id).unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(
        task.snapshot.state,
        floe_agent_contract::TaskState::Completed
    );
    assert!(task.snapshot.result.is_some());
}

#[test]
fn production_conversation_replays_the_same_request_without_model_redispatch() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let (mut route, server) = answer_server(vec![floe_agent::ModelStep::Answer {
        text: "One durable answer".into(),
    }]);
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "conversation-replay-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let action = AgentVaultActionDto::ConversationTurn {
        request: floe_protocol::AgentConversationTurnRequestDto {
            session_id: session.id.to_string(),
            expected_revision: session.revision,
            text: "Answer once".into(),
            device_id: "mac-local".into(),
            continuation: false,
            remote_route: Some(route),
        },
    };
    let request_id = Uuid::new_v4();
    worker
        .request(
            person,
            request_id,
            AgentVaultOperationDto::Submit {
                action: action.clone(),
            },
        )
        .unwrap();
    let first = wait(&worker, person, request_id);
    let completed_job_cancellation = worker
        .jobs
        .lock()
        .unwrap()
        .get(&request_id)
        .unwrap()
        .cancellation
        .clone();
    assert!(!completed_job_cancellation.is_cancelled());
    worker
        .request(person, request_id, AgentVaultOperationDto::Stop {})
        .unwrap();
    assert!(!completed_job_cancellation.is_cancelled());
    worker
        .request(person, request_id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(first.failure, None, "first: {first:?}");

    worker
        .request(
            person,
            request_id,
            AgentVaultOperationDto::Submit {
                action: action.clone(),
            },
        )
        .unwrap();
    let replay = wait(&worker, person, request_id);
    worker
        .request(person, request_id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(replay.failure, None, "replay: {replay:?}");
    assert_eq!(replay.session, first.session);

    let mut changed = action;
    let AgentVaultActionDto::ConversationTurn { request } = &mut changed else {
        unreachable!()
    };
    request.text = "Changed payload".into();
    worker
        .request(
            person,
            request_id,
            AgentVaultOperationDto::Submit { action: changed },
        )
        .unwrap();
    let conflict = wait(&worker, person, request_id);
    worker
        .request(person, request_id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(conflict.failure, Some(AgentFailure::Conflict));
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn terminal_conversation_accepts_the_next_run_without_ui_release() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let (mut route, server) = answer_server(vec![
        floe_agent::ModelStep::Answer {
            text: "First durable answer".into(),
        },
        floe_agent::ModelStep::Answer {
            text: "Second durable answer".into(),
        },
    ]);
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "conversation-release-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let first_id = Uuid::new_v4();
    worker
        .request(
            person,
            first_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::ConversationTurn {
                    request: floe_protocol::AgentConversationTurnRequestDto {
                        session_id: session.id.to_string(),
                        expected_revision: session.revision,
                        text: "Answer first".into(),
                        device_id: "mac-local".into(),
                        continuation: false,
                        remote_route: Some(route.clone()),
                    },
                },
            },
        )
        .unwrap();
    let first = wait(&worker, person, first_id);
    assert_eq!(first.failure, None, "first: {first:?}");
    let first_session = first.session.unwrap();

    let second_id = Uuid::new_v4();
    worker
        .request(
            person,
            second_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::ConversationTurn {
                    request: floe_protocol::AgentConversationTurnRequestDto {
                        session_id: session.id.to_string(),
                        expected_revision: first_session.revision,
                        text: "Answer second".into(),
                        device_id: "mac-local".into(),
                        continuation: false,
                        remote_route: Some(route),
                    },
                },
            },
        )
        .unwrap();
    let second = wait(&worker, person, second_id);
    assert_eq!(second.failure, None, "second: {second:?}");
    assert!(second.session.unwrap().revision > first_session.revision);
    let retained = worker
        .request(
            person,
            first_id,
            AgentVaultOperationDto::Poll { after_sequence: 0 },
        )
        .unwrap();
    assert!(retained.done);
    assert_eq!(retained.session.unwrap(), first_session);
    worker
        .request(person, first_id, AgentVaultOperationDto::Release {})
        .unwrap();
    worker
        .request(person, second_id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn t09_preview_does_not_stop_chat_and_t22_network_wait_does_not_hold_vault() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let setup_id = Uuid::new_v4();
    let empty = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts { setup: None },
    )
    .calendar_experts
    .unwrap();
    let installed = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts {
            setup: Some(
                encode_contract(&CalendarExpertSetup {
                    instance_id: empty.registry.instance_id,
                    expected_revision: empty.registry.revision,
                    setup_id,
                    provider: floe_domain::CalendarProvider::Fixture,
                    device_id: "mac-local".into(),
                    calendar_ids: vec!["fixture-calendar".into()],
                    connection_scope: floe_domain::CalendarScope::Selected,
                    connection_revision: 1,
                    source_authority: None,
                    reviewed_native_subject_fingerprint: None,
                })
                .unwrap(),
            ),
        },
    )
    .calendar_experts
    .unwrap();
    let enabled = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarAccess {
            change: encode_contract(&CalendarAccessConfiguration {
                instance_id: installed.registry.instance_id,
                expected_revision: installed.registry.revision,
                setup_id,
                change: CalendarAccessChange::SetEnabled { enabled: true },
            })
            .unwrap(),
        },
    )
    .calendar_experts
    .unwrap();
    assert!(enabled.views.iter().any(|view| view.enabled));
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let (mut route, entered, release, server) = blocking_answer_server();
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "conversation-concurrency-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let competing_route = route.clone();
    let conversation_id = Uuid::new_v4();
    worker
        .request(
            person,
            conversation_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::ConversationTurn {
                    request: floe_protocol::AgentConversationTurnRequestDto {
                        session_id: session.id.to_string(),
                        expected_revision: session.revision,
                        text: "Wait for the model".into(),
                        device_id: "mac-local".into(),
                        continuation: false,
                        remote_route: Some(route),
                    },
                },
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let root_cancellation = worker
        .jobs
        .lock()
        .unwrap()
        .get(&conversation_id)
        .unwrap()
        .cancellation
        .clone();

    let competing_id = Uuid::new_v4();
    worker
        .request(
            person,
            competing_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::ConversationTurn {
                    request: floe_protocol::AgentConversationTurnRequestDto {
                        session_id: session.id.to_string(),
                        expected_revision: session.revision,
                        text: "Compete for the same session".into(),
                        device_id: "mac-local".into(),
                        continuation: false,
                        remote_route: Some(competing_route),
                    },
                },
            },
        )
        .unwrap();
    let competing = wait(&worker, person, competing_id);
    assert_eq!(competing.failure, Some(AgentFailure::Conflict));
    assert!(!root_cancellation.is_cancelled());

    let preview_id = Uuid::new_v4();
    worker
        .request(
            person,
            preview_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::CalendarExperts { setup: None },
            },
        )
        .unwrap();
    let preview = wait(&worker, person, preview_id);
    assert_eq!(preview.failure, None, "preview: {preview:?}");
    let current = preview.calendar_experts.unwrap();
    assert!(!root_cancellation.is_cancelled());

    let revoke_id = Uuid::new_v4();
    worker
        .request(
            person,
            revoke_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::CalendarAccess {
                    change: encode_contract(&CalendarAccessConfiguration {
                        instance_id: current.registry.instance_id,
                        expected_revision: current.registry.revision,
                        setup_id,
                        change: CalendarAccessChange::SetEnabled { enabled: false },
                    })
                    .unwrap(),
                },
            },
        )
        .unwrap();
    let revoked = wait(&worker, person, revoke_id);
    assert_eq!(revoked.failure, None, "revoke: {revoked:?}");
    assert!(
        revoked
            .calendar_experts
            .as_ref()
            .unwrap()
            .views
            .iter()
            .all(|view| !view.enabled)
    );
    assert!(!root_cancellation.is_cancelled());

    let query_id = Uuid::new_v4();
    worker
        .request(
            person,
            query_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::Connections {},
            },
        )
        .unwrap();
    let query = wait(&worker, person, query_id);
    assert_eq!(query.failure, None, "query: {query:?}");
    assert!(query.connections.is_some());
    assert!(!root_cancellation.is_cancelled());
    assert!(
        !worker
            .request(
                person,
                conversation_id,
                AgentVaultOperationDto::Poll { after_sequence: 0 },
            )
            .unwrap()
            .done
    );

    release.store(true, Ordering::Release);
    let conversation = wait(&worker, person, conversation_id);
    assert_eq!(conversation.failure, None, "conversation: {conversation:?}");
    assert!(!root_cancellation.is_cancelled());
    worker
        .request(
            person,
            competing_id,
            AgentVaultOperationDto::Release {},
        )
        .unwrap();
    worker
        .request(person, preview_id, AgentVaultOperationDto::Release {})
        .unwrap();
    worker
        .request(person, revoke_id, AgentVaultOperationDto::Release {})
        .unwrap();
    worker
        .request(person, query_id, AgentVaultOperationDto::Release {})
        .unwrap();
    worker
        .request(
            person,
            conversation_id,
            AgentVaultOperationDto::Release {},
        )
        .unwrap();
    server.join().unwrap();
}

#[test]
fn t08_stop_cancels_the_admitted_production_root() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let (mut route, entered, release, server) = blocking_answer_server();
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "conversation-cancel-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let request_id = Uuid::new_v4();
    worker
        .request(
            person,
            request_id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::ConversationTurn {
                    request: floe_protocol::AgentConversationTurnRequestDto {
                        session_id: session.id.to_string(),
                        expected_revision: session.revision,
                        text: "Cancel this run".into(),
                        device_id: "mac-local".into(),
                        continuation: false,
                        remote_route: Some(route),
                    },
                },
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }

    worker
        .request(person, request_id, AgentVaultOperationDto::Stop {})
        .unwrap();
    let cancelled = wait(&worker, person, request_id);
    assert_eq!(
        cancelled.session.unwrap().last_outcome,
        Some(floe_agent::AgentOutcome::Halted {
            reason: AgentFailure::Cancelled,
        })
    );
    let command = worker
        .conversation_query(
            person,
            ConversationQuery::Command(floe_kernel::CommandId::from_uuid(request_id).unwrap()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(command.state, floe_conversation::RunState::Cancelled);
    assert_eq!(
        worker
            .conversation_query(person, ConversationQuery::Run(command.run_id))
            .unwrap(),
        Some(command.clone())
    );
    assert_eq!(
        worker
            .conversation_query(person, ConversationQuery::Message(command.run_id))
            .unwrap(),
        Some(command)
    );
    worker
        .request(person, request_id, AgentVaultOperationDto::Release {})
        .unwrap();
    release.store(true, Ordering::Release);
    server.join().unwrap();
}

#[test]
fn production_general_turn_does_not_require_or_install_builtin_setup() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let vault = runtime
        .block_on(EncryptedAgentVault::create(&root, person, keys.clone()))
        .unwrap();
    let session = runtime.block_on(vault.create_session()).unwrap();
    assert!(runtime
        .block_on(vault.builtin_expert_overview())
        .unwrap()
        .is_none());
    drop(vault);

    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    let fetched = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Get {
                session_id: session.id.to_string(),
            },
        },
    );
    assert_eq!(fetched.failure, None);
    assert_eq!(fetched.session.unwrap().id, session.id);
    let resumed = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Resume {},
        },
    );
    assert_eq!(resumed.failure, None);
    assert_eq!(resumed.session.unwrap().id, session.id);
    let (mut route, server) = answer_server(vec![floe_agent::ModelStep::Answer {
        text: "General answer without experts".into(),
    }]);
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "conversation-without-builtins-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let result = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationTurn {
            request: floe_protocol::AgentConversationTurnRequestDto {
                session_id: session.id.to_string(),
                expected_revision: session.revision,
                text: "Answer without expert setup".into(),
                device_id: "mac-local".into(),
                continuation: false,
                remote_route: Some(route),
            },
        },
    );
    assert_eq!(result.failure, None, "general turn: {result:?}");
    assert!(matches!(
        result.session.unwrap().messages.last(),
        Some(AgentMessage::Assistant { text, .. }) if text == "General answer without experts"
    ));
    assert_eq!(server.join().unwrap().len(), 1);
    perform(&worker, person, AgentVaultActionDto::Lock {});
    drop(worker);

    let vault = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    assert!(runtime
        .block_on(vault.builtin_expert_overview())
        .unwrap()
        .is_none());
}

#[test]
fn production_continuation_uses_the_persisted_conversation_run_without_duplicate_user_text() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let person = PersonId::new();
    let keys = Keys::default();
    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    perform(&worker, person, AgentVaultActionDto::Lock {});
    drop(worker);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let vault = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys.clone()))
        .unwrap();
    runtime
        .block_on(vault.activate_conversation_executor())
        .unwrap();
    let run_id = floe_agent_contract::RunId::new();
    runtime
        .block_on(vault.admit_conversation_turn(
            floe_core::VaultConversationAdmissionRequest {
                run_id,
                command_id: floe_agent_contract::CommandId::new(),
                session_id: session.id,
                person_id: person,
                expected_session_revision: session.revision,
                request_digest: [7; 32],
                text: "Finish after the deadline".into(),
                continuation: None,
                model_placement: floe_agent::ModelPlacement::DeviceLocal,
            },
        ))
        .unwrap();
    runtime
        .block_on(vault.finish_conversation_run(
            run_id,
            1,
            floe_core::VaultConversationTerminal {
                state: floe_core::VaultConversationRunState::TimedOut,
                output: None,
                coverage: floe_agent_contract::DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        ))
        .unwrap();
    drop(vault);

    let worker = Worker::new(root, keys).unwrap();
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    let (mut route, server) = answer_server(vec![floe_agent::ModelStep::Answer {
        text: "Continued once".into(),
    }]);
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "conversation-continuation-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let action = AgentVaultActionDto::ConversationTurn {
            request: floe_protocol::AgentConversationTurnRequestDto {
                session_id: session.id.to_string(),
                expected_revision: session.revision + 2,
                text: "Finish after the deadline".into(),
                device_id: "mac-local".into(),
                continuation: true,
                remote_route: Some(route),
            },
        };
    let request_id = Uuid::new_v4();
    worker
        .request(
            person,
            request_id,
            AgentVaultOperationDto::Submit {
                action: action.clone(),
            },
        )
        .unwrap();
    let result = wait(&worker, person, request_id);
    worker
        .request(person, request_id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(result.failure, None, "continuation: {result:?}");
    let continued = result.session.clone().unwrap();
    assert_eq!(
        continued
            .messages
            .iter()
            .filter(|message| matches!(message, AgentMessage::User { .. }))
            .count(),
        1
    );
    assert!(matches!(
        continued.messages.last(),
        Some(AgentMessage::Assistant { text, .. }) if text == "Continued once"
    ));
    assert_eq!(continued.continuation, None);

    worker
        .request(
            person,
            request_id,
            AgentVaultOperationDto::Submit { action },
        )
        .unwrap();
    let replay = wait(&worker, person, request_id);
    worker
        .request(person, request_id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(replay.failure, None, "continuation replay: {replay:?}");
    assert_eq!(replay.session, result.session);
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn production_builtin_expert_persists_access_denial_through_registered_task() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let keys = Keys::default();
    let person = PersonId::new();
    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Create {}).failure,
        None
    );
    let session = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationSession {
            operation: AgentConversationSessionOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let (mut route, server) = commitments_denial_server();
    route.pairing = Some(floe_protocol::AgentRemotePairingDto {
        client_id: "commitments-expert-test".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    });
    let result = perform(
        &worker,
        person,
        AgentVaultActionDto::ConversationTurn {
            request: floe_protocol::AgentConversationTurnRequestDto {
                session_id: session.id.to_string(),
                expected_revision: session.revision,
                text: "Review my commitments".into(),
                device_id: "mac-local".into(),
                continuation: false,
                remote_route: Some(route),
            },
        },
    );
    assert_eq!(result.failure, None, "result: {result:?}");
    let session = result.session.unwrap();
    let task_id = session
        .messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::Delegation { task, .. }
                if task.agent_id == floe_agent::BuiltinExpertKind::Commitments.package_id()
                    && task.state == floe_agent::A2ATaskState::Failed
                    && task.failure == Some(AgentFailure::AccessReviewRequired) =>
            {
                Some(task.id)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("durable Commitments denial: {session:?}"));
    assert_eq!(server.join().unwrap().len(), 2);
    assert_eq!(
        perform(&worker, person, AgentVaultActionDto::Lock {}).failure,
        None
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reopened = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    let task = runtime
        .block_on(reopened.task(floe_agent_contract::TaskId::from_uuid(task_id).unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(task.snapshot.state, floe_agent_contract::TaskState::Failed);
    assert_eq!(
        task.snapshot.issue,
        Some(AgentFailure::AccessReviewRequired)
    );
}

fn commitments_denial_server() -> (
    floe_protocol::AgentRemoteRouteDto,
    std::thread::JoinHandle<Vec<String>>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let steps = [
            floe_agent::ModelStep::Delegate {
                agent_id: floe_agent::BuiltinExpertKind::Commitments
                    .package_id()
                    .into(),
                message: "Review my commitments".into(),
            },
            floe_agent::ModelStep::Answer {
                text: "Mail access needs review before I can check commitments.".into(),
            },
        ];
        let mut requests = vec![];
        let mut model_index = 0;
        for _ in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let count = socket.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if body.len() >= length {
                        requests.push(text.to_string());
                        break;
                    }
                }
            }
            let request = requests.last().unwrap();
            let body = if request.starts_with("POST /v1/views/mail.communication ") {
                assert!(
                    request.starts_with("POST /v1/views/mail.communication "),
                    "unexpected request: {}",
                    request.lines().next().unwrap_or_default()
                );
                let now = chrono::Utc::now().timestamp_millis();
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "mail.communication",
                        "source_handle": "mail:selected",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 300_000,
                        "coverage_complete": true,
                        "items": []
                    }
                })
                .to_string()
            } else {
                assert!(request.starts_with("POST /v1/agent "));
                let step = &steps[model_index];
                model_index += 1;
                let output = serde_json::json!({"output": [step], "used_tokens": 10});
                serde_json::json!({
                    "schema_version": 1,
                    "purpose": "everyday_assistance",
                    "trace_id": "a".repeat(32),
                    "routing": {
                        "placement": "server_local",
                        "external_transfer": false,
                        "replay_source": "a".repeat(64)
                    },
                    "output": output.to_string()
                })
                .to_string()
            };
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
        requests
    });
    (
        floe_protocol::AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "a".repeat(32),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            recipient: None,
            calendar_connections: vec![],
            pairing: None,
        },
        server,
    )
}

fn answer_server(
    steps: Vec<floe_agent::ModelStep>,
) -> (
    floe_protocol::AgentRemoteRouteDto,
    std::thread::JoinHandle<Vec<String>>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = vec![];
        for (index, mut step) in steps.into_iter().enumerate() {
            let has_call = matches!(step, floe_agent::ModelStep::Call { .. });
            if let floe_agent::ModelStep::Call { capability_id, .. } = &mut step {
                *capability_id = remote_tool_name(capability_id);
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let count = socket.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if body.len() >= length {
                        requests.push(text.to_string());
                        break;
                    }
                }
            }
            let output = if has_call {
                serde_json::json!({
                    "output": [step],
                    "used_tokens": 10,
                    "call_ids": [format!("call-{index}")],
                    "replay": [{"type": "reasoning", "encrypted_content": format!("replay-{index}")}],
                })
            } else {
                serde_json::json!({"output": [step], "used_tokens": 10})
            };
            let response = serde_json::json!({
                "schema_version": 1, "purpose": "everyday_assistance", "trace_id": "a".repeat(32),
                "routing": { "placement": "server_local", "external_transfer": false, "replay_source": "a".repeat(64) },
                "output": output.to_string(),
            }).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).unwrap();
        }
        requests
    });
    (
        floe_protocol::AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "a".repeat(32),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            recipient: None,
            calendar_connections: vec![],
            pairing: None,
        },
        server,
    )
}

fn blocking_answer_server() -> (
    floe_protocol::AgentRemoteRouteDto,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let server_entered = Arc::clone(&entered);
    let server_release = Arc::clone(&release);
    let server = std::thread::spawn(move || {
        let mut socket = listener.accept().unwrap().0;
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        loop {
            let mut chunk = [0; 4096];
            let count = socket.read(&mut chunk).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            let text = String::from_utf8_lossy(&bytes);
            let Some((headers, body)) = text.split_once("\r\n\r\n") else {
                continue;
            };
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap();
            if body.len() >= length {
                break;
            }
        }
        server_entered.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !server_release.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let output = serde_json::json!({
            "output": [{"type": "answer", "text": "The model resumed."}],
            "used_tokens": 10,
        });
        let response = serde_json::json!({
            "schema_version": 1,
            "purpose": "everyday_assistance",
            "trace_id": "a".repeat(32),
            "routing": {
                "placement": "server_local",
                "external_transfer": false,
                "replay_source": "a".repeat(64),
            },
            "output": output.to_string(),
        })
        .to_string();
        let _ = socket.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )
            .as_bytes(),
        );
    });
    (
        floe_protocol::AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "a".repeat(32),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            recipient: None,
            calendar_connections: vec![],
            pairing: None,
        },
        entered,
        release,
        server,
    )
}

fn remote_tool_name(identifier: &str) -> String {
    let hash = identifier
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, value| {
            (hash ^ u64::from(value)).wrapping_mul(0x100000001b3)
        });
    format!("floe_{hash:016x}")
}

#[test]
fn calendar_setup_worker_inspects_without_initializing_installs_and_reconciles_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let person = PersonId::new();
    let keys = Keys::default();
    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    let inspect = AgentVaultActionDto::CalendarExperts { setup: None };
    assert_eq!(
        perform(&worker, person, inspect.clone()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    assert!(!root.exists());
    perform(&worker, person, AgentVaultActionDto::Create {});
    let empty = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    assert_eq!(empty.registry.revision, 0);
    assert!(empty.views.is_empty() && empty.setups.is_empty());
    assert!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::Registry { change: None }
        )
        .registry
        .is_none()
    );
    let setup = CalendarExpertSetup {
        instance_id: empty.registry.instance_id,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        provider: floe_domain::CalendarProvider::Fixture,
        device_id: "mac-local".into(),
        calendar_ids: vec!["explicit-native-setup-canary".into()],
        connection_scope: floe_domain::CalendarScope::Selected,
        connection_revision: 1,
        source_authority: None,
        reviewed_native_subject_fingerprint: None,
    };
    let action = AgentVaultActionDto::CalendarExperts {
        setup: Some(encode_contract(&setup).unwrap()),
    };
    let id = Uuid::new_v4();
    worker
        .request(
            person,
            id,
            AgentVaultOperationDto::Submit {
                action: action.clone(),
            },
        )
        .unwrap();
    let completed = wait(&worker, person, id);
    assert_eq!(
        worker
            .request(
                person,
                id,
                AgentVaultOperationDto::Submit {
                    action: action.clone()
                }
            )
            .unwrap(),
        completed
    );
    assert_eq!(
        worker.request(
            PersonId::new(),
            id,
            AgentVaultOperationDto::Poll { after_sequence: 0 }
        ),
        Err(AgentFailure::NotFound)
    );
    assert!(
        completed.events.is_empty() && completed.session.is_none() && completed.registry.is_none()
    );
    let installed = completed.calendar_experts.unwrap();
    assert_eq!(installed.registry.revision, 1);
    assert!(!installed.views[0].enabled);
    assert!(
        installed
            .registry
            .assignments
            .iter()
            .all(|entry| !entry.enabled)
    );
    worker
        .request(person, id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(
        perform(&worker, person, action.clone())
            .calendar_experts
            .as_ref(),
        Some(&installed)
    );
    let mut changed = setup;
    changed.calendar_ids = vec!["retarget".into()];
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarExperts {
                setup: Some(encode_contract(&changed).unwrap())
            }
        )
        .failure,
        Some(AgentFailure::Conflict)
    );
    for enabled in [true, false] {
        let current = perform(&worker, person, inspect.clone())
            .calendar_experts
            .unwrap();
        let result = perform(
            &worker,
            person,
            AgentVaultActionDto::Registry {
                change: Some(
                    encode_contract(&RegistryConfiguration {
                        instance_id: current.registry.instance_id,
                        expected_revision: current.registry.revision,
                        target: RegistryConfigurationTarget::CalendarView {
                            id: installed.views[0].handle,
                            enabled,
                        },
                    })
                    .unwrap(),
                ),
            },
        );
        assert!(result.failure.is_none());
    }
    let before = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    perform(&worker, person, AgentVaultActionDto::Lock {});
    drop(worker);
    let worker = Worker::new(root, keys.clone()).unwrap();
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    assert_eq!(
        perform(&worker, person, action).calendar_experts.as_ref(),
        Some(&before)
    );
    assert_eq!(
        perform(&worker, PersonId::new(), inspect.clone()).failure,
        Some(AgentFailure::NotFound)
    );
    keys.0.unavailable.store(true, Ordering::Release);
    let denied = perform(&worker, person, inspect.clone());
    assert_eq!(denied.failure, Some(AgentFailure::VaultUnavailable));
    assert!(denied.calendar_experts.is_none());
    keys.0.unavailable.store(false, Ordering::Release);
    assert_eq!(
        perform(&worker, person, inspect).failure,
        Some(AgentFailure::VaultUnavailable)
    );
}

#[test]
fn blocked_setup_keeps_worker_ownership_until_cancelled_work_really_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let keys = Keys::default();
    let worker = Worker::new(directory.path().join("vaults"), keys.clone()).unwrap();
    let person = PersonId::new();
    perform(&worker, person, AgentVaultActionDto::Create {});
    let inspect = AgentVaultActionDto::CalendarExperts { setup: None };
    let empty = perform(&worker, person, inspect.clone())
        .calendar_experts
        .unwrap();
    let request = CalendarExpertSetup {
        instance_id: empty.registry.instance_id,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        provider: floe_domain::CalendarProvider::Fixture,
        device_id: "test-device".into(),
        calendar_ids: vec!["bounded-scope".into()],
        connection_scope: floe_domain::CalendarScope::Selected,
        connection_revision: 1,
        source_authority: None,
        reviewed_native_subject_fingerprint: None,
    };
    *keys.0.paused.lock().unwrap() = true;
    keys.0.entered.store(false, Ordering::Release);
    let id = Uuid::new_v4();
    worker
        .request(
            person,
            id,
            AgentVaultOperationDto::Submit {
                action: AgentVaultActionDto::CalendarExperts {
                    setup: Some(encode_contract(&request).unwrap()),
                },
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
            .request(person, id, AgentVaultOperationDto::Stop {})
            .unwrap()
            .done
    );
    assert_eq!(
        worker.request(person, id, AgentVaultOperationDto::Release {}),
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        worker.request(
            person,
            Uuid::new_v4(),
            AgentVaultOperationDto::Submit {
                action: inspect.clone()
            }
        ),
        Err(AgentFailure::Conflict)
    );
    *keys.0.paused.lock().unwrap() = false;
    keys.0.wake.notify_all();
    let cancelled = wait(&worker, person, id);
    assert_eq!(cancelled.failure, Some(AgentFailure::Cancelled));
    assert!(cancelled.calendar_experts.is_none());
    worker
        .request(person, id, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(
        perform(&worker, person, inspect).calendar_experts.as_ref(),
        Some(&empty)
    );
    let retry = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts {
            setup: Some(encode_contract(&request).unwrap()),
        },
    );
    assert_eq!(retry.calendar_experts.unwrap().registry.revision, 1);
}
