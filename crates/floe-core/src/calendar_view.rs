use std::{collections::HashSet, future::Future, sync::Mutex, time::Duration};

use chrono::{DateTime, Utc};
use floe_agent::{
    AgentFailure, Cancellation, DataClass, ExpertTimelineView, ExpertViews,
    MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS, MAX_TIMELINE_VIEW_ITEMS, TimelineViewItem,
    TimelineViewRead,
};
use floe_domain::{
    CalendarMirror, CalendarProvider, CalendarRange, EventSchedule, PersonId, SourceRef,
};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::FloeCore;

#[derive(Clone)]
pub struct CalendarTimelineGrant {
    pub person_id: PersonId,
    pub handle: Uuid,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub connection_revision: u64,
    pub day: CalendarRange,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl CalendarTimelineGrant {
    pub fn data_class(&self) -> DataClass {
        match self.provider {
            CalendarProvider::Fixture => DataClass::Synthetic,
            CalendarProvider::EventKit
            | CalendarProvider::Google
            | CalendarProvider::Microsoft
            | CalendarProvider::Android => DataClass::Personal,
        }
    }

    fn validate(&self, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        let identifiers: HashSet<_> = self.calendar_ids.iter().collect();
        let (day_start, day_end) = range_bounds(&self.day)?;
        let range_days = (self.day.end_date_exclusive - self.day.start_date).num_days();
        if !(1..=MAX_TIMELINE_VIEW_DAYS).contains(&range_days)
            || self.calendar_ids.is_empty()
            || self.calendar_ids.len() > 4
            || identifiers.len() != self.calendar_ids.len()
            || self
                .calendar_ids
                .iter()
                .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
            || self.starts_at < day_start
            || self.ends_at > day_end
            || self.starts_at >= self.ends_at
            || self.expires_at - now > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.expires_at <= now {
            return Err(AgentFailure::StaleContext);
        }
        Ok(())
    }
}

pub struct CalendarReadAccessRequest {
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub struct CalendarObserveRequest {
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub struct CalendarObservation {
    pub stamp: CalendarReadAccessStamp,
    pub observed_at: DateTime<Utc>,
    pub batches: Vec<floe_domain::CalendarBatch>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarReadAccessStamp {
    pub schema_version: u32,
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub generation: String,
}

pub trait CalendarReadAccess: Sync {
    fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> impl Future<Output = Result<CalendarReadAccessStamp, AgentFailure>> + Send;

    fn observe(
        &self,
        _: CalendarObserveRequest,
    ) -> impl Future<Output = Result<Option<CalendarObservation>, AgentFailure>> + Send {
        async { Ok(None) }
    }
}

pub struct CalendarTimelineViews<'host, Access, Clock> {
    core: &'host FloeCore,
    access: &'host Access,
    clock: Clock,
    grant: CalendarTimelineGrant,
    stamp: Mutex<Option<CalendarReadAccessStamp>>,
    live_observation_expires_at: Mutex<Option<DateTime<Utc>>>,
}

impl<'host, Access: CalendarReadAccess, Clock: Fn() -> DateTime<Utc> + Sync>
    CalendarTimelineViews<'host, Access, Clock>
{
    pub fn new(
        core: &'host FloeCore,
        access: &'host Access,
        grant: CalendarTimelineGrant,
        clock: Clock,
    ) -> Result<Self, AgentFailure> {
        grant.validate(clock())?;
        Ok(Self {
            core,
            access,
            clock,
            grant,
            stamp: Mutex::new(None),
            live_observation_expires_at: Mutex::new(None),
        })
    }

    pub fn grant(&self) -> &CalendarTimelineGrant {
        &self.grant
    }

    pub fn current_time(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    async fn authorized(
        &self,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        check_running(deadline, &cancellation)?;
        let mut actual = self
            .access
            .check(CalendarReadAccessRequest {
                person_id: self.grant.person_id,
                provider: self.grant.provider,
                calendar_ids: self.grant.calendar_ids.clone(),
                deadline,
                cancellation: cancellation.clone(),
            })
            .await?;
        check_running(deadline, &cancellation)?;
        actual.calendar_ids.sort();
        let mut expected = self.grant.calendar_ids.clone();
        expected.sort();
        if actual.schema_version != 1
            || actual.person_id != self.grant.person_id
            || actual.provider != self.grant.provider
            || actual.calendar_ids != expected
            || actual.generation.is_empty()
            || actual.generation.len() > 128
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        if self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .as_ref()
            .is_some_and(|previous| previous != &actual)
        {
            return Err(AgentFailure::StaleContext);
        }
        Ok(actual)
    }

    pub async fn revalidate(
        &self,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<(), AgentFailure> {
        check_running(deadline, &cancellation)?;
        if self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .is_none()
        {
            return Ok(());
        }
        let deadline = deadline.min(Instant::now() + Duration::from_secs(30));
        let child = Cancellation::default();
        let _cancel = CancelAccess(child.clone());
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
            result = async {
                self.authorized(deadline, child.clone()).await?;
                let live_observation_expires_at = {
                    *self
                        .live_observation_expires_at
                        .lock()
                        .map_err(|_| AgentFailure::CapabilityUnavailable)?
                };
                if let Some(expires_at) = live_observation_expires_at {
                    if expires_at <= (self.clock)() {
                        return Err(AgentFailure::StaleContext);
                    }
                    self.authorized(deadline, child.clone()).await?;
                    return Ok(());
                }
                let mirror = self.core.store.bounded_calendar_mirror(self.grant.person_id).await?;
                self.validate_mirror(&mirror, (self.clock)())?;
                self.authorized(deadline, child.clone()).await?;
                Ok(())
            } => result,
        };
        check_running(deadline, &cancellation)?;
        result
    }

    fn validate_mirror(
        &self,
        mirror: &CalendarMirror,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, AgentFailure> {
        self.grant.validate(now)?;
        let connection = &mirror.connection;
        if connection.disconnected
            || connection.provider != self.grant.provider
            || connection.revision != self.grant.connection_revision
        {
            return Err(AgentFailure::StaleContext);
        }
        let selected = connection.selected_calendars();
        let mut expiry = self.grant.expires_at;
        for identifier in &self.grant.calendar_ids {
            if !selected
                .iter()
                .any(|calendar| &calendar.calendar_id == identifier)
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let status = connection
                .source_statuses
                .get(identifier)
                .ok_or(AgentFailure::StaleContext)?;
            if let Some(failure) = status.error {
                return Err(match failure {
                    floe_domain::CalendarFailure::PermissionDenied => {
                        AgentFailure::CapabilityDenied
                    }
                    _ => AgentFailure::CapabilityUnavailable,
                });
            }
            let success = status.last_success_at.ok_or(AgentFailure::StaleContext)?;
            let source_expiry = success
                .checked_add_signed(chrono::Duration::minutes(5))
                .ok_or(AgentFailure::InvalidInput)?;
            let range = status
                .last_range
                .as_ref()
                .ok_or(AgentFailure::StaleContext)?;
            let (start, end) = range_bounds(range)?;
            if success > now
                || source_expiry <= now
                || self.grant.starts_at < start
                || self.grant.ends_at > end
                || self.grant.day.start_date < range.start_date
                || self.grant.day.end_date_exclusive > range.end_date_exclusive
            {
                return Err(AgentFailure::StaleContext);
            }
            expiry = expiry.min(source_expiry);
        }
        if mirror.events.len() > 10_000
            || mirror
                .events
                .iter()
                .any(|event| event.person_id != self.grant.person_id)
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(expiry)
    }

    async fn read(
        &self,
        request: &TimelineViewRead,
        deadline: Instant,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        if request.person_id != self.grant.person_id || request.handle != self.grant.handle {
            return Err(AgentFailure::CapabilityDenied);
        }
        if request.max_items == 0 || request.max_bytes == 0 || request.cursor.is_some() {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.grant.validate((self.clock)())?;
        let (range_start, range_end) = requested_range(request, &self.grant)?;
        let before = self
            .authorized(deadline, request.cancellation.clone())
            .await?;
        if let Some(observation) = self
            .access
            .observe(CalendarObserveRequest {
                person_id: self.grant.person_id,
                provider: self.grant.provider,
                calendar_ids: self.grant.calendar_ids.clone(),
                starts_at: range_start,
                ends_at: range_end,
                deadline,
                cancellation: request.cancellation.clone(),
            })
            .await?
        {
            return self
                .project_observation(
                    request,
                    range_start,
                    range_end,
                    before,
                    observation,
                    deadline,
                )
                .await;
        }
        let mirror = self
            .core
            .store
            .bounded_calendar_mirror(self.grant.person_id)
            .await?;
        let expires = self.validate_mirror(&mirror, (self.clock)())?;
        let mut items = vec![];
        let mut identifiers = HashSet::new();
        for event in &mirror.events {
            if event.deleted_at.is_some() {
                continue;
            }
            let SourceRef::Calendar(source) = &event.source else {
                return Err(AgentFailure::CapabilityUnavailable);
            };
            if !self.grant.calendar_ids.contains(&source.calendar_id) {
                continue;
            }
            if source.provider != self.grant.provider || !identifiers.insert(event.id) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let (start, end) = match &event.schedule {
                EventSchedule::Timed(schedule) => {
                    if schedule.starts_at >= schedule.ends_at {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    (
                        schedule.starts_at.max(range_start),
                        schedule.ends_at.min(range_end),
                    )
                }
                EventSchedule::AllDay(schedule) => {
                    if schedule.start_date >= schedule.end_date_exclusive {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    if schedule.start_date >= self.grant.day.end_date_exclusive
                        || schedule.end_date_exclusive <= self.grant.day.start_date
                    {
                        continue;
                    }
                    let initial_offset = self.grant.day.timezone_offset_seconds;
                    let final_offset = self
                        .grant
                        .day
                        .end_timezone_offset_seconds
                        .unwrap_or(initial_offset);
                    (
                        date_boundary(schedule.start_date, initial_offset.max(final_offset))?
                            .max(range_start),
                        date_boundary(
                            schedule.end_date_exclusive,
                            initial_offset.min(final_offset),
                        )?
                        .min(range_end),
                    )
                }
            };
            if start >= end {
                continue;
            }
            if items.len() >= request.max_items.min(MAX_TIMELINE_VIEW_ITEMS) {
                return Err(AgentFailure::BudgetExceeded);
            }
            items.push(TimelineViewItem {
                evidence_handle: event.id.0,
                untrusted_title: bounded_title(&event.title),
                starts_at_unix_ms: milliseconds(start)?,
                ends_at_unix_ms: milliseconds(end)?,
            });
        }
        items.sort_by_key(|item| {
            (
                item.starts_at_unix_ms,
                item.ends_at_unix_ms,
                item.evidence_handle,
            )
        });
        let view = ExpertTimelineView {
            schema_version: 1,
            handle: self.grant.handle,
            person_id: self.grant.person_id,
            data_class: self.grant.data_class(),
            source_handle: format!(
                "calendar.timeline:{}:{}",
                self.grant.handle, self.grant.connection_revision
            ),
            range_start_unix_ms: milliseconds(range_start)?,
            range_end_unix_ms: milliseconds(range_end)?,
            expires_at_unix_ms: milliseconds(expires)?,
            coverage_complete: true,
            next_cursor: None,
            items,
        };
        if serde_json::to_vec(&view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let after = self
            .authorized(deadline, request.cancellation.clone())
            .await?;
        if before != after {
            return Err(AgentFailure::StaleContext);
        }
        let latest = self
            .core
            .store
            .bounded_calendar_mirror(self.grant.person_id)
            .await?;
        self.validate_mirror(&latest, (self.clock)())?;
        if latest != mirror {
            return Err(AgentFailure::StaleContext);
        }
        check_running(deadline, &request.cancellation)?;
        let mut saved = self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if saved.as_ref().is_some_and(|previous| previous != &before) {
            return Err(AgentFailure::StaleContext);
        }
        *saved = Some(before);
        Ok(view)
    }

    async fn project_observation(
        &self,
        request: &TimelineViewRead,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
        before: CalendarReadAccessStamp,
        mut observation: CalendarObservation,
        deadline: Instant,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        observation.stamp.calendar_ids.sort();
        if observation.stamp != before
            || observation.observed_at > (self.clock)()
            || (self.clock)() - observation.observed_at > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::StaleContext);
        }
        let expected: HashSet<_> = self.grant.calendar_ids.iter().map(String::as_str).collect();
        let received: HashSet<_> = observation
            .batches
            .iter()
            .map(|batch| batch.calendar_id.as_str())
            .collect();
        if expected != received || observation.batches.len() != expected.len() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let mut items = vec![];
        let mut evidence = HashSet::new();
        for batch in observation.batches {
            if let Some(failure) = batch.failure {
                return Err(match failure {
                    floe_domain::CalendarFailure::PermissionDenied => {
                        AgentFailure::CapabilityDenied
                    }
                    _ => AgentFailure::CapabilityUnavailable,
                });
            }
            for record in batch.records {
                if record.calendar_id.as_deref() != Some(batch.calendar_id.as_str())
                    || record.external_id.trim().is_empty()
                    || record.external_revision.trim().is_empty()
                {
                    return Err(AgentFailure::CapabilityUnavailable);
                }
                let (start, end) = observation_schedule_bounds(
                    &record.schedule,
                    range_start,
                    range_end,
                    self.grant.day.timezone_offset_seconds,
                    self.grant.day.end_timezone_offset_seconds,
                )?;
                if start >= end {
                    continue;
                }
                let evidence_handle = Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "floe:calendar:{:?}:{}:{}",
                        self.grant.provider, batch.calendar_id, record.external_id
                    )
                    .as_bytes(),
                );
                if !evidence.insert(evidence_handle) {
                    return Err(AgentFailure::CapabilityUnavailable);
                }
                if items.len() >= request.max_items.min(MAX_TIMELINE_VIEW_ITEMS) {
                    return Err(AgentFailure::BudgetExceeded);
                }
                items.push(TimelineViewItem {
                    evidence_handle,
                    untrusted_title: bounded_title(&record.title),
                    starts_at_unix_ms: milliseconds(start)?,
                    ends_at_unix_ms: milliseconds(end)?,
                });
            }
        }
        items.sort_by_key(|item| {
            (
                item.starts_at_unix_ms,
                item.ends_at_unix_ms,
                item.evidence_handle,
            )
        });
        let expires = self
            .grant
            .expires_at
            .min(observation.observed_at + chrono::Duration::minutes(5));
        let view = ExpertTimelineView {
            schema_version: 1,
            handle: self.grant.handle,
            person_id: self.grant.person_id,
            data_class: self.grant.data_class(),
            source_handle: format!(
                "calendar.observe:{}:{}",
                self.grant.handle, observation.stamp.generation
            ),
            range_start_unix_ms: milliseconds(range_start)?,
            range_end_unix_ms: milliseconds(range_end)?,
            expires_at_unix_ms: milliseconds(expires)?,
            coverage_complete: true,
            next_cursor: None,
            items,
        };
        if serde_json::to_vec(&view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > request.max_bytes.min(MAX_TIMELINE_VIEW_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let after = self
            .authorized(deadline, request.cancellation.clone())
            .await?;
        if before != after {
            return Err(AgentFailure::StaleContext);
        }
        *self
            .stamp
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)? = Some(before);
        *self
            .live_observation_expires_at
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)? = Some(expires);
        Ok(view)
    }
}

fn observation_schedule_bounds(
    schedule: &EventSchedule,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    start_offset: i32,
    end_offset: Option<i32>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    let (start, end) = match schedule {
        EventSchedule::Timed(schedule) => (schedule.starts_at, schedule.ends_at),
        EventSchedule::AllDay(schedule) => {
            let end_offset = end_offset.unwrap_or(start_offset);
            (
                date_boundary(schedule.start_date, start_offset.max(end_offset))?,
                date_boundary(schedule.end_date_exclusive, start_offset.min(end_offset))?,
            )
        }
    };
    if start >= end {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok((start.max(range_start), end.min(range_end)))
}

impl<Access: CalendarReadAccess, Clock: Fn() -> DateTime<Utc> + Sync> ExpertViews
    for CalendarTimelineViews<'_, Access, Clock>
{
    async fn timeline(
        &self,
        request: TimelineViewRead,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        let deadline = request
            .deadline
            .min(Instant::now() + Duration::from_secs(30));
        let child = Cancellation::default();
        let _cancel = CancelAccess(child.clone());
        let bounded = TimelineViewRead {
            person_id: request.person_id,
            handle: request.handle,
            range_start_unix_ms: request.range_start_unix_ms,
            range_end_unix_ms: request.range_end_unix_ms,
            cursor: request.cursor,
            max_items: request.max_items,
            max_bytes: request.max_bytes,
            deadline,
            cancellation: child,
        };
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
            result = self.read(&bounded, deadline) => result,
        };
        check_running(deadline, &request.cancellation)?;
        result
    }
}

fn requested_range(
    request: &TimelineViewRead,
    grant: &CalendarTimelineGrant,
) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    let (start, end) = match (request.range_start_unix_ms, request.range_end_unix_ms) {
        (None, None) => (grant.starts_at, grant.ends_at),
        (Some(start), Some(end)) => (
            DateTime::from_timestamp_millis(
                i64::try_from(start).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?,
            DateTime::from_timestamp_millis(
                i64::try_from(end).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?,
        ),
        _ => return Err(AgentFailure::InvalidInput),
    };
    if start >= end || end - start > chrono::Duration::days(MAX_TIMELINE_VIEW_DAYS + 1) {
        return Err(AgentFailure::InvalidInput);
    }
    Ok((start, end))
}

struct CancelAccess(Cancellation);

impl Drop for CancelAccess {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn range_bounds(range: &CalendarRange) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    if !range.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let start = range
        .start_date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range.timezone_offset_seconds,
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    let end = range
        .end_date_exclusive
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range
                .end_timezone_offset_seconds
                .unwrap_or(range.timezone_offset_seconds),
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    Ok((start, end))
}

fn date_boundary(
    date: chrono::NaiveDate,
    timezone_offset_seconds: i32,
) -> Result<DateTime<Utc>, AgentFailure> {
    date.and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            timezone_offset_seconds,
        )))
        .ok_or(AgentFailure::InvalidInput)
}

fn milliseconds(time: DateTime<Utc>) -> Result<u64, AgentFailure> {
    u64::try_from(time.timestamp_millis()).map_err(|_| AgentFailure::InvalidInput)
}

fn check_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

fn bounded_title(title: &str) -> String {
    if title.len() <= 256 {
        return title.to_owned();
    }
    let boundary = title
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= 253)
        .last()
        .unwrap_or(0);
    format!("{}…", &title[..boundary])
}

#[cfg(test)]
mod tests;
