//! Model transports.

pub mod foundation;
pub mod learner;
pub mod server;
pub(crate) mod wire;

pub use foundation::{FoundationModelRunner, LocalModelAvailability};
pub use learner::FoundationLearnerTransport;
pub use server::{
    RemoteModelRouteResolver, ResolvedRemoteConnection, ServerModelRunner,
    resolve_remote_model_route,
};
