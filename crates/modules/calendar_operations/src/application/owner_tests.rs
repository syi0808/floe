use super::{CalendarOperationsDependencies, CalendarOperationsService};
use crate::*;
use floe_access::{OperationAuthorizationPolicy, OperationPolicyChange, OperationPolicyMode};
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
use floe_connections::{ConnectionResource, ResourceMode, SourceConnection};
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle};
use floe_day::{
    CalendarAcquisition, CalendarMirror, CalendarRefreshError, CalendarRefreshRequest,
    DayCollectionCommit, DayCollectionReceipt, DayError, DayMutationCommand, DayMutationResult,
    DayReadQuery, DayRefreshRepository, DayRepository, DayService, DayWriteFence, RefreshAdmission,
    RefreshAdmissionResult, RefreshCommit, RefreshExecutorReplacement, RefreshLookup,
    RefreshRecord, RefreshTransition, TimelineItem,
};
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor, PersonId, TraceContext};
use std::{
    collections::HashMap,
    future::{Future, poll_fn},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};
use tokio::{
    sync::{Notify, Semaphore, oneshot},
    time::Instant,
};
use uuid::Uuid;

fn unexpected<'a, T: Send + 'a>() -> BoxFuture<'a, T> {
    Box::pin(async { panic!("unexpected fake port call") })
}

struct BarrierRepository {
    receipts: StdMutex<HashMap<Uuid, &'static str>>,
    actions: StdMutex<HashMap<Uuid, (ActionDigest, ActionRecord)>>,
    rejections: StdMutex<HashMap<Uuid, (ActionDigest, AgentFailure)>>,
    find_calls: AtomicUsize,
    admitted: AtomicUsize,
    authority_started: Notify,
    authority_release: Semaphore,
    hold_before_admission: AtomicBool,
    admission_started: Notify,
    admission_before_release: Semaphore,
    admission_committed: Notify,
    admission_release: Semaphore,
    pre_dispatch_stops: AtomicUsize,
    pre_dispatch_stopped: Notify,
}

impl BarrierRepository {
    fn new() -> Self {
        Self {
            receipts: StdMutex::new(HashMap::new()),
            actions: StdMutex::new(HashMap::new()),
            rejections: StdMutex::new(HashMap::new()),
            find_calls: AtomicUsize::new(0),
            admitted: AtomicUsize::new(0),
            authority_started: Notify::new(),
            authority_release: Semaphore::new(0),
            hold_before_admission: AtomicBool::new(false),
            admission_started: Notify::new(),
            admission_before_release: Semaphore::new(0),
            admission_committed: Notify::new(),
            admission_release: Semaphore::new(0),
            pre_dispatch_stops: AtomicUsize::new(0),
            pre_dispatch_stopped: Notify::new(),
        }
    }
}

