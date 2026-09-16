//! Local Vault adapter: SQL, encryption at rest, key identity, compare-and-swap
//! and atomic transactions for every owner's durable state.
//!
//! Business policy belongs to the owning module. This crate only implements the
//! owner-defined repository ports and the physical storage guarantees.

mod engine;
mod error;
mod repositories;
#[cfg(unix)]
mod vault;

pub use engine::TursoStore;
pub use error::{StoreError, StoreErrorCode};
#[cfg(unix)]
pub use repositories::{
    ContextEvidenceReader, VaultConversationRepository, VaultTaskRepository, execution_profile,
};
#[cfg(unix)]
pub use vault::*;
