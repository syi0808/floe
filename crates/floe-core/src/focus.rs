use std::{collections::BTreeSet, future::Future, time::Duration as Timeout};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use floe_domain::*;
use serde::{Deserialize, Serialize};

use crate::{CoreError, ErrorCode, FloeCore};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ScheduleView {
    pub date: NaiveDate,
    pub timezone_offset_seconds: i32,
    pub slots: Vec<FocusSlot>,
    pub preference: Option<FocusPreferenceInput>,
    pub busy: Vec<BusyInterval>,
    pub required_source_ids: Vec<String>,
    pub calendar_warning: bool,
    #[serde(skip)]
    evidence: Vec<FocusEvidence>,
    #[serde(skip)]
    preference_record: Option<FocusPreference>,
    #[serde(skip)]
    events: Vec<Event>,
    #[serde(skip)]
    calendar: Option<CalendarConnection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BusyInterval {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

pub trait ScheduleModel {
    fn name(&self) -> &str;
    fn generate(&self, view: &ScheduleView) -> impl Future<Output = Result<String, CoreError>>;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleCandidate {
    slot_id: String,
    reason: String,
    source_ids: Vec<String>,
}

impl FloeCore {
    pub async fn focus_preference(
        &self,
        person_id: PersonId,
    ) -> Result<Option<FocusPreference>, CoreError> {
        self.store.focus_preference(person_id).await
    }

    pub async fn set_focus_preference(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        value: Option<FocusPreferenceInput>,
        now: DateTime<Utc>,
    ) -> Result<FocusPreference, CoreError> {
        if value.as_ref().is_some_and(|value| !value.is_valid()) {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "invalid focus window or duration",
            ));
        }
        let previous = self.store.focus_preference(person_id).await?;
        if previous.as_ref().map_or(0, |value| value.revision) != expected_revision {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "focus preference changed; reload and retry",
            ));
        }
        let preference = FocusPreference {
            person_id,
            revision: expected_revision.checked_add(1).ok_or_else(|| {
                CoreError::new(ErrorCode::Conflict, "focus preference revision exhausted")
            })?,
            source: FocusPreferenceSource::UserEntered,
            updated_at: now,
            value,
        };
        self.store
            .put_focus_preference(&preference, previous.as_ref())
            .await?;
        Ok(preference)
    }

    pub async fn schedule_view(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        now: DateTime<Utc>,
    ) -> Result<ScheduleView, CoreError> {
        if timezone_offset_seconds.unsigned_abs() > 50_400
            || !(1970..=9998).contains(&chrono::Datelike::year(&date))
        {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "unsupported focus date or timezone offset",
            ));
        }
        let snapshot = self
            .day_snapshot(person_id, date, timezone_offset_seconds, now)
            .await?;
        let preference_record = self.store.focus_preference(person_id).await?;
        let preference = preference_record
            .as_ref()
            .and_then(|record| record.value.clone());
        let window = preference.clone().unwrap_or_default();
        let day_start = date.and_hms_opt(0, 0, 0).unwrap().and_utc()
            - Duration::seconds(i64::from(timezone_offset_seconds));
        let day_end = day_start + Duration::days(1);
        let events: Vec<Event> = snapshot
            .items
            .into_iter()
            .filter_map(|item| match item {
                TimelineItem::Event(event) => Some(event),
                _ => None,
            })
            .collect();
        if events.len() > 512 {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "too many events for a bounded schedule view",
            ));
        }
        let busy: Vec<BusyInterval> = events
            .iter()
            .map(|event| match &event.schedule {
                EventSchedule::Timed(schedule) => BusyInterval {
                    starts_at: schedule.starts_at.max(day_start),
                    ends_at: schedule.ends_at.min(day_end),
                },
                EventSchedule::AllDay(_) => BusyInterval {
                    starts_at: day_start,
                    ends_at: day_end,
                },
            })
            .collect();
        let mut slots = Vec::new();
        for minute in window.start_minute..=window.end_minute - window.duration_minutes {
            let starts_at = day_start + Duration::minutes(i64::from(minute));
            let ends_at = starts_at + Duration::minutes(i64::from(window.duration_minutes));
            if starts_at < now
                || busy
                    .iter()
                    .any(|busy| busy.starts_at < ends_at && busy.ends_at > starts_at)
            {
                continue;
            }
            if slots.last().is_some_and(|previous: &FocusSlot| {
                starts_at - previous.starts_at < Duration::minutes(15)
            }) {
                continue;
            }
            slots.push(FocusSlot {
                id: format!("slot_{}", slots.len() + 1),
                starts_at,
                ends_at,
            });
        }
        let calendar_warning = snapshot.calendar.as_ref().is_none_or(|calendar| {
            calendar.error.is_some()
                || calendar
                    .last_success_at
                    .is_none_or(|last| now - last > Duration::minutes(15))
                || calendar.last_range.as_ref().is_none_or(|range| {
                    date < range.start_date
                        || date >= range.end_date_exclusive
                        || range.timezone_offset_seconds != timezone_offset_seconds
                })
        });
        let mut evidence = vec![FocusEvidence {
            id: "schedule".into(),
            label: format!(
                "Loaded schedule for {date} ({} events); not a guarantee of availability",
                events.len()
            ),
        }];
        if let Some(value) = &preference {
            evidence.push(FocusEvidence {
                id: "preference".into(),
                label: format!(
                    "User-entered preference: {:02}:{:02}–{:02}:{:02}, {} minutes",
                    value.start_minute / 60,
                    value.start_minute % 60,
                    value.end_minute / 60,
                    value.end_minute % 60,
                    value.duration_minutes
                ),
            });
        }
        let required_source_ids = evidence.iter().map(|source| source.id.clone()).collect();
        for event in &events {
            let source = match &event.source {
                SourceRef::Calendar(source) => source.calendar_name.as_str(),
                SourceRef::External(_) => "Previous calendar connection",
                _ => "Local calendar",
            };
            evidence.push(FocusEvidence {
                id: event.id.to_string(),
                label: format!("{} · {source}", event.title),
            });
        }
        Ok(ScheduleView {
            date,
            timezone_offset_seconds,
            slots,
            preference,
            busy,
            calendar_warning,
            required_source_ids,
            evidence,
            preference_record,
            events,
            calendar: snapshot.calendar,
        })
    }

    pub async fn suggest_focus(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        now: DateTime<Utc>,
        model: &impl ScheduleModel,
    ) -> Result<FocusProposal, CoreError> {
        let view = self
            .schedule_view(person_id, date, timezone_offset_seconds, now)
            .await?;
        if view.slots.is_empty() {
            return Err(CoreError::new(
                ErrorCode::NoFocusSlot,
                "no unoccupied focus slot in the requested window",
            ));
        }
        let started = std::time::Instant::now();
        let output = tokio::time::timeout(Timeout::from_secs(45), model.generate(&view))
            .await
            .map_err(|_| CoreError::new(ErrorCode::ModelTimeout, "model request timed out"))??;
        let finished_at = now + Duration::from_std(started.elapsed()).unwrap_or_default();
        let current = self
            .schedule_view(person_id, date, timezone_offset_seconds, now)
            .await?;
        if current != view {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "schedule or preference changed; request a new suggestion",
            ));
        }
        validate_candidate(&view, &output, person_id, finished_at, model.name())
    }
}

