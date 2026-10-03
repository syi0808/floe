//! Actual Calendar provider/OS transport, separated from product-read policy.
use floe_access::{ProductCalendarDispatchFence, ProductCalendarResultBinding, ProductSourceObservation};
use floe_connections::SourceConnection;
use floe_day::CalendarResourceOutcome;
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Exact normalized private Gateway page. It never crosses product AppWire.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarProductPage {
    pub schema_version: u32,
    pub result_kind: floe_access::CalendarProductResultKind,
    pub refresh_operation_id: Uuid,
    pub read_operation_id: Uuid,
    pub page_id: Uuid,
    pub person_id: String,
    pub device_id: String,
    pub source: floe_access::ProductCalendarSourceClaims,
    pub calendar_id: String,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub outcome: CalendarProductPageOutcome,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarProductPageOutcome {
    Complete { records: Vec<floe_day::CalendarRecord> },
    More { records: Vec<floe_day::CalendarRecord>, cursor: String },
    Failed { reason: floe_day::CalendarFailure },
}

pub struct CalendarProductReadResult {
    pub batches: Vec<CalendarResourceOutcome>,
    pub binding: ProductCalendarResultBinding,
    /// Actual cumulative acquisition, including pages discarded on failure.
    /// Counts must cover retained normalized batches and stay inside the permit.
    pub consumed_records: u32,
    pub consumed_bytes: u32,
}

pub trait CalendarProductTransport: Send + Sync {
    /// Metadata-only permission/subject/provider probe over this exact source.
    fn observe<'a>(&'a self, actor: &'a OwnerActor, source: &'a SourceConnection, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<ProductSourceObservation, AgentFailure>>;
    /// Native reads one exact bounded host request; Gateway transport completes
    /// strict pages under the retained permit's per-page and total ceilings.
    /// Return exactly one terminal result per configured calendar; incomplete
    /// pages become a failure for that calendar, never an empty complete batch.
    fn acquire<'a>(&'a self, fence: ProductCalendarDispatchFence<'a>, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<CalendarProductReadResult, AgentFailure>>;
}
