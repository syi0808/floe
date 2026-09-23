use std::collections::HashSet;

use chrono::{DateTime, Local, TimeZone, Utc};
use floe_access::{
    CalendarLeaseKey, RemoteCallWindow, admission_matches_dependency, calendar_lease_dependency,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_context_contract::{
    CALENDAR_CONTEXT_VIEW_ID, CalendarContextItem, CalendarContextView, CalendarProvider,
    CalendarViewQuery, ContextDependency, MAX_CALENDAR_CONTEXT_BYTES,
    validate_calendar_context_view_for_query,
};
use floe_day::{CalendarBatch, CalendarFailure, EventSchedule};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    CalendarConnectionReader, CalendarObserveRequest, CalendarSource, NativeCalendarGrantReader,
    SourceLeaseRegistry, admit_current_native_calendar_read,
};

pub struct NativeCalendarViewRead<'a> {
    pub person_id: PersonId,
    pub device_id: &'a str,
    pub consumer: &'a str,
    pub query: &'a CalendarViewQuery,
    pub window: &'a RemoteCallWindow,
}

pub async fn read_native_calendar_view(
    connections: &impl CalendarConnectionReader,
    source: &impl CalendarSource,
    grants: &impl NativeCalendarGrantReader,
    leases: &SourceLeaseRegistry,
    read: NativeCalendarViewRead<'_>,
) -> Result<(CalendarContextView, ContextDependency), AgentFailure> {
    read.query.validate()?;
    if read.query.cursor().is_some() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    check_running(read.window)?;
    let before = admit_current_native_calendar_read(
        connections,
        source,
        grants,
        read.person_id,
        read.device_id,
        read.consumer,
        read.window,
    )
    .await?;
    let range_start = DateTime::from_timestamp_millis(read.query.range_start_unix_ms())
        .ok_or(AgentFailure::InvalidInput)?;
    let range_end = DateTime::from_timestamp_millis(read.query.range_end_unix_ms())
        .ok_or(AgentFailure::InvalidInput)?;
    let calendar_ids = before.stamp.calendar_ids.clone();
    let mut observation = source
        .observe(CalendarObserveRequest {
            person_id: read.person_id,
            device_id: read.device_id.to_owned(),
            provider: before.connection.provider,
            calendar_ids: calendar_ids.clone(),
            expected_native_subject_fingerprint: Some(
                before.stamp.native_subject_fingerprint.clone(),
            ),
            starts_at: range_start,
            ends_at: range_end,
            cursor: None,
            deadline: read.window.deadline,
            cancellation: read.window.cancellation.clone(),
        })
        .await?
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    check_running(read.window)?;
    observation.stamp.calendar_ids.sort();
    if observation.stamp != before.stamp {
        return Err(AgentFailure::StaleContext);
    }
    let now = Utc::now();
    if observation.observed_at > now || now - observation.observed_at > chrono::Duration::minutes(5)
    {
        return Err(AgentFailure::StaleContext);
    }
    let items = project_native_items(
        before.connection.provider,
        &before.connection.connection_id,
        &calendar_ids,
        observation.batches,
        range_start,
        range_end,
        read.query.limit(),
    )?;
    let after = admit_current_native_calendar_read(
        connections,
        source,
        grants,
        read.person_id,
        read.device_id,
        read.consumer,
        read.window,
    )
    .await?;
    if after.connection != before.connection
        || after.stamp != before.stamp
        || after.admission != before.admission
    {
        return Err(AgentFailure::StaleContext);
    }
    let expires_at = observation.observed_at + chrono::Duration::minutes(5);
    if expires_at <= Utc::now() {
        return Err(AgentFailure::StaleContext);
    }
    let observation_id = Uuid::new_v4();
    let view = CalendarContextView {
        schema_version: floe_agent_contract::AGENT_VERSION,
        view_id: CALENDAR_CONTEXT_VIEW_ID.into(),
        source_handle: format!("calendar.observe:{observation_id}"),
        observed_at_unix_ms: observation.observed_at.timestamp_millis(),
        expires_at_unix_ms: expires_at.timestamp_millis(),
        range_start_unix_ms: read.query.range_start_unix_ms(),
        range_end_unix_ms: read.query.range_end_unix_ms(),
        coverage_complete: true,
        next_cursor: None,
        items,
    };
    validate_calendar_context_view_for_query(&view, read.query, Utc::now().timestamp_millis())?;
    let invocation_id = Uuid::new_v4();
    let key = CalendarLeaseKey {
        invocation_id,
        person_id: read.person_id,
        handle: Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            before.connection.connection_id.as_bytes(),
        ),
        device_id: read.device_id.to_owned(),
        calendar_ids,
        range_start_unix_ms: read.query.range_start_unix_ms(),
        range_end_unix_ms: read.query.range_end_unix_ms(),
        cursor: None,
        timezone_offset_seconds: range_start.with_timezone(&Local).offset().local_minus_utc(),
        end_timezone_offset_seconds: Some(
            range_end.with_timezone(&Local).offset().local_minus_utc(),
        ),
        max_items: read.query.limit(),
        max_bytes: MAX_CALENDAR_CONTEXT_BYTES,
    };
    let dependency = calendar_lease_dependency(
        invocation_id,
        leases.process_incarnation(),
        observation_id,
        &before.admission,
        &key,
        observation.observed_at,
        expires_at,
    )?;
    let duration = (expires_at - Utc::now())
        .to_std()
        .map_err(|_| AgentFailure::StaleContext)?;
    let expires_at_monotonic = Instant::now()
        .checked_add(duration)
        .ok_or(AgentFailure::BudgetExceeded)?;
    check_running(read.window)?;
    leases.retain_observation(
        dependency.clone(),
        before.stamp.native_subject_fingerprint,
        expires_at_monotonic,
    )?;
    Ok((view, dependency))
}

