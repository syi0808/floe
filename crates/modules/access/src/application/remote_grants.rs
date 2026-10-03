//! Granting a remote view: preview it, review it, activate it.
//!
//! The whole judgment lives here — that the producer is the one the Person
//! pinned, that the signed descriptor names this pairing and this source, that
//! what the Person reviewed still matches, and whether the existing grant
//! already covers it. The transport only fetches; the store only verifies keys
//! and commits.

use floe_context_contract::{GrantConsumer, SourceAuthority};
use floe_kernel::{AgentFailure, PersonId};

use crate::application::remote_view::{
    RemoteProducerIdentity, RemoteViewSourceReference, producer_is_pinned, source_matches_producer,
};
use crate::data_access_grant::DataAccessGrant;
use crate::ports::remote_grants::{
    RemoteCallWindow, RemoteGrantTransport, RemotePairingIdentity,
    RemoteSourceQuery,
};

/// Which remote view is being granted, to whom, over which connection.
#[derive(Clone, Copy)]
pub struct RemoteViewGrantRequest<'a> {
    pub person_id: PersonId,
    pub pairing: RemotePairingIdentity<'a>,
    pub view_id: &'a str,
    pub connector_id: &'a str,
    pub connection_id: &'a str,
    pub resource: &'a str,
    pub consumers: &'a [GrantConsumer],
    /// The exact data categories this view's contents fall under.
    pub data_categories: &'a [floe_context_contract::GrantDataCategory],
}

/// What the Person is being shown before they decide.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteViewGrantPreview {
    pub reference: RemoteViewSourceReference,
    pub producer: RemoteProducerIdentity,
    pub connection_revision: u64,
    pub consumers: Vec<String>,
}

fn admissible(request: &RemoteViewGrantRequest<'_>, resource_matches: bool) -> bool {
    request.pairing.person_id == request.person_id.to_string()
        && !request.pairing.client_id.is_empty()
        && !request.pairing.device_id.is_empty()
        && resource_matches
        && !request.consumers.is_empty()
}

/// Show the Person what they would be granting.
///
/// Nothing is stored. The producer must already be the one they pinned, and the
/// descriptor it signs must name this pairing, this source and this resource.
pub async fn preview_remote_view_grant(
    trust: &(impl crate::GatewayTrustReader + ?Sized),
    verifier: &(impl crate::SourcePreviewVerifier + ?Sized),
    transport: &(impl RemoteGrantTransport + ?Sized),
    request: RemoteViewGrantRequest<'_>,
    resource_matches_view: bool,
    is_remote_view: bool,
    window: &RemoteCallWindow,
) -> Result<RemoteViewGrantPreview, AgentFailure> {
    if !admissible(&request, resource_matches_view) || !is_remote_view {
        return Err(AgentFailure::PolicyDenied);
    }
    // The consumer name has to be one the product may name at all.
    if request.consumers.iter().any(|consumer| {
        !matches!(consumer, GrantConsumer::Builtin(identifier) if !identifier.is_empty())
    }) {
        return Err(AgentFailure::InvalidInput);
    }
    let producer = transport.producer_identity(window).await?;
    producer_is_pinned(&trust.pinned_producer().await?, &producer)?;
    let query = RemoteSourceQuery {
        view_id: request.view_id,
        connector_id: request.connector_id,
        connection_id: request.connection_id,
        resource: request.resource,
    };
    let preview = transport.view_source_preview(query, window).await?;
    let reference = verifier
        .verify(&preview, request.pairing, query)
        .await?;
    source_matches_producer(&reference, &producer, preview.connection_revision)?;
    Ok(RemoteViewGrantPreview {
        reference,
        producer,
        connection_revision: preview.connection_revision,
        consumers: request
            .consumers
            .iter()
            .map(|consumer| consumer.identifier().to_owned())
            .collect(),
    })
}

