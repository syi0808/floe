//! The bounded calendar timeline view one Expert reads under one grant.
//!
//! Context owns the order of this read: Access judges the grant, the source
//! performs the acquisition, and Context re-checks between them and bounds what
//! comes back. These tests give it a stored mirror and a source it controls, so
//! what they assert is that ordering and those bounds.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Duration as TimeDelta, TimeZone, Utc};
use floe_access::{CalendarReadAccessRequest, CalendarReadAccessStamp, CalendarReadAdmission};
use floe_agent_contract::{
    AgentFailure, DataClass, MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS,
    MAX_TIMELINE_VIEW_ITEMS, TimelineViewRead,
};
use floe_context::{
    CalendarMirrorReader, CalendarObservation, CalendarObserveRequest, CalendarSource,
    CalendarTimelineViews, ProjectedCalendarItem, ProjectedCalendarObservation,
    SourceLeaseRegistry,
};
use floe_context_contract::CalendarProvider;
use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarMirror, CalendarRange, CalendarRecord,
    CalendarSelection, CalendarTimelineGrant, DayService, EventSchedule, PersonId, TimedSchedule,
    TimelineRepository, range_bounds,
};
use floe_execution::Cancellation;
use tokio::time::Instant;
use uuid::Uuid;

mod support;
use support::TestTimelineRepository;

/// The payload bound the mirror port promises its readers, which the Vault
/// adapter enforces when it loads a stored mirror.
const MAX_MIRROR_BYTES: usize = 4_194_304;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2050, 1, 15, 9, 0, 0).unwrap()
}

fn milliseconds(time: DateTime<Utc>) -> Result<u64, AgentFailure> {
    u64::try_from(time.timestamp_millis()).map_err(|_| AgentFailure::InvalidInput)
}

fn day() -> CalendarRange {
    CalendarRange {
        start_date: now().date_naive(),
        end_date_exclusive: (now() + TimeDelta::days(1)).date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    }
}

fn record(calendar: &str, identifier: &str, title: &str, start: i64, end: i64) -> CalendarRecord {
    CalendarRecord {
        can_modify: false,
        calendar_id: calendar.into(),
        external_id: identifier.into(),
        external_revision: "private-external-revision".into(),
        title: title.into(),
        schedule: EventSchedule::Timed(
            TimedSchedule::new(
                now() + TimeDelta::minutes(start),
                now() + TimeDelta::minutes(end),
                "UTC",
            )
            .unwrap(),
        ),
    }
}

/// The stored calendar state a read stands on, and the lease registry that
/// bounds how much of it may be held live at once.
struct Fixture {
    timeline: TestTimelineRepository,
    person: PersonId,
    leases: Arc<SourceLeaseRegistry>,
}

impl Fixture {
    async fn new() -> Self {
        let fixture = Self {
            timeline: TestTimelineRepository::new(),
            person: PersonId::new(),
            leases: Arc::new(SourceLeaseRegistry::new()),
        };
        fixture
            .day()
            .select_calendars(
                fixture.person,
                CalendarProvider::Fixture,
                vec![
                    CalendarSelection {
                        calendar_id: "home-secret-id".into(),
                        calendar_name: "Private home calendar".into(),
                    },
                    CalendarSelection {
                        calendar_id: "work-secret-id".into(),
                        calendar_name: "Private work calendar".into(),
                    },
                ],
            )
            .await
            .unwrap();
        fixture
            .day()
            .import_calendar(
                fixture.person,
                1,
                day(),
                vec![
                    record(
                        "home-secret-id",
                        "private-provider-id",
                        "Home appointment",
                        30,
                        90,
                    ),
                    record(
                        "work-secret-id",
                        "work-native-id",
                        "Hidden work title",
                        90,
                        120,
                    ),
                ],
                now(),
            )
            .await
            .unwrap();
        fixture
    }

