//! Actual encrypted product-store capacity/recovery checks in the development profile.
use crate::{ProductStoreIdentity, TursoStore};
use chrono::Utc;
use floe_day::*;
use floe_execution::{
    BoxFuture, Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{OwnerActor, PersonId, TraceContext};
use std::os::unix::fs::DirBuilderExt;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

struct TestRoot(std::path::PathBuf);
impl TestRoot {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("floe-day-capacity-{}", Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        Self(root)
    }
}
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct NoAcquisition;
impl CalendarAcquisitionPort for NoAcquisition {
    fn inspect_sources<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CalendarCacheInspection, CalendarRefreshError>> {
        Box::pin(async { panic!("local receipt test must not inspect a provider") })
    }
    fn acquire<'a>(
        &'a self,
        _: CalendarRefreshRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CalendarAcquisition, CalendarRefreshError>> {
        Box::pin(async { panic!("local receipt test must not call a provider") })
    }
}
fn scope() -> ExecutionScope {
    ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + Duration::from_secs(300),
        BudgetLedger::new(BudgetConfig::new(0, 0), ModelUsage::default()).root_lease(),
        TraceContext::new(Uuid::new_v4()),
    )
}
fn query() -> DayQuery {
    let now = Utc::now();
    DayQuery {
        date: now.date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
        now,
    }
}
fn note(day: &DayQuery, text: &str) -> DayMutationRequest {
    DayMutationRequest {
        command_id: Uuid::new_v4(),
        day: day.clone(),
        mutation: DayMutation::CreateNote {
            content: text.into(),
            occurred_at: day.now,
        },
    }
}
async fn fixture() -> (TestRoot, Arc<TursoStore>, DayService, OwnerActor) {
    let root = TestRoot::new();
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "day-capacity-device".into(),
        runtime_epoch: 1,
    };
    let identity = ProductStoreIdentity::new(actor.person_id, actor.device_id.clone()).unwrap();
    let store = Arc::new(
        TursoStore::create_new(root.0.join("product.db"), identity)
            .await
            .unwrap(),
    );
    let service = DayService::new(
        store.clone(),
        Arc::new(NoAcquisition),
        Arc::new(SystemDayClock),
    );
    service.activate(&actor, &scope()).await.unwrap();
    (root, store, service, actor)
}
async fn generation(store: &TursoStore, actor: &OwnerActor) -> Uuid {
    let connection = store.connection().await.unwrap();
    let mut rows = connection
        .query(
            "SELECT executor_generation FROM day_executors WHERE person_id=? AND device_id=?",
            (actor.person_id.to_string(), actor.device_id.clone()),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    Uuid::parse_str(&row.get::<String>(0).unwrap()).unwrap()
}
async fn seed_terminal_refreshes(
    store: &TursoStore,
    actor: &OwnerActor,
    generation: Uuid,
    day: &DayQuery,
    count: usize,
    completed: Option<&DaySnapshot>,
) -> RefreshRecord {
    let connection = store.connection().await.unwrap();
    connection.execute("BEGIN IMMEDIATE", ()).await.unwrap();
    let mut last = None;
    for _ in 0..count {
        let admission = RefreshAdmission {
            operation_id: Uuid::new_v4(),
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            command_id: Uuid::new_v4(),
            query: day.clone(),
            executor_generation: generation,
            admitted_at: day.now,
        };
        let pending = admission
            .record(
                day.refresh_intent_digest(actor.person_id, &actor.device_id, admission.command_id)
                    .unwrap(),
                MirrorExpectation::Absent,
            )
            .unwrap();
        let record = match completed {
            Some(snapshot) => pending
                .transition(DayRefreshState::Running, day.now)
                .unwrap()
                .transition(
                    DayRefreshState::Completed {
                        day: snapshot.clone(),
                    },
                    day.now,
                )
                .unwrap(),
            None => pending
                .transition(
                    DayRefreshState::Interrupted {
                        failure: DayRefreshFailure::HostInterrupted,
                    },
                    day.now,
                )
                .unwrap(),
        };
        connection.execute("INSERT INTO day_refreshes(operation_id,person_id,device_id,command_id,executor_generation,revision,payload,is_terminal) VALUES(?,?,?,?,?,?,?,?)", (record.operation_id.to_string(),record.person_id.to_string(),record.device_id.clone(),record.command_id.to_string(),record.executor_generation.to_string(),record.revision.0 as i64,serde_json::to_string(&record).unwrap(),i64::from(record.state.terminal()))).await.unwrap();
        last = Some(record);
    }
    connection.execute("COMMIT", ()).await.unwrap();
    last.unwrap()
}

#[tokio::test]
async fn day_capacity_terminal_receipts_do_not_block_new_commands_or_exact_replay() {
    let (root, store, service, actor) = fixture().await;
    let day = query();
    let original = note(&day, "original note");
    let receipt = service
        .mutate(&actor, original.clone(), &scope())
        .await
        .unwrap();
    let generation = generation(&store, &actor).await;
    let retained = seed_terminal_refreshes(&store, &actor, generation, &day, 4095, None).await;
    let admission = RefreshAdmission {
        operation_id: Uuid::new_v4(),
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        command_id: Uuid::new_v4(),
        query: day.clone(),
        executor_generation: generation,
        admitted_at: day.now,
    };
    let refresh = store.admit_refresh(admission).await;
    let mutation = service
        .mutate(&actor, note(&day, "after the old lifetime cap"), &scope())
        .await;
    assert_eq!(
        service
            .mutate(&actor, original.clone(), &scope())
            .await
            .unwrap(),
        receipt
    );
    let stored = store
        .read_refresh(RefreshLookup {
            operation_id: retained.operation_id,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored, retained);
    assert!(
        refresh.is_ok() && mutation.is_ok(),
        "terminal receipts must not exhaust future work: refresh={refresh:?}, mutation={mutation:?}"
    );
    service.shutdown(&scope()).await.unwrap();
    drop(service);
    drop(store);
    let reopened = Arc::new(
        TursoStore::open_existing(
            root.0.join("product.db"),
            ProductStoreIdentity::new(actor.person_id, actor.device_id.clone()).unwrap(),
        )
        .await
        .unwrap(),
    );
    let next = DayService::new(
        reopened.clone(),
        Arc::new(NoAcquisition),
        Arc::new(SystemDayClock),
    );
    next.activate(&actor, &scope()).await.unwrap();
    assert_eq!(
        next.mutate(&actor, original, &scope()).await.unwrap(),
        receipt
    );
    assert!(
        next.mutate(&actor, note(&day, "after reopen"), &scope())
            .await
            .is_ok()
    );
    next.shutdown(&scope()).await.unwrap();
}

#[tokio::test]
async fn day_capacity_executor_lifecycle_does_not_hydrate_completed_snapshots() {
    let (_root, store, service, actor) = fixture().await;
    let day = query();
    let generation = generation(&store, &actor).await;
    let notes = (0..64)
        .map(|_| {
            Note::new(
                actor.person_id,
                "n".repeat(4_000),
                SourceRef::Manual,
                day.now,
            )
            .unwrap()
        })
        .collect();
    let snapshot =
        project_day(actor.person_id, day.date, 0, day.now, vec![], vec![], notes).unwrap();
    let retained =
        seed_terminal_refreshes(&store, &actor, generation, &day, 128, Some(&snapshot)).await;
    let before = super::day_refresh::refresh_decode_count();
    service.shutdown(&scope()).await.unwrap();
    let retired_decodes = super::day_refresh::refresh_decode_count() - before;
    let before = super::day_refresh::refresh_decode_count();
    store
        .interrupt_refreshes(RefreshExecutorReplacement {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            executor_generation: Uuid::new_v4(),
            now: Utc::now(),
        })
        .await
        .unwrap();
    let activation_decodes = super::day_refresh::refresh_decode_count() - before;
    assert_eq!(
        (retired_decodes, activation_decodes),
        (0, 0),
        "terminal receipts are point-read replay data, not active recovery work"
    );
    assert_eq!(
        store
            .read_refresh(RefreshLookup {
                operation_id: retained.operation_id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone()
            })
            .await
            .unwrap()
            .unwrap(),
        retained
    );
}

fn admission(actor: &OwnerActor, generation: Uuid, day: &DayQuery) -> RefreshAdmission {
    RefreshAdmission {
        operation_id: Uuid::new_v4(),
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        command_id: Uuid::new_v4(),
        query: day.clone(),
        executor_generation: generation,
        admitted_at: day.now,
    }
}
async fn seed_pending(
    store: &TursoStore,
    actor: &OwnerActor,
    generation: Uuid,
    day: &DayQuery,
    count: usize,
) -> Vec<RefreshRecord> {
    let connection = store.connection().await.unwrap();
    connection.execute("BEGIN IMMEDIATE", ()).await.unwrap();
    let mut records = Vec::new();
    for _ in 0..count {
        let request = admission(actor, generation, day);
        let record = request
            .record(
                day.refresh_intent_digest(actor.person_id, &actor.device_id, request.command_id)
                    .unwrap(),
                MirrorExpectation::Absent,
            )
            .unwrap();
        connection.execute("INSERT INTO day_refreshes(operation_id,person_id,device_id,command_id,executor_generation,revision,payload,is_terminal) VALUES(?,?,?,?,?,?,?,0)", (record.operation_id.to_string(),record.person_id.to_string(),record.device_id.clone(),record.command_id.to_string(),record.executor_generation.to_string(),record.revision.0 as i64,serde_json::to_string(&record).unwrap())).await.unwrap();
        records.push(record);
    }
    connection.execute("COMMIT", ()).await.unwrap();
    records
}
async fn read(store: &TursoStore, record: &RefreshRecord) -> RefreshRecord {
    store
        .read_refresh(RefreshLookup {
            operation_id: record.operation_id,
            person_id: record.person_id,
            device_id: record.device_id.clone(),
        })
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn day_capacity_only_active_refreshes_consume_admission_slots() {
    let (_root, store, service, actor) = fixture().await;
    let day = query();
    let generation = generation(&store, &actor).await;
    let records = seed_pending(&store, &actor, generation, &day, MAX_ACTIVE_DAY_REFRESHES).await;
    let request = admission(&actor, generation, &day);
    assert!(
        matches!(store.admit_refresh(request.clone()).await, Err(floe_kernel::CommandFailure::NotApplied(error)) if error.metadata.get("reason_code").map(String::as_str)==Some("budget_exceeded"))
    );
    assert!(
        service
            .mutate(
                &actor,
                note(&day, "manual edits do not consume refresh slots"),
                &scope()
            )
            .await
            .is_ok()
    );
    let previous = records[0].clone();
    let next = previous
        .transition(
            DayRefreshState::Failed {
                failure: DayRefreshFailure::HostInterrupted,
            },
            day.now,
        )
        .unwrap();
    store
        .transition_refresh(RefreshTransition { previous, next })
        .await
        .unwrap();
    assert!(matches!(
        store.admit_refresh(request).await.unwrap(),
        RefreshAdmissionResult::New(_)
    ));
    service.shutdown(&scope()).await.unwrap();
}

#[tokio::test]
async fn day_capacity_recovery_pages_pending_work_and_preserves_generation_fences() {
    let (_root, store, service, actor) = fixture().await;
    let day = query();
    let old_generation = generation(&store, &actor).await;
    let records = seed_pending(&store, &actor, old_generation, &day, 130).await;
    let new_generation = Uuid::new_v4();
    let current = seed_pending(&store, &actor, new_generation, &day, 1)
        .await
        .remove(0);
    let other_actor = OwnerActor {
        device_id: "other-device".into(),
        ..actor.clone()
    };
    let other = seed_pending(&store, &other_actor, old_generation, &day, 1)
        .await
        .remove(0);
    let before = super::day_refresh::refresh_decode_count();
    store
        .interrupt_refreshes(RefreshExecutorReplacement {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            executor_generation: new_generation,
            now: day.now,
        })
        .await
        .unwrap();
    assert_eq!(super::day_refresh::refresh_decode_count() - before, 130);
    for record in &records {
        assert!(matches!(
            read(&store, record).await.state,
            DayRefreshState::Interrupted { .. }
        ));
    }
    assert_eq!(read(&store, &current).await, current);
    assert_eq!(read(&store, &other).await, other);
    let late = records[0]
        .transition(DayRefreshState::Running, day.now)
        .unwrap();
    assert!(
        store
            .transition_refresh(RefreshTransition {
                previous: records[0].clone(),
                next: late
            })
            .await
            .is_err()
    );
    service.shutdown(&scope()).await.unwrap();
    assert_eq!(generation(&store, &actor).await, new_generation);
    assert_eq!(
        read(&store, &current).await,
        current,
        "old owner shutdown must not retire the new generation"
    );
    store
        .retire_refresh_executor(RefreshExecutorReplacement {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            executor_generation: new_generation,
            now: day.now,
        })
        .await
        .unwrap();
    assert!(matches!(
        read(&store, &current).await.state,
        DayRefreshState::Interrupted { .. }
    ));
    assert_eq!(read(&store, &other).await, other);
}

#[tokio::test]
async fn day_capacity_corrupt_pending_page_rolls_back_executor_and_prior_pages() {
    let (_root, store, _service, actor) = fixture().await;
    let day = query();
    let old_generation = generation(&store, &actor).await;
    let mut records = seed_pending(&store, &actor, old_generation, &day, 130).await;
    records.sort_by_key(|record| record.operation_id.to_string());
    let connection = store.connection().await.unwrap();
    connection
        .execute(
            "UPDATE day_refreshes SET payload='invalid' WHERE operation_id=?",
            (records.last().unwrap().operation_id.to_string(),),
        )
        .await
        .unwrap();
    let result = store
        .interrupt_refreshes(RefreshExecutorReplacement {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            executor_generation: Uuid::new_v4(),
            now: day.now,
        })
        .await;
    assert!(matches!(result, Err(error) if error.code == DayErrorCode::Storage));
    assert_eq!(generation(&store, &actor).await, old_generation);
    assert_eq!(read(&store, &records[0]).await, records[0]);
    connection
        .execute(
            "UPDATE day_refreshes SET is_terminal=1 WHERE operation_id=?",
            (records[0].operation_id.to_string(),),
        )
        .await
        .unwrap();
    assert!(
        matches!(store.read_refresh(RefreshLookup { operation_id: records[0].operation_id, person_id: actor.person_id, device_id: actor.device_id.clone() }).await, Err(error) if error.code == DayErrorCode::Storage)
    );
}
