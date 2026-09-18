//! Root Run turn execution: the session contract it drives, and the capability
//! and model attempt records it keeps.

#[cfg(test)]
mod capability;
pub mod journal;
mod model_recovery;
mod runtime;
mod session;
mod source_history;
mod usage;

pub use model_recovery::{generate_once, generate_with_recovery};
pub use runtime::AgentRuntime;
pub use session::*;
pub use source_history::{
    bounded_source_history_start, carries_source_history, project_source_history,
};
pub use usage::{ModelUsage, UsageAttempt, UsageLedger, sync_usage, turn_ledger};
