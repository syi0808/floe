//! Owner-scoped repository implementations over the local encrypted engine.

mod actions;
pub use actions::VaultActionsRepository;
mod connections;
#[cfg(unix)]
mod context_evidence;
#[cfg(unix)]
mod conversation;
mod day;
mod day_collection;
mod day_mutation;
mod day_refresh;
#[cfg(unix)]
mod knowledge;
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

pub(crate) use day::{initialize_day_schema, validate_day_schema};

#[cfg(unix)]
mod expert_binding_reviews;
#[cfg(unix)]
mod expert_registry;
#[cfg(unix)]
pub use expert_binding_reviews::VaultExpertBindingReviewRepository;
#[cfg(unix)]
pub use expert_registry::VaultExpertRegistryRepository;

#[cfg(unix)]
pub use knowledge::VaultKnowledgeRepository;
