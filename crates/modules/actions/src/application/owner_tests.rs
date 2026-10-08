use super::{ActionsDependencies, ActionsService};
use crate::*;
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
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
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
    find_calls: AtomicUsize,
    authority_started: Notify,
    authority_release: Semaphore,
}

impl BarrierRepository {
    fn new() -> Self {
        Self {
            receipts: StdMutex::new(HashMap::new()),
            find_calls: AtomicUsize::new(0),
            authority_started: Notify::new(),
            authority_release: Semaphore::new(0),
        }
    }
}

impl ActionsRepository for BarrierRepository {
    fn validate_proposal_coverage<'a>(
        &'a self,
        _person_id: PersonId,
        _coverage: &'a floe_agent_contract::DependencyCoverage,
    ) -> BoxFuture<'a, Result<(), ActionStoreError>> {
        unexpected()
    }

    fn get<'a>(
        &'a self,
        _person_id: PersonId,
        _action_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, ActionStoreError>> {
        unexpected()
    }

    fn list<'a>(
        &'a self,
        _person_id: PersonId,
        _cursor: Option<Uuid>,
        _limit: u16,
    ) -> BoxFuture<'a, Result<ActionPage, ActionStoreError>> {
        unexpected()
    }

    fn find_admission<'a>(
        &'a self,
        _person_id: PersonId,
        command_id: Uuid,
        _request_digest: ActionDigest,
    ) -> BoxFuture<'a, Result<Option<ActionRecord>, ActionStoreError>> {
        Box::pin(async move {
            self.find_calls.fetch_add(1, Ordering::AcqRel);
            let occupied = self
                .receipts
                .lock()
                .map_err(|_| ActionStoreError::Unavailable)?
                .contains_key(&command_id);
            if occupied {
                Err(ActionStoreError::Conflict)
            } else {
                Ok(None)
            }
        })
    }

    fn admit<'a>(
        &'a self,
        _admission: ActionAdmission,
    ) -> BoxFuture<'a, Result<AdmittedAction, CommandFailure<ActionStoreError>>> {
        unexpected()
    }

    fn record_decision<'a>(
        &'a self,
        _decision: ActionDecision,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<ActionStoreError>>> {
        unexpected()
    }

    fn admit_reconciliation<'a>(
        &'a self,
        _command: ActionReconciliation,
    ) -> BoxFuture<'a, Result<ActionRecord, CommandFailure<ActionStoreError>>> {
        unexpected()
    }

    fn stop_before_dispatch<'a>(
        &'a self,
        _stop: PreDispatchStop,
    ) -> BoxFuture<'a, Result<ActionRecord, ActionStoreError>> {
        unexpected()
    }

    fn prepare_dispatch<'a>(
        &'a self,
        _intent: DispatchIntent,
    ) -> BoxFuture<'a, Result<DispatchAdmission, ActionStoreError>> {
        unexpected()
    }

    fn load_execution<'a>(
        &'a self,
        _person_id: PersonId,
        _execution_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<DispatchAdmission>, ActionStoreError>> {
        unexpected()
    }

    fn settle_execution<'a>(
        &'a self,
        _settlement: ExecutionSettlement,
    ) -> BoxFuture<'a, Result<ActionRecord, ActionStoreError>> {
        unexpected()
    }

    fn pending_recovery<'a>(
        &'a self,
        _person_id: PersonId,
        _cursor: Option<Uuid>,
        _limit: u16,
    ) -> BoxFuture<'a, Result<RecoveryPage, ActionStoreError>> {
        unexpected()
    }

    fn ack_collection<'a>(
        &'a self,
        _ack: CollectionAck,
    ) -> BoxFuture<'a, Result<CollectionTicket, ActionStoreError>> {
        unexpected()
    }

    fn read_authority<'a>(
        &'a self,
        _person_id: PersonId,
    ) -> BoxFuture<'a, Result<ActionsAuthority, ActionStoreError>> {
        unexpected()
    }

    fn compare_and_set_authority<'a>(
        &'a self,
        change: AuthorityChange,
    ) -> BoxFuture<'a, Result<ActionsAuthority, CommandFailure<ActionStoreError>>> {
        Box::pin(async move {
            self.authority_started.notify_one();
            self.authority_release
                .acquire()
                .await
                .map_err(|_| CommandFailure::Indeterminate(ActionStoreError::Unavailable))?
                .forget();
            self.receipts
                .lock()
                .map_err(|_| CommandFailure::Indeterminate(ActionStoreError::Unavailable))?
                .insert(change.command_id, "authority");
            Ok(ActionsAuthority {
                person_id: change.person_id,
                revision: change.expected_revision + 1,
                calendar_create: change.mode,
            })
        })
    }
}

struct UnusedSources;
impl ActionSourceReader for UnusedSources {
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
impl ActionCalendarExecutor for UnusedExecutor {
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
    ) -> BoxFuture<'a, Result<Vec<RefreshRecord>, DayError>> {
        unexpected()
    }
    fn retire_refresh_executor<'a>(
        &'a self,
        _expected: RefreshExecutorReplacement,
    ) -> BoxFuture<'a, Result<(), DayError>> {
        unexpected()
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
        unexpected()
    }
    fn calendar_mirror<'a>(
        &'a self,
        _person_id: PersonId,
    ) -> BoxFuture<'a, Result<Option<CalendarMirror>, DayError>> {
        unexpected()
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

fn service(repository: Arc<BarrierRepository>, actor: OwnerActor) -> ActionsService {
    let day = Arc::new(DayService::new(
        Arc::new(UnusedDay),
        Arc::new(UnusedAcquisition),
        Arc::new(floe_day::SystemDayClock),
    ));
    ActionsService::new(
        actor,
        ActionsDependencies {
            repository,
            sources: Arc::new(UnusedSources),
            proposals: Arc::new(UnusedProposals),
            day,
            executor: Arc::new(UnusedExecutor),
            clock: Arc::new(SystemActionsClock),
        },
    )
    .expect("construct test Actions owner")
}

fn scope_for(timeout: Duration) -> floe_execution::ExecutionScope {
    let ledger = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(100, 100),
        floe_execution::budget::ModelUsage::default(),
    );
    floe_execution::ExecutionScope::root(
        floe_execution::Cancellation::new(),
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
                ActionAuthorityMode::Deny,
                1,
                &first_scope,
            )
            .await
    });
    authority_started.await;

    let (second_started_tx, second_started_rx) = oneshot::channel();
    let second_service = service.clone();
    let second_actor = actor.clone();
    let second_scope = scope();
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
    tokio::task::yield_now().await;

    assert_eq!(
        repository.find_calls.load(Ordering::Acquire),
        0,
        "same-ID submit must wait for authority receipt before replay lookup"
    );
    assert!(
        !second.is_finished(),
        "same-ID submit returned a precommit validation result during authority admission"
    );

    repository.authority_release.add_permits(1);
    let authority = first
        .await
        .expect("authority task")
        .expect("authority admitted");
    assert_eq!(authority.revision, 2);
    let submit = second.await.expect("submit task");
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
                ActionAuthorityMode::Deny,
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
