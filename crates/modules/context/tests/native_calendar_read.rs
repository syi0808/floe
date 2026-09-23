use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::TimeZone;

use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, ConnectionId, ConnectorId,
    ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory,
    GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
    RemoteCallWindow, ResourceHandle,
};
use floe_context::{
    CalendarConnectionReader, CalendarObservation, CalendarObserveRequest, CalendarSource,
    NativeCalendarGrantReader, NativeCalendarViewRead, SourceLeaseRegistry,
    admit_current_native_calendar_read, authorize_native_calendar_dependency,
    read_native_calendar_view,
};
use floe_context_contract::{
    CalendarProvider, CalendarReadAccessStamp, CalendarScope, CalendarViewQuery, SourceAuthority,
};
use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarConnection, CalendarFailure, CalendarRecord,
    CalendarSelection, EventSchedule,
};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

struct Connections {
    value: Arc<Mutex<Option<CalendarConnection>>>,
    reads: AtomicUsize,
}

impl CalendarConnectionReader for Connections {
    async fn calendar_connection(&self) -> Result<Option<CalendarConnection>, AgentFailure> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.value.lock().unwrap().clone())
    }
}

struct Device {
    person_id: PersonId,
    fingerprint: String,
    checks: AtomicUsize,
    observations: AtomicUsize,
    partial_batch: AtomicBool,
    failure: Mutex<Option<CalendarFailure>>,
    records: Mutex<Vec<CalendarRecord>>,
}

impl CalendarSource for Device {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(CalendarReadAccessStamp {
            schema_version: 1,
            person_id: self.person_id,
            device_id: request.device_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            native_subject_fingerprint: self.fingerprint.clone(),
            generation: "generation-1".into(),
        })
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        self.observations.fetch_add(1, Ordering::SeqCst);
        if request.expected_native_subject_fingerprint.as_deref() != Some(self.fingerprint.as_str())
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        Ok(Some(CalendarObservation {
            stamp: CalendarReadAccessStamp {
                schema_version: 1,
                person_id: self.person_id,
                device_id: request.device_id,
                provider: request.provider,
                calendar_ids: request.calendar_ids,
                native_subject_fingerprint: self.fingerprint.clone(),
                generation: "generation-1".into(),
            },
            observed_at: chrono::Utc::now(),
            batches: if self.partial_batch.load(Ordering::SeqCst) {
                vec![]
            } else {
                vec![CalendarBatch {
                    calendar_id: "primary".into(),
                    records: std::mem::take(&mut *self.records.lock().unwrap()),
                    failure: self.failure.lock().unwrap().take(),
                }]
            },
        }))
    }
}

struct Grants {
    connections: Arc<Mutex<Option<CalendarConnection>>>,
    change_connection: AtomicBool,
    change_on_second_call: AtomicBool,
    wrong_source: AtomicBool,
    wrong_consumer: AtomicBool,
    calls: AtomicUsize,
    grant_id: GrantId,
    authority: GrantAuthority,
    policy: ConsumerPolicyAuthority,
}