    fn day(&self) -> DayService<'_, TestTimelineRepository> {
        DayService::new(&self.timeline)
    }

    fn grant(&self) -> CalendarTimelineGrant {
        CalendarTimelineGrant {
            person_id: self.person,
            handle: Uuid::new_v4(),
            provider: CalendarProvider::Fixture,
            device_id: "test-device".into(),
            calendar_ids: vec!["home-secret-id".into()],
            connection_revision: 2,
            day: day(),
            starts_at: now() + TimeDelta::hours(1),
            ends_at: now() + TimeDelta::hours(3),
            expires_at: now() + TimeDelta::minutes(2),
        }
    }
}

/// The mirror as a store hands it back: absent, oversized, or whole.
impl CalendarMirrorReader for Fixture {
    async fn bounded_calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<CalendarMirror, AgentFailure> {
        let mirror = self
            .timeline
            .calendar_mirror(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if serde_json::to_string(&mirror)
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .len()
            > MAX_MIRROR_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(mirror)
    }
}

/// A source that only stamps the native subject, so the read falls back to the
/// stored mirror.
#[derive(Default)]
struct Access {
    calls: AtomicUsize,
    denied: AtomicBool,
    invalid_subject: bool,
    change_on_second: bool,
    subject_change_on_call: Option<usize>,
    foreign: bool,
    pending: bool,
    started: tokio::sync::Notify,
    child: Mutex<Option<Cancellation>>,
}

impl CalendarReadAdmission for Access {}

impl CalendarSource for Access {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if self.denied.load(Ordering::SeqCst) {
            return Err(AgentFailure::CapabilityDenied);
        }
        if self.pending {
            *self.child.lock().unwrap() = Some(request.cancellation.clone());
            self.started.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(CalendarReadAccessStamp {
            schema_version: 1,
            person_id: if self.foreign {
                PersonId::new()
            } else {
                request.person_id
            },
            device_id: request.device_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            native_subject_fingerprint: if self.invalid_subject {
                String::new()
            } else if self
                .subject_change_on_call
                .is_some_and(|threshold| call >= threshold)
            {
                "d".repeat(64)
            } else {
                "c".repeat(64)
            },
            generation: if self.change_on_second && call > 0 {
                "second"
            } else {
                "first"
            }
            .into(),
        })
    }
}

/// A source that answers the read itself with raw batches.
struct ObservationAccess {
    calendar_id: String,
    record: CalendarRecord,
    subject_change_during_read: bool,
}

impl CalendarReadAdmission for ObservationAccess {}

impl CalendarSource for ObservationAccess {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        Ok(CalendarReadAccessStamp {
            schema_version: 1,
            person_id: request.person_id,
            device_id: request.device_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            native_subject_fingerprint: "c".repeat(64),
            generation: "live-observation".into(),
        })
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        Ok(Some(CalendarObservation {
            stamp: CalendarReadAccessStamp {
                schema_version: 1,
                person_id: request.person_id,
                device_id: request.device_id,
                provider: request.provider,
                calendar_ids: request.calendar_ids,
                native_subject_fingerprint: if self.subject_change_during_read {
                    "d".repeat(64)
                } else {
                    "c".repeat(64)
                },
                generation: "live-observation".into(),
            },
            observed_at: now(),
            batches: vec![CalendarBatch {
                calendar_id: self.calendar_id.clone(),
                records: vec![self.record.clone()],
                failure: None,
            }],
        }))
    }
}

/// A remote producer that returns a projection it already bounded.
struct ProjectedObservationAccess;

impl CalendarReadAdmission for ProjectedObservationAccess {}

impl CalendarSource for ProjectedObservationAccess {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        Ok(CalendarReadAccessStamp {
            schema_version: 1,
            person_id: request.person_id,
            device_id: request.device_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            native_subject_fingerprint: "c".repeat(64),
            generation: "server-7".into(),
        })
    }

    async fn observe_projected(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        Ok(Some(ProjectedCalendarObservation {
            stamp: CalendarReadAccessStamp {
                schema_version: 1,
                person_id: request.person_id,
                device_id: request.device_id,
                provider: request.provider,
                calendar_ids: request.calendar_ids,
                native_subject_fingerprint: "c".repeat(64),
                generation: "server-7".into(),
            },
            source_handle: "calendar.timeline:server".into(),
            observed_at: now(),
            expires_at: now() + TimeDelta::minutes(5),
            range_start: request.starts_at,
            range_end: request.ends_at,
            coverage_complete: false,
            next_cursor: Some("next-page".into()),
            items: vec![ProjectedCalendarItem {
                evidence_handle: "opaque-server-event".into(),
                untrusted_title: "Server review".into(),
                starts_at: request.starts_at + TimeDelta::minutes(10),
                ends_at: request.starts_at + TimeDelta::minutes(40),
            }],
        }))
    }
}

