use floe_conversation::AgentMessage;
use floe_conversation::ProfileSelection;

use crate::{ConversationSessionOperation, ConversationTurnRequest};
use std::os::unix::fs::PermissionsExt;

use super::*;

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireStep {
    Answer { text: String },
    Call { capability_id: String, input: String },
    Delegate {
        agent_id: String,
        message: String,
        context_refs: Vec<String>,
    },
}

#[test]
fn production_conversation_replays_the_same_request_without_model_redispatch() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let started = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    );
    assert_eq!(started.failure, None, "started: {started:?}");
    let session = started.session.unwrap();
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "One durable answer".into(),
    }]);
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let action = || WorkerAction::ConversationTurn {
        request: Box::new(ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Answer once".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )),
    };
    let request_id = Uuid::new_v4();
    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(action()),
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
        .request(person, request_id, WorkerOperation::Stop)
        .unwrap();
    assert!(!completed_job_cancellation.is_cancelled());
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(first.failure, None, "first: {first:?}");

    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(action()),
            },
        )
        .unwrap();
    let replay = wait(&worker, person, request_id);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(replay.failure, None, "replay: {replay:?}");
    assert_eq!(replay.session, first.session);

    let mut changed = action();
    let WorkerAction::ConversationTurn { request } = &mut changed else {
        unreachable!()
    };
    request.text = "Changed payload".into();
    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(changed),
            },
        )
        .unwrap();
    let conflict = wait(&worker, person, request_id);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(conflict.failure, Some(AgentFailure::Conflict));
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn terminal_conversation_accepts_the_next_run_without_ui_release() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, server) = answer_server(vec![
        WireStep::Answer {
            text: "First durable answer".into(),
        },
        WireStep::Answer {
            text: "Second durable answer".into(),
        },
    ]);
    let first_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            first_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Answer first".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
            },
        )
        .unwrap();
    let first = wait(&worker, person, first_id);
    assert_eq!(first.failure, None, "first: {first:?}");
    let first_session = first.session.unwrap();

    let second_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            second_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        first_session.revision,
                        "Answer second".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
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
            WorkerOperation::Poll { after_sequence: 0 },
        )
        .unwrap();
    assert!(retained.done);
    assert_eq!(retained.session.unwrap(), first_session);
    worker
        .request(person, first_id, WorkerOperation::Release)
        .unwrap();
    worker
        .request(person, second_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn t09_preview_does_not_stop_chat_and_t22_network_wait_does_not_hold_vault() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        keys.clone(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    install_builtin_calendar_setup(&worker, &directory.path().join("vaults"), person, &keys);
    let started = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    );
    assert_eq!(started.failure, None, "started: {started:?}");
    let session = started.session.unwrap();
    let (mock, entered, release, server) = blocking_answer_server();
    let conversation_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            conversation_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Wait for the model".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
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
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            competing_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Compete for the same session".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
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
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::Registry { change: None }),
            },
        )
        .unwrap();
    let preview = wait(&worker, person, preview_id);
    assert_eq!(preview.failure, None, "preview: {preview:?}");
    assert!(preview.registry.is_some());
    assert!(!root_cancellation.is_cancelled());

    let query_id = Uuid::new_v4();
    worker
        .request(
            person,
            query_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::Connections),
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
                WorkerOperation::Poll { after_sequence: 0 },
            )
            .unwrap()
            .done
    );

    release.store(true, Ordering::Release);
    let conversation = wait(&worker, person, conversation_id);
    assert_eq!(conversation.failure, None, "conversation: {conversation:?}");
    assert!(!root_cancellation.is_cancelled());
    worker
        .request(person, competing_id, WorkerOperation::Release)
        .unwrap();
    worker
        .request(person, preview_id, WorkerOperation::Release)
        .unwrap();
    worker
        .request(person, query_id, WorkerOperation::Release)
        .unwrap();
    worker
        .request(person, conversation_id, WorkerOperation::Release)
        .unwrap();
    server.join().unwrap();
}

#[test]
fn t08_cancel_run_is_principal_bound_and_cancels_the_admitted_production_root() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    assert_eq!(
        worker
            .app_events
            .read(&person.to_string(), 7, None, None, 16),
        crate::EventRead::ResyncRequired { snapshot_cursor: 0 }
    );
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, entered, release, server) = blocking_answer_server();
    let request_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let request = || {
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Cancel this run".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )
    };
    let admitted = worker
        .start_conversation(
            person,
            floe_kernel::CommandId::from_uuid(request_id).unwrap(),
            request(),
        )
        .unwrap();
    assert_eq!(admitted.state, floe_conversation::RunState::Working);
    assert!(matches!(
        worker.app_events.read(&person.to_string(), 7, Some(7), Some(0), 16),
        crate::EventRead::Events {
            next_cursor: 1,
            events
        } if matches!(
            events.as_slice(),
            [crate::ConversationEvent {
                payload: crate::EventPayload::CommandUpdated { run_id, .. },
                ..
            }] if *run_id == admitted.run_id
        )
    ));
    assert_eq!(
        worker
            .start_conversation(
                person,
                floe_kernel::CommandId::from_uuid(request_id).unwrap(),
                request(),
            )
            .unwrap(),
        admitted
    );
    let mut conflicting = request();
    conflicting.text = "Different payload".into();
    assert_eq!(
        worker.start_conversation(
            person,
            floe_kernel::CommandId::from_uuid(request_id).unwrap(),
            conflicting,
        ),
        Err(AgentFailure::Conflict)
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(
        worker.cancel_conversation(
            PersonId::new(),
            floe_kernel::CommandId::new(),
            admitted.run_id,
        ),
        Err(AgentFailure::NotFound)
    );
    let cancel_command_id = floe_kernel::CommandId::new();
    assert_eq!(
        worker.cancel_conversation(person, cancel_command_id, admitted.run_id),
        Ok(floe_conversation::CancelRunStatus::Cancelled)
    );
    let cancelled = wait(&worker, person, request_id);
    assert_eq!(
        cancelled.session.unwrap().last_outcome,
        Some(floe_conversation::AgentOutcome::Halted {
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
    assert!(matches!(
        worker.app_events.read(&person.to_string(), 7, Some(7), Some(1), 16),
        crate::EventRead::Events {
            next_cursor: 2,
            events
        } if matches!(
            events.as_slice(),
            [crate::ConversationEvent {
                payload: crate::EventPayload::RunUpdated(run),
                ..
            }] if run.run_id == admitted.run_id
                && run.state == floe_conversation::RunState::Cancelled
        )
    ));
    assert_eq!(
        worker.cancel_conversation(person, cancel_command_id, admitted.run_id),
        Ok(floe_conversation::CancelRunStatus::Inactive)
    );
    assert_eq!(
        worker.cancel_conversation(person, cancel_command_id, floe_kernel::RunId::new()),
        Err(AgentFailure::Conflict)
    );
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
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    release.store(true, Ordering::Release);
    server.join().unwrap();
}

#[test]
fn same_request_id_with_normalization_equivalent_text_replays_without_redispatch() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "One durable answer".into(),
    }]);
    let request_id = Uuid::new_v4();
    let command_id = floe_kernel::CommandId::from_uuid(request_id).unwrap();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let request = || {
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Answer once".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )
    };
    let admitted = worker
        .start_conversation(person, command_id, request())
        .unwrap();
    let mut equivalent = request();
    equivalent.text = "  Answer once\n".into();
    assert_eq!(
        worker
            .start_conversation(person, command_id, equivalent)
            .unwrap(),
        admitted
    );
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "first: {finished:?}");
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn same_request_id_with_different_profile_conflicts() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "One durable answer".into(),
    }]);
    let request_id = Uuid::new_v4();
    let command_id = floe_kernel::CommandId::from_uuid(request_id).unwrap();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let request = || {
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Answer once".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )
    };
    worker
        .start_conversation(person, command_id, request())
        .unwrap();
    let mut other = request();
    other.profile = ProfileSelection::Explicit("local-fast".into());
    assert_eq!(
        worker.start_conversation(person, command_id, other),
        Err(AgentFailure::Conflict)
    );
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "first: {finished:?}");
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn same_request_id_with_different_continuation_claim_conflicts() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "One durable answer".into(),
    }]);
    let request_id = Uuid::new_v4();
    let command_id = floe_kernel::CommandId::from_uuid(request_id).unwrap();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let request = || {
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Answer once".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )
    };
    worker
        .start_conversation(person, command_id, request())
        .unwrap();
    let mut continued = request();
    continued.continuation = true;
    assert_eq!(
        worker.start_conversation(person, command_id, continued),
        Err(AgentFailure::Conflict)
    );
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "first: {finished:?}");
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn same_request_id_with_connection_refresh_only_replays_without_redispatch() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "One durable answer".into(),
    }]);
    let (refreshed_mock, idle) = answer_server(vec![]);
    let request_id = Uuid::new_v4();
    let command_id = floe_kernel::CommandId::from_uuid(request_id).unwrap();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let request = || {
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Answer once".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )
    };
    let admitted = worker
        .start_conversation(person, command_id, request())
        .unwrap();
    let finished = wait(&worker, person, request_id);
    let refreshed_request = request();
    // A refreshed stored connection (a different mock server) is runtime
    // state, not command identity: the duplicate still replays.
    connections.replace(Some(saved_server_connection(
        &refreshed_mock,
        person,
        "mac-local",
    )));
    assert_eq!(
        worker
            .start_conversation(person, command_id, refreshed_request)
            .unwrap(),
        admitted
    );
    assert_eq!(finished.failure, None, "first: {finished:?}");
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
    assert!(idle.join().unwrap().is_empty());
}

#[test]
fn inflight_connection_refresh_replays_without_redispatch() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, entered, release, server) = blocking_answer_server();
    let (refreshed_mock, idle) = answer_server(vec![]);
    let request_id = Uuid::new_v4();
    let command_id = floe_kernel::CommandId::from_uuid(request_id).unwrap();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let request = || {
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Answer once".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        )
    };
    let admitted = worker
        .start_conversation(person, command_id, request())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        worker
            .conversation_query(person, ConversationQuery::Run(admitted.run_id))
            .unwrap()
            .unwrap()
            .state,
        floe_conversation::RunState::Working,
    );
    let refreshed_request = request();
    connections.replace(Some(saved_server_connection(
        &refreshed_mock,
        person,
        "mac-local",
    )));
    assert_eq!(
        worker
            .start_conversation(person, command_id, refreshed_request)
            .unwrap(),
        admitted
    );
    release.store(true, Ordering::Release);
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "first: {finished:?}");
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    server.join().unwrap();
    assert!(idle.join().unwrap().is_empty());
}

