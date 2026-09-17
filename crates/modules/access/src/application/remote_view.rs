//! Reading a source through a Person's paired server, and the grant that admits
//! it.
//!
//! A remote view is served by a producer the Person pinned, over a connection
//! they reviewed, to a recipient they named. Every one of those has to still be
//! the one they reviewed before anything is read, and a grant that already says
//! something different is theirs to change, not this path's.

use serde::{Deserialize, Serialize};

use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory,
    GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
    ResourceHandle, SourceAuthority,
};
use floe_kernel::{AgentFailure, PersonId};

use crate::{DataAccessGrant, GrantState};

/// The server that serves a Person's remote views, as it identifies itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProducerIdentity {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

/// One remote view source, as the producer signed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteViewSourceReference {
    pub view_id: String,
    pub person_id: String,
    pub client_id: String,
    pub device_id: String,
    pub audience: String,
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub execution_owner: String,
    pub source_authority: SourceAuthority,
    pub resource: String,
    pub provider_identity: String,
}

/// What the Person reviewed when they approved this remote view.
pub struct RemoteViewApproval<'a> {
    pub producer_fingerprint: &'a str,
    pub source_authority: SourceAuthority,
    pub connection_revision: u64,
    pub provider_identity: &'a str,
    pub recipient: &'a str,
}

/// That the server answering is the one the Person pinned.
pub fn producer_is_pinned(
    pinned: &RemoteProducerIdentity,
    observed: &RemoteProducerIdentity,
) -> Result<(), AgentFailure> {
    if pinned == observed {
        Ok(())
    } else {
        Err(AgentFailure::PolicyDenied)
    }
}

/// That the signed source describes the producer that served it.
///
/// A descriptor signed for one execution owner, audience or connection revision
/// says nothing about another.
pub fn source_matches_producer(
    reference: &RemoteViewSourceReference,
    producer: &RemoteProducerIdentity,
    connection_revision: u64,
) -> Result<(), AgentFailure> {
    if reference.execution_owner != producer.execution_owner
        || reference.audience != producer.audience
        || reference.connection_revision != connection_revision
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// That what the device observed is what the Person reviewed.
pub fn matches_review(
    approval: &RemoteViewApproval<'_>,
    reference: &RemoteViewSourceReference,
    producer: &RemoteProducerIdentity,
    connection_revision: u64,
) -> Result<(), AgentFailure> {
    if producer.fingerprint != approval.producer_fingerprint
        || reference.source_authority != approval.source_authority
        || connection_revision != approval.connection_revision
        || reference.provider_identity != approval.provider_identity
        || producer.audience != approval.recipient
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// The source binding a signed remote view descriptor names.
pub fn remote_view_source(
    reference: &RemoteViewSourceReference,
) -> Result<GrantSourceBinding, AgentFailure> {
    GrantSourceBinding::try_new(
        PersonId(
            reference
                .person_id
                .parse()
                .map_err(|_| AgentFailure::InvalidInput)?,
        ),
        ConnectionId::try_new(reference.connection_id.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(reference.connector_id.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(reference.execution_owner.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        reference.source_authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The scope one reviewed remote view grant carries.
///
/// The recipient is part of the grant, not a property of the transport: the
/// Person approved this data reaching that audience and no other.
pub fn remote_view_scope(
    resource: &str,
    category: GrantDataCategory,
    consumer: GrantConsumer,
    recipient: String,
) -> Result<GrantScope, AgentFailure> {
    GrantScope::try_new(
        vec![ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?],
        vec![category.clone()],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        vec![consumer],
        ProcessingRestriction::ApprovedRecipient {
            recipient,
            categories: vec![category],
        },
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// What a review does to the grant the Person already holds for this view.
pub enum RemoteViewGrantReview {
    /// The Person already granted exactly this. There is nothing to review.
    AlreadyGranted,
    /// Activate the grant under the reviewed scope, at the authority it is
    /// expected to still be at.
    Activate {
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
    },
}

/// Decide what this review does.
///
/// An active grant whose scope differs is the Person having approved something
/// else; widening it silently is not this path's to do.
pub fn review_remote_view_grant(
    existing: Option<&DataAccessGrant>,
    scope: &GrantScope,
) -> Result<RemoteViewGrantReview, AgentFailure> {
    if let Some(grant) = existing
        && grant.state() == GrantState::Active
    {
        return if grant.scope() == scope {
            Ok(RemoteViewGrantReview::AlreadyGranted)
        } else {
            Err(AgentFailure::PolicyDenied)
        };
    }
    Ok(RemoteViewGrantReview::Activate {
        grant_id: existing.map(|grant| grant.id()).unwrap_or_else(GrantId::new),
        expected: existing.map(|grant| grant.authority()),
    })
}
