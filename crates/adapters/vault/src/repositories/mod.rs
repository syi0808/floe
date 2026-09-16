//! Owner-scoped repository implementations over the local encrypted engine.

mod actions;
mod day;
#[cfg(unix)]
mod context_evidence;
#[cfg(unix)]
mod conversation;
#[cfg(unix)]
mod expert_actions;
#[cfg(unix)]
mod task;

#[cfg(unix)]
pub(crate) use context_evidence::ContextEvidenceReader;
#[cfg(unix)]
pub use conversation::VaultConversationRepository;
#[cfg(unix)]
pub use task::VaultTaskRepository;
