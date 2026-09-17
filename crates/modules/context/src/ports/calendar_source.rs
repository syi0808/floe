//! What Context asks a calendar source for.
//!
//! Access decides whether a grant admits the read; this port is the acquisition
//! itself. A source confirms the native subject it is about to read under, and
//! then returns either raw batches (Day's mirror shape) or a projection a remote
//! producer already bounded. Context owns the order the two are called in and
//! the re-check that follows.

use std::future::Future;

use chrono::{DateTime, Utc};
use floe_access::{CalendarReadAccessRequest, CalendarReadAccessStamp};
use floe_context_contract::CalendarProvider;
use floe_day::{CalendarBatch, CalendarMirror};
use floe_execution::Cancellation;
use floe_agent_contract::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

#[derive(Clone)]
pub struct CalendarObserveRequest {
    pub person_id: PersonId,
    pub device_id: String,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub expected_native_subject_fingerprint: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub struct CalendarObservation {
    pub stamp: CalendarReadAccessStamp,
    pub observed_at: DateTime<Utc>,
    pub batches: Vec<CalendarBatch>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedCalendarObservation {
    pub stamp: CalendarReadAccessStamp,
    pub source_handle: String,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub range_start: DateTime<Utc>,
    pub range_end: DateTime<Utc>,
    pub coverage_complete: bool,
    pub next_cursor: Option<String>,
    pub items: Vec<ProjectedCalendarItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedCalendarItem {
    pub evidence_handle: String,
    pub untrusted_title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// The source a calendar read is acquired from.
pub trait CalendarSource: Sync {
    /// Confirm the native subject this read will stand on, and stamp it.
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

    fn observe_projected(
        &self,
        _: CalendarObserveRequest,
    ) -> impl Future<Output = Result<Option<ProjectedCalendarObservation>, AgentFailure>> + Send
    {
        async { Ok(None) }
    }
}

/// The stored calendar mirror a fixture-backed read falls back to.
pub trait CalendarMirrorReader: Sync {
    fn bounded_calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> impl Future<Output = Result<CalendarMirror, AgentFailure>> + Send;
}