#[test]
fn production_general_turn_does_not_require_or_install_builtin_setup() {
    let connections = TestConnections::default();
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
    assert!(
        runtime
            .block_on(vault.expert_install_overview())
            .unwrap()
            .is_none()
    );
    drop(vault);

    let worker =
        Worker::new_with_connection_store(root.clone(), keys.clone(), connections.store()).unwrap();
    perform(&worker, person, WorkerAction::Unlock);
    let fetched = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Get {
                session_id: session.id,
            },
        },
    );
    assert_eq!(fetched.failure, None);
    assert_eq!(fetched.session.unwrap().id, session.id);
    let resumed = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Resume,
        },
    );
    assert_eq!(resumed.failure, None);
    assert_eq!(resumed.session.unwrap().id, session.id);
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "General answer without experts".into(),
    }]);
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let result = perform(
        &worker,
        person,
        WorkerAction::ConversationTurn {
            request: Box::new(ConversationTurnRequest::new(
                session.id,
                session.revision,
                "Answer without expert setup".into(),
                "mac-local".into(),
                ProfileSelection::Explicit("server-model".into()),
                false,
                None,
            )),
        },
    );
    assert_eq!(result.failure, None, "general turn: {result:?}");
    assert!(matches!(
        result.session.unwrap().messages.last(),
        Some(AgentMessage::Assistant { text, .. }) if text == "General answer without experts"
    ));
    assert_eq!(server.join().unwrap().len(), 1);
    perform(&worker, person, WorkerAction::Lock);
    drop(worker);

    let vault = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    assert!(
        runtime
            .block_on(vault.expert_install_overview())
            .unwrap()
            .is_none()
    );
}

#[test]
fn production_continuation_uses_the_persisted_conversation_run_without_duplicate_user_text() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let person = PersonId::new();
    let keys = Keys::default();
    let worker =
        Worker::new_with_connection_store(root.clone(), keys.clone(), connections.store()).unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    perform(&worker, person, WorkerAction::Lock);
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
        .block_on(
            vault.admit_conversation_turn(floe_vault::VaultConversationAdmissionRequest {
                run_id,
                command_id: floe_agent_contract::CommandId::new(),
                session_id: session.id,
                person_id: person,
                expected_session_revision: session.revision,
                request_digest: [7; 32],
                text: "Finish after the deadline".into(),
                continuation: None,
                retry_of: None,
                resume: None,
                profile: floe_conversation::ProfileSelection::Explicit("server-model".into()),
            }),
        )
        .unwrap();
    runtime
        .block_on(vault.finish_conversation_run(
            run_id,
            1,
            floe_vault::VaultConversationTerminal {
                state: floe_vault::VaultConversationRunState::TimedOut,
                output: None,
                coverage: floe_agent_contract::DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        ))
        .unwrap();
    drop(vault);

    let worker = Worker::new_with_connection_store(root, keys, connections.store()).unwrap();
    perform(&worker, person, WorkerAction::Unlock);
    let (mock, server) = answer_server(vec![WireStep::Answer {
        text: "Continued once".into(),
    }]);
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let action = || WorkerAction::ConversationTurn {
        request: Box::new(ConversationTurnRequest::new(
            session.id,
            session.revision + 2,
            "Finish after the deadline".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            true,
            None,
        )),
    };
    let request_id = Uuid::new_v4();
    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(action()),
            },
        )
        .unwrap();
    let result = wait(&worker, person, request_id);
    worker
        .request(person, request_id, WorkerOperation::Release)
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
            WorkerOperation::Submit {
                action: Box::new(action()),
            },
        )
        .unwrap();
    let replay = wait(&worker, person, request_id);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(replay.failure, None, "continuation replay: {replay:?}");
    assert_eq!(replay.session, result.session);
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn production_builtin_expert_completes_blocked_task_with_durable_ref() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let keys = Keys::default();
    let person = PersonId::new();
    // The mock answers both the root model and the delegated builtin
    // endpoint; the endpoint reads it from its injected fixture store.
    let (mock, server) = commitments_denial_server();
    let worker = Worker::new_with_connection_store(
        root.clone(),
        keys.clone(),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
            saved_server_connection(&mock, person, "mac-local"),
        )),
    )
    .unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    install_builtin_mail_setup(&worker, &root, person, &keys);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let result = perform(
        &worker,
        person,
        WorkerAction::ConversationTurn {
            request: Box::new(ConversationTurnRequest::new(
                session.id,
                session.revision,
                "Review my commitments".into(),
                "mac-local".into(),
                ProfileSelection::Explicit("server-model".into()),
                false,
                None,
            )),
        },
    );
    assert_eq!(result.failure, None, "result: {result:?}");
    let session = result.session.unwrap();
    // The blocked mandatory source completes the task with a
    // needs_user_action report and a durable safe ref — never a failure.
    let task_id = session
        .messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::Delegation { task, .. }
                if task.agent_id
                    == floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()
                    && task.state == floe_experts::A2ATaskState::Completed
                    && task.failure.is_none()
                    && task.artifacts.iter().any(|artifact| {
                        artifact.parts.iter().any(|part| {
                            matches!(
                                part,
                                floe_experts::A2APart::Data { data, .. }
                                    if data.contains("needs_user_action")
                            )
                        })
                    })
                    && task.artifacts.iter().any(|artifact| {
                        artifact.parts.iter().any(|part| {
                            matches!(
                                part,
                                floe_experts::A2APart::Data { media_type, .. }
                                    if media_type
                                        == floe_agent_contract::USER_INTERACTION_MEDIA_TYPE
                            )
                        })
                    }) =>
            {
                Some(task.id)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("durable Commitments blocker: {session:?}"));
    assert!(
        session
            .messages
            .iter()
            .any(|message| matches!(message, AgentMessage::Interaction { .. })),
        "completed turn must carry the Interaction message: {session:?}"
    );
    assert_eq!(server.join().unwrap().len(), 2);
    assert_eq!(perform(&worker, person, WorkerAction::Lock).failure, None);
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
    assert_eq!(
        task.snapshot.state,
        floe_agent_contract::TaskState::Completed
    );
    assert_eq!(task.snapshot.issue, None);
    let reference = task
        .snapshot
        .artifacts
        .iter()
        .find_map(|artifact| {
            artifact.parts.iter().find_map(|part| match part {
                floe_agent_contract::ArtifactPart::Data { media_type, data }
                    if media_type == floe_agent_contract::USER_INTERACTION_MEDIA_TYPE =>
                {
                    serde_json::from_str::<floe_agent_contract::UserInteractionRef>(data).ok()
                }
                _ => None,
            })
        })
        .expect("blocked task must carry the durable ref");
    assert_eq!(
        reference.kind,
        floe_agent_contract::UserInteractionKind::SourceAccess
    );
    let repository = floe_vault::VaultConversationRepository::new(std::sync::Arc::new(reopened));
    let stored = runtime
        .block_on(floe_conversation::InteractionRepository::get_interaction(
            &repository,
            person,
            reference.interaction_id,
        ))
        .unwrap()
        .expect("blocked task must publish a durable interaction");
    assert_eq!(stored.state, floe_conversation::InteractionState::Pending);
    assert_eq!(
        stored.origin,
        floe_conversation::InteractionOrigin::Task {
            task_id,
            capability_call_id: None,
        }
    );
}

fn install_builtin_mail_setup(
    worker: &Worker,
    root: &std::path::Path,
    person: PersonId,
    keys: &Keys,
) {
    assert_eq!(perform(worker, person, WorkerAction::Lock).failure, None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let vault = runtime
        .block_on(EncryptedAgentVault::open(root, person, keys.clone()))
        .unwrap();
    let setup = floe_experts::ExpertInstallOperation {
        instance_id: vault.registry_instance_id(),
        expected_revision: 0,
        operation_id: Uuid::new_v4(),
    };
    runtime
        .block_on(vault.install_expert_bundle(
            setup,
            &floe_experts_builtin::manifests(),
            floe_execution::Cancellation::default(),
        ))
        .unwrap();
    drop(vault);
    assert_eq!(perform(worker, person, WorkerAction::Unlock).failure, None);
}

fn install_builtin_calendar_setup(
    worker: &Worker,
    root: &std::path::Path,
    person: PersonId,
    keys: &Keys,
) {
    assert_eq!(perform(worker, person, WorkerAction::Lock).failure, None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let vault = runtime
        .block_on(EncryptedAgentVault::open(root, person, keys.clone()))
        .unwrap();
    runtime
        .block_on(vault.install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: vault.registry_instance_id(),
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &floe_experts_builtin::manifests(),
            floe_execution::Cancellation::default(),
        ))
        .unwrap();
    assert!(
        runtime
            .block_on(vault.enabled_expert_cards())
            .unwrap()
            .iter()
            .any(|card| {
                card.id == floe_experts_builtin::BuiltinExpertKind::Schedule.package_id()
            })
    );
    drop(vault);
    assert_eq!(perform(worker, person, WorkerAction::Unlock).failure, None);
}

#[test]
fn production_builtin_setup_installs_through_vault_without_sources() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let keys = Keys::default();
    let person = PersonId::new();
    let worker = Worker::new(root.clone(), keys.clone()).unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    install_builtin_mail_setup(&worker, &root, person, &keys);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Lock).failure, None);
    let vault = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    let overview = runtime
        .block_on(vault.expert_install_overview())
        .unwrap()
        .unwrap();
    assert_eq!(overview.receipt.installed.len(), 8);
    let cards = runtime.block_on(vault.enabled_expert_cards()).unwrap();
    assert_eq!(cards.len(), 8);
}

fn commitments_denial_server() -> (MockServer, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let steps = [
            WireStep::Delegate {
                agent_id: floe_experts_builtin::BuiltinExpertKind::Commitments
                    .package_id()
                    .into(),
                message: "Review my commitments".into(),
                context_refs: vec![],
            },
            WireStep::Answer {
                text: "Mail access needs review before I can check commitments.".into(),
            },
        ];
        let mut requests = vec![];
        let mut model_index = 0;
        // Two scripted non-discovery requests; canonical purposes discovery
        // is served but never counted.
        while requests.len() < 2 {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "requests: {requests:?}");
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
            let (headers, body) = loop {
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
                        .unwrap_or(0);
                    if body.len() >= length {
                        break (headers.to_string(), body.to_string());
                    }
                }
            };
            let path = headers
                .lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .nth(1)
                .unwrap_or_default();
            if path == "/v1/inference-purposes" {
                let inventory = canonical_inventory_body();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            inventory.len(),
                            inventory
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                continue;
            }
            requests.push(format!("{headers}\r\n\r\n{body}"));
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
        MockServer {
            base_url: format!("http://{address}"),
            token: "a".repeat(32),
        },
        server,
    )
}

/// A mock paired server: endpoint and credential only. Tests admit it through
/// the stored-credential path; no route is ever pre-resolved for a turn.
#[derive(Clone)]
struct MockServer {
    base_url: String,
    token: String,
}

