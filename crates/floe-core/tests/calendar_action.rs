use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use chrono::{DateTime, Duration, TimeZone, Utc};
use floe_core::*;
use floe_domain::*;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 5, 0, 0, 0).unwrap()
}

#[derive(Default)]
struct Provider {
    creates: AtomicUsize,
    preflights: AtomicUsize,
    receipt: Mutex<Option<CalendarCreateReceipt>>,
    block: Option<ActionBlockReason>,
    failure: Option<ActionFailure>,
    mismatch: bool,
    duplicate: bool,
    suspend_after_create: bool,
}

impl CalendarActionProvider for Provider {
    async fn preflight(
        &self,
        action: &CalendarAction,
        local_events: &[Event],
    ) -> Result<CalendarPreflight, ActionFailure> {
        self.preflights.fetch_add(1, Ordering::SeqCst);
        if self.block == Some(ActionBlockReason::ProviderUnavailable) {
            return Err(ActionFailure::Timeout);
        }
        Ok(CalendarPreflight {
            person_id: action.person_id,
            provider: action.provider,
            calendar_id: action.calendar_id.clone(),
            can_create: self.block != Some(ActionBlockReason::CapabilityUnavailable),
            permission_granted: self.block != Some(ActionBlockReason::PermissionDenied),
            timezone_valid: self.block != Some(ActionBlockReason::InvalidTimezone),
            has_conflict: self.block == Some(ActionBlockReason::ScheduleConflict)
                || local_events.iter().any(|event| event.deleted_at.is_none()
                    && matches!(&event.schedule, EventSchedule::Timed(schedule)
                        if schedule.starts_at < action.schedule.ends_at && schedule.ends_at > action.schedule.starts_at)),
        })
    }

    async fn create(
        &self,
        action: &CalendarAction,
    ) -> Result<CalendarCreateReceipt, ActionFailure> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        let receipt = CalendarCreateReceipt {
            execution_id: action.execution_id,
            person_id: action.person_id,
            provider: action.provider,
            calendar_id: action.calendar_id.clone(),
            external_id: "external-1".into(),
            title: if self.mismatch {
                "Wrong title".into()
            } else {
                action.title.clone()
            },
            schedule: action.schedule.clone(),
        };
        *self.receipt.lock().unwrap() = Some(receipt.clone());
        if self.suspend_after_create {
            std::future::pending::<()>().await;
        }
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        Ok(receipt)
    }

    async fn lookup(
        &self,
        _: &CalendarAction,
    ) -> Result<Vec<CalendarCreateReceipt>, ActionFailure> {
        let receipts: Vec<_> = self.receipt.lock().unwrap().clone().into_iter().collect();
        if self.duplicate {
            return Ok([receipts.clone(), receipts].concat());
        }
        Ok(receipts)
    }
}

async fn fixture() -> (
    tempfile::TempDir,
    FloeCore,
    CalendarAction,
    CalendarActionPolicy,
) {
    let directory = tempfile::tempdir().unwrap();
    let core = FloeCore::open(directory.path().join("actions.db"))
        .await
        .unwrap();
    let person = PersonId::new();
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "calendar-1".into(),
        "Test".into(),
    )
    .await
    .unwrap();
    let action = core
        .propose_calendar_action(
            person,
            "calendar-1".into(),
            "Focus".into(),
            TimedSchedule::new(
                now() + Duration::hours(1),
                now() + Duration::hours(2),
                "Asia/Seoul",
            )
            .unwrap(),
            now(),
        )
        .await
        .unwrap();
    let policy = CalendarActionPolicy {
        person_id: person,
        provider: CalendarProvider::Fixture,
        allowed_calendar_ids: vec!["calendar-1".into()],
        allow_create: true,
    };
    (directory, core, action, policy)
}

async fn approve(core: &FloeCore, action: &CalendarAction) {
    core.decide_calendar_action(action.person_id, action.id, true, now())
        .await
        .unwrap();
}

