//! Owner-scoped repository implementations over the local encrypted engine.

mod actions;
mod connections;
#[cfg(unix)]
mod context_evidence;
#[cfg(unix)]
mod conversation;
mod day;
#[cfg(unix)]
mod expert_actions;
#[cfg(unix)]
mod memory_review;
#[cfg(unix)]
mod task;

#[cfg(unix)]
pub use context_evidence::ContextEvidenceReader;
#[cfg(unix)]
pub use conversation::VaultConversationRepository;
#[cfg(unix)]
pub use task::VaultTaskRepository;

#[cfg(unix)]
mod learner_journal;
#[cfg(unix)]
pub use learner_journal::VaultLearnerJournalFactory;

pub(crate) use connections::initialize_source_operations;
