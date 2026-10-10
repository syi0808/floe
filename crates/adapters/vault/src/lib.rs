//! Local Vault adapter: SQL, encryption at rest, key identity, compare-and-swap
//! and atomic transactions for every owner's durable state.
//!
//! Business policy belongs to the owning module. This crate only implements the
//! owner-defined repository ports and the physical storage guarantees.

#[cfg(all(feature = "os-keyring", feature = "development-storage"))]
compile_error!("Choose exactly one storage profile; development requires --no-default-features.");
#[cfg(not(any(feature = "os-keyring", feature = "development-storage")))]
compile_error!("A storage profile is required.");
#[cfg(all(feature = "development-storage", not(debug_assertions)))]
compile_error!("Development file-key custody cannot be compiled into a release/profile build.");

mod root_key;
pub use root_key::RootKey;
mod product_keys;
pub use product_keys::ProductStoreIdentity;
mod engine;
mod error;
mod repositories;
mod schema;
mod schema_sql;
#[cfg(unix)]
mod vault;
mod write_fence;

pub use engine::TursoStore;
pub use error::{StoreError, StoreErrorCode};
#[cfg(unix)]
pub use repositories::{
    ContextEvidenceReader, VaultCalendarOperationsRepository, VaultConversationRepository,
    VaultTaskRepository,
};
#[cfg(unix)]
pub use vault::*;

#[cfg(unix)]
pub use repositories::{VaultKnowledgeRepository, VaultLearnerJournalFactory};

#[cfg(unix)]
pub use vault::{VaultAuthorizationSigner, VaultEnrollmentSigner};

#[cfg(unix)]
pub use repositories::{VaultExpertBindingReviewRepository, VaultExpertRegistryRepository};

pub use repositories::StoredVaultLifecycleReceipt;