impl NativeCalendarGrantReader for Grants {
    async fn admit(
        &self,
        connection: &CalendarConnection,
        person_id: PersonId,
        calendar_ids: &[String],
        consumer: &str,
        native_subject_fingerprint: &str,
    ) -> Result<CalendarReadAccessAdmission, AgentFailure> {
        let call_number = self.calls.fetch_add(1, Ordering::SeqCst);
        if native_subject_fingerprint != "a".repeat(64) {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if self.change_connection.load(Ordering::SeqCst)
            || self.change_on_second_call.load(Ordering::SeqCst) && call_number == 1
        {
            self.connections.lock().unwrap().as_mut().unwrap().revision += 1;
        }
        let consumer = GrantConsumer::builtin(if self.wrong_consumer.load(Ordering::SeqCst) {
            "another.expert"
        } else {
            consumer
        })
        .unwrap();
        let consumers = vec![consumer.clone()];
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new(&connection.connection_id).unwrap(),
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ExecutionOwnerId::try_new(if self.wrong_source.load(Ordering::SeqCst) {
                "another-device"
            } else {
                &connection.device_id
            })
            .unwrap(),
            connection.source_authority,
        )
        .unwrap();
        let scope = GrantScope::try_new(
            calendar_ids
                .iter()
                .map(|calendar_id| ResourceHandle::try_new(calendar_id.clone()).unwrap())
                .collect(),
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            consumers,
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        Ok(CalendarReadAccessAdmission::device_local(
            person_id,
            self.grant_id,
            self.authority,
            source,
            scope,
            self.policy,
            consumer,
        ))
    }
}

fn fixture() -> (Connections, Device, Grants, PersonId) {
    let person_id = PersonId::new();
    let value = Arc::new(Mutex::new(Some(CalendarConnection {
        connection_id: Uuid::new_v4().to_string(),
        device_id: "device".into(),
        disconnected: false,
        scope: CalendarScope::Selected,
        provider: CalendarProvider::EventKit,
        calendars: vec![CalendarSelection {
            calendar_id: "primary".into(),
            calendar_name: "Primary".into(),
        }],
        revision: 1,
        source_authority: SourceAuthority::new(),
        last_success_at: None,
        last_range: None,
        error: None,
        error_at: None,
        source_statuses: BTreeMap::new(),
    })));
    (
        Connections {
            value: Arc::clone(&value),
            reads: AtomicUsize::new(0),
        },
        Device {
            person_id,
            fingerprint: "a".repeat(64),
            checks: AtomicUsize::new(0),
            observations: AtomicUsize::new(0),
            partial_batch: AtomicBool::new(false),
            failure: Mutex::new(None),
            records: Mutex::new(vec![]),
        },
        Grants {
            connections: value,
            change_connection: AtomicBool::new(false),
            change_on_second_call: AtomicBool::new(false),
            wrong_source: AtomicBool::new(false),
            wrong_consumer: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            grant_id: GrantId::new(),
            authority: GrantAuthority::new(),
            policy: ConsumerPolicyAuthority::new(),
        },
        person_id,
    )
}

fn window() -> RemoteCallWindow {
    RemoteCallWindow {
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Cancellation::default(),
    }
}

#[tokio::test]
async fn native_admission_checks_subject_grant_and_current_connection() {
    let (connections, device, grants, person_id) = fixture();
    let admitted = admit_current_native_calendar_read(
        &connections,
        &device,
        &grants,
        person_id,
        "device",
        "floe.builtin.schedule",
        &window(),
    )
    .await
    .unwrap();
    assert_eq!(admitted.stamp.native_subject_fingerprint, "a".repeat(64));
    assert_eq!(
        admitted.admission.scope().resources()[0].as_str(),
        "primary"
    );
    assert_eq!(connections.reads.load(Ordering::SeqCst), 2);
    assert_eq!(device.checks.load(Ordering::SeqCst), 1);
    assert_eq!(grants.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn native_admission_serves_two_canonical_consumers_under_one_grant() {
    let (connections, device, grants, person_id) = fixture();
    let schedule = admit_current_native_calendar_read(
        &connections,
        &device,
        &grants,
        person_id,
        "device",
        "floe.builtin.schedule",
        &window(),
    )
    .await
    .unwrap();
    let focus = admit_current_native_calendar_read(
        &connections,
        &device,
        &grants,
        person_id,
        "device",
        "floe.builtin.focus-attention",
        &window(),
    )
    .await
    .unwrap();
    assert_eq!(schedule.admission.grant_id(), focus.admission.grant_id());
    for (admission, consumer) in [
        (&schedule.admission, "floe.builtin.schedule"),
        (&focus.admission, "floe.builtin.focus-attention"),
    ] {
        let consumers = admission.scope().consumers();
        assert_eq!(consumers.len(), 1, "admission is per-consumer scoped");
        assert_eq!(consumers[0].identifier(), consumer);
    }
}

#[tokio::test]
async fn native_admission_rejects_missing_or_changed_authority() {
    let (connections, device, grants, person_id) = fixture();
    *connections.value.lock().unwrap() = None;
    assert!(matches!(
        admit_current_native_calendar_read(
            &connections,
            &device,
            &grants,
            person_id,
            "device",
            "floe.builtin.schedule",
            &window(),
        )
        .await,
        Err(AgentFailure::AccessReviewRequired)
    ));
    assert_eq!(device.checks.load(Ordering::SeqCst), 0);

    let (connections, device, grants, person_id) = fixture();
    grants.change_connection.store(true, Ordering::SeqCst);
    assert!(matches!(
        admit_current_native_calendar_read(
            &connections,
            &device,
            &grants,
            person_id,
            "device",
            "floe.builtin.schedule",
            &window(),
        )
        .await,
        Err(AgentFailure::StaleContext)
    ));
    assert_eq!(grants.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn native_admission_rejects_unbound_stamp_and_grant() {
    let (connections, mut device, grants, person_id) = fixture();
    device.fingerprint = "invalid".into();
    assert!(matches!(
        admit_current_native_calendar_read(
            &connections,
            &device,
            &grants,
            person_id,
            "device",
            "floe.builtin.schedule",
            &window(),
        )
        .await,
        Err(AgentFailure::CapabilityDenied)
    ));
    assert_eq!(grants.calls.load(Ordering::SeqCst), 0);

    let (connections, device, grants, person_id) = fixture();
    grants.wrong_source.store(true, Ordering::SeqCst);
    assert!(matches!(
        admit_current_native_calendar_read(
            &connections,
            &device,
            &grants,
            person_id,
            "device",
            "floe.builtin.schedule",
            &window(),
        )
        .await,
        Err(AgentFailure::CapabilityDenied)
    ));

    let (connections, device, grants, person_id) = fixture();
    grants.wrong_consumer.store(true, Ordering::SeqCst);
    assert!(matches!(
        admit_current_native_calendar_read(
            &connections,
            &device,
            &grants,
            person_id,
            "device",
            "floe.builtin.schedule",
            &window(),
        )
        .await,
        Err(AgentFailure::CapabilityDenied)
    ));
}

#[tokio::test]
async fn native_view_records_complete_empty_coverage_and_exact_dependency() {
    let (connections, device, grants, person_id) = fixture();
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    let leases = SourceLeaseRegistry::new();
    let (view, dependency) = read_native_calendar_view(
        &connections,
        &device,
        &grants,
        &leases,
        NativeCalendarViewRead {
            person_id,
            device_id: "device",
            consumer: "floe.builtin.schedule",
            query: &query,
            window: &window(),
        },
    )
    .await
    .unwrap();
    assert!(view.coverage_complete);
    assert!(view.items.is_empty());
    assert_eq!(view.range_start_unix_ms, query.range_start_unix_ms());
    assert_eq!(view.range_end_unix_ms, query.range_end_unix_ms());
    assert_eq!(dependency.person_id(), person_id);
    assert_eq!(dependency.consumer().identifier(), "floe.builtin.schedule");
    assert_eq!(
        dependency.source().source_authority(),
        view_source_authority(&connections)
    );
    assert_eq!(device.checks.load(Ordering::SeqCst), 2);
    assert_eq!(device.observations.load(Ordering::SeqCst), 1);
    assert_eq!(grants.calls.load(Ordering::SeqCst), 2);
    assert!(leases.observation(&dependency).is_ok());
}

fn view_source_authority(connections: &Connections) -> SourceAuthority {
    connections
        .value
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .source_authority
}

#[tokio::test]
async fn native_view_rejects_partial_batch_and_unpageable_cursor() {
    let (connections, device, grants, person_id) = fixture();
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    device.partial_batch.store(true, Ordering::SeqCst);
    assert!(matches!(
        read_native_calendar_view(
            &connections,
            &device,
            &grants,
            &SourceLeaseRegistry::new(),
            NativeCalendarViewRead {
                person_id,
                device_id: "device",
                consumer: "floe.builtin.schedule",
                query: &query,
                window: &window(),
            },
        )
        .await,
        Err(AgentFailure::CapabilityUnavailable)
    ));
    let query =
        CalendarViewQuery::try_new(now - 60_000, now + 60_000, Some("next".into()), 8).unwrap();
    let prior_checks = device.checks.load(Ordering::SeqCst);
    assert!(matches!(
        read_native_calendar_view(
            &connections,
            &device,
            &grants,
            &SourceLeaseRegistry::new(),
            NativeCalendarViewRead {
                person_id,
                device_id: "device",
                consumer: "floe.builtin.schedule",
                query: &query,
                window: &window(),
            },
        )
        .await,
        Err(AgentFailure::CapabilityUnavailable)
    ));
    assert_eq!(device.checks.load(Ordering::SeqCst), prior_checks);
}

#[tokio::test]
async fn native_permission_denial_requires_review_without_issuing_a_view() {
    let (connections, device, grants, person_id) = fixture();
    *device.failure.lock().unwrap() = Some(CalendarFailure::PermissionDenied);
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    let leases = SourceLeaseRegistry::new();
    assert!(matches!(
        read_native_calendar_view(
            &connections,
            &device,
            &grants,
            &leases,
            NativeCalendarViewRead {
                person_id,
                device_id: "device",
                consumer: "floe.builtin.schedule",
                query: &query,
                window: &window(),
            },
        )
        .await,
        Err(AgentFailure::AccessReviewRequired)
    ));
    assert_eq!(grants.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn native_view_preserves_all_day_calendar_evidence() {
    let (connections, device, grants, person_id) = fixture();
    let date = chrono::Local::now().date_naive();
    let end_date = date.succ_opt().unwrap();
    let start = chrono::Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis();
    let end = chrono::Local
        .from_local_datetime(&end_date.and_hms_opt(0, 0, 0).unwrap())
        .latest()
        .unwrap()
        .timestamp_millis();
    device.records.lock().unwrap().push(CalendarRecord {
        can_modify: false,
        calendar_id: "primary".into(),
        external_id: "event-one".into(),
        external_revision: "r1".into(),
        title: "Day off".into(),
        schedule: EventSchedule::AllDay(AllDaySchedule::new(date, end_date).unwrap()),
    });
    let query = CalendarViewQuery::try_new(start - 3_600_000, end + 3_600_000, None, 8).unwrap();
    let (view, _) = read_native_calendar_view(
        &connections,
        &device,
        &grants,
        &SourceLeaseRegistry::new(),
        NativeCalendarViewRead {
            person_id,
            device_id: "device",
            consumer: "floe.builtin.schedule",
            query: &query,
            window: &window(),
        },
    )
    .await
    .unwrap();
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].starts_at_unix_ms, start);
    assert_eq!(view.items[0].ends_at_unix_ms, end);
    assert!(view.items[0].all_day);
    assert_eq!(view.items[0].untrusted_title, "Day off");
}

#[tokio::test]
async fn native_dependency_rechecks_the_current_grant_source() {
    let (connections, device, grants, person_id) = fixture();
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    let leases = SourceLeaseRegistry::new();
    let (_, dependency) = read_native_calendar_view(
        &connections,
        &device,
        &grants,
        &leases,
        NativeCalendarViewRead {
            person_id,
            device_id: "device",
            consumer: "floe.builtin.schedule",
            query: &query,
            window: &window(),
        },
    )
    .await
    .unwrap();
    authorize_native_calendar_dependency(
        &connections,
        &device,
        &grants,
        &leases,
        &dependency,
        &window(),
    )
    .await
    .unwrap();
    connections
        .value
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .source_authority = SourceAuthority::new();
    assert!(matches!(
        authorize_native_calendar_dependency(
            &connections,
            &device,
            &grants,
            &leases,
            &dependency,
            &window(),
        )
        .await,
        Err(AgentFailure::StaleContext)
    ));
}

#[tokio::test]
async fn native_view_rejects_connection_change_after_observation() {
    let (connections, device, grants, person_id) = fixture();
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    grants.change_on_second_call.store(true, Ordering::SeqCst);
    assert!(matches!(
        read_native_calendar_view(
            &connections,
            &device,
            &grants,
            &SourceLeaseRegistry::new(),
            NativeCalendarViewRead {
                person_id,
                device_id: "device",
                consumer: "floe.builtin.schedule",
                query: &query,
                window: &window(),
            },
        )
        .await,
        Err(AgentFailure::StaleContext)
    ));
    assert_eq!(device.observations.load(Ordering::SeqCst), 1);
    assert_eq!(grants.calls.load(Ordering::SeqCst), 2);
}
