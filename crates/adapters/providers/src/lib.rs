//! Provider adapters: the real transports that reach models, sources and the
//! control plane.
//!
//! An adapter connects an owner-defined port to a concrete protocol. Business
//! policy lives in the owning module, OS handles live in `floe-native`.

pub mod control;
pub mod gateway;
mod local_identity;
pub mod models;
pub mod sources;

pub use local_identity::{
    LocalDatabaseAdmission, LocalIdentityError, LocalInstallation, LocalInstallationLease,
    NativeInstallationError, VerifiedLocalIdentity, local_identity_for_database,
    lock_existing_local_installation, prepare_local_installation,
};
#[cfg(debug_assertions)]
pub use local_identity::DevelopmentResetReason;
