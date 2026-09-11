use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chrono::{Duration as TimeDelta, TimeZone};
use floe_agent::*;
use floe_domain::*;

use super::*;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2050, 1, 15, 9, 0, 0).unwrap()
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

struct Fixture {
    core: FloeCore,
    person: PersonId,
    root: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let core = FloeCore::open(root.path().join("view.db")).await.unwrap();
        let person = PersonId::new();
        core.select_calendars(
            person,
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
        core.import_calendar(
            person,
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
        Self { core, person, root }
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

#[derive(Default)]
struct Access {
    calls: AtomicUsize,
    denied: AtomicBool,
    change_on_second: bool,
    foreign: bool,
    pending: bool,
    started: tokio::sync::Notify,
    child: Mutex<Option<Cancellation>>,
}

impl CalendarReadAccess for Access {
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
            generation: if self.change_on_second && call > 0 {
                "second"
            } else {
                "first"
            }
            .into(),
        })
    }
}

struct ObservationAccess {
    calendar_id: String,
    record: CalendarRecord,
}

struct ProjectedObservationAccess;

impl CalendarReadAccess for ProjectedObservationAccess {
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

impl CalendarReadAccess for ObservationAccess {
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
    assert!(fixture.root.path().join("view.db").exists());
}

#[tokio::test]
async fn read_range_is_request_scoped_within_the_authorized_source() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let access = Access::default();
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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

    let expanded = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now)
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
    };
    let view = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now)
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
async fn projected_observation_preserves_server_coverage_and_opaque_provenance() {
    let fixture = Fixture::new().await;
    let grant = fixture.grant();
    let view = CalendarTimelineViews::new(
        &fixture.core,
        &ProjectedObservationAccess,
        grant.clone(),
        now,
    )
    .unwrap()
    .timeline(request(&grant))
    .await
    .unwrap();

    assert_eq!(view.source_handle, "calendar.timeline:server");
    assert!(!view.coverage_complete);
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
        .core
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
        let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
        .core
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
    assert!(
        views
            .timeline(request(&grant))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    grant.calendar_ids.push("work-secret-id".into());
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
                .core
                .disconnect_calendar(fixture.person, 2)
                .await
                .unwrap();
        }
        let views =
            CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), clock).unwrap();
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
    let views = CalendarTimelineViews::new(&fixture.core, &changed, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::StaleContext)
    );
    let access = Access::default();
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
    views.timeline(request(&grant)).await.unwrap();
    views
        .revalidate(
            Instant::now() + Duration::from_secs(1),
            Cancellation::default(),
        )
        .await
        .unwrap();
    fixture
        .core
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
async fn all_day_and_long_events_block_the_entire_requested_window_without_false_focus() {
    let fixture = Fixture::new().await;
    let mut all_day = record("home-secret-id", "all-day", "All day", 0, 1);
    all_day.schedule = EventSchedule::AllDay(
        AllDaySchedule::new(day().start_date, day().end_date_exclusive).unwrap(),
    );
    fixture
        .core
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
        .core
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
    let view = views.timeline(request(&grant)).await.unwrap();
    assert!(view.items[0].untrusted_title.len() <= 256);
    assert!(view.items[0].untrusted_title.ends_with('…'));
    let records = (0..=MAX_TIMELINE_VIEW_ITEMS)
        .map(|index| record("home-secret-id", &format!("busy-{index}"), "Busy", 60, 90))
        .collect();
    fixture
        .core
        .import_calendar(fixture.person, 3, day(), records, now())
        .await
        .unwrap();
    grant.connection_revision = 4;
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
        let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
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
        .core
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::BudgetExceeded)
    );
}

