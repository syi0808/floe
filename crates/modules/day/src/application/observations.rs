use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::{
    CalendarBatch, CalendarFailure, CalendarMirror, CalendarMirrorInput, CalendarMirrorState,
    CalendarRange, CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, DayError,
    DayService, Event, EventSchedule, SourceRef, TimelineRepository,
};
use floe_kernel::PersonId;

impl<'a, Repository: TimelineRepository + ?Sized> DayService<'a, Repository> {
    pub async fn record_calendar_failure(
        &self,
        person_id: PersonId,
        expected_mirror_revision: Option<u64>,
        input: CalendarMirrorInput,
        failure: CalendarFailure,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        let (mut mirror, previous) = self
            .calendar_at_revision(person_id, expected_mirror_revision, &input)
            .await?;
        mirror.state.error = Some(failure);
        mirror.state.error_at = Some(now);
        for calendar in &input.calendars {
            let status = mirror
                .state
                .source_statuses
                .entry(calendar.calendar_id.clone())
                .or_insert(CalendarSyncStatus {
                    last_success_at: mirror.state.last_success_at,
                    last_range: mirror.state.last_range.clone(),
                    error: None,
                    error_at: None,
                });
            status.error = Some(failure);
            status.error_at = Some(now);
        }
        self.put_calendar_mirror(person_id, mirror, previous).await
    }

    pub async fn import_calendar(
        &self,
        person_id: PersonId,
        expected_mirror_revision: Option<u64>,
        input: CalendarMirrorInput,
        range: CalendarRange,
        records: Vec<CalendarRecord>,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        if !range.is_valid() || records.len() > 10_000 {
            return Err(validation("invalid calendar range, source, or batch size"));
        }
        let (mut mirror, previous) = self
            .calendar_at_revision(person_id, expected_mirror_revision, &input)
            .await?;
        mirror.events =
            reconcile_records(person_id, &mirror, &input.calendars, &range, records, now)?;
        mirror.state.last_success_at = Some(now);
        mirror.state.last_range = Some(range.clone());
        mirror.state.error = None;
        mirror.state.error_at = None;
        for calendar in &input.calendars {
            mirror.state.source_statuses.insert(
                calendar.calendar_id.clone(),
                CalendarSyncStatus {
                    last_success_at: Some(now),
                    last_range: Some(range.clone()),
                    error: None,
                    error_at: None,
                },
            );
        }
        self.put_calendar_mirror(person_id, mirror, previous).await
    }

    pub async fn import_calendar_sources(
        &self,
        person_id: PersonId,
        expected_mirror_revision: Option<u64>,
        input: CalendarMirrorInput,
        range: CalendarRange,
        batches: Vec<CalendarBatch>,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        if !range.is_valid()
            || batches
                .iter()
                .map(|batch| batch.records.len())
                .sum::<usize>()
                > 10_000
        {
            return Err(validation("invalid range or batch size"));
        }
        let (mut mirror, previous) = self
            .calendar_at_revision(person_id, expected_mirror_revision, &input)
            .await?;
        let expected: HashSet<_> = input
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.as_str())
            .collect();
        let received: HashSet<_> = batches
            .iter()
            .map(|batch| batch.calendar_id.as_str())
            .collect();
        if expected != received
            || received.len() != batches.len()
            || batches
                .iter()
                .any(|batch| batch.failure.is_some() && !batch.records.is_empty())
        {
            return Err(validation(
                "exactly one complete result per selected source is required",
            ));
        }
        for batch in batches {
            let calendar = input
                .calendars
                .iter()
                .find(|calendar| calendar.calendar_id == batch.calendar_id)
                .expect("batch handles validated against source input");
            let mut source = mirror.clone();
            source.events.retain(|event| matches!(&event.source, SourceRef::Calendar(origin) if origin.calendar_id == batch.calendar_id));
            let result = if let Some(failure) = batch.failure {
                Err(failure)
            } else {
                reconcile_records(
                    person_id,
                    &source,
                    std::slice::from_ref(calendar),
                    &range,
                    batch.records,
                    now,
                )
                .map_err(|_| CalendarFailure::ProviderUnavailable)
            };
            let status = mirror
                .state
                .source_statuses
                .entry(batch.calendar_id.clone())
                .or_insert(CalendarSyncStatus {
                    last_success_at: previous
                        .as_ref()
                        .and_then(|mirror| mirror.state.last_success_at),
                    last_range: previous
                        .as_ref()
                        .and_then(|mirror| mirror.state.last_range.clone()),
                    error: None,
                    error_at: None,
                });
            match result {
                Ok(events) => {
                    mirror.events.retain(|event| !matches!(&event.source, SourceRef::Calendar(origin) if origin.calendar_id == batch.calendar_id));
                    mirror.events.extend(events);
                    *status = CalendarSyncStatus {
                        last_success_at: Some(now),
                        last_range: Some(range.clone()),
                        error: None,
                        error_at: None,
                    };
                }
                Err(failure) => {
                    status.error = Some(failure);
                    status.error_at = Some(now);
                }
            }
        }
        mirror.state.error = mirror
            .state
            .source_statuses
            .values()
            .find_map(|status| status.error);
        mirror.state.error_at = mirror
            .state
            .source_statuses
            .values()
            .filter_map(|status| status.error_at)
            .max();
        if mirror.state.error.is_none() {
            mirror.state.last_success_at = Some(now);
            mirror.state.last_range = Some(range);
        }
        self.put_calendar_mirror(person_id, mirror, previous).await
    }

    async fn calendar_at_revision(
        &self,
        person_id: PersonId,
        expected_revision: Option<u64>,
        input: &CalendarMirrorInput,
    ) -> Result<(CalendarMirror, Option<CalendarMirror>), DayError> {
        validate_input(input)?;
        let previous = self.repository.calendar_mirror(person_id).await?;
        if previous.as_ref().map(|mirror| mirror.mirror_revision) != expected_revision {
            return Err(DayError::conflict(
                "calendar mirror changed; reload and retry",
            ));
        }
        let same_source = previous.as_ref().is_some_and(|mirror| {
            mirror.state.source_connection_id == input.source_connection_id
                && mirror.state.provider == input.provider
        });
        let mut mirror = if same_source {
            previous.as_ref().expect("same source has mirror").clone()
        } else {
            CalendarMirror {
                mirror_revision: previous.as_ref().map_or(0, |mirror| mirror.mirror_revision),
                state: CalendarMirrorState {
                    source_connection_id: input.source_connection_id.clone(),
                    provider: input.provider,
                    last_success_at: None,
                    last_range: None,
                    error: None,
                    error_at: None,
                    source_statuses: Default::default(),
                },
                events: Vec::new(),
            }
        };
        let handles: HashSet<_> = input
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.as_str())
            .collect();
        mirror.events.retain(|event| matches!(&event.source, SourceRef::Calendar(source) if source.provider == input.provider && handles.contains(source.calendar_id.as_str())));
        mirror
            .state
            .source_statuses
            .retain(|handle, _| handles.contains(handle.as_str()));
        Ok((mirror, previous))
    }

    async fn put_calendar_mirror(
        &self,
        person_id: PersonId,
        mut mirror: CalendarMirror,
        previous: Option<CalendarMirror>,
    ) -> Result<(), DayError> {
        mirror.mirror_revision = next_mirror_revision(mirror.mirror_revision)?;
        self.repository
            .put_calendar_mirror(person_id, &mirror, previous.as_ref())
            .await
    }
}

