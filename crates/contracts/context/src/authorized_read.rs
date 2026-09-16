//! One authorized source read, held open for as long as its reader holds it.
//!
//! Context grants the read, bounds it and keeps the lease alive; a reader sees
//! the payload it was authorized for and the dependency it must record, and
//! releases the lease by dropping the read. Which registry issued the lease is
//! not the reader's to know.

use crate::ContextDependency;

pub trait AuthorizedRead: Send {
    /// What the result of reading this owes its provenance to.
    fn dependency(&self) -> &ContextDependency;

    /// The authorized payload itself.
    fn payload(&self) -> &serde_json::Value;

    /// Whether the read is still inside the freshness it was granted under.
    fn is_fresh(&self) -> bool;
}
