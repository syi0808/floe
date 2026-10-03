//! Acquiring the Person's confirmed memories for one context.
//!
//! How an optional source reports a refusal is the context contract's; what
//! this module adds is the Knowledge reader it acquires through.

use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextSource, acquire_optional_source};

pub async fn acquire_memory_context(
    reader: &(impl floe_knowledge::KnowledgeRead + ?Sized),
    actor: &floe_kernel::OwnerActor,
    scope: &floe_execution::ExecutionScope,
) -> Result<floe_knowledge::MemoryContextSnapshot, AgentFailure> {
    let acquired = acquire_optional_source(ContextSource::Memory, reader.read_context(actor, scope)).await?;
    match acquired.value {
        Some(value) => Ok(value),
        None => Ok(floe_knowledge::MemoryContextSnapshot { memories: vec![], issue: acquired.issue.map(|issue| issue.reason) }),
    }
}