impl CalendarOperationsRepository for BarrierRepository {
    fn validate_proposal_coverage<'a>(
        &'a self,
        _person_id: PersonId,
        _coverage: &'a floe_agent_contract::DependencyCoverage,
    ) -> BoxFuture<'a, Result<(), CalendarOperationStoreError>> {
        unexpected()
    }

    fn get<'a>(
        &'a self,
        _person_id: PersonId,
        action_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, CalendarOperationStoreError>> {
        Box::pin(async move {
            Ok(self
                .actions
                .lock()
                .map_err(|_| CalendarOperationStoreError::Unavailable)?
                .values()
                .find_map(|(_, record)| (record.id == action_id).then(|| record.clone())))
        })
    }

    fn list<'a>(
        &'a self,
        _person_id: PersonId,
        _cursor: Option<Uuid>,
        _limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, CalendarOperationStoreError>> {
        unexpected()
    }

    fn list_direct<'a>(
        &'a self,
        _person_id: PersonId,
        _cursor: Option<Uuid>,
        _limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, CalendarOperationStoreError>> {
        unexpected()
    }

    fn find_admission<'a>(
        &'a self,
        _person_id: PersonId,
        command_id: Uuid,
        request_digest: ActionDigest,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, CalendarOperationStoreError>> {
        Box::pin(async move {
            self.find_calls.fetch_add(1, Ordering::AcqRel);
            if let Some((stored_digest, record)) = self
                .actions
                .lock()
                .map_err(|_| CalendarOperationStoreError::Unavailable)?
                .get(&command_id)
            {
                return if stored_digest == &request_digest {
                    Ok(Some(record.clone()))
                } else {
                    Err(CalendarOperationStoreError::Conflict)
                };
            }
            if let Some((digest, reason)) = self.rejections.lock().unwrap().get(&command_id) {
                return Err(if *digest == request_digest {
                    CalendarOperationStoreError::Rejected(*reason)
                } else {
                    CalendarOperationStoreError::Conflict
                });
            }
            let occupied = self
                .receipts
                .lock()
                .map_err(|_| CalendarOperationStoreError::Unavailable)?
                .contains_key(&command_id);
            if occupied {
                Err(CalendarOperationStoreError::Conflict)
            } else {
                Ok(None)
            }
        })
    }

    fn reject_admission<'a>(
        &'a self,
        _person_id: PersonId,
        command_id: Uuid,
        request_digest: ActionDigest,
        reason: AgentFailure,
    ) -> BoxFuture<
        'a,
        Result<OperationAdmissionResolution, CommandFailure<CalendarOperationStoreError>>,
    > {
        Box::pin(async move {
            let actions = self.actions.lock().unwrap();
            if let Some((digest, record)) = actions.get(&command_id) {
                return if *digest == request_digest {
                    Ok(OperationAdmissionResolution::Admitted(record.clone()))
                } else {
                    Err(CommandFailure::Indeterminate(
                        CalendarOperationStoreError::Conflict,
                    ))
                };
            }
            if self.receipts.lock().unwrap().contains_key(&command_id) {
                return Err(CommandFailure::Indeterminate(
                    CalendarOperationStoreError::Conflict,
                ));
            }
            let mut rejections = self.rejections.lock().unwrap();
            let (digest, reason) = rejections
                .entry(command_id)
                .or_insert((request_digest, reason));
            if *digest != request_digest {
                return Err(CommandFailure::Indeterminate(
                    CalendarOperationStoreError::Conflict,
                ));
            }
            Ok(OperationAdmissionResolution::Rejected(*reason))
        })
    }

    fn admit<'a>(
        &'a self,
        admission: OperationAdmission,
    ) -> BoxFuture<'a, Result<AdmittedOperation, CommandFailure<CalendarOperationStoreError>>> {
        Box::pin(async move {
            if self.hold_before_admission.swap(false, Ordering::AcqRel) {
                self.admission_started.notify_one();
                self.admission_before_release
                    .acquire()
                    .await
                    .unwrap()
                    .forget();
            }
            let (record, replayed) = {
                let mut actions = self.actions.lock().map_err(|_| {
                    CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
                })?;
                if let Some((stored_digest, record)) = actions.get(&admission.command_id) {
                    if stored_digest != &admission.request_digest {
                        return Err(CommandFailure::Indeterminate(
                            CalendarOperationStoreError::Conflict,
                        ));
                    }
                    (record.clone(), true)
                } else {
                    if self
                        .receipts
                        .lock()
                        .map_err(|_| {
                            CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
                        })?
                        .contains_key(&admission.command_id)
                    {
                        return Err(CommandFailure::Indeterminate(
                            CalendarOperationStoreError::Conflict,
                        ));
                    }
                    if let Some((digest, reason)) =
                        self.rejections.lock().unwrap().get(&admission.command_id)
                    {
                        return Err(if *digest == admission.request_digest {
                            CommandFailure::NotApplied(CalendarOperationStoreError::Rejected(
                                *reason,
                            ))
                        } else {
                            CommandFailure::Indeterminate(CalendarOperationStoreError::Conflict)
                        });
                    }
                    let record = admission.record.clone();
                    actions.insert(
                        admission.command_id,
                        (admission.request_digest, record.clone()),
                    );
                    self.admitted.fetch_add(1, Ordering::AcqRel);
                    (record, false)
                }
            };
            if !replayed {
                self.admission_committed.notify_one();
                self.admission_release
                    .acquire()
                    .await
                    .map_err(|_| {
                        CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
                    })?
                    .forget();
            }
            Ok(AdmittedOperation { record, replayed })
        })
    }

    fn record_decision<'a>(
        &'a self,
        _decision: ActionDecision,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<CalendarOperationStoreError>>> {
        unexpected()
    }

    fn admit_reconciliation<'a>(
        &'a self,
        _command: ActionReconciliation,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<CalendarOperationStoreError>>> {
        unexpected()
    }

    fn stop_before_dispatch<'a>(
        &'a self,
        stop: PreDispatchStop,
    ) -> BoxFuture<'a, Result<ActionRecord, CalendarOperationStoreError>> {
        Box::pin(async move {
            let record = self
                .actions
                .lock()
                .map_err(|_| CalendarOperationStoreError::Unavailable)?
                .values()
                .find_map(|(_, record)| (record.id == stop.action_id).then(|| record.clone()))
                .ok_or(CalendarOperationStoreError::NotFound)?;
            self.pre_dispatch_stops.fetch_add(1, Ordering::AcqRel);
            self.pre_dispatch_stopped.notify_one();
            Ok(record)
        })
    }

    fn prepare_dispatch<'a>(
        &'a self,
        _intent: DispatchIntent,
    ) -> BoxFuture<'a, Result<DispatchAdmission, CalendarOperationStoreError>> {
        unexpected()
    }

    fn load_execution<'a>(
        &'a self,
        _person_id: PersonId,
        _execution_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<DispatchAdmission>, CalendarOperationStoreError>> {
        unexpected()
    }

    fn settle_execution<'a>(
        &'a self,
        _settlement: ExecutionSettlement,
    ) -> BoxFuture<'a, Result<ActionRecord, CalendarOperationStoreError>> {
        unexpected()
    }

    fn pending_recovery<'a>(
        &'a self,
        _person_id: PersonId,
        _cursor: Option<Uuid>,
        _limit: u16,
    ) -> BoxFuture<'a, Result<RecoveryPage, CalendarOperationStoreError>> {
        unexpected()
    }

    fn ack_collection<'a>(
        &'a self,
        _ack: CollectionAck,
    ) -> BoxFuture<'a, Result<CollectionTicket, CalendarOperationStoreError>> {
        unexpected()
    }

    fn read_authority<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<OperationAuthorizationPolicy, CalendarOperationStoreError>> {
        Box::pin(async move {
            Ok(OperationAuthorizationPolicy {
                person_id,
                revision: 1,
                calendar_create: OperationPolicyMode::Ask,
            })
        })
    }

    fn compare_and_set_authority<'a>(
        &'a self,
        change: OperationPolicyChange,
    ) -> BoxFuture<
        'a,
        Result<OperationAuthorizationPolicy, CommandFailure<CalendarOperationStoreError>>,
    > {
        Box::pin(async move {
            self.authority_started.notify_one();
            self.authority_release
                .acquire()
                .await
                .map_err(|_| {
                    CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
                })?
                .forget();
            self.receipts
                .lock()
                .map_err(|_| {
                    CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
                })?
                .insert(change.command_id, "authority");
            Ok(OperationAuthorizationPolicy {
                person_id: change.person_id,
                revision: change.expected_revision + 1,
                calendar_create: change.mode,
            })
        })
    }
}

