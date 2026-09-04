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
        self.select_calendars(
            person_id,
            provider,
            vec![CalendarSelection {
                calendar_id,
                calendar_name,
            }],
        )
        .await
    }

    pub async fn select_calendars(
        &self,
        person_id: PersonId,
        provider: CalendarProvider,
        mut calendars: Vec<CalendarSelection>,
    ) -> Result<(), CoreError> {
        calendars.sort_by(|left, right| left.calendar_id.cmp(&right.calendar_id));
        let mut identifiers = HashSet::new();
        if calendars.is_empty()
            || calendars.iter().any(|calendar| {
                calendar.calendar_id.trim().is_empty()
                    || calendar.calendar_name.trim().is_empty()
                    || !identifiers.insert(calendar.calendar_id.clone())
            })
        {
            return Err(validation("calendar identity must not be empty"));
        }
        let previous = self.store.calendar_mirror(person_id).await?;
        if previous.as_ref().is_some_and(|mirror| {
            mirror.connection.provider == provider
                && mirror.connection.selected_calendars() == calendars
        }) {
            return Ok(());
        }
        let revision = previous
            .as_ref()
            .map_or(1, |mirror| mirror.connection.revision + 1);
        let events = previous.as_ref().map_or_else(Vec::new, |mirror| {
            mirror.events.iter().filter(|event| {
                matches!(&event.source, SourceRef::Calendar(source) if source.provider == provider && identifiers.contains(&source.calendar_id))
            }).cloned().collect()
        });
        self.store
            .put_calendar_mirror(
                person_id,
                &CalendarMirror {
                    connection: CalendarConnection {
                        provider,
                        calendar_id: calendars[0].calendar_id.clone(),
                        calendar_name: calendars[0].calendar_name.clone(),
                        calendars,
                        revision,
                        last_success_at: None,
                        last_range: None,
                        error: None,
                    },
                    events,
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
        let calendars = mirror.connection.selected_calendars();
        for record in records {
            let calendar_id = record
                .calendar_id
                .as_deref()
                .or_else(|| (calendars.len() == 1).then_some(calendars[0].calendar_id.as_str()))
                .ok_or_else(|| validation("calendar identity is required"))?;
            let calendar = calendars
                .iter()
                .find(|calendar| calendar.calendar_id == calendar_id)
                .ok_or_else(|| validation("calendar is not selected"))?;
            if record.external_id.trim().is_empty()
                || record.external_revision.trim().is_empty()
                || !seen.insert((calendar_id.to_owned(), record.external_id.clone()))
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
                calendar_id: calendar.calendar_id.clone(),
                calendar_name: calendar.calendar_name.clone(),
                external_id: record.external_id.clone(),
                external_revision: record.external_revision,
            });
            let mut event = Event::new(person_id, record.title, record.schedule, source, now)?;
            if let Some(previous) = mirror.events.iter().find(|event| {
                matches!(&event.source, SourceRef::Calendar(source) if source.calendar_id == calendar_id && source.external_id == record.external_id)
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
                && !matches!(&event.source, SourceRef::Calendar(source) if seen.contains(&(source.calendar_id.clone(), source.external_id.clone())))
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
