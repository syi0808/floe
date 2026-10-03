pub mod conversion;
mod dto;
pub mod wire;

pub use dto::*;

/// The shared identity values this wire carries.
///
/// A binding names them to build one command or read one result; naming a
/// value here is not a way to reach past the wire into the module that owns it.
pub use floe_kernel::{AgentFailure, CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
