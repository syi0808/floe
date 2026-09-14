//! Small execution primitives shared by agent runtimes.
//!
//! This crate deliberately does not own model, task, or application state. It
//! only provides scope-local cancellation and the execution budget helpers.

pub mod budget;
pub mod limits;
mod cancellation;
mod scope;
pub mod tasks;

pub use cancellation::{CancelReason, Cancellation};
pub use scope::ExecutionScope;