/// Saved server connection matching a mock server. The canonical owners admit
/// it against the verified caller after admission.
fn saved_server_connection(
    mock: &MockServer,
    person: PersonId,
    device: &str,
) -> floe_inference::SavedServerConnection {
    floe_inference::SavedServerConnection {
        base_url: mock.base_url.clone(),
        token: mock.token.clone(),
        client_id: "test-client".into(),
        person_id: person.to_string(),
        device_id: device.into(),
    }
}

fn canonical_inventory_body() -> String {
    serde_json::json!({
        "schema_version": 1,
        "purposes": {
            "everyday_assistance": {
                "available": true,
                "requires_external_consent": false,
                "placement": "server_local"
            }
        }
    })
    .to_string()
}

fn answer_server(
    steps: Vec<WireStep>,
) -> (MockServer, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = vec![];
        let mut steps = steps.into_iter().enumerate().peekable();
        loop {
            // Exact termination like before: exit once the scripted steps
            // are served, without waiting on the listener.
            if steps.peek().is_none() {
                return requests;
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "requests: {requests:?}");
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
            let (headers, body) = loop {
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
                        .unwrap_or(0);
                    if body.len() >= length {
                        break (headers.to_string(), body.to_string());
                    }
                }
            };
            let path = headers
                .lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            // Canonical discovery is served but never counted: only model
            // calls advance the scripted steps.
            if path == "/v1/inference-purposes" {
                let inventory = canonical_inventory_body();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            inventory.len(),
                            inventory
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                continue;
            }
            assert_eq!(path, "/v1/agent");
            let (index, mut step) = steps.next().expect("scripted steps peeked Some");
            let has_call = matches!(step, WireStep::Call { .. });
            if let WireStep::Call { capability_id, .. } = &mut step {
                *capability_id = remote_tool_name(capability_id);
            }
            requests.push(format!("{headers}\r\n\r\n{body}"));
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
    });
    (
        MockServer {
            base_url: format!("http://{address}"),
            token: "a".repeat(32),
        },
        server,
    )
}

fn blocking_answer_server() -> (
    MockServer,
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
        // Canonical discovery first, then the single blocking model call.
        let mut socket = loop {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let path = loop {
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
                        .unwrap_or(0);
                    if body.len() >= length {
                        break headers
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or_default()
                            .to_string();
                    }
                }
            };
            if path == "/v1/inference-purposes" {
                let inventory = canonical_inventory_body();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            inventory.len(),
                            inventory
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                continue;
            }
            assert_eq!(path, "/v1/agent");
            break socket;
        };
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
        MockServer {
            base_url: format!("http://{address}"),
            token: "a".repeat(32),
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

/// Mock that serves canonical discovery, then watches for model calls until
/// released. Discovery hits and model posts are counted separately, so tests
/// can prove the canonical order: profiles first, transport after Access.
fn observing_server(
    inventory: serde_json::Value,
    agent_script: Vec<serde_json::Value>,
    external_routing: bool,
) -> (
    MockServer,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let purposes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let agent_posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let server_purposes = Arc::clone(&purposes);
    let server_posts = Arc::clone(&agent_posts);
    let server_done = Arc::clone(&done);
    let inventory_text = inventory.to_string();
    let server = std::thread::spawn(move || {
        let mut script = agent_script.into_iter();
        loop {
            if server_done.load(Ordering::Acquire) {
                return;
            }
            let mut socket = match listener.accept() {
                Ok((socket, _)) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("accept: {error}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let (headers, body) = loop {
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
                        .unwrap_or(0);
                    if body.len() >= length {
                        break (headers.to_string(), body.to_string());
                    }
                }
            };
            let path = headers
                .lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .nth(1)
                .unwrap_or_default();
            if path == "/v1/inference-purposes" {
                server_purposes.fetch_add(1, Ordering::SeqCst);
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            inventory_text.len(),
                            inventory_text
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                continue;
            }
            assert_eq!(path, "/v1/agent", "unexpected request: {headers}");
            server_posts.fetch_add(1, Ordering::SeqCst);
            let output = script.next().expect("unexpected model call").to_string();
            let (placement, external_transfer) = if external_routing {
                ("remote", true)
            } else {
                ("server_local", false)
            };
            let response = serde_json::json!({
                "schema_version": 1,
                "purpose": "everyday_assistance",
                "trace_id": "c".repeat(32),
                "routing": {
                    "placement": placement,
                    "external_transfer": external_transfer,
                    "replay_source": "c".repeat(64),
                },
                "output": output,
            })
            .to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .as_bytes(),
                )
                .unwrap();
            let _ = body;
        }
    });
    (
        MockServer {
            base_url: format!("http://{address}"),
            token: "a".repeat(32),
        },
        purposes,
        agent_posts,
        done,
        server,
    )
}

fn server_local_inventory() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "purposes": {
            "everyday_assistance": {
                "available": true,
                "requires_external_consent": false,
                "placement": "server_local"
            }
        }
    })
}

fn canonical_answer_script(text: &str) -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "output": [{"kind": "answer", "text": text}],
        "used_tokens": 7,
    })]
}

#[test]
fn canonical_root_turn_discovers_profiles_before_posting_to_transport() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, purposes, agent_posts, done, server) = observing_server(
        server_local_inventory(),
        canonical_answer_script("Canonical hello."),
        false,
    );
    let request_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Hello".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
            },
        )
        .unwrap();
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "canonical turn: {finished:?}");
    let session = finished.session.unwrap();
    assert_eq!(
        session.last_outcome,
        Some(floe_conversation::AgentOutcome::Completed)
    );
    assert!(session.messages.iter().any(|message| matches!(
        message,
        AgentMessage::Assistant { text, .. } if text == "Canonical hello."
    )));
    done.store(true, Ordering::Release);
    server.join().unwrap();
    // The legacy root path never fetched purposes: discovery-then-transport
    // proves the turn ran through InferenceService, not LegacyModelPort.
    assert!(purposes.load(Ordering::SeqCst) >= 1);
    assert_eq!(agent_posts.load(Ordering::SeqCst), 1);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
}

#[test]
fn canonical_root_explicit_unknown_profile_fails_without_agent_post() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, purposes, agent_posts, done, server) =
        observing_server(server_local_inventory(), vec![], false);
    let request_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Hello".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("no-such-profile".into()),
                        false,
                        None,
                    )),
                }),
            },
        )
        .unwrap();
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "explicit turn job: {finished:?}");
    // Explicit selection is exact: a missing profile never falls back to
    // another profile or transport.
    assert_eq!(
        finished.session.unwrap().last_outcome,
        Some(floe_conversation::AgentOutcome::Halted {
            reason: AgentFailure::ModelUnavailable
        })
    );
    done.store(true, Ordering::Release);
    server.join().unwrap();
    assert!(purposes.load(Ordering::SeqCst) >= 1);
    assert_eq!(agent_posts.load(Ordering::SeqCst), 0);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
}

#[test]
fn canonical_root_unconsented_external_recipient_blocks_with_card_without_agent_post() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let keys = Keys::default();
    let person = PersonId::new();
    let worker =
        Worker::new_with_connection_store(root.clone(), keys.clone(), connections.store()).unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let inventory = serde_json::json!({
        "schema_version": 1,
        "purposes": {
            "everyday_assistance": {
                "available": true,
                "requires_external_consent": true,
                "placement": "external",
                "recipient": "someone-else.example"
            }
        }
    });
    let (mock, purposes, agent_posts, done, server) = observing_server(inventory, vec![], false);
    let request_id = Uuid::new_v4();
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    worker
        .request(
            person,
            request_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Hello".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                        // No contextual consent covers the selected
                        // recipient, so the dispatch blocks into a
                        // reviewable card before any transport handoff.
                    )),
                }),
            },
        )
        .unwrap();
    let finished = wait(&worker, person, request_id);
    assert_eq!(finished.failure, None, "external turn job: {finished:?}");
    let session = finished.session.unwrap();
    // The missing eligible consent blocks the first dispatch into a
    // deterministic completed turn: the limitation text, no fabricated
    // model output, and exactly one interaction ref.
    assert_eq!(
        session.last_outcome,
        Some(floe_conversation::AgentOutcome::Completed),
        "session: {session:?}"
    );
    assert!(session.messages.iter().any(|message| matches!(
        message,
        AgentMessage::Assistant { text, .. }
            if text == floe_conversation::MODEL_CONSENT_LIMITATION
    )));
    let interactions: Vec<_> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Interaction { interaction_id, .. } => Some(*interaction_id),
            _ => None,
        })
        .collect();
    assert_eq!(interactions.len(), 1, "session: {session:?}");
    done.store(true, Ordering::Release);
    server.join().unwrap();
    assert!(purposes.load(Ordering::SeqCst) >= 1);
    assert_eq!(agent_posts.load(Ordering::SeqCst), 0);
    // The single ref resolves to a durable pending Model-origin card
    // naming the exact selected recipient under review.
    assert_eq!(perform(&worker, person, WorkerAction::Lock).failure, None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reopened = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    let repository = floe_vault::VaultConversationRepository::new(std::sync::Arc::new(reopened));
    let stored = runtime
        .block_on(floe_conversation::InteractionRepository::get_interaction(
            &repository,
            person,
            interactions[0],
        ))
        .unwrap()
        .expect("blocked dispatch must publish a durable interaction");
    assert_eq!(stored.state, floe_conversation::InteractionState::Pending);
    assert!(
        matches!(
            stored.origin,
            floe_conversation::InteractionOrigin::Model { .. }
        ),
        "blocked root dispatch publishes under the Model origin: {:?}",
        stored.origin
    );
    assert_eq!(stored.requirement.source_id, "someone-else.example");
    let floe_conversation::ReviewedTarget::RecipientConsent(target) = &stored.target else {
        panic!(
            "model blockage must review a recipient consent: {:?}",
            stored.target
        );
    };
    assert_eq!(target.recipient, "someone-else.example");
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
}

