//! What a remote view grant is decided from.
//!
//! Access states which producer, which source authority and which scope a grant
//! may stand on. Fetching the producer's signed descriptor is a transport's
//! work; verifying the signature and committing the grant atomically is the
//! store's. Neither of them decides whether the grant is admissible.

use floe_context_contract::{
    ConsumerPolicyAuthority, GrantAuthority, GrantId, GrantScope, GrantSourceBinding,
    SourceAuthority,
};
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

use crate::application::remote_calendar::RemoteCalendarSourceReference;
use crate::application::remote_view::{RemoteProducerIdentity, RemoteViewSourceReference};
use crate::data_access_grant::DataAccessGrant;

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// The grant a source read runs under, with the consumer policy that admitted it.
#[derive(Clone, Debug)]
pub struct RemoteGrantBinding {
    pub grant: DataAccessGrant,
    pub consumer_policy: ConsumerPolicyAuthority,
}

/// One signed source descriptor, exactly as the producer returned it.
pub struct SignedSourcePreview {
    pub descriptor_b64url: String,
    pub producer_signature: String,
    pub connection_revision: u64,
    pub producer: RemoteProducerIdentity,
}

/// One signed calendar source descriptor, exactly as the producer returned it.
///
/// A calendar descriptor names no view and no connection revision: the calendar
/// is the source, and the connection it is served over is named by the grant.
pub struct SignedCalendarPreview {
    pub descriptor_b64url: String,
    pub producer_signature: String,
    pub producer: RemoteProducerIdentity,
}

/// Which calendar a preview is being asked about.
#[derive(Clone, Copy)]
pub struct RemoteCalendarQuery<'a> {
    pub connector_id: &'a str,
    pub connection_id: &'a str,
    pub resource: &'a str,
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

    fn calendar_source_preview<'a>(
        &'a self,
        query: RemoteCalendarQuery<'a>,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<SignedCalendarPreview, AgentFailure>>;
}

/// The Person's own record of who they trust and what they have granted.
pub trait RemoteGrantStore: Sync {
    fn pinned_producer<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>>;

    /// Check the producer's signature over the descriptor, and read out what it
    /// names. A descriptor that does not name this pairing and this source is
    /// rejected here, not by the caller.
    fn verify_view_source_preview<'a>(
        &'a self,
        preview: &'a SignedSourcePreview,
        pairing: RemotePairingIdentity<'a>,
        query: RemoteSourceQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>>;

    fn grants<'a>(&'a self, limit: usize)
    -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>>;

    fn find_view_grant<'a>(
        &'a self,
        view_id: &'a str,
        source: &'a GrantSourceBinding,
        consumer: &'a str,
    ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>>;

    /// Commit the activation atomically against the authority it expects.
    fn activate_view_grant<'a>(
        &'a self,
        view_id: &'a str,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    /// Check the producer's signature over a calendar descriptor, and read out
    /// what it names.
    fn verify_calendar_source_preview<'a>(
        &'a self,
        preview: &'a SignedCalendarPreview,
        pairing: RemotePairingIdentity<'a>,
        query: RemoteCalendarQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteCalendarSourceReference, AgentFailure>>;

    fn activate_calendar_grant<'a>(
        &'a self,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    fn calendar_grant<'a>(
        &'a self,
        grant_id: GrantId,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    fn pause_calendar_grant<'a>(
        &'a self,
        grant_id: GrantId,
        expected: GrantAuthority,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    fn view_grant_binding<'a>(
        &'a self,
        view_id: &'a str,
        connector_id: &'a str,
        connection_id: &'a str,
        source_authority: SourceAuthority,
    ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>>;
}
