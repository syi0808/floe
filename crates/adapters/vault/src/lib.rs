//! Local Vault adapter: SQL, encryption at rest, key identity, compare-and-swap
//! and atomic transactions for every owner's durable state.
//!
//! Business policy belongs to the owning module. This crate only implements the
//! owner-defined repository ports and the physical storage guarantees.

mod engine;
mod error;
mod repositories;
mod schema_sql;
#[cfg(unix)]
mod vault;

pub use engine::TursoStore;
pub use error::{StoreError, StoreErrorCode};
#[cfg(unix)]
pub use repositories::{
    ContextEvidenceReader, VaultActionsRepository, VaultConversationRepository, VaultTaskRepository,
};
#[cfg(unix)]
pub use vault::*;

#[cfg(unix)]
pub use repositories::{VaultKnowledgeRepository, VaultLearnerJournalFactory};

#[cfg(unix)]
pub use vault::{VaultAuthorizationSigner, VaultEnrollmentSigner};

#[cfg(unix)]
pub use repositories::{VaultExpertBindingReviewRepository, VaultExpertRegistryRepository};
