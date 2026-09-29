use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::TimeZone;

use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, ConnectionId, ConnectorId,
    ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory,
    GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
    RemoteCallWindow, ResourceHandle,
};
use floe_connections::{ConnectionResource, ResourceMode, SourceConnection};
use floe_context::{
    CalendarConnectionReader, CalendarObservation, CalendarObserveRequest, CalendarSource,
    NativeCalendarGrantReader, NativeCalendarViewRead, SourceLeaseRegistry,
    admit_current_native_calendar_read, authorize_native_calendar_dependency,
    read_native_calendar_view,
};
use floe_context_contract::{CalendarReadAccessStamp, CalendarViewQuery, SourceAuthority};
use floe_day::{AllDaySchedule, CalendarBatch, CalendarFailure, CalendarRecord, EventSchedule};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;

struct Connections {
    value: Arc<Mutex<Option<SourceConnection>>>,
    reads: AtomicUsize,
}

impl CalendarConnectionReader for Connections {
    async fn calendar_connection(&self) -> Result<Option<SourceConnection>, AgentFailure> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.value.lock().unwrap().clone())
    }
}

struct Device {
    person_id: PersonId,
    fingerprint: String,
    checks: AtomicUsize,
    observations: AtomicUsize,
    checked_calendar_ids: Mutex<Vec<Vec<String>>>,
    observed_calendar_ids: Mutex<Vec<Vec<String>>>,
    partial_batch: AtomicBool,
    generation_drift: AtomicBool,
    failure: Mutex<Option<CalendarFailure>>,
    records: Mutex<Vec<CalendarRecord>>,
}

impl CalendarSource for Device {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        self.checked_calendar_ids
            .lock()
            .unwrap()
            .push(request.calendar_ids.clone());
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
        self.observed_calendar_ids
            .lock()
            .unwrap()
            .push(request.calendar_ids.clone());
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
                calendar_ids: request.calendar_ids.clone(),
                native_subject_fingerprint: self.fingerprint.clone(),
                generation: if self.generation_drift.load(Ordering::SeqCst) {
                    "generation-2".into()
                } else {
                    "generation-1".into()
                },
            },
            observed_at: chrono::Utc::now(),
            batches: if self.partial_batch.load(Ordering::SeqCst) {
                vec![]
            } else {
                request
                    .calendar_ids
                    .into_iter()
                    .map(|calendar_id| CalendarBatch {
                        records: if calendar_id == "primary" {
                            std::mem::take(&mut *self.records.lock().unwrap())
                        } else {
                            vec![]
                        },
                        failure: if calendar_id == "primary" {
                            self.failure.lock().unwrap().take()
                        } else {
                            None
                        },
                        calendar_id,
                    })
                    .collect()
            },
        }))
    }
}

struct Grants {
    connections: Arc<Mutex<Option<SourceConnection>>>,
    change_connection: AtomicBool,
    change_on_second_call: AtomicBool,
    wrong_source: AtomicBool,
    wrong_consumer: AtomicBool,
    calls: AtomicUsize,
    grant_id: GrantId,
    authority: GrantAuthority,
}

impl NativeCalendarGrantReader for Grants {
    async fn admit(
        &self,
        connection: &SourceConnection,
        person_id: PersonId,
        consumer: &str,
    ) -> Result<CalendarReadAccessAdmission, AgentFailure> {
        let call_number = self.calls.fetch_add(1, Ordering::SeqCst);
        if self.change_connection.load(Ordering::SeqCst)
            || self.change_on_second_call.load(Ordering::SeqCst) && call_number == 1
        {
            let mut stored = self.connections.lock().unwrap();
            let source = stored.as_mut().unwrap();
            source
                .configure(
                    source.revision(),
                    ResourceMode::Selected,
                    vec![resource("primary", "Renamed")],
                )
                .unwrap();
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
            connection.connection_id().clone(),
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ExecutionOwnerId::try_new(if self.wrong_source.load(Ordering::SeqCst) {
                "another-device"
            } else {
                connection.execution_owner_id().as_str()
            })
            .unwrap(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![floe_access::native_calendar_resource(
                connection.connection_id().as_str(),
            )?],
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
            connection.source_authority(),
            scope,
            consumer,
        ))
    }
}

