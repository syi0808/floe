//! How a turn's model usage reaches the Session record.
//!
//! The ledger itself belongs to Inference, which owns model attempts. What
//! Conversation adds is the projection onto the Session's own usage counters.

use crate::AgentUsage;

use floe_execution::budget::ModelUsage;
use floe_inference::UsageLedger;

/// Carry the ledger's model usage onto the Session counters the turn reports.
pub fn sync_usage(ledger: &UsageLedger, usage: &mut AgentUsage) {
    let model = ledger.snapshot();
    usage.tokens = model.tokens;
    usage.cost_micros = model.cost_micros;
    usage.model_attempts = model.attempts;
    usage.estimated_tokens = model.estimated_tokens;
}

/// The ledger one turn starts from, seeded with what the Session already spent.
pub fn turn_ledger(max_tokens: u64, max_cost_micros: u64, usage: AgentUsage) -> UsageLedger {
    UsageLedger::new(
        max_tokens,
        max_cost_micros,
        ModelUsage {
            attempts: usage.model_attempts,
            tokens: usage.tokens,
            cost_micros: usage.cost_micros,
            estimated_tokens: usage.estimated_tokens,
        },
    )
}
