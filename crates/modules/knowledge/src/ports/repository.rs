use std::future::Future;

use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;

use crate::ContextMemory;

pub trait MemoryContextReader: Send + Sync {
    fn read_memory_context(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<ContextMemory>, AgentFailure>> + Send;
}
