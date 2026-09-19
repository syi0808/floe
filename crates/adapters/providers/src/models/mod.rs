//! Model transports.

pub mod foundation;
pub mod learner;
pub mod root;
pub mod server;
pub(crate) mod wire;

pub use foundation::{
    FoundationModelProvider, FoundationModelRunner, LocalModelAvailability,
    PreparedFoundationTransport,
};
pub use root::{PreparedRootTransport, RootModelProvider};
pub use learner::FoundationLearnerTransport;
pub use server::{
    PreparedServerTransport, RemoteModelRouteResolver, ResolvedRemoteConnection, ServerModelProvider,
    ServerModelRunner, resolve_remote_model_route,
};