fn fixture() -> (Connections, Device, Grants, PersonId) {
    let person_id = PersonId::new();
    let mut source = SourceConnection::establish(
        person_id,
        ConnectorId::try_new("calendar.event_kit").unwrap(),
        ConnectionId::new(),
        ExecutionOwnerId::try_new("device").unwrap(),
        ResourceMode::Selected,
        vec![resource("primary", "Primary")],
    )
    .unwrap();
    source.update_native_subject(1, "a".repeat(64)).unwrap();
    let value = Arc::new(Mutex::new(Some(source)));
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
            checked_calendar_ids: Mutex::new(vec![]),
            observed_calendar_ids: Mutex::new(vec![]),
            partial_batch: AtomicBool::new(false),
            generation_drift: AtomicBool::new(false),
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
        },
        person_id,
    )
}

fn resource(handle: &str, label: &str) -> ConnectionResource {
    ConnectionResource::new(ResourceHandle::try_new(handle).unwrap(), label.into()).unwrap()
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
        floe_access::native_calendar_resource(admitted.connection.connection_id().as_str())
            .unwrap()
            .as_str()
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
async fn native_admission_reads_more_than_128_current_calendars_under_one_logical_grant() {
    let (connections, device, grants, person_id) = fixture();
    let mut stored = connections.value.lock().unwrap();
    let source = stored.as_mut().unwrap();
    source
        .configure(
            source.revision(),
            ResourceMode::Selected,
            (0..129)
                .map(|index| resource(&format!("calendar-{index}"), &format!("Calendar {index}")))
                .collect(),
        )
        .unwrap();
    drop(stored);

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

    assert_eq!(admitted.stamp.calendar_ids.len(), 129);
    assert_eq!(admitted.admission.scope().resources().len(), 1);
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
        dependency.source_authority(),
        view_source_authority(&connections)
    );
    assert_eq!(device.checks.load(Ordering::SeqCst), 2);
    assert_eq!(device.observations.load(Ordering::SeqCst), 1);
    assert_eq!(grants.calls.load(Ordering::SeqCst), 2);
    assert!(leases.observation(&dependency).is_ok());
}

#[tokio::test]
async fn current_resource_growth_reads_all_calendars_and_stales_old_dependency() {
    let (connections, device, grants, person_id) = fixture();
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    let leases = SourceLeaseRegistry::new();
    let first_window = window();
    let (_, old_dependency) = read_native_calendar_view(
        &connections,
        &device,
        &grants,
        &leases,
        NativeCalendarViewRead {
            person_id,
            device_id: "device",
            consumer: "floe.builtin.schedule",
            query: &query,
            window: &first_window,
        },
    )
    .await
    .unwrap();
    let old_source_authority = old_dependency.source_authority();
    {
        let mut stored = connections.value.lock().unwrap();
        let source = stored.as_mut().unwrap();
        source
            .configure(
                source.revision(),
                ResourceMode::Selected,
                vec![
                    resource("primary", "Primary"),
                    resource("secondary", "Secondary"),
                ],
            )
            .unwrap();
    }
    assert_ne!(view_source_authority(&connections), old_source_authority);
    assert_eq!(
        authorize_native_calendar_dependency(
            &connections,
            &device,
            &grants,
            &leases,
            &old_dependency,
            &window(),
        )
        .await,
        Err(AgentFailure::StaleContext),
    );
    let next_window = window();
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
            window: &next_window,
        },
    )
    .await
    .unwrap();
    assert!(view.items.is_empty());
    let expected_ids = vec!["primary", "secondary"];
    assert_eq!(
        device.checked_calendar_ids.lock().unwrap().last().unwrap(),
        &expected_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        device.observed_calendar_ids.lock().unwrap().last().unwrap(),
        &expected_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        dependency.resources(),
        &[floe_access::native_calendar_resource(
            connections
                .value
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .connection_id()
                .as_str()
        )
        .unwrap()],
    );
    assert_eq!(
        dependency
            .source_resources()
            .iter()
            .map(|resource| resource.as_str())
            .collect::<Vec<_>>(),
        expected_ids,
    );
    assert_eq!(
        dependency.source_authority(),
        view_source_authority(&connections)
    );
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
}
fn view_source_authority(connections: &Connections) -> SourceAuthority {
    connections
        .value
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .source_authority()
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
    let mut stored = connections.value.lock().unwrap();
    let source = stored.as_mut().unwrap();
    source
        .configure(
            source.revision(),
            ResourceMode::Selected,
            vec![
                resource("primary", "Primary"),
                resource("secondary", "Secondary"),
            ],
        )
        .unwrap();
    drop(stored);
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

#[tokio::test]
async fn native_view_rejects_device_generation_drift() {
    let (connections, device, grants, person_id) = fixture();
    let now = chrono::Utc::now().timestamp_millis();
    let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 8).unwrap();
    device.generation_drift.store(true, Ordering::SeqCst);
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
}