fn request(grant: &CalendarTimelineGrant) -> TimelineViewRead {
    TimelineViewRead {
        person_id: grant.person_id,
        handle: grant.handle,
        range_start_unix_ms: None,
        range_end_unix_ms: None,
        cursor: None,
        max_items: 32,
        max_bytes: 16_384,
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Cancellation::default(),
    }
}
#[tokio::test]
async fn projection_is_scoped_clipped_bounded_and_contains_no_provider_native_metadata() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    let projected = views.timeline(request(&grant)).await.unwrap();
    assert_eq!(projected.items.len(), 1);
    assert_eq!(projected.items[0].untrusted_title, "Home appointment");
    assert_eq!(
        projected.items[0].starts_at_unix_ms,
        milliseconds(grant.starts_at).unwrap()
    );
    assert_eq!(
        projected.items[0].ends_at_unix_ms,
        milliseconds(now() + TimeDelta::minutes(90)).unwrap()
    );
    assert_eq!(projected.data_class, DataClass::Synthetic);
    let payload = serde_json::to_string(&projected).unwrap();
    for hidden in [
        "secret-id",
        "Private home",
        "Hidden work",
        "private-provider",
        "external_revision",
        "can_modify",
        "UTC",
    ] {
        assert!(!payload.contains(hidden));
    }
    assert_eq!(access.calls.load(Ordering::SeqCst), 2);
    let again = views.timeline(request(&grant)).await.unwrap();
    assert_eq!(again, projected);
}

#[tokio::test]
async fn read_range_is_request_scoped_within_the_authorized_source() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    let range_start = now() + TimeDelta::minutes(75);
    let range_end = now() + TimeDelta::minutes(105);
    let projected = views
        .timeline(TimelineViewRead {
            range_start_unix_ms: Some(milliseconds(range_start).unwrap()),
            range_end_unix_ms: Some(milliseconds(range_end).unwrap()),
            ..request(&grant)
        })
        .await
        .unwrap();

    assert_eq!(
        projected.range_start_unix_ms,
        milliseconds(range_start).unwrap()
    );
    assert_eq!(
        projected.range_end_unix_ms,
        milliseconds(range_end).unwrap()
    );
    assert_eq!(projected.items.len(), 1);
    assert_eq!(projected.items[0].untrusted_title, "Home appointment");
    assert_eq!(
        projected.items[0].starts_at_unix_ms,
        milliseconds(range_start).unwrap()
    );

    let expanded =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now)
            .unwrap()
            .timeline(TimelineViewRead {
                range_start_unix_ms: Some(
                    milliseconds(grant.starts_at - TimeDelta::minutes(1)).unwrap(),
                ),
                range_end_unix_ms: Some(milliseconds(grant.ends_at).unwrap()),
                ..request(&grant)
            })
            .await
            .unwrap();
    assert_eq!(
        expanded.range_start_unix_ms,
        milliseconds(grant.starts_at - TimeDelta::minutes(1)).unwrap()
    );
}