pub async fn authorize_native_calendar_dependency(
    connections: &impl CalendarConnectionReader,
    source: &impl CalendarSource,
    grants: &impl NativeCalendarGrantReader,
    leases: &SourceLeaseRegistry,
    dependency: &ContextDependency,
    window: &RemoteCallWindow,
) -> Result<(), AgentFailure> {
    let now = Utc::now();
    if dependency.observed_at() > now || dependency.expires_at() <= now {
        return Err(AgentFailure::StaleContext);
    }
    if !matches!(
        dependency.source().connector().as_str(),
        "calendar.event_kit" | "calendar.android"
    ) {
        return Err(AgentFailure::CapabilityDenied);
    }
    let (_, subject_fingerprint) = leases.observation(dependency)?;
    let admitted = admit_current_native_calendar_read(
        connections,
        source,
        grants,
        dependency.person_id(),
        dependency.source().execution_owner().as_str(),
        dependency.consumer().identifier(),
        window,
    )
    .await?;
    if admitted.stamp.native_subject_fingerprint != subject_fingerprint
        || !admission_matches_dependency(&admitted.admission, dependency)
    {
        return Err(AgentFailure::StaleContext);
    }
    check_running(window)
}

fn project_native_items(
    provider: CalendarProvider,
    connection_id: &str,
    calendar_ids: &[String],
    batches: Vec<CalendarBatch>,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<CalendarContextItem>, AgentFailure> {
    let expected: HashSet<_> = calendar_ids.iter().map(String::as_str).collect();
    let received: HashSet<_> = batches
        .iter()
        .map(|batch| batch.calendar_id.as_str())
        .collect();
    if expected != received || batches.len() != expected.len() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let mut handles = HashSet::new();
    let mut items = Vec::new();
    for batch in batches {
        if let Some(failure) = batch.failure {
            return Err(match failure {
                CalendarFailure::PermissionDenied => AgentFailure::CapabilityDenied,
                CalendarFailure::CalendarUnavailable | CalendarFailure::ProviderUnavailable => {
                    AgentFailure::CapabilityUnavailable
                }
            });
        }
        for record in batch.records {
            if record.calendar_id != batch.calendar_id
                || record.external_id.trim().is_empty()
                || record.external_revision.trim().is_empty()
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let (start, end, all_day) = match record.schedule {
                EventSchedule::Timed(schedule) => (schedule.starts_at, schedule.ends_at, false),
                EventSchedule::AllDay(schedule) => (
                    local_midnight(schedule.start_date, false)?,
                    local_midnight(schedule.end_date_exclusive, true)?,
                    true,
                ),
            };
            if start >= end {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let start = start.max(range_start);
            let end = end.min(range_end);
            if start >= end {
                continue;
            }
            let evidence_handle = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!(
                    "floe:calendar:{provider:?}:{connection_id}:{}:{}",
                    batch.calendar_id, record.external_id
                )
                .as_bytes(),
            )
            .to_string();
            if !handles.insert(evidence_handle.clone()) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            if items.len() >= limit {
                return Err(AgentFailure::BudgetExceeded);
            }
            items.push(CalendarContextItem {
                evidence_handle,
                untrusted_title: record.title.chars().take(256).collect(),
                starts_at_unix_ms: start.timestamp_millis(),
                ends_at_unix_ms: end.timestamp_millis(),
                all_day,
            });
        }
    }
    items.sort_by_key(|item| {
        (
            item.starts_at_unix_ms,
            item.ends_at_unix_ms,
            item.evidence_handle.clone(),
        )
    });
    Ok(items)
}

fn local_midnight(date: chrono::NaiveDate, latest: bool) -> Result<DateTime<Utc>, AgentFailure> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    let local = Local.from_local_datetime(&midnight);
    let instant = if latest {
        local.latest()
    } else {
        local.earliest()
    }
    .ok_or(AgentFailure::CapabilityUnavailable)?;
    Ok(instant.with_timezone(&Utc))
}

fn check_running(window: &RemoteCallWindow) -> Result<(), AgentFailure> {
    if window.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= window.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}
