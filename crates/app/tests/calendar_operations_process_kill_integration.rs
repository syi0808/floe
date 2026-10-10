#![cfg(all(feature = "qa-fixtures", target_os = "linux"))]

mod support;

use std::os::unix::process::ExitStatusExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use floe_calendar_operations::{
    ActionBlockedReason, ActionOriginKind, ActionSnapshot, ActionSourceFence, ActionStatus,
    ActionUnknownReason, CalendarDestinationObservation, CalendarEffect, CalendarEffectOutcome,
    CalendarEffectReceipt, CalendarOperationExecutor, CalendarReceiptEvidence, CalendarWriteResult,
    CommittedCalendarEffect, DispatchAdmission, ExecutionIntent, PreparedCalendarEffect,
};
use floe_execution::{BoxFuture, Cancellation, ExecutionScope};
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_app::{
    AppComposition, AppHost, DayCommand, DayCommandOutcome, DayProductQuery, ProductCommand,
    ProductCommandOutcome, ProductCommandRequest, ProductQuery, ProductQueryOutcome,
    RuntimeReadinessState,
};

const MODEL_INPUT: &str = "/process-kill-calendar-operation-fixture";
const CHILD_TIMEOUT: Duration = Duration::from_secs(100);
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DispatchCut {
    BeforeEffect,
    AfterEffectBeforeAck,
    RecoveryOnly,
}