#[tokio::test]
async fn request_scoped_observation_does_not_depend_on_page_mirror_coverage() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let requested_start = now() + TimeDelta::days(7);
    let requested_end = requested_start + TimeDelta::days(7);
    let access = ObservationAccess {
        calendar_id: "home-secret-id".into(),
        record: record(
            "home-secret-id",
            "next-week-event",
            "Next week review",
            8 * 24 * 60,
            8 * 24 * 60 + 60,
        ),
        subject_change_during_read: false,
    };
    let view = CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now)
        .unwrap()
        .timeline(TimelineViewRead {
            range_start_unix_ms: Some(milliseconds(requested_start).unwrap()),
            range_end_unix_ms: Some(milliseconds(requested_end).unwrap()),
            ..request(&grant)
        })
        .await
        .unwrap();

    assert_eq!(
        view.range_start_unix_ms,
        milliseconds(requested_start).unwrap()
    );
    assert_eq!(view.range_end_unix_ms, milliseconds(requested_end).unwrap());
    assert!(view.coverage_complete);
    assert_eq!(view.items[0].untrusted_title, "Next week review");
}

#[tokio::test]
async fn native_subject_change_during_read_denies_projection() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = ObservationAccess {
        calendar_id: "home-secret-id".into(),
        record: record(
            "home-secret-id",
            "subject-change",
            "Subject change",
            60,
            120,
        ),
        subject_change_during_read: true,
    };
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::StaleContext)
    );
}

#[tokio::test]
async fn projected_observation_preserves_server_coverage_and_opaque_provenance() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let views = CalendarTimelineViews::new(
        &fixture.leases,
        &fixture,
        &ProjectedObservationAccess,
        grant.clone(),
        now,
    )
    .unwrap();
    let view = views.timeline(request(&grant)).await.unwrap();

    assert_eq!(view.source_handle, "calendar.timeline:server");
    assert!(!view.coverage_complete);
    assert!(views.source_was_observed());
    assert_eq!(view.next_cursor.as_deref(), Some("next-page"));
    assert_eq!(view.items[0].untrusted_title, "Server review");
    assert_eq!(
        view.items[0].evidence_handle,
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            b"floe:calendar:calendar.timeline:server:opaque-server-event"
        )
    );
}

#[tokio::test]
async fn projection_reads_events_across_a_bounded_multi_day_range() {
    let fixture = Fixture::new().await;
    let range = CalendarRange {
        start_date: now().date_naive(),
        end_date_exclusive: (now() + TimeDelta::days(7)).date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    };
    fixture
        .day()
        .import_calendar(
            fixture.person,
            2,
            range.clone(),
            vec![
                record("home-secret-id", "first-day", "First day", 60, 90),
                record(
                    "home-secret-id",
                    "fifth-day",
                    "Fifth day",
                    4 * 24 * 60 + 60,
                    4 * 24 * 60 + 90,
                ),
                CalendarRecord {
                    can_modify: false,
                    calendar_id: "home-secret-id".into(),
                    external_id: "third-day-all-day".into(),
                    external_revision: "1".into(),
                    title: "Third day all day".into(),
                    schedule: EventSchedule::AllDay(
                        AllDaySchedule::new(
                            now().date_naive() + TimeDelta::days(2),
                            now().date_naive() + TimeDelta::days(3),
                        )
                        .unwrap(),
                    ),
                },
            ],
            now(),
        )
        .await
        .unwrap();
    let grant = CalendarTimelineGrant {
        person_id: fixture.person,
        handle: Uuid::new_v4(),
        provider: CalendarProvider::Fixture,
        device_id: "test-device".into(),
        calendar_ids: vec!["home-secret-id".into()],
        connection_revision: 3,
        day: range.clone(),
        starts_at: range_bounds(&range).unwrap().0,
        ends_at: range_bounds(&range).unwrap().1,
        expires_at: now() + TimeDelta::minutes(2),
    };
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    let projected = views
        .timeline(TimelineViewRead {
            max_items: MAX_TIMELINE_VIEW_ITEMS,
            max_bytes: MAX_TIMELINE_VIEW_BYTES,
            ..request(&grant)
        })
        .await
        .unwrap();

    assert_eq!(
        projected
            .items
            .iter()
            .map(|item| item.untrusted_title.as_str())
            .collect::<Vec<_>>(),
        ["First day", "Third day all day", "Fifth day"]
    );
    let all_day = &projected.items[1];
    assert_eq!(
        all_day.ends_at_unix_ms - all_day.starts_at_unix_ms,
        86_400_000
    );
    assert_eq!(
        projected.range_start_unix_ms,
        milliseconds(range_bounds(&grant.day).unwrap().0).unwrap()
    );
    assert_eq!(
        projected.range_end_unix_ms,
        milliseconds(range_bounds(&grant.day).unwrap().1).unwrap()
    );
}

