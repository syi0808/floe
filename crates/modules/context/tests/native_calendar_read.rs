use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, ConnectionId, ConnectorId,
    ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory,
    GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
    RemoteCallWindow, ResourceHandle,
};
use floe_context::{
    CalendarConnectionReader, CalendarSource, NativeCalendarGrantReader,
    admit_current_native_calendar_read,
};
use floe_context_contract::{
    CalendarProvider, CalendarReadAccessStamp, CalendarScope, SourceAuthority,
};
use floe_day::{CalendarConnection, CalendarSelection};
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
}

struct Grants {
    connections: Arc<Mutex<Option<CalendarConnection>>>,
    change_connection: AtomicBool,
    wrong_source: AtomicBool,
    wrong_consumer: AtomicBool,
    calls: AtomicUsize,
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        if native_subject_fingerprint != "a".repeat(64) {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if self.change_connection.load(Ordering::SeqCst) {
            self.connections.lock().unwrap().as_mut().unwrap().revision += 1;
        }
        let consumer = GrantConsumer::builtin(if self.wrong_consumer.load(Ordering::SeqCst) {
            "another.expert"
        } else {
            consumer
        })
        .unwrap();
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
            vec![consumer.clone()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        Ok(CalendarReadAccessAdmission::device_local(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            scope,
            ConsumerPolicyAuthority::new(),
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
        },
        Grants {
            connections: value,
            change_connection: AtomicBool::new(false),
            wrong_source: AtomicBool::new(false),
            wrong_consumer: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
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
        "calendar.expert",
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
            "calendar.expert",
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
            "calendar.expert",
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
            "calendar.expert",
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
            "calendar.expert",
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
            "calendar.expert",
            &window(),
        )
        .await,
        Err(AgentFailure::CapabilityDenied)
    ));
}
