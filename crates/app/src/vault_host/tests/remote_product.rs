use super::*;
use crate::{PairingTarget, RemoteAccessCommand, RemotePairingCommand};

#[test]
fn remote_job_results_are_caller_epoch_domain_bound_and_not_legacy_jobs() {
    let directory = tempfile::tempdir().unwrap();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let person = PersonId::new();
    let caller = remote_caller(person, "verified-device");
    let id = Uuid::new_v4();
    let action = || WorkerAction::RemotePairing {
        caller: caller.clone(),
        command: RemotePairingCommand::Prepare,
    };
    worker
        .remote_request(&caller, id, Some(action()), true, false)
        .unwrap();
    wait(&worker, person, id);
    assert_eq!(
        worker
            .remote_request(&caller, id, Some(action()), true, false)
            .unwrap()
            .request_id,
        id
    );
    for operation in [
        WorkerOperation::Poll { after_sequence: 0 },
        WorkerOperation::Stop,
        WorkerOperation::Release,
    ] {
        assert_eq!(
            worker.request(person, id, operation).unwrap_err(),
            AgentFailure::PolicyDenied
        );
    }
    for foreign in [
        remote_caller(PersonId::new(), "verified-device"),
        remote_caller(person, "foreign-device"),
        crate::CallerContext::verified(
            crate::LocalIdentityClaim {
                person_id: person.0,
                device_id: "verified-device".into(),
            },
            2,
        )
        .unwrap(),
    ] {
        assert_eq!(
            worker
                .remote_request(&foreign, id, None, true, false)
                .unwrap_err(),
            AgentFailure::NotFound
        );
    }
    assert_eq!(
        worker
            .remote_request(&caller, id, None, false, false)
            .unwrap_err(),
        AgentFailure::NotFound
    );
    let changed = WorkerAction::RemotePairing {
        caller: caller.clone(),
        command: RemotePairingCommand::Status {
            target: PairingTarget {
                base_url: "http://localhost:8431".into(),
            },
            pairing_id: Uuid::new_v4().to_string(),
            polling_proof: "proof".into(),
        },
    };
    assert_eq!(
        worker
            .remote_request(&caller, id, Some(changed), true, false)
            .unwrap_err(),
        AgentFailure::Conflict
    );
    worker
        .remote_request(&caller, id, None, true, true)
        .unwrap();
    assert_eq!(
        worker
            .remote_request(&caller, id, None, true, false)
            .unwrap_err(),
        AgentFailure::NotFound
    );
}

#[test]
fn all_remote_access_operations_reload_and_reject_foreign_or_missing_saved_identity() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let connections = TestConnections::default();
    let worker = Worker::new_with_connection_store(
        directory.path().join("vaults"),
        Keys::default(),
        connections.store(),
    )
    .unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    let producer = floe_access::RemoteProducerIdentity {
        schema_version: 1,
        instance_id: Uuid::new_v4().to_string(),
        execution_owner: Uuid::new_v4().to_string(),
        audience: "test".into(),
        key_id: Uuid::new_v4().to_string(),
        public_key: "test".into(),
        fingerprint: "test".into(),
    };
    let commands = [
        RemoteAccessCommand::InspectProducer,
        RemoteAccessCommand::ReviewAndEnroll {
            producer: Box::new(producer),
        },
        RemoteAccessCommand::EnrollmentStatus {
            enrollment_id: Uuid::new_v4().to_string(),
        },
        RemoteAccessCommand::ConnectionObserve {
            connector_id: "gmail".into(),
            connection_id: Uuid::new_v4().to_string(),
            resource: None,
            enabled: None,
            disconnecting: false,
        },
    ];
    for saved in [
        Some(saved_remote_connection(
            PersonId::new(),
            "verified-device",
            "http://not-loopback.invalid".into(),
        )),
        Some(saved_remote_connection(
            person,
            "foreign-device",
            "http://not-loopback.invalid".into(),
        )),
        None,
    ] {
        connections.replace(saved);
        for command in commands.clone() {
            let result = perform(
                &worker,
                person,
                WorkerAction::RemoteAccess {
                    caller: remote_caller(person, "verified-device"),
                    command,
                },
            );
            if result.stage == "remote_connection_observe_inspect" {
                assert_eq!(result.failure, None);
                assert_eq!(
                    result.connection_observe_status.as_deref(),
                    Some("needs_review")
                );
            } else {
                assert_eq!(
                    result.failure,
                    Some(AgentFailure::PolicyDenied),
                    "{}",
                    result.stage
                );
            }
        }
    }
}

#[test]
fn pairing_setup_uses_verified_identity_and_never_sends_saved_credentials() {
    for mismatch in ["none", "person", "device", "pairing"] {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let pairing_id = Uuid::new_v4().to_string();
        let token = "approved_pairing_secret_for_keychain";
        let body = serde_json::json!({
            "schema_version": 1,
            "pairing_id": if mismatch == "pairing" { Uuid::new_v4().to_string() } else { pairing_id.clone() },
            "status": "approved",
            "person_id": if mismatch == "person" { PersonId::new().to_string() } else { person.to_string() },
            "device_id": if mismatch == "device" { "foreign-device" } else { "verified-device" },
            "client_id": pairing_id,
            "token": token,
        }).to_string();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = [0; 8192];
            let count = socket.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..count]);
            assert!(request.starts_with("POST /pair/poll "));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(!request.contains("bearer"));
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .unwrap();
        });
        let result = perform(
            &worker,
            person,
            WorkerAction::RemotePairing {
                caller: remote_caller(person, "verified-device"),
                command: RemotePairingCommand::Status {
                    target: PairingTarget {
                        base_url: format!("http://{address}"),
                    },
                    pairing_id,
                    polling_proof: "bounded_poll_proof".into(),
                },
            },
        );
        server.join().unwrap();
        if mismatch == "none" {
            assert_eq!(result.failure, None);
            assert_eq!(
                result.remote_pairing.as_ref().unwrap().token.as_deref(),
                Some(token)
            );
        } else {
            assert!(matches!(
                result.failure,
                Some(AgentFailure::PolicyDenied | AgentFailure::CapabilityUnavailable)
            ));
            assert!(result.remote_pairing.is_none());
        }
        assert!(!format!("{result:?}").contains(token));
        assert!(!format!("{result:?}").contains("bounded_poll_proof"));
    }
}

#[test]
fn pairing_setup_rejects_non_loopback_before_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let result = perform(
        &worker,
        person,
        WorkerAction::RemotePairing {
            caller: remote_caller(person, "verified-device"),
            command: RemotePairingCommand::Status {
                target: PairingTarget {
                    base_url: "https://example.com".into(),
                },
                pairing_id: Uuid::new_v4().to_string(),
                polling_proof: "proof".into(),
            },
        },
    );
    assert_eq!(result.failure, Some(AgentFailure::InvalidInput));
}