#[test]
fn delegated_model_dispatch_uses_the_same_owner_checks_as_root() {
    use std::sync::Mutex;

    use floe_agent_contract::ExpertModel;

    // The delegated path runs the same canonical InferenceService with the
    // same Access fence and the same vault-backed recipient authority as
    // the root path: an unconsented remote-only expert dispatch blocks on
    // the exact selected recipient before any agent post.
    let person = PersonId::new();
    let fixture = direct_endpoint_fixture_for(person);
    let inventory = serde_json::json!({
        "schema_version": 1,
        "purposes": {
            "everyday_assistance": {
                "available": true,
                "requires_external_consent": true,
                "placement": "external",
                "recipient": "expert-model.example"
            }
        }
    });
    let (mock, purposes, agent_posts, done, server) = observing_server(inventory, vec![], false);
    let connection = saved_server_connection(&mock, person, "mac-local");
    let store =
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(connection));
    let provider =
        floe_provider_adapters::models::RootModelProvider::from_current_connection_scoped(
            &store,
            &person.to_string(),
            "mac-local",
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
        )
        .unwrap();
    let admission = floe_provider_adapters::control::SavedConnectionAdmission::new(
        store,
        person.to_string(),
        "mac-local".into(),
    );
    let authority = floe_access::ContextualRecipientAuthority::new(
        std::sync::Arc::clone(&fixture.vault),
        admission,
        floe_access::SystemConsentClock,
    );
    let personal = crate::vault_host::personal_grants::PersonalDependencyResolver {
        vault: &fixture.vault,
        local_context: &fixture.local_context,
        person_id: person,
        device_id: "mac-local",
    };
    let service = floe_inference::InferenceService::new(provider, personal, authority);
    let ledger = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(50_000, 100_000),
        Default::default(),
    );
    let scope = floe_execution::ExecutionScope::root(
        floe_execution::Cancellation::new(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(10),
        ledger.work_lease(),
        floe_agent_contract::TraceContext::new(Uuid::new_v4()),
    );
    let captured = Mutex::new(Vec::new());
    let stash = Mutex::new(None);
    let host = crate::vault_host::conversation_turn::expert_host::ExpertModelHost {
        executor: &service,
        scope: &scope,
        captured: &captured,
        lineage: Some(
            floe_context_contract::RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4())
                .unwrap(),
        ),
        model_blocked: &stash,
    };
    let call = floe_agent_contract::ExpertModelCall {
        person_id: person,
        invocation_id: Uuid::new_v4(),
        prompt: floe_experts_builtin::prompts::focus_expert_prompt(),
        policy: floe_context::InferencePolicyDecision {
            purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE.into(),
            data_classes: vec![floe_agent_contract::DataClass::Personal],
            allowed_placements: vec![
                floe_agent_contract::ModelPlacement::DeviceLocal,
                floe_agent_contract::ModelPlacement::Remote,
            ],
            performance_class: "interactive".into(),
            projection_version: 1,
            external_transfer_consent: floe_agent_contract::TransferConsent::NotGranted,
            bounded_sensitive_projection: false,
        },
        context: floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        assignment: "Protect the current focus period.".into(),
        requirement: floe_agent_contract::ExpertModelRequirement::RemoteOnly,
        max_output_bytes: 8192,
        max_tokens: 4096,
        max_cost_micros: 1_000,
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(9),
        cancellation: floe_execution::Cancellation::default(),
    };
    let outcome = fixture.runtime.block_on(ExpertModel::answer(&host, call));
    done.store(true, Ordering::Release);
    server.join().unwrap();
    let floe_agent_contract::ExpertModelOutcome::Blocked(requirement) = outcome.unwrap() else {
        panic!("unconsented delegated dispatch must block");
    };
    assert_eq!(requirement.recipient(), "expert-model.example");
    assert_eq!(*stash.lock().unwrap(), Some(requirement));
    assert!(purposes.load(Ordering::SeqCst) >= 1);
    assert_eq!(agent_posts.load(Ordering::SeqCst), 0);
}

fn recording_answer_server(
    steps: Vec<WireStep>,
) -> (
    MockServer,
    Arc<Mutex<Vec<String>>>,
    std::thread::JoinHandle<()>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let server_paths = Arc::clone(&paths);
    let server = std::thread::spawn(move || {
        let mut steps = steps.into_iter().peekable();
        loop {
            if steps.peek().is_none() {
                return;
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
            let (headers, body) = loop {
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
                        .unwrap_or(0);
                    if body.len() >= length {
                        break (headers.to_string(), body.to_string());
                    }
                }
            };
            let path = headers
                .lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            server_paths.lock().unwrap().push(path.clone());
            if path == "/v1/inference-purposes" {
                let inventory = canonical_inventory_body();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            inventory.len(),
                            inventory
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                continue;
            }
            assert_eq!(path, "/v1/agent", "unexpected request: {headers}");
            let step = steps.next().expect("scripted steps peeked Some");
            let _ = body;
            let output = serde_json::json!({"output": [step], "used_tokens": 10});
            let response = serde_json::json!({
                "schema_version": 1, "purpose": "everyday_assistance", "trace_id": "a".repeat(32),
                "routing": { "placement": "server_local", "external_transfer": false, "replay_source": "a".repeat(64) },
                "output": output.to_string(),
            }).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).unwrap();
        }
    });
    (
        MockServer {
            base_url: format!("http://{address}"),
            token: "a".repeat(32),
        },
        paths,
        server,
    )
}

#[test]
fn plain_turn_contacts_only_post_admission_discovery_and_transport() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, paths, server) = recording_answer_server(vec![WireStep::Answer {
        text: "Hello.".into(),
    }]);
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let result = perform(
        &worker,
        person,
        WorkerAction::ConversationTurn {
            request: Box::new(ConversationTurnRequest::new(
                session.id,
                session.revision,
                "Hello".into(),
                "mac-local".into(),
                ProfileSelection::Explicit("server-model".into()),
                false,
                None,
            )),
        },
    );
    assert_eq!(result.failure, None, "result: {result:?}");
    server.join().unwrap();
    let paths = paths.lock().unwrap();
    // Canonical discovery runs (after admission), transport runs, and the
    // connector catalog is never observed for a plain turn.
    assert!(paths.iter().any(|path| path == "/v1/inference-purposes"));
    assert!(paths.iter().any(|path| path == "/v1/agent"));
    assert!(
        paths
            .iter()
            .all(|path| path == "/v1/inference-purposes" || path == "/v1/agent"),
        "paths: {paths:?}"
    );
}

#[test]
fn unavailable_saved_server_does_not_prevent_conversation_admission() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    // A loopback address with no listener: connecting fails, and the paired
    // server never answers.
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);
    let request_id = Uuid::new_v4();
    connections.replace(Some(floe_inference::SavedServerConnection {
        base_url,
        token: "d".repeat(32),
        client_id: "test-client".into(),
        person_id: person.to_string(),
        device_id: "mac-local".into(),
    }));
    let request = ConversationTurnRequest::new(
        session.id,
        session.revision,
        "Hello".into(),
        "mac-local".into(),
        ProfileSelection::Explicit("server-model".into()),
        false,
        None,
    );
    // Admission succeeds even though the saved server is unreachable: no
    // pre-turn discovery gates the Run.
    let admission = worker.start_conversation(
        person,
        floe_kernel::CommandId::from_uuid(request_id).unwrap(),
        request,
    );
    assert!(admission.is_ok(), "admission: {admission:?}");
    let finished = wait(&worker, person, request_id);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(finished.failure, None, "finished: {finished:?}");
    let session = finished.session.unwrap();
    // The dead server observes no profiles, so the explicit server profile
    // matches nothing after admission: a clean model-unavailable halt, not an
    // admission failure and not a hang.
    assert_eq!(
        session.last_outcome,
        Some(floe_conversation::AgentOutcome::Halted {
            reason: AgentFailure::ModelUnavailable,
        }),
        "session: {session:?}"
    );
}

