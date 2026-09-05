use chrono::{Duration, TimeZone, Utc};
use floe_core::{CoreError, ErrorCode, FloeCore, ScheduleModel, ScheduleView};
use floe_domain::*;
use serde_json::json;

struct FixtureModel {
    output: Option<String>,
}

impl ScheduleModel for FixtureModel {
    fn name(&self) -> &str {
        "fixture-not-live"
    }
    async fn generate(&self, view: &ScheduleView) -> Result<String, CoreError> {
        Ok(self.output.clone().unwrap_or_else(|| {
            json!({
            "slot_id": view.slots.last().unwrap().id,
            "reason": "This interval fits the supplied window and avoids the loaded busy times.",
            "source_ids": view.required_source_ids,
        }).to_string()
        }))
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 5, 0, 0, 0).unwrap()
}
fn preference() -> FocusPreferenceInput {
    FocusPreferenceInput {
        start_minute: 600,
        end_minute: 900,
        duration_minutes: 45,
    }
}

#[tokio::test]
async fn preference_crud_is_person_scoped_revision_checked_and_persistent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("focus.db");
    let person = PersonId::new();
    let other = PersonId::new();
    let core = FloeCore::open(&path).await.unwrap();
    assert!(core.focus_preference(person).await.unwrap().is_none());
    let saved = core
        .set_focus_preference(person, 0, Some(preference()), now())
        .await
        .unwrap();
    assert_eq!(saved.source, FocusPreferenceSource::UserEntered);
    assert_eq!(saved.person_id, person);
    assert!(core.focus_preference(other).await.unwrap().is_none());
    assert_eq!(
        core.set_focus_preference(person, 0, None, now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    let updated = core
        .set_focus_preference(
            person,
            saved.revision,
            Some(FocusPreferenceInput::default()),
            now(),
        )
        .await
        .unwrap();
    assert_eq!(updated.revision, 2);
    drop(core);
    let core = FloeCore::open(&path).await.unwrap();
    assert_eq!(core.focus_preference(person).await.unwrap(), Some(updated));
    core.set_focus_preference(person, 2, None, now())
        .await
        .unwrap();
    drop(core);
    let core = FloeCore::open(&path).await.unwrap();
    let deleted = core.focus_preference(person).await.unwrap().unwrap();
    assert_eq!(deleted.revision, 3);
    assert!(deleted.value.is_none());
    let view = core
        .schedule_view(person, now().date_naive(), 32400, now())
        .await
        .unwrap();
    assert!(view.preference.is_none());
    assert_eq!(view.required_source_ids, ["schedule"]);
    let encoded = serde_json::to_string(&view).unwrap();
    assert!(!encoded.contains("user_entered"));
    assert!(!encoded.contains(&person.to_string()));
    assert_eq!(
        core.set_focus_preference(person, 2, Some(preference()), now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
}

#[tokio::test]
async fn invalid_preferences_never_persist() {
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    for value in [
        FocusPreferenceInput {
            start_minute: 900,
            end_minute: 600,
            duration_minutes: 30,
        },
        FocusPreferenceInput {
            start_minute: 0,
            end_minute: 1441,
            duration_minutes: 30,
        },
        FocusPreferenceInput {
            start_minute: 600,
            end_minute: 620,
            duration_minutes: 30,
        },
        FocusPreferenceInput {
            start_minute: 600,
            end_minute: 900,
            duration_minutes: 0,
        },
    ] {
        assert_eq!(
            core.set_focus_preference(person, 0, Some(value), now())
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
    }
    assert!(core.focus_preference(person).await.unwrap().is_none());
}

#[tokio::test]
async fn schedule_expert_receives_only_minimal_view_and_proposal_does_not_mutate() {
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    let event = core
        .create_event(
            person,
            "PRIVATE TITLE",
            EventSchedule::Timed(
                TimedSchedule::new(
                    now() + Duration::hours(1),
                    now() + Duration::hours(2),
                    "Asia/Seoul",
                )
                .unwrap(),
            ),
            now(),
        )
        .await
        .unwrap();
    core.create_note(person, "PRIVATE NOTE", now())
        .await
        .unwrap();
    core.create_task(person, "PRIVATE TASK", None, Priority::Normal, now())
        .await
        .unwrap();
    core.set_focus_preference(person, 0, Some(preference()), now())
        .await
        .unwrap();
    let before = core
        .day_snapshot(person, now().date_naive(), 32400, now())
        .await
        .unwrap();
    let view = core
        .schedule_view(person, now().date_naive(), 32400, now())
        .await
        .unwrap();
    assert_eq!(view.busy.len(), 1);
    assert_eq!(view.required_source_ids, ["schedule", "preference"]);
    assert!(
        view.slots
            .iter()
            .all(|slot| slot.starts_at >= now() + Duration::hours(2))
    );
    let encoded = serde_json::to_string(&view).unwrap();
    for private in [
        "PRIVATE",
        "Asia/Seoul",
        &person.to_string(),
        &event.id.to_string(),
    ] {
        assert!(!encoded.contains(private));
    }
    let proposal = core
        .suggest_focus(
            person,
            now().date_naive(),
            32400,
            now(),
            &FixtureModel { output: None },
        )
        .await
        .unwrap();
    assert!(
        proposal
            .evidence
            .iter()
            .any(|source| source.label.contains("PRIVATE TITLE"))
    );
    assert_eq!(proposal.person_id, person);
    assert_eq!(proposal.inference_class, "fixture-not-live");
    assert_eq!(
        core.day_snapshot(person, now().date_naive(), 32400, now())
            .await
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn empty_schedule_uses_defaults_not_fabricated_memory() {
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    let view = core
        .schedule_view(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert!(view.busy.is_empty());
    assert!(view.preference.is_none());
    assert!(!view.slots.is_empty());
    let proposal = core
        .suggest_focus(
            person,
            now().date_naive(),
            0,
            now(),
            &FixtureModel { output: None },
        )
        .await
        .unwrap();
    assert_eq!(proposal.evidence.len(), 1);
    assert!(proposal.calendar_warning);
    assert_eq!(
        proposal.slot.ends_at - proposal.slot.starts_at,
        Duration::minutes(60)
    );
}

#[tokio::test]
async fn malformed_and_fabricated_candidates_are_rejected() {
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    for output in [
        "not JSON".into(),
        json!({"slot_id": "tomorrow", "reason": "Free", "source_ids": ["schedule"]}).to_string(),
        json!({"slot_id": "slot_1", "reason": "Free", "source_ids": ["made_up"]}).to_string(),
        json!({"slot_id": "slot_1", "reason": "Free", "source_ids": ["schedule", "schedule"]}).to_string(),
        json!({"slot_id": "slot_1", "reason": "Free", "source_ids": ["schedule"], "create_event": true}).to_string(),
        json!({"slot_id": "slot_1", "reason": "  ", "source_ids": ["schedule"]}).to_string(),
        json!({"slot_id": "slot_1", "reason": "x".repeat(1001), "source_ids": ["schedule"]}).to_string(),
        "x".repeat(8193),
    ] {
        assert_eq!(core.suggest_focus(person, now().date_naive(), 0, now(), &FixtureModel { output: Some(output) }).await.unwrap_err().code, ErrorCode::InvalidProposal);
    }
    assert!(
        core.day_snapshot(person, now().date_naive(), 0, now())
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn deleted_preference_cannot_be_cited() {
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    core.set_focus_preference(person, 0, Some(preference()), now())
        .await
        .unwrap();
    core.set_focus_preference(person, 1, None, now())
        .await
        .unwrap();
    let output = json!({"slot_id": "slot_1", "reason": "Matches preference", "source_ids": ["schedule", "preference"]}).to_string();
    assert_eq!(
        core.suggest_focus(
            person,
            now().date_naive(),
            0,
            now(),
            &FixtureModel {
                output: Some(output)
            }
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::InvalidProposal
    );
}

#[tokio::test]
async fn all_day_and_elapsed_days_do_not_call_the_model() {
    struct Uncalled;
    impl ScheduleModel for Uncalled {
        fn name(&self) -> &str {
            "uncalled"
        }
        async fn generate(&self, _: &ScheduleView) -> Result<String, CoreError> {
            panic!("model should not run")
        }
    }
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    core.create_event(
        person,
        "Away",
        EventSchedule::AllDay(
            AllDaySchedule::new(now().date_naive(), now().date_naive().succ_opt().unwrap())
                .unwrap(),
        ),
        now(),
    )
    .await
    .unwrap();
    assert_eq!(
        core.suggest_focus(person, now().date_naive(), 0, now(), &Uncalled)
            .await
            .unwrap_err()
            .code,
        ErrorCode::NoFocusSlot
    );
    assert_eq!(
        core.suggest_focus(
            PersonId::new(),
            now().date_naive().pred_opt().unwrap(),
            0,
            now(),
            &Uncalled
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::NoFocusSlot
    );
}

#[tokio::test]
async fn midnight_overlap_and_other_person_events_are_handled() {
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    let day_start = now() - Duration::hours(9);
    core.create_event(
        person,
        "Overnight",
        EventSchedule::Timed(
            TimedSchedule::new(
                day_start - Duration::hours(1),
                now() + Duration::hours(2),
                "UTC",
            )
            .unwrap(),
        ),
        day_start,
    )
    .await
    .unwrap();
    core.create_event(
        PersonId::new(),
        "Other person",
        EventSchedule::AllDay(
            AllDaySchedule::new(now().date_naive(), now().date_naive().succ_opt().unwrap())
                .unwrap(),
        ),
        now(),
    )
    .await
    .unwrap();
    let view = core
        .schedule_view(person, now().date_naive(), 32400, now())
        .await
        .unwrap();
    assert_eq!(view.busy.len(), 1);
    assert_eq!(view.busy[0].starts_at, day_start);
    assert!(
        view.slots
            .iter()
            .all(|slot| slot.starts_at >= now() + Duration::hours(2)
                && slot.ends_at <= day_start + Duration::hours(18))
    );
    assert_eq!(
        core.schedule_view(person, now().date_naive(), i32::MAX, now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Validation
    );
}

#[tokio::test]
async fn preference_change_during_inference_discards_response() {
    struct ChangingModel<'core> {
        core: &'core FloeCore,
        person: PersonId,
    }
    impl ScheduleModel for ChangingModel<'_> {
        fn name(&self) -> &str {
            "changing-fixture"
        }
        async fn generate(&self, view: &ScheduleView) -> Result<String, CoreError> {
            self.core
                .set_focus_preference(self.person, 0, Some(preference()), now())
                .await?;
            FixtureModel { output: None }.generate(view).await
        }
    }
    let core = FloeCore::open(":memory:").await.unwrap();
    let person = PersonId::new();
    let model = ChangingModel {
        core: &core,
        person,
    };
    assert_eq!(
        core.suggest_focus(person, now().date_naive(), 0, now(), &model)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
}