#[tokio::test]
async fn eventkit_mirror_projection_is_personal_and_foreign_rows_are_not_evidence() {
    let fixture = Fixture::new().await;
    fixture
        .core
        .select_calendar(
            fixture.person,
            CalendarProvider::EventKit,
            "home-secret-id".into(),
            "Fake EventKit".into(),
        )
        .await
        .unwrap();
    fixture
        .core
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
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await.unwrap().data_class,
        DataClass::Personal
    );
    let previous = fixture
        .core
        .store
        .calendar_mirror(fixture.person)
        .await
        .unwrap()
        .unwrap();
    let mut malformed = previous.clone();
    malformed.events[0].person_id = PersonId::new();
    fixture
        .core
        .store
        .put_calendar_mirror(fixture.person, &malformed, Some(&previous))
        .await
        .unwrap();
    assert_eq!(
        views.timeline(request(&grant)).await,
        Err(AgentFailure::CapabilityUnavailable)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bounded_mirror_view_runs_the_real_expert_and_enters_the_existing_encrypted_s3_bridge() {
    use crate::*;
    use std::os::unix::fs::PermissionsExt;
    #[derive(Default)]
    struct Keys(Mutex<Option<(PersonId, Uuid, [u8; 32])>>);
    impl VaultKeyProvider for Keys {
        fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .as_ref()
                .filter(|(owner, identifier, _)| *owner == person && *identifier == vault)
                .map(|(_, _, key)| VaultKey::from_bytes(*key))
                .ok_or(AgentFailure::VaultUnavailable)
        }
        fn insert(
            &self,
            person: PersonId,
            vault: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            *self.0.lock().unwrap() = Some((person, vault, *key.as_bytes()));
            Ok(())
        }
    }
    let fixture = Fixture::new().await;
    std::fs::set_permissions(fixture.root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let vault = EncryptedAgentVault::create(fixture.root.path(), fixture.person, Keys::default())
        .await
        .unwrap();
    let sample = crate::agent_fixture::FixtureCapabilities::new_with_instance(
        fixture.person,
        vault.registry_instance_id(),
    )
    .unwrap();
    let mut snapshot = sample.snapshot().unwrap();
    snapshot.calendar_views.push(CalendarViewBinding {
        handle: snapshot.assignments[0].granted_view_handles[0],
        person_id: fixture.person,
        provider: CalendarProvider::Fixture,
        device_id: "test-device".into(),
        calendar_ids: fixture.grant().calendar_ids,
        enabled: true,
    });
    vault.initialize_expert_registry(&snapshot).await.unwrap();
    let assignment = snapshot
        .assignments
        .iter()
        .find(|assignment| !assignment.granted_tool_assignments.is_empty())
        .unwrap();
    let mut grant = fixture.grant();
    grant.handle = assignment.granted_view_handles[0];
    let assignment_id = assignment.id;
    let revision = snapshot.revision;
    let registry =
        Mutex::new(AgentRegistry::restore(snapshot, vault.registry_instance_id()).unwrap());
    let access = Access::default();
    let views = CalendarTimelineViews::new(&fixture.core, &access, grant.clone(), now).unwrap();
    let mut session = vault.create_sample_session().await.unwrap();
    let turn_id = Uuid::new_v4();
    let invocation_id = Uuid::new_v4();
    session.active_turn = Some(turn_id);
    session.revision = 1;
    session.messages.push(AgentMessage::User {
        turn_id,
        text: "Find a focus interval in the granted calendars".into(),
    });
    vault.compare_and_swap(&session, 0).await.unwrap();
    let result = ExpertHost {
        registry: &registry,
        views: &views,
    }
    .invoke(ExpertInvocation {
        usage: Default::default(),
        context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        },
        schema_version: 1,
        invocation_id,
        instance_id: vault.registry_instance_id(),
        person_id: fixture.person,
        assignment_id,
        expected_registry_revision: revision,
        granted_view_handles: vec![grant.handle],
        allowed_data_classes: vec![DataClass::Synthetic],
        current_time_unix_ms: milliseconds(now()).unwrap(),
        timezone_offset_seconds: grant.day.timezone_offset_seconds,
        suggested_range_start_unix_ms: Some(milliseconds(grant.starts_at).unwrap()),
        suggested_range_end_unix_ms: Some(milliseconds(grant.ends_at).unwrap()),
        input: ExpertInput::ProposeFocus { focus_minutes: 60 },
        budget: ExpertBudget::default(),
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Cancellation::default(),
    })
    .await
    .unwrap();
    assert_eq!(
        result.action_proposals[0].starts_at_unix_ms,
        milliseconds(now() + TimeDelta::minutes(90)).unwrap()
    );
    assert_eq!(
        result.insights[0],
        ExpertInsight::Commitment {
            evidence_handle: views.timeline(request(&grant)).await.unwrap().items[0]
                .evidence_handle,
            untrusted_title: "Home appointment".into(),
            starts_at_unix_ms: milliseconds(grant.starts_at).unwrap(),
            ends_at_unix_ms: milliseconds(now() + TimeDelta::minutes(90)).unwrap()
        }
    );
    views
        .revalidate(
            Instant::now() + Duration::from_secs(1),
            Cancellation::default(),
        )
        .await
        .unwrap();
    session.revision = 2;
    let context_id = Uuid::new_v4();
    session.messages.push(AgentMessage::Delegation {
        turn_id,
        task: A2ATask {
            id: invocation_id,
            context_id,
            agent_id: result.package.id.clone(),
            state: A2ATaskState::Completed,
            history: vec![A2AMessage {
                message_id: Uuid::new_v4(),
                context_id,
                task_id: Some(invocation_id),
                role: A2AMessageRole::User,
                parts: vec![A2APart::Text {
                    text: "Prepare a bounded scheduling proposal.".into(),
                }],
            }],
            artifacts: vec![A2AArtifact {
                artifact_id: Uuid::new_v4(),
                name: "Schedule expert result".into(),
                parts: vec![A2APart::Data {
                    media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                    data: serde_json::to_string(&result).unwrap(),
                }],
            }],
            failure: None,
        },
    });
    let staged = registry.lock().unwrap().snapshot();
    vault
        .commit_expert_session(&session, 1, revision, &staged)
        .await
        .unwrap();
    let action = fixture
        .core
        .prepare_expert_calendar_action(
            &vault,
            ExpertCalendarRequest {
                reference: ExpertProposalReference {
                    person_id: fixture.person,
                    session_id: session.id,
                    invocation_id,
                },
                destination: ExpertCalendarDestination {
                    provider: CalendarProvider::Fixture,
                    calendar_id: "home-secret-id".into(),
                    connection_revision: 2,
                    timezone: "UTC".into(),
                },
                cancellation: Cancellation::default(),
                deadline: Instant::now() + Duration::from_secs(2),
            },
            now,
        )
        .await
        .unwrap();
    assert_eq!(action.state, CalendarActionState::Pending);
    assert_eq!(
        action.schedule.starts_at.timestamp_millis() as u64,
        result.action_proposals[0].starts_at_unix_ms
    );
    assert_eq!(vault.expert_registry().await.unwrap().unwrap(), staged);
}
