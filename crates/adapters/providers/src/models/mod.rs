//! Model transports.

pub mod foundation;
pub mod root;
pub mod server;
pub(crate) mod wire;

pub use foundation::{
    FoundationModelProvider, FoundationModelRunner, LocalModelAvailability,
    PreparedFoundationTransport,
};
pub use root::{PreparedRootTransport, RootModelProvider};
pub use server::{PreparedServerTransport, ServerModelProvider, ServerModelRunner};