#[tokio::test]
async fn unknown_scope_identity_budgets_and_permission_are_denied_before_projection() {
    let fixture = Fixture::new().await;
    for mode in 0..7 {
        let mut grant = fixture.grant();
        let access = Access {
            foreign: mode == 5,
            ..Access::default()
        };
        if mode == 6 {
            access.denied.store(true, Ordering::SeqCst);
        }
        if mode == 2 {
            grant.calendar_ids = vec!["unselected".into()];
        }
        let views =
            CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now)
                .unwrap();
        let mut read = request(&grant);
        match mode {
            0 => read.person_id = PersonId::new(),
            1 => read.handle = Uuid::new_v4(),
            3 => read.max_items = 0,
            4 => read.max_bytes = 1,
            _ => {}
        }
        assert_eq!(
            views.timeline(read).await,
            Err(if mode == 3 || mode == 4 {
                AgentFailure::BudgetExceeded
            } else {
                AgentFailure::CapabilityDenied
            })
        );
        if mode <= 1 || mode == 3 {
            assert_eq!(access.calls.load(Ordering::SeqCst), 0);
        }
    }
}

#[tokio::test]
async fn failed_other_calendar_does_not_poison_a_healthy_explicit_subset() {
    let fixture = Fixture::new().await;
    fixture
        .day()
        .import_calendar_sources(
            fixture.person,
            2,
            day(),
            vec![
                CalendarBatch {
                    calendar_id: "home-secret-id".into(),
                    records: vec![],
                    failure: None,
                },
                CalendarBatch {
                    calendar_id: "work-secret-id".into(),
                    records: vec![],
                    failure: Some(CalendarFailure::PermissionDenied),
                },
            ],
            now(),
        )
        .await
        .unwrap();
    let access = Access::default();
    let mut grant = fixture.grant();
    grant.connection_revision = 3;
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert!(
        views
            .timeline(request(&grant))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    grant.calendar_ids.push("work-secret-id".into());
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::CapabilityDenied)
    );
}

#[tokio::test]
async fn stale_cache_uncovered_ranges_revisions_and_revocation_never_return_empty_success() {
    let fixture = Fixture::new().await;
    let access = Access::default();
    for mode in 0..4 {
        let mut grant = fixture.grant();
        let clock = || {
            if mode == 0 {
                now() + TimeDelta::minutes(6)
            } else {
                now()
            }
        };
        if mode == 0 {
            grant.expires_at = clock() + TimeDelta::minutes(1);
        }
        if mode == 1 {
            grant.connection_revision += 1;
        }
        if mode == 2 {
            grant.day.start_date += TimeDelta::days(1);
            grant.day.end_date_exclusive += TimeDelta::days(1);
            grant.starts_at += TimeDelta::days(1);
            grant.ends_at += TimeDelta::days(1);
        }
        if mode == 3 {
            fixture
                .day()
                .disconnect_calendar(fixture.person, 2)
                .await
                .unwrap();
        }
        let views =
            CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), clock)
                .unwrap();
        assert_eq!(
            views.timeline(request(&grant)).await,
            Err(AgentFailure::StaleContext)
        );
    }
}

#[tokio::test]
async fn access_generation_change_and_later_calendar_change_invalidate_a_read_lease() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let changed = Access {
        change_on_second: true,
        ..Access::default()
    };
    let views = CalendarTimelineViews::new(&fixture.leases, &fixture, &changed, grant.clone(), now)
        .unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::StaleContext)
    );
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    views.timeline(request(&grant)).await.unwrap();
    views
        .revalidate(
            Instant::now() + Duration::from_secs(1),
            Cancellation::default(),
        )
        .await
        .unwrap();
    fixture
        .day()
        .import_calendar(fixture.person, 2, day(), vec![], now())
        .await
        .unwrap();
    assert_eq!(
        views
            .revalidate(
                Instant::now() + Duration::from_secs(1),
                Cancellation::default()
            )
            .await,
        Err(AgentFailure::StaleContext)
    );
}

