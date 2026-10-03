//! What a remote view grant is decided from.
//!
//! Access states which producer, which source authority and which scope a grant
//! may stand on. Fetching the producer's signed descriptor is a transport's
//! work; verifying the signature and committing the grant atomically is the
//! store's. Neither of them decides whether the grant is admissible.

use floe_context_contract::{GrantAuthority, GrantId, GrantScope, GrantSourceBinding};
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

use crate::application::remote_view::{RemoteProducerIdentity, RemoteViewSourceReference};
use crate::data_access_grant::DataAccessGrant;

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// One signed source descriptor, exactly as the producer returned it.
pub struct SignedSourcePreview {
    pub descriptor_b64url: String,
    pub producer_signature: String,
    pub connection_revision: u64,
    pub producer: RemoteProducerIdentity,
}

/// Which source a preview is being asked about.
#[derive(Clone, Copy)]
pub struct RemoteSourceQuery<'a> {
    pub view_id: &'a str,
    pub connector_id: &'a str,
    pub connection_id: &'a str,
    pub resource: &'a str,
}

/// The pairing a signed descriptor must name.
#[derive(Clone, Copy)]
pub struct RemotePairingIdentity<'a> {
    pub person_id: &'a str,
    pub client_id: &'a str,
    pub device_id: &'a str,
}

/// The window one remote call must finish inside.
#[derive(Clone)]
pub struct RemoteCallWindow {
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

/// The paired producer, reached over the wire.
pub trait RemoteGrantTransport: Sync {
    fn producer_identity<'a>(
        &'a self,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>>;

    fn view_source_preview<'a>(
        &'a self,
        query: RemoteSourceQuery<'a>,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>>;
}

