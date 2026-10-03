//! Pure Day reconciliation. Complete exact resource batches replace data.
use std::collections::{BTreeMap, BTreeSet};
use chrono::{DateTime, Utc};
use crate::{CalendarAcquisition, CalendarMirror, CalendarMirrorSourceState, CalendarMirrorState, CalendarRecord, CalendarRefreshRequest, CalendarResourceOutcome, CalendarSource, CalendarSourceOutcome, CalendarSourceVersion, CalendarSyncStatus, DayError, Event, EventSchedule, MirrorExpectation, SourceRef};

pub(crate) fn reconcile_refresh(request: &CalendarRefreshRequest, acquisition: &CalendarAcquisition, previous: Option<&CalendarMirror>, now: DateTime<Utc>) -> Result<CalendarMirror, DayError> {
    acquisition.validate(request, now)?;
    reconcile(request.expected_mirror_revision, acquisition, previous)
}
fn reconcile(expectation: MirrorExpectation, acquisition: &CalendarAcquisition, previous: Option<&CalendarMirror>) -> Result<CalendarMirror, DayError> {
    let now = acquisition.completed_at;
    let next_mirror_revision = expectation.next_revision()?;
    if MirrorExpectation::of(previous)? != expectation { return Err(DayError::conflict("calendar mirror changed")); }
    let mut events = previous.map(|mirror| mirror.events.clone()).unwrap_or_default();
    // Reconfigured/removed source data cannot return through a failed refresh.
    events.retain(|event| match &event.source { SourceRef::Calendar(origin) => acquisition.inventory.iter().any(|source| source.source.connection_id() == origin.connection_id && source.provider == origin.provider && source.calendars.iter().any(|calendar| calendar.calendar_id == origin.calendar_id) && previous.and_then(|mirror| mirror.state.source_state(origin.connection_id.as_str())).is_some_and(|state| state.source == *source)), _ => false });
    let mut states = Vec::with_capacity(acquisition.sources.len());
    for outcome in &acquisition.sources {
        let source = outcome.source();
        let mut state = previous.and_then(|mirror| mirror.state.source_state(source.source.connection_id().as_str())).filter(|state| state.source == *source).cloned().unwrap_or_else(|| CalendarMirrorSourceState { source: source.clone(), last_success_at: None, last_range: None, error: None, error_at: None, calendar_statuses: BTreeMap::new() });
        match outcome {
            CalendarSourceOutcome::Unavailable { reason, observed_at, .. } => {
                state.error = Some(*reason); state.error_at = Some(*observed_at);
                for calendar in &source.calendars { let status = state.calendar_statuses.entry(calendar.calendar_id.clone()).or_insert_with(empty_status); status.error = Some(*reason); status.error_at = Some(*observed_at); }
            }
            CalendarSourceOutcome::Acquired { batches, observed_at, .. } => {
                for batch in batches {
                    match batch {
                        CalendarResourceOutcome::Complete { calendar_id, records, observed_at } => {
                            reconcile_resource(&mut events, source, calendar_id, records, &acquisition.range, acquisition.person_id, now, next_mirror_revision)?;
                            state.calendar_statuses.insert(calendar_id.clone(), CalendarSyncStatus { last_success_at: Some(*observed_at), last_range: Some(acquisition.range.clone()), error: None, error_at: None });
                        }
                        CalendarResourceOutcome::Failed { calendar_id, reason, observed_at } => { let status = state.calendar_statuses.entry(calendar_id.clone()).or_insert_with(empty_status); status.error = Some(*reason); status.error_at = Some(*observed_at); }
                    }
                }
                state.error = state.calendar_statuses.values().find_map(|status| status.error);
                state.error_at = state.calendar_statuses.values().filter_map(|status| status.error_at).max();
                if state.error.is_none() { state.last_success_at = Some(*observed_at); state.last_range = Some(acquisition.range.clone()); }
            }
        }
        states.push(state);
    }
    Ok(CalendarMirror { mirror_revision: next_mirror_revision, state: CalendarMirrorState { sources: states }, events })
}
fn empty_status() -> CalendarSyncStatus { CalendarSyncStatus { last_success_at: None, last_range: None, error: None, error_at: None } }
fn reconcile_resource(events: &mut Vec<Event>, source: &CalendarSourceVersion, calendar_id: &str, records: &[CalendarRecord], range: &crate::CalendarRange, person_id: floe_kernel::PersonId, now: DateTime<Utc>, next_mirror_revision: u64) -> Result<(), DayError> {
    let calendar = source.calendars.iter().find(|calendar| calendar.calendar_id == calendar_id).ok_or_else(|| DayError::validation("unselected calendar batch"))?;
    let mut seen = BTreeSet::new(); let mut imported = Vec::with_capacity(records.len());
    for record in records {
        if record.calendar_id != calendar_id || record.external_id.is_empty() || record.external_id.len() > 512 || !record.external_revision.is_valid() || record.title.len() > 4096 || !seen.insert(record.external_id.clone()) { return Err(DayError::validation("invalid or duplicate calendar record")); }
        match &record.schedule { EventSchedule::Timed(value) => { crate::TimedSchedule::new(value.starts_at, value.ends_at, &value.timezone)?; }, EventSchedule::AllDay(value) => { crate::AllDaySchedule::new(value.start_date, value.end_date_exclusive)?; } }
        if !range.contains(&record.schedule) { if matches!(&record.schedule, EventSchedule::AllDay(_)) { continue; } return Err(DayError::validation("timed Calendar record outside requested interval")); }
        let origin = CalendarSource { can_modify: record.can_modify, connection_id: source.source.connection_id(), provider: source.provider, calendar_id: calendar_id.to_owned(), calendar_name: calendar.calendar_name.clone(), external_id: record.external_id.clone(), external_revision: record.external_revision.clone() };
        let mut event = Event::observed_calendar(person_id, record.title.clone(), record.schedule.clone(), origin, now)?;
        event.id = crate::domain::action_collection::calendar_event_id(source, calendar_id, &record.external_id)?;
        // A reappearing cache item must not reuse an old public CAS revision.
        event.revision = floe_kernel::Revision(next_mirror_revision);
        if let Some(previous) = events.iter().find(|event| matches!(&event.source, SourceRef::Calendar(origin) if origin.connection_id == source.source.connection_id() && origin.provider == source.provider && origin.calendar_id == calendar_id && origin.external_id == record.external_id)) {
            event.id = previous.id; event.created_at = previous.created_at; event.revision = previous.revision;
            if event.title == previous.title && event.schedule == previous.schedule && event.source == previous.source { event.updated_at = previous.updated_at; } else { event.revision = floe_kernel::Revision(previous.revision.0.checked_add(1).filter(|value| *value <= i64::MAX as u64).ok_or_else(|| DayError::conflict("event revision exhausted"))?); }
        }
        imported.push(event);
    }
    // One resource retains exactly its latest complete interval. A failed
    // batch never reaches this replacement and preserves its prior interval.
    events.retain(|event| !matches!(&event.source, SourceRef::Calendar(origin) if origin.connection_id == source.source.connection_id() && origin.provider == source.provider && origin.calendar_id == calendar_id));
    events.extend(imported); Ok(())
}