#[tokio::test]
async fn native_subject_change_invalidates_cached_read_before_egress() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = Access {
        subject_change_on_call: Some(2),
        ..Access::default()
    };
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    views.timeline(request(&grant)).await.unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::StaleContext)
    );
}

#[tokio::test]
async fn missing_native_subject_fingerprint_denies_calendar_read() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = Access {
        invalid_subject: true,
        ..Access::default()
    };
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::CapabilityDenied)
    );
}

#[tokio::test]
async fn all_day_and_long_events_block_the_entire_requested_window_without_false_focus() {
    let fixture = Fixture::new().await;
    let mut all_day = record("home-secret-id", "all-day", "All day", 0, 1);
    all_day.schedule = EventSchedule::AllDay(
        AllDaySchedule::new(day().start_date, day().end_date_exclusive).unwrap(),
    );
    fixture
        .day()
        .import_calendar(
            fixture.person,
            2,
            day(),
            vec![all_day, record("home-secret-id", "long", "Long", -60, 720)],
            now(),
        )
        .await
        .unwrap();
    let mut grant = fixture.grant();
    grant.connection_revision = 3;
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    let view = views.timeline(request(&grant)).await.unwrap();
    assert_eq!(view.items.len(), 2);
    assert!(
        view.items
            .iter()
            .all(|item| item.starts_at_unix_ms == view.range_start_unix_ms
                && item.ends_at_unix_ms == view.range_end_unix_ms)
    );
}

#[tokio::test]
async fn oversized_titles_are_unicode_safe_but_overfull_calendars_are_not_truncated() {
    let fixture = Fixture::new().await;
    fixture
        .day()
        .import_calendar(
            fixture.person,
            2,
            day(),
            vec![record(
                "home-secret-id",
                "long-title",
                &"개인 제목".repeat(100),
                60,
                90,
            )],
            now(),
        )
        .await
        .unwrap();
    let mut grant = fixture.grant();
    grant.connection_revision = 3;
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    let view = views.timeline(request(&grant)).await.unwrap();
    assert!(view.items[0].untrusted_title.len() <= 256);
    assert!(view.items[0].untrusted_title.ends_with('…'));
    let records = (0..=MAX_TIMELINE_VIEW_ITEMS)
        .map(|index| record("home-secret-id", &format!("busy-{index}"), "Busy", 60, 90))
        .collect();
    fixture
        .day()
        .import_calendar(fixture.person, 3, day(), records, now())
        .await
        .unwrap();
    grant.connection_revision = 4;
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::BudgetExceeded)
    );
}

#[tokio::test]
async fn input_window_honors_dst_day_bounds_without_assuming_twenty_four_hours() {
    let fixture = Fixture::new().await;
    for (initial, final_offset, hours) in [(-18_000, -14_400, 23), (-14_400, -18_000, 25)] {
        let mut grant = fixture.grant();
        grant.day.timezone_offset_seconds = initial;
        grant.day.end_timezone_offset_seconds = Some(final_offset);
        let (start, end) = range_bounds(&grant.day).unwrap();
        assert_eq!((end - start).num_hours(), hours);
        grant.starts_at = start;
        grant.ends_at = end;
        grant.validate(now()).unwrap();
        grant.starts_at = start + TimeDelta::hours(8);
        grant.ends_at = start + TimeDelta::hours(16);
        grant.validate(now()).unwrap();
    }
}

