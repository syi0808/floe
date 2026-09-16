//! Platform layer: OS handles, bundled native drivers, key access and the
//! thread constraints they impose.
//!
//! No product judgment lives here. Adapters map these results onto owner ports.

pub mod dylib;
mod host;
mod keychain;

pub use dylib::{
    BUNDLE_SIBLING, ByteCall, GatedStringCall, MACOS_BUNDLE_ROOT, NativeCallError, NativeLibrary,
};
pub use host::{NativeIdentityError, NativeLocalIdentity, local_identity_for_database};
pub use keychain::{KeychainError, read_generic_password};
