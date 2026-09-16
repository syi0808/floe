//! Provider adapters: the real transports that reach models, sources and the
//! control plane.
//!
//! An adapter connects an owner-defined port to a concrete protocol. Business
//! policy lives in the owning module, OS handles live in `floe-native`.

pub mod control;
mod local_identity;
pub mod models;
pub mod sources;

pub use local_identity::{LocalIdentityError, VerifiedLocalIdentity, local_identity_for_database};