fn validate_input(input: &CalendarMirrorInput) -> Result<(), DayError> {
    if input.source_connection_id.trim().is_empty() {
        return Err(validation("calendar source identity is empty"));
    }
    let mut handles = HashSet::new();
    if input.calendars.iter().any(|calendar| {
        calendar.calendar_id.trim().is_empty()
            || calendar.calendar_name.trim().is_empty()
            || !handles.insert(calendar.calendar_id.as_str())
    }) {
        return Err(validation("invalid calendar source input"));
    }
    Ok(())
}

fn next_mirror_revision(current: u64) -> Result<u64, DayError> {
    current
        .checked_add(1)
        .filter(|revision| *revision <= i64::MAX as u64)
        .ok_or_else(|| validation("calendar mirror revision exhausted"))
}

fn reconcile_records(
    person_id: PersonId,
    mirror: &CalendarMirror,
    calendars: &[CalendarSelection],
    range: &CalendarRange,
    records: Vec<CalendarRecord>,
    now: DateTime<Utc>,
) -> Result<Vec<Event>, DayError> {
    let mut seen = HashSet::new();
    let mut imported = Vec::new();
    for record in records {
        let calendar = calendars
            .iter()
            .find(|calendar| calendar.calendar_id == record.calendar_id)
            .ok_or_else(|| validation("calendar is not in the current source input"))?;
        if record.external_id.trim().is_empty()
            || record.external_revision.trim().is_empty()
            || !seen.insert((record.calendar_id.clone(), record.external_id.clone()))
            || !range.contains(&record.schedule)
        {
            return Err(validation(
                "invalid, duplicate, or out-of-range calendar record",
            ));
        }
        match &record.schedule {
            EventSchedule::Timed(value) => {
                crate::TimedSchedule::new(value.starts_at, value.ends_at, &value.timezone)?;
            }
            EventSchedule::AllDay(value) => {
                crate::AllDaySchedule::new(value.start_date, value.end_date_exclusive)?;
            }
        }
        let source = SourceRef::Calendar(CalendarSource {
            can_modify: record.can_modify,
            provider: mirror.state.provider,
            calendar_id: calendar.calendar_id.clone(),
            calendar_name: calendar.calendar_name.clone(),
            external_id: record.external_id.clone(),
            external_revision: record.external_revision,
        });
        let mut event = Event::new(person_id, record.title, record.schedule, source, now)?;
        if let Some(previous) = mirror.events.iter().find(|event| matches!(&event.source, SourceRef::Calendar(source) if source.calendar_id == calendar.calendar_id && source.external_id == record.external_id)) {
            event.id = previous.id;
            event.created_at = previous.created_at;
            event.revision = previous.revision;
            if event.title == previous.title && event.schedule == previous.schedule && event.source == previous.source { event.updated_at = previous.updated_at; } else { event.revision = previous.revision.next(); }
        }
        imported.push(event);
    }
    let mut retained = mirror.events.clone();
    retained.retain(|event| !range.contains(&event.schedule) && !imported.iter().any(|updated| updated.id == event.id) && !matches!(&event.source, SourceRef::Calendar(source) if seen.contains(&(source.calendar_id.clone(), source.external_id.clone()))));
    retained.extend(imported);
    Ok(retained)
}

fn validation(message: &str) -> DayError {
    DayError::validation(message)
}
