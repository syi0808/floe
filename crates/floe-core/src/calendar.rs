use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_domain::*;

use crate::{CoreError, ErrorCode, FloeCore};

impl FloeCore {
    pub async fn calendar_connection(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarConnection>, CoreError> {
        Ok(self
            .store
            .calendar_mirror(person_id)
            .await?
            .map(|mirror| mirror.connection))
    }

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
        let previous = self.store.calendar_mirror(person_id).await?;
        let revision = previous.as_ref().map_or(1, |mirror| {
            if mirror.connection.provider == provider
                && !mirror.connection.disconnected
                && mirror.connection.calendars == calendars
            {
                mirror.connection.revision
            } else {
                mirror.connection.revision + 1
            }
        });
        self.set_calendar_scope(
            person_id,
            format!("calendar.{}", provider_identifier(provider)),
            revision,
            "fixture-device".into(),
            provider,
            calendars,
            CalendarScope::Selected,
        )
        .await
    }

    pub async fn set_calendar_scope(
        &self,
        person_id: PersonId,
        connection_id: String,
        connection_revision: u64,
        device_id: String,
        provider: CalendarProvider,
        mut calendars: Vec<CalendarSelection>,
        scope: CalendarScope,
    ) -> Result<(), CoreError> {
        calendars.sort_by(|left, right| left.calendar_id.cmp(&right.calendar_id));
        let mut identifiers = HashSet::new();
        if connection_id.trim().is_empty()
            || device_id.trim().is_empty()
            || connection_revision == 0
            || calendars.is_empty()
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
            mirror.connection.connection_id == connection_id
                && mirror.connection.device_id == device_id
                && mirror.connection.revision == connection_revision
                && mirror.connection.provider == provider
                && !mirror.connection.disconnected
                && mirror.connection.scope == scope
                && mirror.connection.calendars == calendars
        }) {
            return Ok(());
        }
        if let Some(previous) = previous
            .as_ref()
            .filter(|mirror| mirror.connection.connection_id == connection_id)
        {
            if connection_revision < previous.connection.revision {
                return Err(CoreError::new(
                    ErrorCode::Conflict,
                    "calendar connection revision cannot decrease",
                ));
            }
            if connection_revision == previous.connection.revision {
                return Err(CoreError::new(
                    ErrorCode::Conflict,
                    "equal calendar connection revision requires an identical binding and scope",
                ));
            }
        }
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
                        connection_id,
                        device_id,
                        disconnected: false,
                        scope,
                        provider,
                        calendars,
                        revision: connection_revision,
                        last_success_at: None,
                        last_range: None,
                        error: None,
                        error_at: None,
                        source_statuses: previous.as_ref().map_or_else(
                            Default::default,
                            |mirror| {
                                mirror
                                    .connection
                                    .source_statuses
                                    .iter()
                                    .filter(|(identifier, _)| identifiers.contains(*identifier))
                                    .map(|(identifier, status)| {
                                        (identifier.clone(), status.clone())
                                    })
                                    .collect()
                            },
                        ),
                    },
                    events,
                },
                previous.as_ref(),
            )
            .await
    }

    pub async fn disconnect_calendar(
        &self,
        person_id: PersonId,
        expected_revision: u64,
    ) -> Result<(), CoreError> {
        let mut mirror = self
            .calendar_at_revision(person_id, expected_revision)
            .await?;
        let previous = mirror.clone();
        mirror.events.clear();
        mirror.connection.disconnected = true;
        mirror.connection.revision += 1;
        mirror.connection.calendars.clear();
        mirror.connection.source_statuses.clear();
        mirror.connection.last_success_at = None;
        mirror.connection.last_range = None;
        mirror.connection.error = None;
        mirror.connection.error_at = None;
        self.store
            .put_calendar_mirror(person_id, &mirror, Some(&previous))
            .await
    }

    pub async fn discover_calendars(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        calendars: Vec<CalendarSelection>,
    ) -> Result<(), CoreError> {
        let mut mirror = self
            .calendar_at_revision(person_id, expected_revision)
            .await?;
        if mirror.connection.scope != CalendarScope::All {
            return Err(validation(
                "discovery cannot expand selected-calendar scope",
            ));
        }
        let previous = mirror.clone();
        let mut seen = HashSet::new();
        let mut included: std::collections::BTreeMap<_, _> = mirror
            .connection
            .calendars
            .iter()
            .cloned()
            .map(|calendar| (calendar.calendar_id.clone(), calendar))
            .collect();
        for calendar in calendars {
            if calendar.calendar_id.trim().is_empty()
                || calendar.calendar_name.trim().is_empty()
                || !seen.insert(calendar.calendar_id.clone())
            {
                return Err(validation("invalid calendar inventory"));
            }
            if !included.contains_key(&calendar.calendar_id) {
                mirror.connection.source_statuses.insert(
                    calendar.calendar_id.clone(),
                    CalendarSyncStatus {
                        last_success_at: None,
                        last_range: None,
                        error: None,
                        error_at: None,
                    },
                );
            }
            included.insert(calendar.calendar_id.clone(), calendar);
        }
        mirror.connection.calendars = included.into_values().collect();
        if mirror.connection.calendars == previous.connection.calendars {
            return Ok(());
        }
        mirror.connection.revision += 1;
        self.store
            .put_calendar_mirror(person_id, &mirror, Some(&previous))
            .await
    }

    pub async fn record_calendar_failure(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        failure: CalendarFailure,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut mirror = self
            .calendar_at_revision(person_id, expected_revision)
            .await?;
        let previous = mirror.clone();
        mirror.connection.error = Some(failure);
        mirror.connection.error_at = Some(now);
        for calendar in &mirror.connection.calendars {
            let status = mirror
                .connection
                .source_statuses
                .entry(calendar.calendar_id.clone())
                .or_insert(CalendarSyncStatus {
                    last_success_at: mirror.connection.last_success_at,
                    last_range: mirror.connection.last_range.clone(),
                    error: None,
                    error_at: None,
                });
            status.error = Some(failure);
            status.error_at = Some(now);
        }
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
        mirror.events = reconcile_records(person_id, &mirror, &range, records, now)?;
        mirror.connection.last_success_at = Some(now);
        mirror.connection.last_range = Some(range.clone());
        mirror.connection.error = None;
        mirror.connection.error_at = None;
        for calendar in &mirror.connection.calendars {
            mirror.connection.source_statuses.insert(
                calendar.calendar_id.clone(),
                CalendarSyncStatus {
                    last_success_at: Some(now),
                    last_range: Some(range.clone()),
                    error: None,
                    error_at: None,
                },
            );
        }
        mirror.connection.revision += 1;
        self.store
            .put_calendar_mirror(person_id, &mirror, Some(&previous))
            .await
    }

    pub async fn import_calendar_sources(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        range: CalendarRange,
        batches: Vec<CalendarBatch>,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        if !range.is_valid()
            || batches
                .iter()
                .map(|batch| batch.records.len())
                .sum::<usize>()
                > 10_000
        {
            return Err(validation("invalid range or batch size"));
        }
        let mut mirror = self
            .calendar_at_revision(person_id, expected_revision)
            .await?;
        let previous = mirror.clone();
        let calendars = mirror.connection.calendars.clone();
        let expected: HashSet<_> = calendars
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
            let calendar = calendars
                .iter()
                .find(|calendar| calendar.calendar_id == batch.calendar_id)
                .unwrap();
            let mut source = mirror.clone();
            source.connection.calendars = vec![calendar.clone()];
            source.events.retain(|event| matches!(&event.source, SourceRef::Calendar(origin) if origin.calendar_id == batch.calendar_id));
            let result = if let Some(failure) = batch.failure {
                Err(failure)
            } else {
                reconcile_records(person_id, &source, &range, batch.records, now)
                    .map_err(|_| CalendarFailure::ProviderUnavailable)
            };
            let status = mirror
                .connection
                .source_statuses
                .entry(batch.calendar_id.clone())
                .or_insert(CalendarSyncStatus {
                    last_success_at: previous.connection.last_success_at,
                    last_range: previous.connection.last_range.clone(),
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
        mirror.connection.error = mirror
            .connection
            .source_statuses
            .values()
            .find_map(|status| status.error);
        mirror.connection.error_at = mirror
            .connection
            .source_statuses
            .values()
            .filter_map(|status| status.error_at)
            .max();
        if mirror.connection.error.is_none() {
            mirror.connection.last_success_at = Some(now);
            mirror.connection.last_range = Some(range);
        }
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
        if mirror.connection.disconnected || mirror.connection.revision != expected_revision {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "calendar selection or sync has changed; reload and retry",
            ));
        }
        Ok(mirror)
    }
}