struct UnusedSources;
impl OperationSourceReader for UnusedSources {
    fn list_calendar_sources<'a>(
        &'a self,
        _person_id: PersonId,
    ) -> BoxFuture<'a, Result<Vec<floe_connections::SourceConnection>, AgentFailure>> {
        unexpected()
    }
    fn load<'a>(
        &'a self,
        _person_id: PersonId,
        _connection_id: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<Option<floe_connections::SourceConnection>, AgentFailure>> {
        unexpected()
    }
    fn source_is_fenced<'a>(
        &'a self,
        _person_id: PersonId,
        _connection_id: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        unexpected()
    }
    fn read_reservation_fence<'a>(
        &'a self,
        _person_id: PersonId,
        _connection_id: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<floe_connections::SourceReservationFence, AgentFailure>> {
        unexpected()
    }
}

struct UnusedProposals;
impl ExpertProposalReader for UnusedProposals {
    fn read<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _receipt: &'a floe_agent_contract::TaskExecutionReceiptRef,
        _artifact_id: Uuid,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertProposalEvidence, AgentFailure>> {
        unexpected()
    }
}

struct UnusedExecutor;
impl CalendarOperationExecutor for UnusedExecutor {
    fn destinations<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _source: &'a ActionSourceFence,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<CalendarDestinationObservation>, ActionBlockedReason>> {
        unexpected()
    }
    fn prepare<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _record: &'a ActionRecord,
        _dependencies: &'a [ActionDependencySourceFence],
        _local_events: &'a [floe_day::Event],
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedCalendarEffect>, ActionBlockedReason>> {
        unexpected()
    }
    fn recover<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _intent: &'a ExecutionIntent,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, CalendarEffectOutcome> {
        unexpected()
    }
}

