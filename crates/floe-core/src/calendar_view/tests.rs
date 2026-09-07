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
        calendar_id: Some(calendar.into()),
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

fn request(grant: &CalendarTimelineGrant) -> TimelineViewRead {
    TimelineViewRead {
        person_id: grant.person_id,
        handle: grant.handle,
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
    let records = (0..33)
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
        assert_eq!(grant.validate(now()).is_ok(), hours <= 24);
        grant.starts_at = start + TimeDelta::hours(8);
        grant.ends_at = start + TimeDelta::hours(16);
        grant.validate(now()).unwrap();
    }
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
    let snapshot = sample.snapshot().unwrap();
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
        schema_version: 1,
        invocation_id,
        instance_id: vault.registry_instance_id(),
        person_id: fixture.person,
        assignment_id,
        expected_registry_revision: revision,
        granted_view_handles: vec![grant.handle],
        allowed_data_classes: vec![DataClass::Synthetic],
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
    session.messages.push(AgentMessage::Capability {
        turn_id,
        call_id: invocation_id,
        capability_id: "calendar.schedule".into(),
        input: "focus".into(),
        result: Ok(serde_json::to_string(&result).unwrap()),
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
