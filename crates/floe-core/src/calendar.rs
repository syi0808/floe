use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_domain::*;

use crate::{CoreError, ErrorCode, FloeCore};

impl FloeCore {
    pub async fn select_calendar(
        &self,
        person_id: PersonId,
        provider: CalendarProvider,
        calendar_id: String,
        calendar_name: String,
    ) -> Result<(), CoreError> {
        if calendar_id.trim().is_empty() || calendar_name.trim().is_empty() {
            return Err(validation("calendar identity must not be empty"));
        }
        let previous = self.store.calendar_mirror(person_id).await?;
        if previous.as_ref().is_some_and(|mirror| {
            mirror.connection.provider == provider && mirror.connection.calendar_id == calendar_id
        }) {
            return Ok(());
        }
        let revision = previous
            .as_ref()
            .map_or(1, |mirror| mirror.connection.revision + 1);
        self.store
            .put_calendar_mirror(
                person_id,
                &CalendarMirror {
                    connection: CalendarConnection {
                        provider,
                        calendar_id,
                        calendar_name,
                        revision,
                        last_success_at: None,
                        last_range: None,
                        error: None,
                    },
                    events: vec![],
                },
                previous.as_ref(),
            )
            .await
    }

    pub async fn record_calendar_failure(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        failure: CalendarFailure,
    ) -> Result<(), CoreError> {
        let mut mirror = self
            .calendar_at_revision(person_id, expected_revision)
            .await?;
        let previous = mirror.clone();
        mirror.connection.error = Some(failure);
        mirror.connection.revision += 1;
        self.store
            .put_calendar_mirror(person_id, &mirror, Some(&previous))
            .await
    }

    pub async fn import_calendar(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        range: CalendarRange,
        records: Vec<CalendarRecord>,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        if !range.is_valid() || records.len() > 10_000 {
            return Err(validation("invalid calendar range or batch size"));
        }
        let mut mirror = self
            .calendar_at_revision(person_id, expected_revision)
            .await?;
        let previous = mirror.clone();
        let mut seen = HashSet::new();
        let mut imported = Vec::new();
        for record in records {
            if record.external_id.trim().is_empty()
                || record.external_revision.trim().is_empty()
                || !seen.insert(record.external_id.clone())
                || !range.contains(&record.schedule)
            {
                return Err(validation(
                    "invalid, duplicate, or out-of-range calendar record",
                ));
            }
            match &record.schedule {
                EventSchedule::Timed(value) => {
                    TimedSchedule::new(value.starts_at, value.ends_at, &value.timezone)?;
                }
                EventSchedule::AllDay(value) => {
                    AllDaySchedule::new(value.start_date, value.end_date_exclusive)?;
                }
            }
            let source = SourceRef::Calendar(CalendarSource {
                provider: mirror.connection.provider,
                calendar_id: mirror.connection.calendar_id.clone(),
                calendar_name: mirror.connection.calendar_name.clone(),
                external_id: record.external_id.clone(),
                external_revision: record.external_revision,
            });
            let mut event = Event::new(person_id, record.title, record.schedule, source, now)?;
            if let Some(previous) = mirror.events.iter().find(|event| {
                matches!(&event.source, SourceRef::Calendar(source) if source.external_id == record.external_id)
            }) {
                event.id = previous.id;
                event.created_at = previous.created_at;
                event.revision = previous.revision;
                if event.title == previous.title && event.schedule == previous.schedule && event.source == previous.source {
                    event.updated_at = previous.updated_at;
                } else {
                    event.revision = previous.revision.next();
                }
            }
            imported.push(event);
        }
        mirror.events.retain(|event| {
            !range.contains(&event.schedule)
                && !matches!(&event.source, SourceRef::Calendar(source) if seen.contains(&source.external_id))
        });
        mirror.events.extend(imported);
        mirror.connection.last_success_at = Some(now);
        mirror.connection.last_range = Some(range);
        mirror.connection.error = None;
        mirror.connection.revision += 1;
        self.store
            .put_calendar_mirror(person_id, &mirror, Some(&previous))
            .await
    }

    async fn calendar_at_revision(
        &self,
        person_id: PersonId,
        expected_revision: u64,
    ) -> Result<CalendarMirror, CoreError> {
        let mirror = self
            .store
            .calendar_mirror(person_id)
            .await?
            .ok_or_else(|| CoreError::new(ErrorCode::NotFound, "select a calendar first"))?;
        if mirror.connection.revision != expected_revision {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "calendar selection or sync has changed; reload and retry",
            ));
        }
        Ok(mirror)
    }
}

fn validation(message: &str) -> CoreError {
    CoreError::new(ErrorCode::Validation, message)
}
