use super::*;
use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    process::Command,
};

const PERSON: &str = "00000000-0000-4000-8000-000000000001";

struct FixtureLock {
    file: File,
}

impl FixtureLock {
    fn acquire() -> Self {
        if std::env::var_os("FLOE_NATIVE_FIXTURE_CHILD").is_some() {
            panic!("fixture child must not acquire the parent fixture lock");
        }
        let path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("native-calendar-fixture.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        const LOCK_EX: i32 = 2;
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        assert_eq!(unsafe { flock(file.as_raw_fd(), LOCK_EX) }, 0);
        Self { file }
    }
}

impl Drop for FixtureLock {
    fn drop(&mut self) {
        const LOCK_UN: i32 = 8;
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        assert_eq!(unsafe { flock(self.file.as_raw_fd(), LOCK_UN) }, 0);
    }
}

struct NativeActions {
    worker: Worker,
    core: Arc<FloeCore>,
    runtime: tokio::runtime::Runtime,
    caller: crate::CallerContext,
}
impl NativeActions {
    fn open(path: &std::path::Path, keys: Keys, create: bool) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let core = Arc::new(runtime.block_on(FloeCore::open(path)).unwrap());
        let worker = Worker::with_core(
            path.with_extension("vaults"),
            keys,
            core.clone(),
            Arc::default(),
            Arc::default(),
        )
        .unwrap();
        let person = PersonId(Uuid::parse_str(PERSON).unwrap());
        let caller = remote_caller(person, "test-device");
        let operation_id = Uuid::new_v4();
        worker
            .local_request(
                &caller,
                operation_id,
                Some(LocalOperationIntent::VaultCommand(if create {
                    crate::VaultLifecycleCommand::Create
                } else {
                    crate::VaultLifecycleCommand::Unlock
                })),
                LocalOperationOwner::Vault,
                false,
            )
            .unwrap();
        let outcome =
            local_product::finish_owner(&worker, &caller, operation_id, LocalOperationOwner::Vault);
        assert_eq!(outcome.failure, None);
        worker
            .local_request(
                &caller,
                operation_id,
                None,
                LocalOperationOwner::Vault,
                true,
            )
            .unwrap();
        Self {
            worker,
            core,
            runtime,
            caller,
        }
    }
    fn action(&self, operation: CalendarActionOperation) -> Result<Value, AgentFailure> {
        let operation_id = Uuid::new_v4();
        let intent = match operation {
            CalendarActionOperation::Capabilities => {
                LocalOperationIntent::ActionInspection(crate::ActionInspection::Capabilities)
            }
            command => LocalOperationIntent::ActionCommand(command),
        };
        self.worker.local_request(
            &self.caller,
            operation_id,
            Some(intent),
            LocalOperationOwner::Actions,
            false,
        )?;
        let result = local_product::finish_owner(
            &self.worker,
            &self.caller,
            operation_id,
            LocalOperationOwner::Actions,
        );
        self.worker.local_request(
            &self.caller,
            operation_id,
            None,
            LocalOperationOwner::Actions,
            true,
        )?;
        if let Some(failure) = result.failure {
            return Err(failure);
        }
        Ok(serde_json::to_value(result.calendar_actions.unwrap()).unwrap())
    }
}

