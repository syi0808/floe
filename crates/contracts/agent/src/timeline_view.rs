//! What an Expert asks a timeline view for.
//!
//! The view itself is a context value; the request carries the deadline and
//! cancellation the acquisition must run under, so it lives here with the rest
//! of the execution-bearing agent values.

use floe_execution::Cancellation;
use floe_kernel::PersonId;
use tokio::time::Instant;
use uuid::Uuid;

pub struct TimelineViewRead {
    pub person_id: PersonId,
    pub handle: Uuid,
    pub range_start_unix_ms: Option<u64>,
    pub range_end_unix_ms: Option<u64>,
    pub cursor: Option<String>,
    pub max_items: usize,
    pub max_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}