struct UnusedDay;
impl DayRefreshRepository for UnusedDay {
    fn admit_refresh<'a>(
        &'a self,
        _admission: RefreshAdmission,
    ) -> BoxFuture<'a, Result<RefreshAdmissionResult, CommandFailure<DayError>>> {
        unexpected()
    }
    fn read_refresh<'a>(
        &'a self,
        _lookup: RefreshLookup,
    ) -> BoxFuture<'a, Result<Option<RefreshRecord>, DayError>> {
        unexpected()
    }
    fn transition_refresh<'a>(
        &'a self,
        _transition: RefreshTransition,
    ) -> BoxFuture<'a, Result<RefreshRecord, DayError>> {
        unexpected()
    }
    fn commit_refresh<'a>(
        &'a self,
        _commit: RefreshCommit,
    ) -> BoxFuture<'a, Result<RefreshRecord, DayError>> {
        unexpected()
    }
    fn interrupt_refreshes<'a>(
        &'a self,
        _replacement: RefreshExecutorReplacement,
    ) -> BoxFuture<'a, Result<(), DayError>> {
        Box::pin(async { Ok(()) })
    }
    fn retire_refresh_executor<'a>(
        &'a self,
        _expected: RefreshExecutorReplacement,
    ) -> BoxFuture<'a, Result<(), DayError>> {
        Box::pin(async { Ok(()) })
    }
}
impl DayRepository for UnusedDay {
    fn mutate<'a>(
        &'a self,
        _command: DayMutationCommand,
        _fence: &'a DayWriteFence,
    ) -> BoxFuture<'a, Result<DayMutationResult, CommandFailure<DayError>>> {
        unexpected()
    }
    fn collect_action<'a>(
        &'a self,
        _commit: DayCollectionCommit,
        _fence: &'a DayWriteFence,
    ) -> BoxFuture<'a, Result<DayCollectionReceipt, DayError>> {
        unexpected()
    }
    fn read_items<'a>(
        &'a self,
        _query: DayReadQuery,
    ) -> BoxFuture<'a, Result<Vec<TimelineItem>, DayError>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn calendar_mirror<'a>(
        &'a self,
        _person_id: PersonId,
    ) -> BoxFuture<'a, Result<Option<CalendarMirror>, DayError>> {
        Box::pin(async { Ok(None) })
    }
}

struct UnusedAcquisition;
impl floe_day::CalendarAcquisitionPort for UnusedAcquisition {
    fn inspect_sources<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_day::CalendarCacheInspection, CalendarRefreshError>> {
        unexpected()
    }
    fn acquire<'a>(
        &'a self,
        _request: CalendarRefreshRequest,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<CalendarAcquisition, CalendarRefreshError>> {
        unexpected()
    }
}

fn service(repository: Arc<BarrierRepository>, actor: OwnerActor) -> CalendarOperationsService {
    let day = Arc::new(DayService::new(
        Arc::new(UnusedDay),
        Arc::new(UnusedAcquisition),
        Arc::new(floe_day::SystemDayClock),
    ));
    CalendarOperationsService::new(
        actor,
        CalendarOperationsDependencies {
            repository,
            sources: Arc::new(UnusedSources),
            proposals: Arc::new(UnusedProposals),
            day,
            executor: Arc::new(UnusedExecutor),
            clock: Arc::new(SystemCalendarOperationsClock),
        },
    )
    .expect("construct test Actions owner")
}

fn scope_for(timeout: Duration) -> floe_execution::ExecutionScope {
    scope_with(floe_execution::Cancellation::new(), timeout)
}

fn scope_with(
    cancellation: floe_execution::Cancellation,
    timeout: Duration,
) -> floe_execution::ExecutionScope {
    let ledger = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(100, 100),
        floe_execution::budget::ModelUsage::default(),
    );
    floe_execution::ExecutionScope::root(
        cancellation,
        Instant::now() + timeout,
        ledger.work_lease(),
        TraceContext::new(Uuid::new_v4()),
    )
}

fn scope() -> floe_execution::ExecutionScope {
    scope_for(Duration::from_secs(30))
}