impl DispatchCut {
    fn parse(value: &str) -> Self {
        match value {
            "before_effect" => Self::BeforeEffect,
            "after_effect_before_ack" => Self::AfterEffectBeforeAck,
            "recovery_only" => Self::RecoveryOnly,
            other => panic!("unknown process-kill cut point: {other}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::BeforeEffect => "before_effect",
            Self::AfterEffectBeforeAck => "after_effect_before_ack",
            Self::RecoveryOnly => "recovery_only",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderLedger {
    dispatch_attempts: u32,
    lookup_attempts: u32,
    lookup_execution_ids: Vec<Uuid>,
    effects: Vec<ProviderEffect>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderEffect {
    operation_id: Uuid,
    execution_id: Uuid,
    effect_digest: [u8; 32],
    receipt: CalendarEffectReceipt,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DispatchBarrier {
    cut: DispatchCut,
    operation_id: Uuid,
    execution_id: Uuid,
    effect_digest: [u8; 32],
}

#[derive(Clone)]
struct FileLedgerCalendarExecutor {
    ledger_path: PathBuf,
    barrier_path: PathBuf,
    cut: DispatchCut,
}

impl FileLedgerCalendarExecutor {
    fn new(ledger_path: PathBuf, barrier_path: PathBuf, cut: DispatchCut) -> Self {
        Self {
            ledger_path,
            barrier_path,
            cut,
        }
    }
}

impl CalendarOperationExecutor for FileLedgerCalendarExecutor {
    fn destinations<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        source: &'a ActionSourceFence,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<CalendarDestinationObservation>, ActionBlockedReason>> {
        Box::pin(async move {
            if actor.validate().is_err() || scope.cancellation().is_cancelled() {
                return Err(ActionBlockedReason::PolicyDenied);
            }
            Ok(source
                .resources
                .iter()
                .map(|calendar_id| CalendarDestinationObservation {
                    calendar_id: calendar_id.clone(),
                    calendar_name: "Synthetic process-kill calendar".into(),
                    can_modify: true,
                })
                .collect())
        })
    }

    fn prepare<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        _: &'a floe_calendar_operations::ActionRecord,
        _: &'a [floe_calendar_operations::ActionDependencySourceFence],
        _: &'a [floe_day::Event],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedCalendarEffect>, ActionBlockedReason>> {
        Box::pin(async move {
            if actor.validate().is_err() || scope.cancellation().is_cancelled() {
                return Err(ActionBlockedReason::PolicyDenied);
            }
            Ok(Box::new(FileLedgerPreparedEffect {
                ledger_path: self.ledger_path.clone(),
                barrier_path: self.barrier_path.clone(),
                cut: self.cut,
                executor_generation: actor.runtime_epoch,
            }) as Box<dyn PreparedCalendarEffect>)
        })
    }

    fn recover<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        intent: &'a ExecutionIntent,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, CalendarEffectOutcome> {
        Box::pin(async move {
            if actor.validate().is_err()
                || actor.person_id != intent.person_id
                || actor.device_id != intent.device_id
                || scope.cancellation().is_cancelled()
            {
                return CalendarEffectOutcome::Unknown {
                    identity: intent.identity(),
                    reason: ActionUnknownReason::Timeout,
                };
            }

            let mut ledger = match read_ledger(&self.ledger_path) {
                Ok(ledger) => ledger,
                Err(_) => {
                    return CalendarEffectOutcome::Unknown {
                        identity: intent.identity(),
                        reason: ActionUnknownReason::NativeReceiptUnavailable,
                    };
                }
            };
            ledger.lookup_attempts += 1;
            ledger.lookup_execution_ids.push(intent.execution_id);
            if write_json_atomically(&self.ledger_path, &ledger).is_err() {
                return CalendarEffectOutcome::Unknown {
                    identity: intent.identity(),
                    reason: ActionUnknownReason::NativeReceiptUnavailable,
                };
            }

            let mut matching = ledger.effects.iter().filter(|effect| {
                effect.operation_id == intent.action_id
                    && effect.execution_id == intent.execution_id
                    && effect.effect_digest == intent.effect_digest
            });
            match (matching.next(), matching.next()) {
                (Some(effect), None) => CalendarEffectOutcome::Committed {
                    receipt: effect.receipt.clone(),
                },
                (None, None) => CalendarEffectOutcome::Unknown {
                    identity: intent.identity(),
                    reason: ActionUnknownReason::NativeReceiptUnavailable,
                },
                _ => CalendarEffectOutcome::Unknown {
                    identity: intent.identity(),
                    reason: ActionUnknownReason::InconclusiveLookup,
                },
            }
        })
    }
}

struct FileLedgerPreparedEffect {
    ledger_path: PathBuf,
    barrier_path: PathBuf,
    cut: DispatchCut,
    executor_generation: u64,
}

impl PreparedCalendarEffect for FileLedgerPreparedEffect {
    fn executor_generation(&self) -> u64 {
        self.executor_generation
    }

    fn dispatch(
        self: Box<Self>,
        admission: DispatchAdmission,
        _: ExecutionScope,
    ) -> BoxFuture<'static, CalendarEffectOutcome> {
        let ledger_path = self.ledger_path.clone();
        let barrier_path = self.barrier_path.clone();
        let cut = self.cut;
        Box::pin(async move {
            let intent = admission.intent;
            let mut ledger = match read_ledger(&ledger_path) {
                Ok(ledger) => ledger,
                Err(_) => {
                    return unknown(&intent, ActionUnknownReason::NativeReceiptUnavailable);
                }
            };
            ledger.dispatch_attempts += 1;
            if write_json_atomically(&ledger_path, &ledger).is_err() {
                return unknown(&intent, ActionUnknownReason::NativeReceiptUnavailable);
            }

            if cut == DispatchCut::RecoveryOnly {
                return unknown(&intent, ActionUnknownReason::InconclusiveLookup);
            }

            if cut == DispatchCut::AfterEffectBeforeAck {
                let receipt = match synthetic_receipt(&intent) {
                    Ok(receipt) => receipt,
                    Err(_) => return unknown(&intent, ActionUnknownReason::InvalidReceipt),
                };
                ledger.effects.push(ProviderEffect {
                    operation_id: intent.action_id,
                    execution_id: intent.execution_id,
                    effect_digest: intent.effect_digest,
                    receipt,
                });
                if write_json_atomically(&ledger_path, &ledger).is_err() {
                    return unknown(&intent, ActionUnknownReason::NativeReceiptUnavailable);
                }
            }

            let barrier = DispatchBarrier {
                cut,
                operation_id: intent.action_id,
                execution_id: intent.execution_id,
                effect_digest: intent.effect_digest,
            };
            if write_json_atomically(&barrier_path, &barrier).is_err() {
                return unknown(&intent, ActionUnknownReason::NativeReceiptUnavailable);
            }

            // The test parent kills this process only after it reads this exact
            // barrier. Keeping this future pending holds the owner before the
            // provider acknowledgement can reach Calendar Operations.
            std::future::pending::<CalendarEffectOutcome>().await
        })
    }
}

fn synthetic_receipt(intent: &ExecutionIntent) -> Result<CalendarEffectReceipt, AgentFailure> {
    let CalendarEffect::Create {
        title, schedule, ..
    } = &intent.effect
    else {
        return Err(AgentFailure::InvalidInput);
    };
    let committed_at = Utc::now();
    Ok(CalendarEffectReceipt {
        identity: intent.identity(),
        effect: CommittedCalendarEffect::Created {
            event: CalendarWriteResult {
                external_id: format!("synthetic-{}", intent.execution_id),
                external_revision: floe_day::CalendarExternalRevision::ObservationFingerprint(
                    [0x5a; 32],
                ),
                title: title.clone(),
                schedule: schedule.clone(),
                can_modify: true,
            },
        },
        evidence: CalendarReceiptEvidence::NativeAcknowledgement {
            host_epoch: Uuid::from_u128(0x7cf39a2a_4f6a_45dd_8f52_14b5094e57bf),
            receipt_id: Uuid::new_v4(),
        },
        committed_at,
    })
}

fn unknown(intent: &ExecutionIntent, reason: ActionUnknownReason) -> CalendarEffectOutcome {
    CalendarEffectOutcome::Unknown {
        identity: intent.identity(),
        reason,
    }
}

fn read_ledger(path: &Path) -> std::io::Result<ProviderLedger> {
    match File::open(path) {
        Ok(file) => serde_json::from_reader(file)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ProviderLedger::default()),
        Err(error) => Err(error),
    }
}

fn write_json_atomically(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("test file must have a parent directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec(value)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[test]
fn kill_after_dispatch_intent_preserves_unknown_without_blind_redispatch() {
    run_kill_case(DispatchCut::BeforeEffect, "before-effect");
}

#[test]
fn kill_after_provider_receipt_reconciles_without_duplicate_effect() {
    run_kill_case(DispatchCut::AfterEffectBeforeAck, "after-effect-before-ack");
}

#[test]
#[ignore = "subprocess entry point; invoked by the two parent crash tests"]
fn subprocess_driver() {
    let profile_path = env_path("FLOE_P6_CRASH_PROFILE");
    let ledger_path = env_path("FLOE_P6_CRASH_LEDGER");
    let barrier_path = env_path("FLOE_P6_CRASH_BARRIER");
    let cut = DispatchCut::parse(&std::env::var("FLOE_P6_CRASH_CUT").expect("cut point"));
    let command_id = env_uuid("FLOE_P6_CRASH_COMMAND_ID");
    let destination_ref = env_uuid("FLOE_P6_CRASH_DESTINATION_REF");
    let operation_title = std::env::var("FLOE_P6_CRASH_TITLE").expect("operation title");

    let host = open_host(&profile_path, &ledger_path, &barrier_path, cut);
    assert_eq!(
        support::prepare_runtime(&host),
        RuntimeReadinessState::Ready
    );
    let receipt = submit_create(&host, command_id, destination_ref, &operation_title);
    assert!(!receipt.operation_id.is_nil());
    wait_for_file(&barrier_path, CHILD_TIMEOUT)
        .unwrap_or_else(|error| panic!("dispatch barrier was not reached: {error}"));

    // Parent termination is the only release path. Do not drop the live host.
    loop {
        thread::park();
    }
}

fn run_kill_case(cut: DispatchCut, name: &str) {
    let fixture = tempfile::tempdir().expect("create isolated encrypted test fixture");
    let profile_path = fixture.path().join("profile");
    let ledger_path = fixture.path().join("synthetic-provider-ledger.json");
    let barrier_path = fixture.path().join("dispatch-barrier.json");
    fs::create_dir_all(&profile_path).expect("create isolated profile directory");
    let logs_path = retained_logs_dir(name);

    let parent_host = open_host(
        &profile_path,
        &ledger_path,
        &barrier_path,
        DispatchCut::RecoveryOnly,
    );
    assert_eq!(
        support::prepare_runtime(&parent_host),
        RuntimeReadinessState::Ready
    );
    support::configure_fixture_calendar(&parent_host, "Synthetic team calendar");
    let destination_ref = only_destination(&parent_host).destination_ref;
    let command_id = Uuid::new_v4();
    let operation_title = format!("Process-kill fixture {name}");
    let expected_operation_id = support::with_ready(&parent_host, |_, caller, _| {
        floe_calendar_operations::action_uuid(
            b"floe.actions.action.v1\0",
            PersonId(caller.person_id()),
            command_id,
        )
    });
    drop(parent_host);

    let barrier = launch_and_kill_child(
        &profile_path,
        &ledger_path,
        &barrier_path,
        command_id,
        destination_ref,
        &operation_title,
        cut,
        &logs_path,
    );
    assert_eq!(barrier.cut, cut);
    assert_eq!(barrier.operation_id, expected_operation_id);

    let provider_state = read_ledger(&ledger_path).expect("read provider ledger after child kill");
    assert_eq!(provider_state.dispatch_attempts, 1);
    assert_eq!(provider_state.lookup_attempts, 0);
    match cut {
        DispatchCut::BeforeEffect => assert!(provider_state.effects.is_empty()),
        DispatchCut::AfterEffectBeforeAck => {
            assert_eq!(provider_state.effects.len(), 1);
            assert_eq!(provider_state.effects[0].execution_id, barrier.execution_id);
            assert_eq!(
                provider_state.effects[0].receipt.identity.execution_id,
                barrier.execution_id
            );
            assert_eq!(
                provider_state.effects[0].receipt.identity.effect_digest,
                barrier.effect_digest
            );
        }
        DispatchCut::RecoveryOnly => unreachable!(),
    }

    // Reopen the same encrypted App profile in the parent after the only child
    // process has been killed. Activation must classify the durable in-flight
    // intent as uncertain, without calling dispatch or provider lookup.
    let host = open_host(
        &profile_path,
        &ledger_path,
        &barrier_path,
        DispatchCut::RecoveryOnly,
    );
    assert_eq!(
        support::prepare_runtime(&host),
        RuntimeReadinessState::Ready
    );
    let uncertain = inspect_operation(&host, expected_operation_id);
    assert_eq!(uncertain.action_ref, expected_operation_id);
    assert_eq!(uncertain.origin, ActionOriginKind::Direct);
    assert!(matches!(
        uncertain.status,
        ActionStatus::Unknown {
            reason: ActionUnknownReason::ResponseLost
        }
    ));

    // Replaying the exact retained command identity returns the same operation
    // and does not turn uncertainty into another provider dispatch.
    let replay = submit_create(&host, command_id, destination_ref, &operation_title);
    assert_eq!(replay.operation_id, expected_operation_id);
    assert_eq!(
        replay.status,
        floe_day::ManualCalendarOperationStatus::Unknown
    );
    let after_replay = read_ledger(&ledger_path).expect("read provider ledger after replay");
    assert_eq!(after_replay.dispatch_attempts, 1);

    match cut {
        DispatchCut::BeforeEffect => {
            assert!(after_replay.effects.is_empty());
            assert_eq!(after_replay.lookup_attempts, 0);
        }
        DispatchCut::AfterEffectBeforeAck => {
            let reconcile_command_id = Uuid::new_v4();
            let accepted = reconcile(
                &host,
                reconcile_command_id,
                expected_operation_id,
                uncertain.revision,
            );
            assert_eq!(accepted.operation_id, expected_operation_id);
            let recovered = wait_for_status(&host, expected_operation_id, |status| {
                matches!(status, ActionStatus::Succeeded { .. })
            });
            assert!(matches!(recovered.status, ActionStatus::Succeeded { .. }));

            let reconciliation_replay = reconcile(
                &host,
                reconcile_command_id,
                expected_operation_id,
                uncertain.revision,
            );
            assert_eq!(reconciliation_replay.operation_id, expected_operation_id);
            assert_eq!(
                reconciliation_replay.status,
                floe_day::ManualCalendarOperationStatus::Succeeded
            );
            let reconciled_state = read_ledger(&ledger_path)
                .expect("read provider ledger after receipt reconciliation");
            assert_eq!(reconciled_state.dispatch_attempts, 1);
            assert_eq!(reconciled_state.effects.len(), 1);
            assert_eq!(reconciled_state.lookup_attempts, 1);
            assert_eq!(
                reconciled_state.lookup_execution_ids,
                vec![barrier.execution_id]
            );
            assert_eq!(
                reconciled_state.effects[0].operation_id,
                expected_operation_id
            );
            assert_eq!(
                reconciled_state.effects[0].execution_id,
                barrier.execution_id
            );
            assert_eq!(
                reconciled_state.effects[0].effect_digest,
                barrier.effect_digest
            );
        }
        DispatchCut::RecoveryOnly => unreachable!(),
    }

    eprintln!("process-kill raw child logs: {}", logs_path.display());
}

fn open_host(
    profile_path: &Path,
    ledger_path: &Path,
    barrier_path: &Path,
    cut: DispatchCut,
) -> AppHost<AppComposition> {
    let model = support::ScriptedModel::new(
        support::PrimaryBehavior::NoGateway,
        MODEL_INPUT,
        support::ModelOutput::ScheduleOperationApprovalFlow,
    );
    let options =
        model
            .app_options()
            .with_qa_calendar_operation_executor(FileLedgerCalendarExecutor::new(
                ledger_path.to_path_buf(),
                barrier_path.to_path_buf(),
                cut,
            ));
    floe_app::open_default_with_options(
        profile_path.to_str().expect("profile path is UTF-8"),
        options,
    )
    .unwrap_or_else(|error| panic!("open isolated App fixture: {error:?}"))
}

fn only_destination(host: &AppHost<AppComposition>) -> floe_day::ManualCalendarDestination {
    let result = host
        .request(Uuid::new_v4())
        .expect("admit Day Calendar destination query")
        .product_query(ProductQuery::Day(
            DayProductQuery::ExternalCalendarDestinations,
        ))
        .expect("read synthetic Calendar destination");
    let ProductQueryOutcome::Day(floe_app::DayQueryOutcome::ExternalCalendarDestinations(
        destinations,
    )) = result
    else {
        panic!("Day returned a different destination query result")
    };
    let [destination] = destinations.as_slice() else {
        panic!("expected exactly one synthetic writable destination: {destinations:?}")
    };
    destination.clone()
}

fn schedule() -> floe_day::TimedSchedule {
    let starts_at = DateTime::parse_from_rfc3339("2026-10-13T10:00:00Z")
        .expect("fixed start")
        .with_timezone(&Utc);
    let ends_at = DateTime::parse_from_rfc3339("2026-10-13T11:00:00Z")
        .expect("fixed end")
        .with_timezone(&Utc);
    floe_day::TimedSchedule::new(starts_at, ends_at, "UTC").expect("valid test schedule")
}

fn submit_create(
    host: &AppHost<AppComposition>,
    command_id: Uuid,
    destination_ref: Uuid,
    title: &str,
) -> floe_day::ManualCalendarOperationReceipt {
    let result = host
        .request(Uuid::new_v4())
        .expect("admit direct Day Calendar command")
        .product_command(ProductCommandRequest {
            command_id: floe_kernel::CommandId::from_uuid(command_id).expect("command ID"),
            command: ProductCommand::Day(DayCommand::ExternalCalendarOperation {
                operation: floe_day::ManualCalendarOperation::Create {
                    destination_ref,
                    title: title.to_owned(),
                    schedule: schedule(),
                },
            }),
        })
        .expect("admit or replay the exact Calendar command");
    let ProductCommandOutcome::Day(DayCommandOutcome::ExternalCalendarOperation(receipt)) = result
    else {
        panic!("Day returned a different Calendar command result")
    };
    receipt
}

fn reconcile(
    host: &AppHost<AppComposition>,
    command_id: Uuid,
    operation_id: Uuid,
    expected_revision: u64,
) -> floe_day::ManualCalendarOperationReceipt {
    let result = host
        .request(Uuid::new_v4())
        .expect("admit explicit Calendar reconciliation")
        .product_command(ProductCommandRequest {
            command_id: floe_kernel::CommandId::from_uuid(command_id).expect("reconcile ID"),
            command: ProductCommand::Day(DayCommand::ReconcileExternalCalendarOperation {
                operation_ref: operation_id,
                expected_revision,
            }),
        })
        .expect("admit or replay the exact reconciliation command");
    let ProductCommandOutcome::Day(DayCommandOutcome::ReconciledExternalCalendarOperation(receipt)) =
        result
    else {
        panic!("Day returned a different Calendar reconciliation result")
    };
    receipt
}

fn inspect_operation(host: &AppHost<AppComposition>, operation_id: Uuid) -> ActionSnapshot {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                owners
                    .calendar_operations
                    .inspect(
                        &actor,
                        operation_id,
                        &floe_app::host_scope(
                            Uuid::new_v4(),
                            Cancellation::new(),
                            Duration::from_secs(15),
                        ),
                    )
                    .await
            })
            .expect("inspect durable Calendar Operation")
    })
}