impl crate::RefreshCommit {
    /// Repository calls this pure owner validator against the actual mirror
    /// and its current commit clock, before any persistence mutation.
    pub fn validate(&self, current: Option<&CalendarMirror>, now: DateTime<Utc>) -> Result<(), DayError> {
        self.previous.validate()?; self.next.validate()?;
        if !matches!(&self.previous.state, crate::DayRefreshState::Running) || self.previous.transition(self.next.state.clone(), self.next.updated_at)? != self.next { return Err(DayError::conflict("invalid completed refresh transition")); }
        let crate::DayRefreshState::Completed { day } = &self.next.state else { return Err(DayError::conflict("refresh commit requires completion")); };
        self.acquisition.validate_record(&self.previous, now)?;
        let expected = reconcile(self.previous.expected_mirror_revision, &self.acquisition, current)?;
        if expected != self.mirror || day.person_id != self.previous.person_id || day.date != self.previous.query.date || day.generated_at != self.previous.query.now || day.timezone_offset_seconds != self.previous.query.timezone_offset_seconds || day.calendar_mirror_revision != Some(self.mirror.mirror_revision) || day.calendar != Some(crate::project_calendar_coverage(&self.mirror.state, &self.previous.query.range()?, self.acquisition.completed_at)) { return Err(DayError::validation("completed refresh does not match acquired mirror")); }
        Ok(())
    }
}
