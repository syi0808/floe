//! Native acquisition and a verified Gateway session, without Context policy.
use std::sync::Arc;

use floe_access::{CalendarReadAccessRequest, CalendarReadAccessStamp, SourcePreviewVerifier};
use floe_connections::{ConnectorSnapshot, SourceConnection};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};

use crate::{CalendarObservation, CalendarObserveRequest, RemoteViewTransport};

/// One actual, pinned Gateway session. Implementations retain the same
/// credentials and verified producer for metadata and admitted source reads.
pub trait ExpertRemoteTransport: RemoteViewTransport + Send {
    fn client_id(&self) -> &str;
    fn producer(&self) -> &floe_access::RemoteProducerIdentity;
    fn gateway_binding(&self) -> &floe_access::VerifiedGatewayBinding;
    fn catalog<'a>(&'a self, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<Vec<ConnectorSnapshot>, AgentFailure>>;
}

pub struct ExpertRemoteSource {
    pub transport: Arc<dyn ExpertRemoteTransport>,
    pub verifier: Arc<dyn SourcePreviewVerifier>,
}

pub trait ExpertSourceTransport: Send + Sync {
    /// None means proven absence of a paired Gateway. Credential, trust and
    /// transport failures remain errors, never an empty candidate catalog.
    fn remote<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<Option<ExpertRemoteSource>, AgentFailure>>;

    /// Exact source identity comes from Context's current connection row,
    /// never a model argument. These operations do not request OS permission.
    fn check_calendar<'a>(&'a self, actor: &'a OwnerActor,
        source: &'a SourceConnection, request: CalendarReadAccessRequest)
        -> BoxFuture<'a, Result<CalendarReadAccessStamp, AgentFailure>>;
    fn observe_calendar<'a>(&'a self, actor: &'a OwnerActor,
        source: &'a SourceConnection, request: CalendarObserveRequest)
        -> BoxFuture<'a, Result<CalendarObservation, AgentFailure>>;
}