fn wait_for_status(
    host: &AppHost<AppComposition>,
    operation_id: Uuid,
    accepted: impl Fn(&ActionStatus) -> bool,
) -> ActionSnapshot {
    let deadline = Instant::now() + RECOVERY_TIMEOUT;
    loop {
        let snapshot = inspect_operation(host, operation_id);
        if accepted(&snapshot.status) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "Calendar operation did not recover before timeout: {:?}",
            snapshot.status
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn launch_and_kill_child(
    profile_path: &Path,
    ledger_path: &Path,
    barrier_path: &Path,
    command_id: Uuid,
    destination_ref: Uuid,
    operation_title: &str,
    cut: DispatchCut,
    logs_path: &Path,
) -> DispatchBarrier {
    fs::create_dir_all(logs_path).expect("create persistent raw-log directory");
    let stdout =
        File::create(logs_path.join("child.stdout.log")).expect("create raw child stdout log");
    let stderr =
        File::create(logs_path.join("child.stderr.log")).expect("create raw child stderr log");
    let mut child = TestChildGuard::new(
        Command::new(std::env::current_exe().expect("current integration test binary"))
            .args([
                "--ignored",
                "--exact",
                "subprocess_driver",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("FLOE_P6_CRASH_PROFILE", profile_path)
            .env("FLOE_P6_CRASH_LEDGER", ledger_path)
            .env("FLOE_P6_CRASH_BARRIER", barrier_path)
            .env("FLOE_P6_CRASH_CUT", cut.as_str())
            .env("FLOE_P6_CRASH_COMMAND_ID", command_id.to_string())
            .env("FLOE_P6_CRASH_DESTINATION_REF", destination_ref.to_string())
            .env("FLOE_P6_CRASH_TITLE", operation_title)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .expect("spawn isolated Calendar Operation child test"),
    );

    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        if barrier_path.exists() {
            break;
        }
        if let Some(status) = child.try_wait().expect("poll test child") {
            panic!(
                "child exited before its semantic dispatch barrier ({status}); raw logs: {}",
                logs_path.display()
            );
        }
        if Instant::now() >= deadline {
            terminate_child(&mut child);
            panic!(
                "child did not reach its dispatch barrier within {CHILD_TIMEOUT:?}; raw logs: {}",
                logs_path.display()
            );
        }
        // Poll only for the test-owned barrier file; elapsed time never chooses
        // the crash point.
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        child
            .try_wait()
            .expect("confirm child still active")
            .is_none(),
        "child exited after writing its barrier; raw logs: {}",
        logs_path.display()
    );
    let status = child
        .kill_and_wait()
        .expect("kill and reap only the test child process");
    assert_eq!(
        status.signal(),
        Some(libc::SIGKILL),
        "expected the intentional child crash to be SIGKILL, got {status:?}"
    );
    let barrier: DispatchBarrier =
        serde_json::from_reader(File::open(barrier_path).expect("open durable dispatch barrier"))
            .expect("decode dispatch barrier");
    barrier
}

struct TestChildGuard {
    child: Option<Child>,
}

impl TestChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child
            .as_mut()
            .ok_or_else(|| std::io::Error::other("test child was already reaped"))?
            .try_wait()
    }

    fn kill_and_wait(&mut self) -> std::io::Result<ExitStatus> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| std::io::Error::other("test child was already reaped"))?;
        child.kill()?;
        let status = child.wait()?;
        self.child.take();
        Ok(status)
    }

    fn best_effort_terminate(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        let already_exited = matches!(child.try_wait(), Ok(Some(_)));
        if !already_exited {
            let _ = child.kill();
        }
        if child.wait().is_ok() {
            self.child.take();
        }
    }
}

impl Drop for TestChildGuard {
    fn drop(&mut self) {
        self.best_effort_terminate();
    }
}

fn terminate_child(child: &mut TestChildGuard) {
    child.best_effort_terminate();
}

fn wait_for_file(path: &Path, timeout: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if path.is_file() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("waiting for {}", path.display()),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("missing {name}")))
}

fn env_uuid(name: &str) -> Uuid {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("missing {name}"))
        .parse()
        .unwrap_or_else(|_| panic!("invalid UUID in {name}"))
}

fn retained_logs_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir()
        .join("floe-calendar-process-kill-logs")
        .join(format!("{}-{name}-{}", std::process::id(), Uuid::new_v4()));
    fs::create_dir_all(&path).expect("create retained test log directory");
    path
}
