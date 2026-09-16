//! Context view shapes and their validation.
//!
//! A view is a bounded projection of a source. Context owns what a view may
//! contain and how fresh it must be; an Expert only reads it.

pub mod calendar;
pub mod communication;
pub mod interactions;
pub mod native;
pub mod personal;
pub mod portfolio;

pub use calendar::*;
pub use communication::*;
pub use interactions::*;
pub use native::*;
pub use personal::*;
pub use portfolio::*;
