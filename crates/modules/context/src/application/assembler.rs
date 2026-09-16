//! Acquiring the Person's confirmed memories for one context.
//!
//! How an optional source reports a refusal is the context contract's; what
//! this module adds is the Knowledge reader it acquires through.

use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextSource, acquire_optional_source};

pub async fn acquire_memory_context(
    reader: &impl floe_knowledge::MemoryContextReader,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<floe_knowledge::MemoryContextSnapshot, AgentFailure> {
    let acquired =
        acquire_optional_source(ContextSource::Memory, reader.read_memory_context(now)).await?;
    Ok(floe_knowledge::MemoryContextSnapshot {
        memories: acquired.value.unwrap_or_default(),
        issue: acquired.issue.map(|issue| issue.reason),
    })
}