fn provider_identifier(provider: CalendarProvider) -> &'static str {
    match provider {
        CalendarProvider::Fixture => "fixture",
        CalendarProvider::EventKit => "event_kit",
        CalendarProvider::Google => "google",
        CalendarProvider::Microsoft => "microsoft",
        CalendarProvider::Android => "android",
    }
}

fn reconcile_records(
    person_id: PersonId,
    mirror: &CalendarMirror,
    range: &CalendarRange,
    records: Vec<CalendarRecord>,
    now: DateTime<Utc>,
) -> Result<Vec<Event>, CoreError> {
    let mut seen = HashSet::new();
    let mut imported = Vec::new();
    let calendars = &mirror.connection.calendars;
    for record in records {
        let calendar_id = record.calendar_id.as_str();
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
            can_modify: record.can_modify,
            provider: mirror.connection.provider,
            calendar_id: calendar.calendar_id.clone(),
            calendar_name: calendar.calendar_name.clone(),
            external_id: record.external_id.clone(),
            external_revision: record.external_revision,
        });
        let mut event = Event::new(person_id, record.title, record.schedule, source, now)?;
        if let Some(previous) = mirror.events.iter().find(|event| {
            matches!(&event.source, SourceRef::Calendar(source) if source.calendar_id == calendar_id
                && source.external_id == record.external_id)
        }) {
            event.id = previous.id;
            event.created_at = previous.created_at;
            event.revision = previous.revision;
            if event.title == previous.title
                && event.schedule == previous.schedule
                && event.source == previous.source
            {
                event.updated_at = previous.updated_at;
            } else {
                event.revision = previous.revision.next();
            }
        }
        imported.push(event);
    }
    let mut retained = mirror.events.clone();
    retained.retain(|event| {
            !range.contains(&event.schedule)
                && !imported.iter().any(|updated| updated.id == event.id)
                && !matches!(&event.source, SourceRef::Calendar(source) if seen.contains(&(source.calendar_id.clone(), source.external_id.clone())))
        });
    retained.extend(imported);
    Ok(retained)
}

fn validation(message: &str) -> CoreError {
    CoreError::new(ErrorCode::Validation, message)
}