#[test]
fn local_only_root_turn_starts_with_no_remote_connection() {
    // Hermetic absent credential through the injected fixed store: the turn
    // never reads the host keychain slot, so the result cannot depend on
    // ambient keychain state (a test binary the OS has not authorized for
    // the slot can block on an access prompt instead of reading it).
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let request_id = Uuid::new_v4();
    // No stored credential anywhere: the turn is admitted local-only.
    let admission = worker.start_conversation(
        person,
        floe_kernel::CommandId::from_uuid(request_id).unwrap(),
        ConversationTurnRequest::new(
            session.id,
            session.revision,
            "Hello".into(),
            "mac-local".into(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        ),
    );
    assert!(admission.is_ok(), "admission: {admission:?}");
    let finished = wait(&worker, person, request_id);
    worker
        .request(person, request_id, WorkerOperation::Release)
        .unwrap();
    assert_eq!(finished.failure, None, "finished: {finished:?}");
    let session = finished.session.unwrap();
    // Local-only observes no server profiles, so the explicit server profile
    // matches nothing after admission: a clean model-unavailable halt,
    // identical on every machine regardless of ambient keychain state.
    assert_eq!(
        session.last_outcome,
        Some(floe_conversation::AgentOutcome::Halted {
            reason: AgentFailure::ModelUnavailable,
        }),
        "session: {session:?}"
    );
}

struct DirectEndpointFixture {
    _directory: tempfile::TempDir,
    runtime: tokio::runtime::Runtime,
    core: Arc<FloeCore>,
    vault: Arc<EncryptedAgentVault<Keys>>,
    local_context: Arc<LocalContextHost>,
    person: PersonId,
}

fn direct_endpoint_fixture() -> DirectEndpointFixture {
    direct_endpoint_fixture_for(PersonId::new())
}

fn direct_endpoint_fixture_for(person: PersonId) -> DirectEndpointFixture {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let keys = Keys::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let core = Arc::new(runtime.block_on(FloeCore::open(":memory:")).unwrap());
    let vault = Arc::new(
        runtime
            .block_on(EncryptedAgentVault::create(&root, person, keys.clone()))
            .unwrap(),
    );
    DirectEndpointFixture {
        _directory: directory,
        runtime,
        core,
        vault,
        local_context: Arc::new(LocalContextHost::default()),
        person,
    }
}

#[test]
fn common_schedule_endpoint_completes_review_required_task_without_old_setup() {
    use floe_context_contract::{CalendarProvider, CalendarScope};
    use floe_day::CalendarSelection;
    use std::io::{Read, Write};

    let person = PersonId::new();
    let fixture = direct_endpoint_fixture_for(person);
    fixture.runtime.block_on(async {
        fixture
            .core
            .set_calendar_scope(
                person,
                Uuid::new_v4().to_string(),
                1,
                "mac-local".into(),
                CalendarProvider::Google,
                vec![CalendarSelection {
                    calendar_id: "home".into(),
                    calendar_name: "Home".into(),
                }],
                CalendarScope::Selected,
            )
            .await
            .unwrap();
        fixture
            .vault
            .install_expert_bundle(
                floe_experts::ExpertInstallOperation {
                    instance_id: fixture.vault.registry_instance_id(),
                    expected_revision: 0,
                    operation_id: Uuid::new_v4(),
                },
                &floe_experts_builtin::manifests(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
    });
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let inventory_server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let size = socket.read(&mut request).unwrap();
        assert!(
            String::from_utf8_lossy(&request[..size]).starts_with("GET /v1/inference-purposes")
        );
        let inventory = canonical_inventory_body();
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{inventory}",
                    inventory.len(),
                )
                .as_bytes(),
            )
            .unwrap();
    });
    let mut connection = fixture_saved_connection(person, "mac-local");
    connection.base_url = base_url;
    let endpoint = BuiltinExpertEndpoint::new(
        Arc::clone(&fixture.core),
        Arc::clone(&fixture.vault),
        Arc::clone(&fixture.local_context),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(connection)),
        direct_endpoint_admission(floe_experts_builtin::BuiltinExpertKind::Schedule.package_id()),
        direct_endpoint_card(floe_experts_builtin::BuiltinExpertKind::Schedule.package_id()),
    );
    // The direct invocation still needs the validated origin the common
    // endpoint publishes under: an admitted run with the DelegationIntent
    // this exact request journals.
    let run_id = floe_kernel::RunId::new();
    let repository =
        floe_vault::VaultConversationRepository::new(std::sync::Arc::clone(&fixture.vault));
    let session_id = fixture.runtime.block_on(async {
        fixture
            .vault
            .activate_conversation_executor()
            .await
            .unwrap();
        let started = floe_conversation::start_session(
            &repository,
            floe_conversation::SessionRequest {
                principal: person.to_string(),
            },
        )
        .await
        .unwrap();
        let command_id = floe_agent_contract::CommandId::new();
        floe_conversation::ConversationRepository::admit_turn(
            &repository,
            floe_conversation::TurnAdmissionRequest {
                run_id,
                command_id,
                session_id: started.session_id,
                expected_session_revision: 0,
                principal: person.to_string(),
                request_digest: [7; 32],
                mode: floe_conversation::TurnMode::New,
                retry_of: None,
                profile: floe_conversation::ProfileSelection::Auto,
                user_message: floe_agent_contract::AgentMessage {
                    message_id: command_id.as_uuid(),
                    role: floe_agent_contract::MessageRole::User,
                    text: "Review today".into(),
                    call_id: None,
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                },
            },
        )
        .await
        .unwrap();
        started.session_id
    });
    let (invocation, scope) = direct_invocation_in_session(
        &person.to_string(),
        run_id.as_uuid(),
        session_id,
        floe_experts_builtin::BuiltinExpertKind::Schedule.package_id(),
        "mac-local",
        "Review today",
    );
    fixture.runtime.block_on(async {
        floe_conversation::ConversationRepository::journal(&repository, run_id)
            .unwrap()
            .record_intent(floe_agent_contract::JournalEvent::DelegationIntent {
                request: invocation.request.clone(),
            })
            .await
            .unwrap();
    });
    let before = fixture
        .runtime
        .block_on(fixture.vault.expert_registry())
        .unwrap()
        .unwrap();
    let task_uuid = invocation.request.task_id.as_uuid();
    let report = fixture
        .runtime
        .block_on(floe_agent_contract::AgentEndpoint::execute(
            &endpoint, invocation, &scope,
        ));
    inventory_server.join().unwrap();
    let report = report.unwrap();
    assert!(
        report.result.contains("needs your review"),
        "{}",
        report.result
    );
    assert!(report.settlement.is_none());
    assert_eq!(report.artifacts.len(), 2);
    let data = report.artifacts
        .iter()
        .flat_map(|artifact| &artifact.parts)
        .find_map(|part| match part {
            floe_agent_contract::ArtifactPart::Data { media_type, data }
                if media_type == floe_agent_contract::USER_INTERACTION_MEDIA_TYPE =>
            {
                Some(data.clone())
            }
            _ => None,
        })
        .expect("blocked report must carry the safe ref, not a raw requirement");
    let reference: floe_agent_contract::UserInteractionRef = serde_json::from_str(&data).unwrap();
    assert_eq!(
        reference.kind,
        floe_agent_contract::UserInteractionKind::SourceAccess
    );
    let stored = fixture
        .runtime
        .block_on(floe_conversation::InteractionRepository::get_interaction(
            &repository,
            person,
            reference.interaction_id,
        ))
        .unwrap()
        .expect("blocked report must publish a durable interaction");
    assert_eq!(stored.state, floe_conversation::InteractionState::Pending);
    assert_eq!(
        stored.origin,
        floe_conversation::InteractionOrigin::Task {
            task_id: task_uuid,
            capability_call_id: None,
        }
    );
    let after = fixture
        .runtime
        .block_on(fixture.vault.expert_registry())
        .unwrap()
        .unwrap();
    assert_eq!(after.revision, before.revision);
    assert!(after.install_receipts.iter().any(|receipt| {
        receipt.person_id == person
            && receipt.installed.iter().any(|installed| {
                installed.package.id
                    == floe_experts_builtin::BuiltinExpertKind::Schedule.package_id()
            })
    }));
}

#[test]
fn common_schedule_review_requirement_completes_root_run() {
    use floe_context_contract::{CalendarProvider, CalendarScope};
    use floe_day::CalendarSelection;

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
    let device_id = format!("local-{}", std::env::consts::OS);
    runtime
        .block_on(core.set_calendar_scope(
            person,
            Uuid::new_v4().to_string(),
            1,
            device_id.clone(),
            CalendarProvider::Google,
            vec![CalendarSelection {
                calendar_id: "home".into(),
                calendar_name: "Home".into(),
            }],
            CalendarScope::Selected,
        ))
        .unwrap();
    let local = chrono::Local::now();
    runtime
        .block_on(core.import_calendar(
            person,
            1,
            floe_day::CalendarRange {
                start_date: local.date_naive(),
                end_date_exclusive: local.date_naive() + chrono::Duration::days(1),
                timezone_offset_seconds: local.offset().local_minus_utc(),
                end_timezone_offset_seconds: None,
            },
            vec![],
            chrono::Utc::now(),
        ))
        .unwrap();
    let (mock, server) = answer_server(vec![
        WireStep::Delegate {
            agent_id: floe_experts_builtin::BuiltinExpertKind::Schedule
                .package_id()
                .into(),
            message: "Review today".into(),
            context_refs: vec![],
        },
        WireStep::Answer {
            text: "Calendar access needs review before I can answer.".into(),
        },
    ]);
    let worker = Worker::with_core_and_connection_store(
        root.clone(),
        keys.clone(),
        core,
        Arc::new(LocalContextHost::default()),
        Arc::new(crate::events::AppEventBuffer::default()),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
            saved_server_connection(&mock, person, &device_id),
        )),
    )
    .unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    install_builtin_calendar_setup(&worker, &root, person, &keys);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let result = perform(
        &worker,
        person,
        WorkerAction::ConversationTurn {
            request: Box::new(ConversationTurnRequest::new(
                session.id,
                session.revision,
                "What is on my calendar?".into(),
                device_id,
                ProfileSelection::Explicit("server-model".into()),
                false,
                None,
            )),
        },
    );
    assert_eq!(result.failure, None, "result: {result:?}");
    let session = result.session.unwrap();
    assert_eq!(
        session.last_outcome,
        Some(floe_conversation::AgentOutcome::Completed),
        "session: {session:?}"
    );
    assert!(session.messages.iter().any(|message| matches!(
        message,
        AgentMessage::Delegation { task, .. }
            if task.agent_id == floe_experts_builtin::BuiltinExpertKind::Schedule.package_id()
                && task.state == floe_experts::A2ATaskState::Completed
                && task.artifacts.iter().any(|artifact| artifact.parts.iter().any(|part| matches!(
                    part,
                    floe_experts::A2APart::Data { media_type, .. }
                        if media_type == floe_agent_contract::USER_INTERACTION_MEDIA_TYPE
                )))
    )));
    assert!(
        session
            .messages
            .iter()
            .any(|message| matches!(message, AgentMessage::Interaction { .. })),
        "completed turn must carry the Interaction message: {session:?}"
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn direct_attention_tool_blocked_completes_turn_with_one_durable_ref() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let keys = Keys::default();
    let person = PersonId::new();
    // The Manager calls the direct tool first, then explains the blocked
    // state it observed: two model iterations, one durable interaction.
    let (mock, server) = answer_server(vec![
        WireStep::Call {
            capability_id: "attention.coarse.read".into(),
            input: "{}".into(),
        },
        WireStep::Answer {
            text: "Attention access needs your review before I can read it.".into(),
        },
    ]);
    let worker = Worker::new_with_connection_store(
        root.clone(),
        keys.clone(),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
            saved_server_connection(&mock, person, "mac-local"),
        )),
    )
    .unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let result = perform(
        &worker,
        person,
        WorkerAction::ConversationTurn {
            request: Box::new(ConversationTurnRequest::new(
                session.id,
                session.revision,
                "Am I focused right now?".into(),
                "mac-local".into(),
                ProfileSelection::Explicit("server-model".into()),
                false,
                None,
            )),
        },
    );
    assert_eq!(result.failure, None, "result: {result:?}");
    let session = result.session.unwrap();
    assert_eq!(
        session.last_outcome,
        Some(floe_conversation::AgentOutcome::Completed),
        "session: {session:?}"
    );
    let interactions: Vec<_> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Interaction { interaction_id, .. } => Some(*interaction_id),
            _ => None,
        })
        .collect();
    assert_eq!(interactions.len(), 1, "session: {session:?}");
    assert_eq!(server.join().unwrap().len(), 2);
    // The single ref resolves to a durable pending Tool-origin interaction.
    assert_eq!(perform(&worker, person, WorkerAction::Lock).failure, None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reopened = runtime
        .block_on(EncryptedAgentVault::open(&root, person, keys))
        .unwrap();
    let repository = floe_vault::VaultConversationRepository::new(std::sync::Arc::new(reopened));
    let stored = runtime
        .block_on(floe_conversation::InteractionRepository::get_interaction(
            &repository,
            person,
            interactions[0],
        ))
        .unwrap()
        .expect("blocked tool must publish a durable interaction");
    assert_eq!(stored.state, floe_conversation::InteractionState::Pending);
    assert!(
        matches!(
            stored.origin,
            floe_conversation::InteractionOrigin::Tool { .. }
        ),
        "direct blocker publishes under the Tool origin: {:?}",
        stored.origin
    );
}

fn direct_invocation(
    principal: &str,
    parent_run_id: Uuid,
    agent_id: &str,
    device_id: &str,
    message: &str,
) -> (
    floe_agent_contract::EndpointInvocation,
    floe_execution::ExecutionScope,
) {
    direct_invocation_in_session(
        principal,
        parent_run_id,
        Uuid::new_v4(),
        agent_id,
        device_id,
        message,
    )
}

fn direct_endpoint_admission(agent_id: &str) -> floe_experts::ExpertAdmissionIdentity {
    floe_experts::ExpertAdmissionIdentity {
        registry_instance_id: Uuid::new_v4(),
        assignment_id: Uuid::new_v4(),
        installation_id: Uuid::new_v4(),
        package: floe_agent_contract::PackageRef {
            kind: floe_agent_contract::PackageKind::Expert,
            id: agent_id.into(),
            version: "1.0.0".into(),
        },
        definition_revision: 1,
    }
}