fn validate_candidate(
    view: &ScheduleView,
    output: &str,
    person_id: PersonId,
    now: DateTime<Utc>,
    model: &str,
) -> Result<FocusProposal, CoreError> {
    let invalid = || {
        CoreError::new(
            ErrorCode::InvalidProposal,
            "model returned an invalid focus suggestion",
        )
    };
    if output.len() > 8192 {
        return Err(invalid());
    }
    let candidate: ScheduleCandidate = serde_json::from_str(output).map_err(|_| invalid())?;
    let slot = view
        .slots
        .iter()
        .find(|slot| slot.id == candidate.slot_id)
        .ok_or_else(invalid)?;
    let reason = candidate.reason.trim();
    let sources: BTreeSet<_> = candidate.source_ids.iter().collect();
    let required: BTreeSet<_> = view.required_source_ids.iter().collect();
    if reason.is_empty()
        || reason.chars().count() > 1000
        || reason.chars().any(|character| character.is_control())
        || sources != required
        || sources.len() != candidate.source_ids.len()
        || slot.starts_at < now
        || slot.ends_at <= slot.starts_at
    {
        return Err(invalid());
    }
    Ok(FocusProposal {
        id: uuid::Uuid::new_v4().to_string(),
        person_id,
        generated_at: now,
        timezone_offset_seconds: view.timezone_offset_seconds,
        slot: slot.clone(),
        reason: reason.into(),
        evidence: view.evidence.clone(),
        inference_class: model.into(),
        calendar_warning: view.calendar_warning,
    })
}
