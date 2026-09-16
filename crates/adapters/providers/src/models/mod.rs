//! Model transports.

pub mod foundation;
pub mod learner;
pub mod server;

pub use foundation::{FoundationModelRunner, LocalModelAvailability};
pub use learner::FoundationLearnerTransport;
pub use server::{ServerModelRunner, resolve_remote_model_route};