fn direct_endpoint_card(agent_id: &str) -> floe_agent_contract::AgentCard {
    let declaration = floe_experts_builtin::BuiltinExpertKind::from_package_id(agent_id)
        .unwrap()
        .declaration();
    floe_agent_contract::AgentCard {
        schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
        protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
        id: agent_id.into(),
        version: declaration.version.into(),
        name: declaration.name.into(),
        description: declaration.description.into(),
        domain_tags: declaration.domain_tags,
        skills: declaration.skills,
        supported_placements: declaration.supported_placements,
    }
}

fn direct_invocation_in_session(
    principal: &str,
    parent_run_id: Uuid,
    session_id: Uuid,
    agent_id: &str,
    device_id: &str,
    message: &str,
) -> (
    floe_agent_contract::EndpointInvocation,
    floe_execution::ExecutionScope,
) {
    use floe_agent_contract::{
        DelegationExecutionContext, DelegationRequest, EndpointInvocation, InvocationKey, TaskId,
        delegation_request_digest,
    };
    let task_id = TaskId::new();
    let request = DelegationRequest {
        task_id,
        parent_run_id: Some(parent_run_id),
        principal: principal.into(),
        invocation_key: InvocationKey::new(),
        selected_agent_id: agent_id.into(),
        selected_definition_revision: 1,
        message: message.into(),
        context_refs: vec![],
        execution_context: DelegationExecutionContext {
            session_id,
            device_id: device_id.into(),
            agent_context: floe_agent_contract::AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            max_output_bytes: 16 * 1024,
        },
    };
    let request_digest = delegation_request_digest(&request);
    let ledger = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(50_000, 100_000),
        Default::default(),
    );
    let root = floe_execution::ExecutionScope::root(
        floe_execution::Cancellation::new(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        ledger.work_lease(),
        floe_agent_contract::TraceContext::new(Uuid::new_v4()),
    );
    let scope = root.child_scope(root.deadline(), 20_000, 50_000, Some(task_id));
    (
        EndpointInvocation {
            request,
            request_digest,
        },
        scope,
    )
}

fn fixture_saved_connection(
    person: PersonId,
    device: &str,
) -> floe_inference::SavedServerConnection {
    floe_inference::SavedServerConnection {
        base_url: "http://127.0.0.1:9".into(),
        token: "t".repeat(32),
        client_id: "test-client".into(),
        person_id: person.to_string(),
        device_id: device.into(),
    }
}

#[test]
fn builtin_endpoint_denies_forged_principal_without_touching_state() {
    // 2-C C4: a forged principal fails closed before any store read.
    let fixture = direct_endpoint_fixture();
    let endpoint = BuiltinExpertEndpoint::new(
        Arc::clone(&fixture.core),
        Arc::clone(&fixture.vault),
        Arc::clone(&fixture.local_context),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(None),
        direct_endpoint_admission(floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()),
        direct_endpoint_card(floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()),
    );
    let (invocation, scope) = direct_invocation(
        "person:foreign",
        Uuid::new_v4(),
        floe_experts_builtin::BuiltinExpertKind::Commitments.package_id(),
        "mac-local",
        "Review my commitments",
    );
    let result = fixture
        .runtime
        .block_on(floe_agent_contract::AgentEndpoint::execute(
            &endpoint, invocation, &scope,
        ));
    assert_eq!(result.err(), Some(AgentFailure::CapabilityDenied));
}

#[test]
fn builtin_endpoint_concurrent_tasks_under_one_run_are_not_run_gated() {
    // 2-C C4: with run-id staging gone, two Tasks under the same parent Run
    // reach admission independently; both fail on the forged device with the
    // same policy denial, never a staging Conflict.
    let fixture = direct_endpoint_fixture();
    let endpoint = BuiltinExpertEndpoint::new(
        Arc::clone(&fixture.core),
        Arc::clone(&fixture.vault),
        Arc::clone(&fixture.local_context),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
            fixture_saved_connection(fixture.person, "mac-local"),
        )),
        direct_endpoint_admission(floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()),
        direct_endpoint_card(floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()),
    );
    let parent_run_id = Uuid::new_v4();
    let agent_id = floe_experts_builtin::BuiltinExpertKind::Commitments.package_id();
    let (first, first_scope) = direct_invocation(
        &fixture.person.to_string(),
        parent_run_id,
        agent_id,
        "foreign-device",
        "Review my commitments",
    );
    let (second, second_scope) = direct_invocation(
        &fixture.person.to_string(),
        parent_run_id,
        agent_id,
        "foreign-device",
        "Review my commitments",
    );
    assert_ne!(
        first.request.task_id, second.request.task_id,
        "two distinct Tasks share one parent Run"
    );
    let (first_result, second_result) = fixture.runtime.block_on(async {
        tokio::join!(
            floe_agent_contract::AgentEndpoint::execute(&endpoint, first, &first_scope),
            floe_agent_contract::AgentEndpoint::execute(&endpoint, second, &second_scope),
        )
    });
    assert_eq!(first_result.err(), Some(AgentFailure::PolicyDenied));
    assert_eq!(second_result.err(), Some(AgentFailure::PolicyDenied));
}

#[test]
fn builtin_endpoint_offers_only_observed_execution_classes() {
    // 3-A1: the delegated endpoint composes canonical Inference from the
    // stored credential and offers Experts from observed non-secret facts.
    // The paired server is down, so only the device class is observed and
    // the Remote-only Commitments Expert is denied at admission — before any
    // source read or model call. Mail is installed and granted, so an offered
    // Commitments run would fail on the dead-server read with a transport
    // failure, never CapabilityDenied.
    let fixture = direct_endpoint_fixture();
    fixture.runtime.block_on(async {
        let setup = floe_experts::ExpertInstallOperation {
            instance_id: fixture.vault.registry_instance_id(),
            expected_revision: 0,
            operation_id: Uuid::new_v4(),
        };
        fixture
            .vault
            .install_expert_bundle(
                setup,
                &floe_experts_builtin::manifests(),
                floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
    });
    let endpoint = BuiltinExpertEndpoint::new(
        Arc::clone(&fixture.core),
        Arc::clone(&fixture.vault),
        Arc::clone(&fixture.local_context),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
            fixture_saved_connection(fixture.person, "mac-local"),
        )),
        direct_endpoint_admission(floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()),
        direct_endpoint_card(floe_experts_builtin::BuiltinExpertKind::Commitments.package_id()),
    );
    let (invocation, scope) = direct_invocation(
        &fixture.person.to_string(),
        Uuid::new_v4(),
        floe_experts_builtin::BuiltinExpertKind::Commitments.package_id(),
        "mac-local",
        "Review my commitments",
    );
    let result = fixture
        .runtime
        .block_on(floe_agent_contract::AgentEndpoint::execute(
            &endpoint, invocation, &scope,
        ));
    assert_eq!(result.err(), Some(AgentFailure::CapabilityDenied));
}

// ---- linked resume (05-E) ----

use super::super::conversation_turn::{ResumeTurnRequest, evaluate_auto_resume, run, run_resume};
use super::super::interaction_owners::HostInteractionOwners;
use crate::vault_host::interaction_resolution::{
    ResolveInteractionCommand, ResolveOutcome, resolve_interaction,
};

#[allow(clippy::too_many_arguments)]
async fn drive_evaluated_resume(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextHost,
    task_coordinator: &floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    conversation_repository: &Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &Arc<floe_conversation::RunCancellationRegistry>,
    connections: &floe_provider_adapters::control::CurrentSavedConnectionStore,
    person_id: PersonId,
    session_id: Uuid,
    origin_run_id: floe_kernel::RunId,
    device_id: &str,
    cancellation: floe_execution::Cancellation,
    emit: impl FnMut(floe_conversation::AgentEvent) + Send,
) -> Result<Option<floe_conversation::RunReceipt>, AgentFailure> {
    let Some(claim) = evaluate_auto_resume(
        conversation_repository,
        person_id,
        session_id,
        origin_run_id,
    )
    .await?
    else {
        return Ok(None);
    };
    let mut child = None;
    let result = run_resume(
        core,
        vault,
        local_context,
        task_coordinator,
        conversation_repository,
        run_cancellations,
        connections,
        person_id,
        floe_conversation::resume_command_id(origin_run_id)?,
        &ResumeTurnRequest {
            session_id,
            expected_revision: claim.expected_revision,
            device_id: device_id.to_owned(),
            resume: claim.link,
        },
        cancellation,
        |receipt: &floe_conversation::RunReceipt| child = Some(receipt.clone()),
        emit,
    )
    .await;
    match result {
        Ok(_) => child.map(Some).ok_or(AgentFailure::StorageUnavailable),
        Err(AgentFailure::Conflict) => Ok(None),
        Err(failure) => Err(failure),
    }
}

struct StubCalendarSubject;

impl floe_context::NativeCalendarSubjectSource for StubCalendarSubject {
    fn subject(
        &self,
        _request: floe_context::NativeSubjectRequest,
    ) -> impl Future<Output = Result<floe_context::NativeSubjectObservation, AgentFailure>> + Send
    {
        async { panic!("recipient-consent resolution never probes calendar subjects") }
    }
}

struct StubPersonalInspector;

impl floe_access::PersonalSubjectInspector for StubPersonalInspector {
    fn inspect<'a>(
        &'a self,
        _person_id: PersonId,
        _device_id: &'a str,
        _probe: floe_access::PersonalSubjectProbe<'a>,
        _expected_native_subject_fingerprint: Option<String>,
        _deadline: Option<tokio::time::Instant>,
        _cancellation: floe_execution::Cancellation,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_access::PersonalSubjectEvidence, AgentFailure>,
    > {
        Box::pin(async { panic!("recipient-consent resolution never probes personal subjects") })
    }

    fn attention_presence(&self, _person_id: PersonId, _device_id: &str) -> Option<Uuid> {
        None
    }
}

struct ResumeHarness {
    person: PersonId,
    device: String,
    core: Arc<crate::FloeCore>,
    vault: Arc<floe_vault::EncryptedAgentVault<Keys>>,
    repository: Arc<floe_vault::VaultConversationRepository<Keys>>,
    coordinator: floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    cancellations: Arc<floe_conversation::RunCancellationRegistry>,
    local: Arc<crate::local_context::LocalContextHost>,
    connections: floe_provider_adapters::control::CurrentSavedConnectionStore,
    session_id: Uuid,
    agent_posts: Arc<std::sync::atomic::AtomicUsize>,
    done: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
    caller: crate::CallerContext,
    calendar: StubCalendarSubject,
    personal: StubPersonalInspector,
    _root: tempfile::TempDir,
}

fn consent_inventory() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "purposes": {
            "everyday_assistance": {
                "available": true,
                "requires_external_consent": true,
                "placement": "external",
                "recipient": "someone-else.example"
            }
        }
    })
}

