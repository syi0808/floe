use floe_agent::{
    AgentMessage, CalendarExpertSetup, RegistryConfiguration, RegistryConfigurationTarget,
};

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
    let (route, server) = answer_server(vec![
        floe_agent::ModelStep::Answer {
            text: "Hello!".into(),
        },
        floe_agent::ModelStep::Delegate {
            agent_id: floe_agent::BuiltinExpertKind::Schedule.package_id().into(),
            message: "Read my calendar".into(),
        },
        floe_agent::ModelStep::Call {
            capability_id: "calendar.read".into(),
            input: serde_json::json!({
                "range_start_unix_ms": chrono::Utc::now().timestamp_millis(),
                "range_end_unix_ms": chrono::Utc::now().timestamp_millis() + 60_000,
            })
            .to_string(),
        },
        floe_agent::ModelStep::Answer {
            text: "Calendar is unavailable; we can still chat.".into(),
        },
    ]);
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
    assert!(calendar.messages.iter().any(|message| matches!(message,
        AgentMessage::Delegation { task, .. } if task.failure == Some(AgentFailure::CapabilityDenied))));
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .all(|request| request.starts_with("POST /v1/agent "))
    );
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
        for step in steps {
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
            let response = serde_json::json!({
                "schema_version": 1, "purpose": "everyday_assistance", "trace_id": "a".repeat(32),
                "routing": { "placement": "server_local", "external_transfer": false, "replay_source": "a".repeat(64) },
                "output": serde_json::json!({ "output": [step], "used_tokens": 10 }).to_string(),
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