#[tokio::test]
async fn authority_and_submit_serialize_one_command_id_before_submit_validation() {
    let repository = Arc::new(BarrierRepository::new());
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "device-a".to_owned(),
        runtime_epoch: 1,
    };
    let service = service(Arc::clone(&repository), actor.clone());
    let command_id = Uuid::new_v4();
    let authority_started = repository.authority_started.notified();
    tokio::pin!(authority_started);

    let first_service = service.clone();
    let first_actor = actor.clone();
    let first_scope = scope();
    let first = tokio::spawn(async move {
        first_service
            .set_calendar_create_authority(
                &first_actor,
                command_id,
                OperationPolicyMode::Deny,
                1,
                &first_scope,
            )
            .await
    });
    authority_started.await;

    let second_service = service.clone();
    let second_actor = actor.clone();
    let second_scope = scope();
    let now = chrono::Utc::now();
    let mut second = Box::pin(
        second_service.submit(
            &second_actor,
            command_id,
            ActionIntent::DirectCreate {
                destination_ref: Uuid::nil(),
                title: "Changed body".to_owned(),
                schedule: floe_day::TimedSchedule::new(
                    now,
                    now + chrono::Duration::minutes(30),
                    "UTC",
                )
                .expect("valid schedule for invalid destination intent"),
            },
            &second_scope,
        ),
    );
    let first_poll = poll_fn(|context| Poll::Ready(Future::poll(second.as_mut(), context))).await;
    assert!(
        first_poll.is_pending(),
        "same-ID submit must stay pending while authority admission is unsettled"
    );

    assert_eq!(
        repository.find_calls.load(Ordering::Acquire),
        0,
        "same-ID submit must wait for authority receipt before replay lookup"
    );
    repository.authority_release.add_permits(1);
    let authority = first
        .await
        .expect("authority task")
        .expect("authority admitted");
    assert_eq!(authority.revision, 2);
    let submit = second.await;
    assert_eq!(
        submit,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict)),
        "cross-family reuse must retain the occupied command ID"
    );
    assert_eq!(repository.find_calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn waiting_for_same_command_lock_respects_the_callers_deadline() {
    let repository = Arc::new(BarrierRepository::new());
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "device-a".to_owned(),
        runtime_epoch: 1,
    };
    let service = service(Arc::clone(&repository), actor.clone());
    let command_id = Uuid::new_v4();
    let authority_started = repository.authority_started.notified();
    tokio::pin!(authority_started);

    let first_service = service.clone();
    let first_actor = actor.clone();
    let first_scope = scope();
    let first = tokio::spawn(async move {
        first_service
            .set_calendar_create_authority(
                &first_actor,
                command_id,
                OperationPolicyMode::Deny,
                1,
                &first_scope,
            )
            .await
    });
    authority_started.await;

    let (second_started_tx, second_started_rx) = oneshot::channel();
    let second_service = service.clone();
    let second_actor = actor.clone();
    let second_scope = scope_for(Duration::from_millis(100));
    let second = tokio::spawn(async move {
        second_started_tx
            .send(())
            .expect("test is waiting for submit to start");
        let now = chrono::Utc::now();
        second_service
            .submit(
                &second_actor,
                command_id,
                ActionIntent::DirectCreate {
                    destination_ref: Uuid::nil(),
                    title: "Changed body".to_owned(),
                    schedule: floe_day::TimedSchedule::new(
                        now,
                        now + chrono::Duration::minutes(30),
                        "UTC",
                    )
                    .expect("valid schedule for invalid destination intent"),
                },
                &second_scope,
            )
            .await
    });
    second_started_rx.await.expect("submit task started");
    assert_eq!(
        second.await.expect("deadline-bounded submit task"),
        Err(CommandFailure::Indeterminate(
            AgentFailure::DeadlineExceeded
        ))
    );
    assert_eq!(repository.find_calls.load(Ordering::Acquire), 0);

    repository.authority_release.add_permits(1);
    first
        .await
        .expect("authority task")
        .expect("authority admitted");
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
struct SubmissionSources {
    source: SourceConnection,
    failure: Option<AgentFailure>,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl OperationSourceReader for SubmissionSources {
    fn list_calendar_sources<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, AgentFailure>> {
        Box::pin(async move {
            if let Some(reason) = self.failure {
                return Err(reason);
            }
            Ok((self.source.person_id() == person_id)
                .then(|| self.source.clone())
                .into_iter()
                .collect())
        })
    }

    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<Option<SourceConnection>, AgentFailure>> {
        Box::pin(async move {
            Ok((self.source.person_id() == person_id
                && self.source.connection_id() == connection_id)
                .then(|| self.source.clone()))
        })
    }

    fn source_is_fenced<'a>(
        &'a self,
        _person_id: PersonId,
        _connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async { Ok(false) })
    }

    fn read_reservation_fence<'a>(
        &'a self,
        _person_id: PersonId,
        _connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<floe_connections::SourceReservationFence, AgentFailure>> {
        Box::pin(async {
            Ok(floe_connections::SourceReservationFence {
                watermark: floe_connections::SourceReservationWatermark::NeverReserved,
                fenced: false,
            })
        })
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
struct SubmissionExecutor {
    calendar_id: String,
    deny_destinations: AtomicBool,
    prepare_calls: AtomicUsize,
    prepare_started: Notify,
    prepare_release: Semaphore,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl CalendarOperationExecutor for SubmissionExecutor {
    fn destinations<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _source: &'a ActionSourceFence,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<CalendarDestinationObservation>, ActionBlockedReason>> {
        Box::pin(async move {
            if self.deny_destinations.load(Ordering::Acquire) {
                return Err(ActionBlockedReason::PermissionDenied);
            }
            Ok(vec![CalendarDestinationObservation {
                calendar_id: self.calendar_id.clone(),
                calendar_name: "QA Calendar".to_owned(),
                can_modify: true,
            }])
        })
    }

    fn prepare<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _record: &'a ActionRecord,
        _dependencies: &'a [ActionDependencySourceFence],
        _local_events: &'a [floe_day::Event],
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedCalendarEffect>, ActionBlockedReason>> {
        Box::pin(async move {
            self.prepare_calls.fetch_add(1, Ordering::AcqRel);
            self.prepare_started.notify_one();
            self.prepare_release
                .acquire()
                .await
                .map_err(|_| ActionBlockedReason::ExecutorUnavailable)?
                .forget();
            Err(ActionBlockedReason::ScheduleConflict)
        })
    }

    fn recover<'a>(
        &'a self,
        _actor: &'a OwnerActor,
        _intent: &'a ExecutionIntent,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, CalendarEffectOutcome> {
        unexpected()
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
fn fixture_calendar_source(actor: &OwnerActor) -> SourceConnection {
    let connector = ConnectorId::try_new("calendar.fixture").expect("valid fixture connector");
    let connection = ConnectionId::try_new("calendar.fixture.local").expect("valid connection");
    let owner = ExecutionOwnerId::try_new(floe_access::fixture_calendar_execution_owner(
        &actor.device_id,
    ))
    .expect("valid fixture execution owner");
    let resource = ConnectionResource::new(
        ResourceHandle::try_new("calendar-1").expect("valid calendar resource"),
        "QA Calendar".to_owned(),
        None,
    )
    .expect("valid fixture calendar resource");
    SourceConnection::establish_reviewed_native(
        actor.person_id,
        connector,
        connection,
        owner,
        ResourceMode::Selected,
        vec![resource],
        "a".repeat(64),
    )
    .expect("valid synthetic Calendar source")
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
async fn submission_service(
    repository: Arc<BarrierRepository>,
    actor: OwnerActor,
    source: SourceConnection,
    executor: Arc<SubmissionExecutor>,
) -> CalendarOperationsService {
    let day = Arc::new(DayService::new(
        Arc::new(UnusedDay),
        Arc::new(UnusedAcquisition),
        Arc::new(floe_day::SystemDayClock),
    ));
    day.activate(&actor, &scope())
        .await
        .expect("activate synthetic Day owner");
    CalendarOperationsService::new(
        actor,
        CalendarOperationsDependencies {
            repository,
            sources: Arc::new(SubmissionSources {
                source,
                failure: None,
            }),
            proposals: Arc::new(UnusedProposals),
            day,
            executor,
            clock: Arc::new(SystemCalendarOperationsClock),
        },
    )
    .expect("construct test Calendar Operations owner")
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[derive(Clone, Copy)]
enum AdmissionBoundary {
    Shutdown,
    Cancelled,
    Deadline,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[tokio::test]
async fn committed_submit_failures_keep_admitted_disposition_and_replay_same_id() {
    for boundary in [
        AdmissionBoundary::Shutdown,
        AdmissionBoundary::Cancelled,
        AdmissionBoundary::Deadline,
    ] {
        let repository = Arc::new(BarrierRepository::new());
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "device-a".to_owned(),
            runtime_epoch: 1,
        };
        let source = fixture_calendar_source(&actor);
        let executor = Arc::new(SubmissionExecutor {
            calendar_id: "calendar-1".to_owned(),
            deny_destinations: AtomicBool::new(false),
            prepare_calls: AtomicUsize::new(0),
            prepare_started: Notify::new(),
            prepare_release: Semaphore::new(0),
        });
        let initial_service = submission_service(
            Arc::clone(&repository),
            actor.clone(),
            source.clone(),
            Arc::clone(&executor),
        )
        .await;
        let destination_ref = initial_service
            .destinations(&actor, &scope())
            .await
            .expect("synthetic destination")
            .into_iter()
            .next()
            .expect("one writable destination")
            .destination_ref;
        let command_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let intent = ActionIntent::DirectCreate {
            destination_ref,
            title: "Boundary test".to_owned(),
            schedule: floe_day::TimedSchedule::new(
                now + chrono::Duration::hours(2),
                now + chrono::Duration::hours(2) + chrono::Duration::minutes(30),
                "UTC",
            )
            .expect("valid synthetic schedule"),
        };
        let cancellation = floe_execution::Cancellation::new();
        let timeout = if matches!(boundary, AdmissionBoundary::Deadline) {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(30)
        };
        let first_scope = scope_with(cancellation.clone(), timeout);
        let committed = repository.admission_committed.notified();
        tokio::pin!(committed);
        let submit_service = initial_service.clone();
        let submit_actor = actor.clone();
        let submit_intent = intent.clone();
        let submit_scope = first_scope.clone();
        let first_submit = tokio::spawn(async move {
            submit_service
                .submit(&submit_actor, command_id, submit_intent, &submit_scope)
                .await
        });
        committed.await;

        let expected_failure = match boundary {
            AdmissionBoundary::Shutdown => {
                initial_service.shutdown();
                AgentFailure::VaultUnavailable
            }
            AdmissionBoundary::Cancelled => {
                cancellation.cancel();
                AgentFailure::Cancelled
            }
            AdmissionBoundary::Deadline => {
                tokio::time::sleep_until(first_scope.deadline()).await;
                AgentFailure::DeadlineExceeded
            }
        };
        repository.admission_release.add_permits(1);
        assert_eq!(
            first_submit.await.expect("first submit task"),
            Err(CommandFailure::Admitted(expected_failure)),
            "a committed operation must retain its same-ID replay disposition"
        );
        assert_eq!(repository.admitted.load(Ordering::Acquire), 1);

        let replay_service = submission_service(
            Arc::clone(&repository),
            actor.clone(),
            source,
            Arc::clone(&executor),
        )
        .await;
        let executor_started = executor.prepare_started.notified();
        tokio::pin!(executor_started);
        let replay = replay_service
            .submit(&actor, command_id, intent.clone(), &scope())
            .await
            .expect("same-ID replay under a fresh scope");
        executor_started.await;
        let second_replay = replay_service
            .submit(&actor, command_id, intent, &scope())
            .await
            .expect("concurrent same-ID replay");
        assert_eq!(second_replay.action_ref, replay.action_ref);
        assert_eq!(repository.admitted.load(Ordering::Acquire), 1);
        assert_eq!(
            executor.prepare_calls.load(Ordering::Acquire),
            1,
            "concurrent replay must join the existing operation job"
        );

        let stopped = repository.pre_dispatch_stopped.notified();
        tokio::pin!(stopped);
        executor.prepare_release.add_permits(1);
        stopped.await;
        replay_service
            .shutdown_and_drain(&scope())
            .await
            .expect("drain the synthetic pre-dispatch job");
        assert_eq!(executor.prepare_calls.load(Ordering::Acquire), 1);
        assert_eq!(repository.pre_dispatch_stops.load(Ordering::Acquire), 1);
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[tokio::test]
async fn rejected_same_id_delivery_excludes_a_delayed_prepared_admission() {
    let repository = Arc::new(BarrierRepository::new());
    repository
        .hold_before_admission
        .store(true, Ordering::Release);
    repository.admission_release.add_permits(1);
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "device-a".into(),
        runtime_epoch: 1,
    };
    let source = fixture_calendar_source(&actor);
    let executor = Arc::new(SubmissionExecutor {
        calendar_id: "calendar-1".into(),
        deny_destinations: AtomicBool::new(false),
        prepare_calls: AtomicUsize::new(0),
        prepare_started: Notify::new(),
        prepare_release: Semaphore::new(0),
    });
    let service =
        submission_service(repository.clone(), actor.clone(), source, executor.clone()).await;
    let destination_ref = service.destinations(&actor, &scope()).await.unwrap()[0].destination_ref;
    let now = chrono::Utc::now();
    let intent = ActionIntent::DirectCreate {
        destination_ref,
        title: "One exact command".into(),
        schedule: floe_day::TimedSchedule::new(
            now + chrono::Duration::hours(2),
            now + chrono::Duration::hours(3),
            "UTC",
        )
        .unwrap(),
    };
    let command_id = Uuid::new_v4();
    let cancellation = floe_execution::Cancellation::new();
    let first_scope = scope_with(cancellation.clone(), Duration::from_secs(30));
    let prepared = repository.admission_started.notified();
    tokio::pin!(prepared);
    let (first_service, first_actor, first_intent) =
        (service.clone(), actor.clone(), intent.clone());
    let first = tokio::spawn(async move {
        first_service
            .submit(&first_actor, command_id, first_intent, &first_scope)
            .await
    });
    prepared.await;
    executor.deny_destinations.store(true, Ordering::Release);
    let second = service.submit(&actor, command_id, intent, &scope()).await;
    assert!(
        matches!(second, Err(CommandFailure::NotApplied(_))),
        "a durable rejection is the second delivery's requested terminal outcome: {second:?}"
    );
    cancellation.cancel();
    repository.admission_before_release.add_permits(1);
    let delayed = first.await.unwrap();
    assert_eq!(
        repository.admitted.load(Ordering::Acquire),
        0,
        "a command reported NotApplied must exclude the delayed admission; delayed outcome: {delayed:?}"
    );
    assert!(matches!(delayed, Err(CommandFailure::NotApplied(_))));
    assert_eq!(executor.prepare_calls.load(Ordering::Acquire), 0);
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[tokio::test]
async fn transient_preflight_failure_keeps_the_exact_submission_retryable() {
    let repository = Arc::new(BarrierRepository::new());
    repository.admission_release.add_permits(1);
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "device-a".into(),
        runtime_epoch: 1,
    };
    let source = fixture_calendar_source(&actor);
    let executor = Arc::new(SubmissionExecutor {
        calendar_id: "calendar-1".into(),
        deny_destinations: AtomicBool::new(false),
        prepare_calls: AtomicUsize::new(0),
        prepare_started: Notify::new(),
        prepare_release: Semaphore::new(0),
    });
    let mut service = submission_service(
        repository.clone(),
        actor.clone(),
        source.clone(),
        executor.clone(),
    )
    .await;
    let destination_ref = service.destinations(&actor, &scope()).await.unwrap()[0].destination_ref;
    let now = chrono::Utc::now();
    let intent = ActionIntent::DirectCreate {
        destination_ref,
        title: "Retry exact command".into(),
        schedule: floe_day::TimedSchedule::new(
            now + chrono::Duration::hours(2),
            now + chrono::Duration::hours(3),
            "UTC",
        )
        .unwrap(),
    };
    let command_id = Uuid::new_v4();
    for reason in [
        AgentFailure::StorageBusy,
        AgentFailure::StorageUnavailable,
        AgentFailure::VaultLocked,
        AgentFailure::VaultUnavailable,
        AgentFailure::Cancelled,
        AgentFailure::DeadlineExceeded,
        AgentFailure::Interrupted,
    ] {
        service.sources = Arc::new(SubmissionSources {
            source: source.clone(),
            failure: Some(reason),
        });
        assert!(
            matches!(service.submit(&actor, command_id, intent.clone(), &scope()).await, Err(CommandFailure::Indeterminate(failure)) if failure == reason)
        );
        assert!(repository.rejections.lock().unwrap().is_empty());
    }
    service.sources = Arc::new(SubmissionSources {
        source,
        failure: None,
    });
    service
        .submit(&actor, command_id, intent, &scope())
        .await
        .unwrap();
    executor.prepare_started.notified().await;
    executor.prepare_release.add_permits(1);
    repository.pre_dispatch_stopped.notified().await;
    service.shutdown_and_drain(&scope()).await.unwrap();
    assert_eq!(repository.admitted.load(Ordering::Acquire), 1);
    assert_eq!(executor.prepare_calls.load(Ordering::Acquire), 1);
}
