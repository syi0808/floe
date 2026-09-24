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
    RemoteProducerIdentity, RemoteViewApproval, RemoteViewGrantReview, RemoteViewSourceReference,
    matches_review, producer_is_pinned, remote_view_scope, remote_view_source,
    review_remote_view_grant, source_matches_producer,
};
use crate::data_access_grant::DataAccessGrant;
use crate::ports::remote_grants::{
    RemoteCallWindow, RemoteGrantStore, RemoteGrantTransport, RemotePairingIdentity,
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
    /// The data category this view's contents fall under.
    pub data_category: floe_context_contract::GrantDataCategory,
}

/// What the Person is being shown before they decide.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteViewGrantPreview {
    pub reference: RemoteViewSourceReference,
    pub producer: RemoteProducerIdentity,
    pub connection_revision: u64,
    pub consumers: Vec<String>,
}

/// What the Person says they already reviewed.
#[derive(Clone, Copy)]
pub struct RemoteViewGrantExpectation<'a> {
    pub producer_fingerprint: &'a str,
    pub source_authority: SourceAuthority,
    pub connection_revision: u64,
    pub provider_identity: &'a str,
    pub recipient: &'a str,
}

#[derive(Clone, Debug)]
pub struct RemoteViewGrantActivation {
    pub view_id: String,
    pub grant_id: floe_context_contract::GrantId,
    pub expected: Option<floe_context_contract::GrantAuthority>,
    pub source: floe_context_contract::GrantSourceBinding,
    pub scope: floe_context_contract::GrantScope,
}

#[derive(Clone, Debug)]
pub enum RemoteViewGrantPreparation {
    Current(DataAccessGrant),
    Activate(RemoteViewGrantActivation),
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
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
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
    producer_is_pinned(&store.pinned_producer().await?, &producer)?;
    let query = RemoteSourceQuery {
        view_id: request.view_id,
        connector_id: request.connector_id,
        connection_id: request.connection_id,
        resource: request.resource,
    };
    let preview = transport.view_source_preview(query, window).await?;
    let reference = store
        .verify_view_source_preview(&preview, request.pairing, query)
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

/// Grant the view the Person reviewed, or report that it is already granted.
///
/// The preview is taken again here rather than carried across the decision: a
/// producer, authority or revision that moved while the Person was deciding is
/// not what they reviewed.
pub async fn review_and_activate_remote_view_grant(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
    request: RemoteViewGrantRequest<'_>,
    expectation: RemoteViewGrantExpectation<'_>,
    resource_matches_view: bool,
    is_remote_view: bool,
    window: &RemoteCallWindow,
) -> Result<DataAccessGrant, AgentFailure> {
    let preparation = prepare_remote_view_grant_activation(
        store,
        transport,
        request,
        expectation,
        resource_matches_view,
        is_remote_view,
        window,
    )
    .await?;
    match preparation {
        RemoteViewGrantPreparation::Current(grant) => Ok(grant),
        RemoteViewGrantPreparation::Activate(activation) => {
            store
                .activate_view_grant(
                    &activation.view_id,
                    activation.grant_id,
                    activation.expected,
                    activation.source,
                    activation.scope,
                )
                .await
        }
    }
}

pub async fn prepare_remote_view_grant_activation(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
    request: RemoteViewGrantRequest<'_>,
    expectation: RemoteViewGrantExpectation<'_>,
    resource_matches_view: bool,
    is_remote_view: bool,
    window: &RemoteCallWindow,
) -> Result<RemoteViewGrantPreparation, AgentFailure> {
    let preview = preview_remote_view_grant(
        store,
        transport,
        request,
        resource_matches_view,
        is_remote_view,
        window,
    )
    .await?;
    matches_review(
        &RemoteViewApproval {
            producer_fingerprint: expectation.producer_fingerprint,
            source_authority: expectation.source_authority,
            connection_revision: expectation.connection_revision,
            provider_identity: expectation.provider_identity,
            recipient: expectation.recipient,
        },
        &preview.reference,
        &preview.producer,
        preview.connection_revision,
    )?;
    let scope = remote_view_scope(
        request.resource,
        request.data_category,
        request.consumers.to_vec(),
        preview.producer.audience.clone(),
    )?;
    let source = remote_view_source(&preview.reference)?;
    let existing = store
        .find_view_grant(request.view_id, &source, request.consumers[0].identifier())
        .await?;
    match review_remote_view_grant(existing.as_ref(), &scope)? {
        RemoteViewGrantReview::AlreadyGranted => Ok(RemoteViewGrantPreparation::Current(
            existing.ok_or(AgentFailure::PolicyDenied)?,
        )),
        RemoteViewGrantReview::Activate { grant_id, expected } => Ok(
            RemoteViewGrantPreparation::Activate(RemoteViewGrantActivation {
                view_id: request.view_id.to_owned(),
                grant_id,
                expected,
                source,
                scope,
            }),
        ),
    }
}