impl ResumeHarness {
    async fn open(agent_script: Vec<serde_json::Value>) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person = PersonId::new();
        let device = "mac-local".to_string();
        let core = Arc::new(
            crate::FloeCore::open(root.path().join("core.db"))
                .await
                .unwrap(),
        );
        let vault = Arc::new(
            floe_vault::EncryptedAgentVault::create(root.path(), person, Keys::default())
                .await
                .unwrap(),
        );
        let session = vault.create_session().await.unwrap();
        vault.activate_conversation_executor().await.unwrap();
        let repository = Arc::new(floe_vault::VaultConversationRepository::new(Arc::clone(
            &vault,
        )));
        let tasks = Arc::new(floe_vault::VaultTaskRepository::new(Arc::clone(&vault)));
        let (coordinator, _recovered) = floe_experts::TaskCoordinator::activate(
            floe_experts::Directory::default(),
            tasks,
            "everyday-assistance",
            floe_agent_contract::MAX_OUTPUT_BYTES,
        )
        .await
        .unwrap();
        let (mock, _purposes, agent_posts, done, server) =
            observing_server(consent_inventory(), agent_script, true);
        let connections = floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(
            Some(saved_server_connection(&mock, person, &device)),
        );
        let caller = crate::CallerContext::verified(
            crate::LocalIdentityClaim {
                person_id: person.0,
                device_id: device.clone(),
            },
            1,
        )
        .unwrap();
        Self {
            person,
            device,
            core,
            vault,
            repository,
            coordinator,
            cancellations: Arc::new(floe_conversation::RunCancellationRegistry::default()),
            local: Arc::new(crate::local_context::LocalContextHost::default()),
            connections,
            session_id: session.id,
            agent_posts,
            done,
            server: Some(server),
            caller,
            calendar: StubCalendarSubject,
            personal: StubPersonalInspector,
            _root: root,
        }
    }

    async fn drive_origin(&self, text: &str, revision: u64) -> floe_conversation::RunReceipt {
        let mut admitted = None;
        let request = ConversationTurnRequest::new(
            self.session_id,
            revision,
            text.into(),
            self.device.clone(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        );
        run(
            &self.core,
            &self.vault,
            &self.local,
            &self.coordinator,
            &self.repository,
            &self.cancellations,
            &self.connections,
            self.person,
            floe_agent_contract::CommandId::new(),
            &request,
            floe_execution::Cancellation::default(),
            |receipt: &floe_conversation::RunReceipt| {
                admitted = Some(receipt.clone());
            },
            |_| {},
        )
        .await
        .unwrap();
        let admitted = admitted.expect("origin turn admits");
        // on_admitted fires at admission (Working); reload the terminal
        // receipt the drive settled.
        floe_conversation::ConversationRepository::load_receipt(
            self.repository.as_ref(),
            admitted.run_id,
        )
        .await
        .unwrap()
        .unwrap()
    }

    fn owners(
        &self,
    ) -> HostInteractionOwners<'_, Keys, StubCalendarSubject, StubPersonalInspector> {
        HostInteractionOwners {
            core: &self.core,
            vault: &self.vault,
            connections: &self.connections,
            calendar_subject: &self.calendar,
            personal_subject: &self.personal,
            probe_deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        }
    }

    async fn allow(
        &self,
        interaction_id: Uuid,
        expected_revision: u64,
        target_digest: [u8; 32],
    ) -> floe_conversation::ConversationInteraction {
        let owners = self.owners();
        let outcome = resolve_interaction(
            self.repository.as_ref(),
            self.repository.as_ref(),
            &owners,
            &owners,
            &owners,
            &self.caller,
            ResolveInteractionCommand {
                interaction_id,
                command_id: Uuid::new_v4(),
                session_id: self.session_id,
                expected_revision,
                kind: floe_conversation::InteractionDecisionKind::Approve,
                target_digest,
            },
            &floe_execution::Cancellation::default(),
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .unwrap();
        let ResolveOutcome::Resolved { interaction } = outcome else {
            panic!("consent allow resolves synchronously: {outcome:?}");
        };
        interaction
    }

    async fn finish(mut self) {
        self.done.store(true, Ordering::Release);
        self.server.take().unwrap().join().unwrap();
    }
}

fn pending_consent_card(session: &floe_conversation::AgentSession) -> Uuid {
    let cards: Vec<Uuid> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Interaction { interaction_id, .. } => Some(*interaction_id),
            _ => None,
        })
        .collect();
    assert_eq!(cards.len(), 1, "one blocked card: {session:?}");
    cards[0]
}