#[test]
fn native_executor_uses_rust_ledger_and_lookup_only_after_response_loss() {
    if std::env::var_os("FLOE_NATIVE_FIXTURE_CHILD").is_none() {
        let _lock = FixtureLock::acquire();
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("Contents/MacOS/test-host");
        let frameworks = directory.path().join("Contents/Frameworks");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&frameworks).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        assert!(
            Command::new("xcrun")
                .args(["swiftc", "-emit-library", "-warnings-as-errors"])
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../adapters/providers/tests/fixtures/NativeCalendarFixture.swift"
                ))
                .arg("-o")
                .arg(frameworks.join("libfloe_eventkit.dylib"))
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new(executable)
                .args([
                    "--exact",
                    "vault_host::tests::native_actions::native_executor_uses_rust_ledger_and_lookup_only_after_response_loss",
                    "--nocapture"
                ])
                .env("FLOE_NATIVE_FIXTURE_CHILD", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let keys = Keys::default();
    let host = NativeActions::open(&path, keys.clone(), true);
    host.runtime
        .block_on(host.core.set_calendar_scope(
            PersonId(host.caller.person_id()),
            "00000000-0000-4000-8000-000000000010".into(),
            1,
            host.caller.device_id().into(),
            CalendarProvider::EventKit,
            vec![crate::CalendarSelection {
                calendar_id: "target".into(),
                calendar_name: "Fixture · Target".into(),
            }],
            crate::CalendarScope::Selected,
        ))
        .unwrap();
    assert_eq!(
        host.action(CalendarActionOperation::Capabilities).unwrap()["writes_enabled"],
        true
    );
    let now = chrono::Utc::now();
    let propose = |title: &str| {
        CalendarActionOperation::Propose(Box::new(crate::CalendarActionProposal {
            calendar_id: "target".into(),
            title: title.into(),
            starts_at: (now + chrono::Duration::hours(1)).to_rfc3339(),
            ends_at: (now + chrono::Duration::hours(2)).to_rfc3339(),
            timezone: "Etc/UTC".into(),
            event_id: None,
            event_revision: None,
            delete: false,
        }))
    };
    let proposal = host.action(propose("Lost response")).unwrap()["actions"][0].clone();
    let action_id = Uuid::parse_str(proposal["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        host.action(CalendarActionOperation::Execute { action_id })
            .err(),
        Some(AgentFailure::Conflict)
    );
    host.action(CalendarActionOperation::Decide {
        action_id,
        approve: true,
    })
    .unwrap();
    let result = host
        .action(CalendarActionOperation::Execute { action_id })
        .unwrap();
    assert_eq!(
        result["actions"][0]["state"],
        json!({"status":"unknown", "reason":"timeout"})
    );
    drop(host);
    let host = NativeActions::open(&path, keys, false);
    assert_eq!(
        host.action(CalendarActionOperation::Execute { action_id })
            .err(),
        Some(AgentFailure::Conflict)
    );
    let recovered = host
        .action(CalendarActionOperation::Recover { action_id })
        .unwrap();
    assert_eq!(
        recovered["actions"][0]["state"],
        json!({"status":"succeeded", "external_id":"native-fixture-event|"})
    );
    assert_eq!(
        recovered["actions"][0]["execution_id"],
        proposal["execution_id"]
    );
    let blocked = host.action(propose("Conflict")).unwrap()["actions"][0]["id"].clone();
    let action_id = Uuid::parse_str(blocked.as_str().unwrap()).unwrap();
    host.action(CalendarActionOperation::Decide {
        action_id,
        approve: true,
    })
    .unwrap();
    let result = host
        .action(CalendarActionOperation::Execute { action_id })
        .unwrap();
    assert_eq!(
        result["actions"][0]["state"],
        json!({"status":"blocked", "reason":"schedule_conflict"})
    );
    let trace = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("creates.txt");
    assert_eq!(std::fs::read_to_string(trace).unwrap(), "create\n");
}

#[test]
#[ignore = "requires explicit authorization, copied debug app and real EventKit response-loss shim"]
fn authorized_eventkit_response_loss_recovers_exact_disposable_event() {
    assert_eq!(
        std::env::var("FLOE_EVENTKIT_VALIDATION_AUTHORIZATION").as_deref(),
        Ok("iCloud · Floe Validation/Floe S3 — disposable/create-recover-cleanup")
    );
    let root = std::path::PathBuf::from(std::env::var("FLOE_EVENTKIT_VALIDATION_ROOT").unwrap());
    let metadata = std::fs::symlink_metadata(&root).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    let calendar: Value =
        serde_json::from_slice(&std::fs::read(root.join("calendar.json")).unwrap()).unwrap();
    let calendar_id = calendar["calendar_id"].as_str().unwrap();
    let path = root.join("actions.db");
    assert!(
        !path.exists(),
        "Never reuse uncertain validation work for another write"
    );
    let keys = Keys::default();
    let host = NativeActions::open(&path, keys.clone(), true);
    host.runtime
        .block_on(host.core.set_calendar_scope(
            PersonId(host.caller.person_id()),
            Uuid::new_v4().to_string(),
            1,
            host.caller.device_id().into(),
            CalendarProvider::EventKit,
            vec![crate::CalendarSelection {
                calendar_id: calendar_id.into(),
                calendar_name: "iCloud · Floe Validation".into(),
            }],
            crate::CalendarScope::Selected,
        ))
        .unwrap();
    let now = chrono::Utc::now();
    let result = host
        .action(CalendarActionOperation::Propose(Box::new(
            crate::CalendarActionProposal {
                calendar_id: calendar_id.into(),
                title: "Floe S3 — disposable".into(),
                starts_at: (now + chrono::Duration::hours(1)).to_rfc3339(),
                ends_at: (now + chrono::Duration::hours(1) + chrono::Duration::minutes(5))
                    .to_rfc3339(),
                timezone: "Etc/UTC".into(),
                event_id: None,
                event_revision: None,
                delete: false,
            },
        )))
        .unwrap();
    let proposal = result["actions"][0].clone();
    let action_id = Uuid::parse_str(proposal["id"].as_str().unwrap()).unwrap();
    let mut evidence = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join("proposal.json"))
        .unwrap();
    serde_json::to_writer(&mut evidence, &proposal).unwrap();
    evidence.sync_all().unwrap();
    assert_eq!(
        host.action(CalendarActionOperation::Execute { action_id })
            .err(),
        Some(AgentFailure::Conflict)
    );
    host.action(CalendarActionOperation::Decide {
        action_id,
        approve: true,
    })
    .unwrap();
    let uncertain = host
        .action(CalendarActionOperation::Execute { action_id })
        .unwrap();
    std::fs::write(
        root.join("uncertain.json"),
        serde_json::to_vec(&uncertain).unwrap(),
    )
    .unwrap();
    assert_eq!(
        uncertain["actions"][0]["state"],
        json!({"status":"unknown", "reason":"timeout"})
    );
    drop(host);
    let reopened = NativeActions::open(&path, keys, false);
    assert_eq!(
        reopened
            .action(CalendarActionOperation::Execute { action_id })
            .err(),
        Some(AgentFailure::Conflict)
    );
    let recovered = reopened
        .action(CalendarActionOperation::Recover { action_id })
        .unwrap();
    std::fs::write(
        root.join("recovered.json"),
        serde_json::to_vec(&recovered).unwrap(),
    )
    .unwrap();
    assert_eq!(
        recovered["actions"][0]["execution_id"],
        proposal["execution_id"]
    );
    assert_eq!(recovered["actions"][0]["state"]["status"], "succeeded");
    assert!(
        !recovered["actions"][0]["state"]["external_id"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    println!("REAL_EVENTKIT_UNKNOWN_REOPEN_LOOKUP_PASSED; exact external cleanup still required");
}
