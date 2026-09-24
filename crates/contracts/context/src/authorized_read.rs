//! One authorized source read, held open for as long as its reader holds it.
//!
//! Context grants the read, bounds it and keeps the lease alive; a reader sees
//! the payload it was authorized for and the dependency it must record, and
//! releases the lease by dropping the read. Which registry issued the lease is
//! not the reader's to know.

use crate::{ContextDependency, GrantScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedSourceBinding {
    pub dependency: ContextDependency,
    pub scope: GrantScope,
}

/// A read that is held open under a grant.
///
/// What it carries is its reader's business; what it was admitted under is
/// whoever admitted it.
pub trait HeldGrant {
    /// Every exact source authority the held payload was admitted under.
    fn bindings(&self) -> &[AuthorizedSourceBinding];
}

pub trait AuthorizedRead: HeldGrant + Send {
    /// The authorized payload itself.
    fn payload(&self) -> &serde_json::Value;

    /// Whether the read is still inside the freshness it was granted under.
    fn is_fresh(&self) -> bool;
}