#[tokio::test]
async fn grants_accept_bounded_multi_day_and_historical_ranges() {
    let fixture = Fixture::new().await;
    let mut grant = fixture.grant();
    grant.day.end_date_exclusive = grant.day.start_date + TimeDelta::days(7);
    (grant.starts_at, grant.ends_at) = range_bounds(&grant.day).unwrap();
    grant.validate(now()).unwrap();

    grant.day.start_date -= TimeDelta::days(30);
    grant.day.end_date_exclusive -= TimeDelta::days(30);
    (grant.starts_at, grant.ends_at) = range_bounds(&grant.day).unwrap();
    grant.validate(now()).unwrap();

    grant.day.end_date_exclusive =
        grant.day.start_date + TimeDelta::days(MAX_TIMELINE_VIEW_DAYS + 1);
    grant.ends_at = grant.starts_at + TimeDelta::days(MAX_TIMELINE_VIEW_DAYS + 1);
    assert_eq!(grant.validate(now()), Err(AgentFailure::InvalidInput));
}

#[tokio::test]
async fn cancelled_and_expired_reads_do_not_start_authority_work() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    let read = request(&grant);
    read.cancellation.cancel();
    assert_eq!(views.timeline(read).await, Err(AgentFailure::Cancelled));
    let mut read = request(&grant);
    read.deadline = Instant::now();
    assert_eq!(
        views.timeline(read).await,
        Err(AgentFailure::DeadlineExceeded)
    );
    assert_eq!(access.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn pending_access_is_cancelled_on_stop_deadline_and_dropped_view_future() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    for mode in 0..3 {
        let access = Access {
            pending: true,
            ..Access::default()
        };
        let views =
            CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now)
                .unwrap();
        let mut read = request(&grant);
        if mode == 1 {
            read.deadline = Instant::now() + Duration::from_millis(50);
        }
        let cancellation = read.cancellation.clone();
        {
            let operation = views.timeline(read);
            tokio::pin!(operation);
            tokio::select! {
                _ = &mut operation => panic!("access should wait"),
                result = tokio::time::timeout(Duration::from_secs(1), access.started.notified()) => result.unwrap(),
            }
            if mode == 0 {
                cancellation.cancel();
                assert_eq!(operation.await, Err(AgentFailure::Cancelled));
            } else if mode == 1 {
                assert_eq!(operation.await, Err(AgentFailure::DeadlineExceeded));
            }
        }
        assert!(
            access
                .child
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .is_cancelled()
        );
        assert_eq!(cancellation.is_cancelled(), mode == 0);
    }
}

#[tokio::test]
async fn mirror_size_and_invalid_grant_limits_fail_without_partial_projection() {
    let fixture = Fixture::new().await;
    for mode in 0..4 {
        let mut grant = fixture.grant();
        match mode {
            0 => grant.calendar_ids.push(grant.calendar_ids[0].clone()),
            1 => grant.calendar_ids = (0..5).map(|index| format!("calendar-{index}")).collect(),
            2 => grant.expires_at = now() + TimeDelta::minutes(6),
            _ => grant.ends_at = grant.starts_at,
        }
        assert_eq!(grant.validate(now()), Err(AgentFailure::InvalidInput));
    }
    fixture
        .day()
        .import_calendar(
            fixture.person,
            2,
            day(),
            vec![record(
                "home-secret-id",
                "large",
                &"x".repeat(4_194_304),
                60,
                90,
            )],
            now(),
        )
        .await
        .unwrap();
    let mut grant = fixture.grant();
    grant.connection_revision = 3;
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::BudgetExceeded)
    );
}

#[tokio::test]
async fn native_eventkit_without_observation_does_not_use_mirror_payload() {
    let fixture = Fixture::new().await;
    fixture
        .day()
        .select_calendar(
            fixture.person,
            CalendarProvider::EventKit,
            "home-secret-id".into(),
            "Fake EventKit".into(),
        )
        .await
        .unwrap();
    fixture
        .day()
        .import_calendar(
            fixture.person,
            3,
            day(),
            vec![record(
                "home-secret-id",
                "native-id",
                "Personal-class fictional title",
                60,
                90,
            )],
            now(),
        )
        .await
        .unwrap();
    let mut grant = fixture.grant();
    grant.provider = CalendarProvider::EventKit;
    grant.connection_revision = 4;
    let access = Access::default();
    let views =
        CalendarTimelineViews::new(&fixture.leases, &fixture, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::CapabilityUnavailable)
    );
}