#[tokio::test]
async fn pending_rejected_and_foreign_person_cannot_execute() {
    let (_directory, core, action, policy) = fixture().await;
    let provider = Provider::default();
    assert_eq!(
        core.calendar_action(PersonId::new(), action.id)
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    assert!(
        core.decide_calendar_action(PersonId::new(), action.id, true, now())
            .await
            .is_err()
    );
    assert!(
        core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .is_err()
    );
    core.decide_calendar_action(action.person_id, action.id, false, now())
        .await
        .unwrap();
    assert!(
        core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .is_err()
    );
    assert!(
        core.decide_calendar_action(action.person_id, action.id, true, now())
            .await
            .is_err()
    );
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
    assert_eq!(provider.preflights.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn success_is_durable_and_receipt_can_be_reimported() {
    let (directory, core, action, policy) = fixture().await;
    let provider = Provider::default();
    approve(&core, &action).await;
    let result = core
        .execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
        .await
        .unwrap();
    assert_eq!(
        result.state,
        CalendarActionState::Succeeded {
            external_id: "external-1".into()
        }
    );
    assert_eq!(result.approved_at, Some(now()));
    assert_eq!(result.execution_id, action.execution_id);
    let receipt = provider.receipt.lock().unwrap().clone().unwrap();
    let range = CalendarRange {
        start_date: now().date_naive(),
        end_date_exclusive: (now() + Duration::days(1)).date_naive(),
        timezone_offset_seconds: 32_400,
    };
    let records = vec![CalendarRecord {
        calendar_id: Some(receipt.calendar_id),
        external_id: receipt.external_id,
        external_revision: "1".into(),
        title: receipt.title,
        schedule: EventSchedule::Timed(receipt.schedule),
    }];
    core.import_calendar(
        action.person_id,
        action.connection_revision,
        range.clone(),
        records.clone(),
        now(),
    )
    .await
    .unwrap();
    core.import_calendar(
        action.person_id,
        action.connection_revision + 1,
        range,
        records,
        now(),
    )
    .await
    .unwrap();
    drop(core);
    let reopened = FloeCore::open(directory.path().join("actions.db"))
        .await
        .unwrap();
    assert_eq!(
        reopened
            .calendar_action(action.person_id, action.id)
            .await
            .unwrap(),
        result
    );
    let snapshot = reopened
        .day_snapshot(action.person_id, now().date_naive(), 32_400, now())
        .await
        .unwrap();
    assert_eq!(snapshot.items.len(), 1);
    assert!(
        reopened
            .execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .is_err()
    );
    assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_execution_claims_create_once() {
    let (_directory, core, action, policy) = fixture().await;
    let provider = Provider::default();
    approve(&core, &action).await;
    let (first, second) = tokio::join!(
        core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now),
        core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now),
    );
    assert_ne!(first.is_ok(), second.is_ok());
    assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn policy_and_connection_changes_block_execution() {
    for reason in [
        ActionBlockReason::PolicyDenied,
        ActionBlockReason::CalendarChanged,
    ] {
        let (_directory, core, action, mut policy) = fixture().await;
        let provider = Provider::default();
        approve(&core, &action).await;
        if reason == ActionBlockReason::PolicyDenied {
            policy.allow_create = false;
        } else {
            core.select_calendar(
                action.person_id,
                CalendarProvider::Fixture,
                "other".into(),
                "Other".into(),
            )
            .await
            .unwrap();
        }
        let result = core
            .execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .unwrap();
        assert_eq!(result.state, CalendarActionState::Blocked { reason });
        assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn preflight_failures_require_new_proposal_and_approval() {
    for reason in [
        ActionBlockReason::PermissionDenied,
        ActionBlockReason::CapabilityUnavailable,
        ActionBlockReason::InvalidTimezone,
        ActionBlockReason::ScheduleConflict,
        ActionBlockReason::ProviderUnavailable,
    ] {
        let (_directory, core, action, policy) = fixture().await;
        let provider = Provider {
            block: Some(reason),
            ..Default::default()
        };
        approve(&core, &action).await;
        let result = core
            .execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .unwrap();
        assert_eq!(result.state, CalendarActionState::Blocked { reason });
        assert!(
            core.execute_calendar_action(
                action.person_id,
                action.id,
                &policy,
                &Provider::default(),
                now
            )
            .await
            .is_err()
        );
        assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn expiry_is_checked_at_approval_and_after_preflight() {
    let (_directory, core, action, policy) = fixture().await;
    let provider = Provider::default();
    let result = core
        .decide_calendar_action(action.person_id, action.id, true, action.expires_at)
        .await
        .unwrap();
    assert_eq!(
        result.state,
        CalendarActionState::Blocked {
            reason: ActionBlockReason::Expired
        }
    );
    let (_directory, core, action, _) = fixture().await;
    let policy = CalendarActionPolicy {
        person_id: action.person_id,
        ..policy
    };
    approve(&core, &action).await;
    let calls = AtomicUsize::new(0);
    let result = core
        .execute_calendar_action(action.person_id, action.id, &policy, &provider, || {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                now()
            } else {
                action.expires_at
            }
        })
        .await
        .unwrap();
    assert_eq!(
        result.state,
        CalendarActionState::Blocked {
            reason: ActionBlockReason::Expired
        }
    );
    assert_eq!(provider.preflights.load(Ordering::SeqCst), 1);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ambiguous_create_recovers_after_restart_without_retry() {
    for failure in [
        ActionFailure::Timeout,
        ActionFailure::PermissionDenied,
        ActionFailure::ProviderUnavailable,
    ] {
        let (directory, core, action, policy) = fixture().await;
        let provider = Provider {
            failure: Some(failure),
            ..Default::default()
        };
        approve(&core, &action).await;
        assert_eq!(
            core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
                .await
                .unwrap()
                .state,
            CalendarActionState::Unknown { reason: failure }
        );
        drop(core);
        let reopened = FloeCore::open(directory.path().join("actions.db"))
            .await
            .unwrap();
        assert!(
            reopened
                .execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
                .await
                .is_err()
        );
        let recovered = reopened
            .recover_calendar_action(action.person_id, action.id, &provider)
            .await
            .unwrap();
        assert_eq!(
            recovered.state,
            CalendarActionState::Succeeded {
                external_id: "external-1".into()
            }
        );
        assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn absent_duplicate_or_mismatched_receipts_never_retry_create() {
    for case in 0..3 {
        let (_directory, core, action, policy) = fixture().await;
        let provider = Provider {
            failure: Some(ActionFailure::Timeout),
            mismatch: case == 1,
            duplicate: case == 2,
            ..Default::default()
        };
        approve(&core, &action).await;
        core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .unwrap();
        if case == 0 {
            *provider.receipt.lock().unwrap() = None;
        }
        for _ in 0..2 {
            assert_eq!(
                core.recover_calendar_action(action.person_id, action.id, &provider)
                    .await
                    .unwrap()
                    .state,
                CalendarActionState::Unknown {
                    reason: ActionFailure::UncertainResult
                }
            );
        }
        assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn malformed_proposals_are_not_persisted() {
    let (_directory, core, action, _) = fixture().await;
    assert!(
        core.propose_calendar_action(
            action.person_id,
            "missing".into(),
            action.title.clone(),
            action.schedule.clone(),
            now()
        )
        .await
        .is_err()
    );
    assert!(
        core.propose_calendar_action(
            action.person_id,
            action.calendar_id.clone(),
            " ".into(),
            action.schedule.clone(),
            now()
        )
        .await
        .is_err()
    );
    let mut invalid = action.schedule.clone();
    invalid.ends_at = invalid.starts_at;
    assert!(
        core.propose_calendar_action(
            action.person_id,
            action.calendar_id,
            action.title,
            invalid,
            now()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn cancellation_after_external_write_leaves_recoverable_executing_state() {
    let (directory, core, action, policy) = fixture().await;
    let provider = Provider {
        suspend_after_create: true,
        ..Default::default()
    };
    approve(&core, &action).await;
    {
        let mut execution = std::pin::pin!(core.execute_calendar_action(
            action.person_id,
            action.id,
            &policy,
            &provider,
            now
        ));
        std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(execution.as_mut(), context).is_pending());
            if provider.creates.load(Ordering::SeqCst) == 1 {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        })
        .await;
    }
    drop(core);
    let reopened = FloeCore::open(directory.path().join("actions.db"))
        .await
        .unwrap();
    assert_eq!(
        reopened
            .calendar_action(action.person_id, action.id)
            .await
            .unwrap()
            .state,
        CalendarActionState::Executing
    );
    assert!(
        reopened
            .execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .is_err()
    );
    assert_eq!(
        reopened
            .recover_calendar_action(action.person_id, action.id, &provider)
            .await
            .unwrap()
            .state,
        CalendarActionState::Succeeded {
            external_id: "external-1".into()
        }
    );
    assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn preflight_includes_current_local_events() {
    let (_directory, core, action, policy) = fixture().await;
    let provider = Provider::default();
    approve(&core, &action).await;
    core.create_event(
        action.person_id,
        "New local event",
        EventSchedule::Timed(action.schedule.clone()),
        now(),
    )
    .await
    .unwrap();
    assert_eq!(
        core.execute_calendar_action(action.person_id, action.id, &policy, &provider, now)
            .await
            .unwrap()
            .state,
        CalendarActionState::Blocked {
            reason: ActionBlockReason::ScheduleConflict
        }
    );
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
}