#[tokio::test]
async fn allow_resolves_and_auto_child_runs_authorized_under_origin_lineage() {
    let harness = ResumeHarness::open(canonical_answer_script("The model resumed.")).await;
    let origin = harness.drive_origin("Hello", 0).await;
    assert_eq!(origin.state, floe_conversation::RunState::Completed);
    assert_eq!(
        harness.agent_posts.load(Ordering::SeqCst),
        0,
        "blocked origin never reaches transport"
    );
    let stored_origin = harness
        .vault
        .conversation_run(origin.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_origin.state,
        floe_vault::VaultConversationRunState::Completed
    );

    let session = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    assert!(session.messages.iter().any(|message| matches!(
        message,
        AgentMessage::Assistant { text, .. }
            if text == floe_conversation::MODEL_CONSENT_LIMITATION
    )));
    let card_id = pending_consent_card(&session);
    let card = floe_conversation::InteractionRepository::get_interaction(
        harness.repository.as_ref(),
        harness.person,
        card_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(card.state, floe_conversation::InteractionState::Pending);

    let resolved = harness
        .allow(card.id, card.revision, card.target_digest)
        .await;
    assert!(matches!(
        resolved.state,
        floe_conversation::InteractionState::Resolved { .. }
    ));
    assert!(matches!(
        resolved.target,
        floe_conversation::ReviewedTarget::RecipientConsent(_)
    ));

    let outcome = drive_evaluated_resume(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &harness.connections,
        harness.person,
        harness.session_id,
        origin.run_id,
        &harness.device,
        floe_execution::Cancellation::default(),
        |_| {},
    )
    .await
    .unwrap();
    let Some(child) = outcome else {
        panic!("resolved group admits its automatic child");
    };
    assert_eq!(child.resume_of, Some(origin.run_id));
    assert_eq!(child.resume_lineage, 1);
    let child = floe_conversation::ConversationRepository::load_receipt(
        harness.repository.as_ref(),
        child.run_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(child.state, floe_conversation::RunState::Completed);
    assert_eq!(child.output.as_deref(), Some("The model resumed."));
    // The grant admitted under the origin lineage authorizes the child
    // dispatch: lineage continuity is what reaches transport here.
    assert_eq!(harness.agent_posts.load(Ordering::SeqCst), 1);

    // Exactly one User message in the whole Session: the origin's own.
    let session = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    let users: Vec<_> = session
        .messages
        .iter()
        .filter(|message| matches!(message, AgentMessage::User { .. }))
        .collect();
    assert_eq!(users.len(), 1);
    let AgentMessage::User { text, .. } = users[0] else {
        unreachable!()
    };
    assert_eq!(text, "Hello");
    harness.finish().await;
}

#[tokio::test]
async fn deny_all_suppresses_automatic_child_without_dispatch() {
    let harness = ResumeHarness::open(canonical_answer_script("unused")).await;
    let origin = harness.drive_origin("Hello", 0).await;
    let session = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    let card_id = pending_consent_card(&session);
    let card = floe_conversation::InteractionRepository::get_interaction(
        harness.repository.as_ref(),
        harness.person,
        card_id,
    )
    .await
    .unwrap()
    .unwrap();
    let owners = harness.owners();
    let outcome = resolve_interaction(
        harness.repository.as_ref(),
        harness.repository.as_ref(),
        &owners,
        &owners,
        &owners,
        &harness.caller,
        ResolveInteractionCommand {
            interaction_id: card.id,
            command_id: Uuid::new_v4(),
            session_id: harness.session_id,
            expected_revision: card.revision,
            kind: floe_conversation::InteractionDecisionKind::Deny,
            target_digest: card.target_digest,
        },
        &floe_execution::Cancellation::default(),
        chrono::Utc::now().timestamp_millis(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, ResolveOutcome::Denied { .. }));

    let outcome = drive_evaluated_resume(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &harness.connections,
        harness.person,
        harness.session_id,
        origin.run_id,
        &harness.device,
        floe_execution::Cancellation::default(),
        |_| {},
    )
    .await
    .unwrap();
    assert!(outcome.is_none());
    assert_eq!(harness.agent_posts.load(Ordering::SeqCst), 0);
    harness.finish().await;
}

#[tokio::test]
async fn newer_turn_suppresses_auto_but_explicit_continue_claims() {
    let harness = ResumeHarness::open(canonical_answer_script("Resumed answer.")).await;
    let origin = harness.drive_origin("Hello", 0).await;
    let session = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    let card_id = pending_consent_card(&session);
    let card = floe_conversation::InteractionRepository::get_interaction(
        harness.repository.as_ref(),
        harness.person,
        card_id,
    )
    .await
    .unwrap()
    .unwrap();
    harness
        .allow(card.id, card.revision, card.target_digest)
        .await;

    // A newer user turn supersedes the automatic revision. The grant is
    // origin-lineage-scoped, so the newer turn blocks under its own
    // lineage with its own honest card: consent never leaks across runs.
    let mut admitted = None;
    run(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &harness.connections,
        harness.person,
        floe_agent_contract::CommandId::new(),
        &ConversationTurnRequest::new(
            harness.session_id,
            origin.session_revision,
            "Newer question".into(),
            harness.device.clone(),
            ProfileSelection::Explicit("server-model".into()),
            false,
            None,
        ),
        floe_execution::Cancellation::default(),
        |receipt: &floe_conversation::RunReceipt| {
            admitted = Some(receipt.clone());
        },
        |_| {},
    )
    .await
    .unwrap();
    let newer = admitted.unwrap();
    let newer = floe_conversation::ConversationRepository::load_receipt(
        harness.repository.as_ref(),
        newer.run_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(newer.state, floe_conversation::RunState::Completed);
    assert_eq!(
        newer.output.as_deref(),
        Some(floe_conversation::MODEL_CONSENT_LIMITATION)
    );
    assert_eq!(harness.agent_posts.load(Ordering::SeqCst), 0);

    let outcome = drive_evaluated_resume(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &harness.connections,
        harness.person,
        harness.session_id,
        origin.run_id,
        &harness.device,
        floe_execution::Cancellation::default(),
        |_| {},
    )
    .await
    .unwrap();
    assert!(outcome.is_none());

    // The explicit Continue claims the same origin slot at the current
    // revision and runs authorized.
    let current = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    let link = origin.resume().unwrap();
    let mut child = None;
    run_resume(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &harness.connections,
        harness.person,
        floe_agent_contract::CommandId::new(),
        &ResumeTurnRequest {
            session_id: harness.session_id,
            expected_revision: current.revision,
            device_id: harness.device.clone(),
            resume: link,
        },
        floe_execution::Cancellation::default(),
        |receipt: &floe_conversation::RunReceipt| {
            child = Some(receipt.clone());
        },
        |_| {},
    )
    .await
    .unwrap();
    let child = child.unwrap();
    assert_eq!(child.resume_of, Some(origin.run_id));
    let child = floe_conversation::ConversationRepository::load_receipt(
        harness.repository.as_ref(),
        child.run_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(child.output.as_deref(), Some("Resumed answer."));
    harness.finish().await;
}

#[tokio::test]
async fn revoked_consent_blocks_child_fresh_without_stale_release() {
    let harness = ResumeHarness::open(canonical_answer_script("must not leak")).await;
    let origin = harness.drive_origin("Hello", 0).await;
    let session = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    let card_id = pending_consent_card(&session);
    let card = floe_conversation::InteractionRepository::get_interaction(
        harness.repository.as_ref(),
        harness.person,
        card_id,
    )
    .await
    .unwrap()
    .unwrap();
    let resolved = harness
        .allow(card.id, card.revision, card.target_digest)
        .await;
    let floe_conversation::ReviewedTarget::RecipientConsent(target) = &resolved.target else {
        panic!("model blockage reviews a recipient consent");
    };
    // Revoke the grant after resolution but before the child dispatches.
    let consent_id = floe_access::recipient_consent_id(
        harness.person,
        &harness.device,
        "test-client",
        &target.recipient,
        &target.profile_id,
        &target.purpose,
        &target.consumer,
        &target.input_data_classes,
        &target.source_scopes,
        target.lineage,
    );
    harness
        .vault
        .revoke_recipient_consent_record(consent_id)
        .await
        .unwrap();

    let outcome = drive_evaluated_resume(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &harness.connections,
        harness.person,
        harness.session_id,
        origin.run_id,
        &harness.device,
        floe_execution::Cancellation::default(),
        |_| {},
    )
    .await
    .unwrap();
    let Some(child) = outcome else {
        panic!("revocation does not suppress admission; it denies dispatch");
    };
    // The child re-checks live authority: blocked again, honestly, with a
    // fresh card under its own origin and nothing reaching transport.
    let child = floe_conversation::ConversationRepository::load_receipt(
        harness.repository.as_ref(),
        child.run_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(child.state, floe_conversation::RunState::Completed);
    assert_eq!(
        child.output.as_deref(),
        Some(floe_conversation::MODEL_CONSENT_LIMITATION)
    );
    assert_eq!(harness.agent_posts.load(Ordering::SeqCst), 0);
    let fresh_cards = floe_conversation::InteractionRepository::list_run_interactions(
        harness.repository.as_ref(),
        harness.person,
        child.run_id,
    )
    .await
    .unwrap();
    assert_eq!(fresh_cards.len(), 1);
    assert_eq!(
        fresh_cards[0].state,
        floe_conversation::InteractionState::Pending
    );
    harness.finish().await;
}

#[tokio::test]
async fn pairing_removed_after_allow_fails_child_fresh_without_dispatch() {
    let harness = ResumeHarness::open(canonical_answer_script("must not leak")).await;
    let origin = harness.drive_origin("Hello", 0).await;
    let session = harness
        .vault
        .load(harness.person, harness.session_id)
        .await
        .unwrap();
    let card_id = pending_consent_card(&session);
    let card = floe_conversation::InteractionRepository::get_interaction(
        harness.repository.as_ref(),
        harness.person,
        card_id,
    )
    .await
    .unwrap()
    .unwrap();
    harness
        .allow(card.id, card.revision, card.target_digest)
        .await;

    // The server pairing is removed after resolution: the child cannot
    // even prepare its route and fails fresh — the recorded grant never
    // releases a dispatch without its pairing.
    let removed = floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(None);
    let outcome = drive_evaluated_resume(
        &harness.core,
        &harness.vault,
        &harness.local,
        &harness.coordinator,
        &harness.repository,
        &harness.cancellations,
        &removed,
        harness.person,
        harness.session_id,
        origin.run_id,
        &harness.device,
        floe_execution::Cancellation::default(),
        |_| {},
    )
    .await
    .unwrap();
    let Some(child) = outcome else {
        panic!("pairing loss does not suppress admission; it denies dispatch");
    };
    let child = floe_conversation::ConversationRepository::load_receipt(
        harness.repository.as_ref(),
        child.run_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(child.state, floe_conversation::RunState::Failed);
    assert_eq!(
        child.issue,
        Some(floe_agent_contract::AgentFailure::ModelUnavailable)
    );
    assert_eq!(harness.agent_posts.load(Ordering::SeqCst), 0);
    harness.finish().await;
}

// ---- worker-driven interaction commands (05-F) ----

#[test]
fn worker_resolve_drives_auto_child_and_rejoins_retry() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, _purposes, agent_posts, done, server) = observing_server(
        consent_inventory(),
        canonical_answer_script("Resumed."),
        true,
    );
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let turn_id = Uuid::new_v4();
    worker
        .request(
            person,
            turn_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Hello".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
            },
        )
        .unwrap();
    let finished = wait(&worker, person, turn_id);
    assert_eq!(finished.failure, None, "blocked turn job: {finished:?}");
    worker
        .request(person, turn_id, WorkerOperation::Release)
        .unwrap();
    let session = finished.session.unwrap();
    assert_eq!(agent_posts.load(Ordering::SeqCst), 0);
    let card_id = pending_consent_card(&session);
    let origin_run_id = session
        .messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::User { turn_id, .. } => Some(*turn_id),
            _ => None,
        })
        .unwrap();

    let caller = crate::CallerContext::verified(
        crate::LocalIdentityClaim {
            person_id: person.0,
            device_id: "mac-local".into(),
        },
        1,
    )
    .unwrap();
    let card = worker
        .get_interaction(person, card_id)
        .unwrap()
        .expect("pending card reads back");
    assert_eq!(card.state, floe_conversation::InteractionState::Pending);

    // Allow through the worker: resolution plus the automatic child in
    // one response.
    let resolve = crate::ResolveInteraction {
        command_id: Uuid::new_v4(),
        interaction_id: card.id,
        session_id: session.id,
        expected_revision: card.revision,
        decision: crate::InteractionDecision::Approve,
        target_digest: card.target_digest,
    };
    let resolved = worker
        .resolve_interaction(&caller, resolve.clone())
        .unwrap();
    let crate::vault_host::ResolveOutcome::Resolved { interaction } = &resolved.outcome else {
        panic!("allow resolves: {:?}", resolved.outcome);
    };
    assert!(matches!(
        interaction.state,
        floe_conversation::InteractionState::Resolved { .. }
    ));
    let linked = resolved.linked_run.clone().expect("auto child admitted");
    assert_eq!(
        linked.resume_of,
        Some(floe_kernel::RunId::from_uuid(origin_run_id).unwrap())
    );

    // The child drives to completion on the worker; the grant it carries
    // reaches transport exactly once.
    let child_job = floe_conversation::resume_command_id(linked.resume_of.unwrap())
        .unwrap()
        .as_uuid();
    let driven = wait(&worker, person, child_job);
    assert_eq!(driven.failure, None, "child job: {driven:?}");
    worker
        .request(person, child_job, WorkerOperation::Release)
        .unwrap();
    assert_eq!(agent_posts.load(Ordering::SeqCst), 1);

    // A retried resolve (lost response) rejoins the same decision and
    // the same linked child; it never admits a sibling.
    let rejoined = worker.resolve_interaction(&caller, resolve).unwrap();
    assert!(matches!(
        rejoined.outcome,
        crate::vault_host::ResolveOutcome::Resolved { .. }
    ));
    assert_eq!(
        rejoined.linked_run.as_ref().map(|receipt| receipt.run_id),
        Some(linked.run_id)
    );
    assert_eq!(agent_posts.load(Ordering::SeqCst), 1);

    let listed = worker.list_interactions(person, session.id).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(matches!(
        listed[0].state,
        floe_conversation::InteractionState::Resolved { .. }
    ));
    done.store(true, Ordering::Release);
    server.join().unwrap();
}

#[test]
fn worker_explicit_resume_claims_slot_at_current_revision() {
    let connections = TestConnections::default();
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    perform(&worker, person, WorkerAction::Create);
    let session = perform(
        &worker,
        person,
        WorkerAction::ConversationSession {
            operation: ConversationSessionOperation::Start,
        },
    )
    .session
    .unwrap();
    let (mock, _purposes, agent_posts, done, server) = observing_server(
        consent_inventory(),
        canonical_answer_script("Resumed."),
        true,
    );
    connections.replace(Some(saved_server_connection(&mock, person, "mac-local")));
    let turn_id = Uuid::new_v4();
    worker
        .request(
            person,
            turn_id,
            WorkerOperation::Submit {
                action: Box::new(WorkerAction::ConversationTurn {
                    request: Box::new(ConversationTurnRequest::new(
                        session.id,
                        session.revision,
                        "Hello".into(),
                        "mac-local".into(),
                        ProfileSelection::Explicit("server-model".into()),
                        false,
                        None,
                    )),
                }),
            },
        )
        .unwrap();
    let finished = wait(&worker, person, turn_id);
    assert_eq!(finished.failure, None, "blocked turn job: {finished:?}");
    worker
        .request(person, turn_id, WorkerOperation::Release)
        .unwrap();
    let session = finished.session.unwrap();
    let card_id = pending_consent_card(&session);
    let caller = crate::CallerContext::verified(
        crate::LocalIdentityClaim {
            person_id: person.0,
            device_id: "mac-local".into(),
        },
        1,
    )
    .unwrap();
    let card = worker.get_interaction(person, card_id).unwrap().unwrap();

    // Deny the card: no automatic child, and an explicit Continue still
    // conflicts because nothing resolved.
    let denied = worker
        .resolve_interaction(
            &caller,
            crate::ResolveInteraction {
                command_id: Uuid::new_v4(),
                interaction_id: card.id,
                session_id: session.id,
                expected_revision: card.revision,
                decision: crate::InteractionDecision::Deny,
                target_digest: card.target_digest,
            },
        )
        .unwrap();
    assert!(matches!(
        denied.outcome,
        crate::vault_host::ResolveOutcome::Denied { .. }
    ));
    assert!(denied.linked_run.is_none());
    let origin_run_id = card.origin_run_id.as_uuid();
    assert_eq!(
        worker.resume_interaction(
            &caller,
            crate::ResumeInteraction {
                command_id: Uuid::new_v4(),
                session_id: session.id,
                origin_run_id,
                expected_revision: session.revision,
            },
        ),
        Err(floe_agent_contract::AgentFailure::Conflict)
    );
    assert_eq!(agent_posts.load(Ordering::SeqCst), 0);
    done.store(true, Ordering::Release);
    server.join().unwrap();
}
