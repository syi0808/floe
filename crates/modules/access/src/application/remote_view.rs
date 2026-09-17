//! Reading a source through a Person's paired server, and the grant that admits
//! it.
//!
//! A remote view is served by a producer the Person pinned, over a connection
//! they reviewed, to a recipient they named. Every one of those has to still be
//! the one they reviewed before anything is read, and a grant that already says
//! something different is theirs to change, not this path's.

use serde::{Deserialize, Serialize};

use floe_context_contract::{
    ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency, ExecutionOwnerId,
    GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
    GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
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
        grant_id: existing
            .map(|grant| grant.id())
            .unwrap_or_else(GrantId::new),
        expected: existing.map(|grant| grant.authority()),
    })
}

/// The signed source a remote read is about to use, still describing the grant
/// it runs under.
///
/// A descriptor signed under a different source authority, or naming no
/// connection revision or provider at all, describes some other read.
pub fn admit_remote_view_source(
    reference: &RemoteViewSourceReference,
    source: &GrantSourceBinding,
) -> Result<(), AgentFailure> {
    if reference.source_authority != source.source_authority()
        || reference.connection_revision == 0
        || reference.provider_identity.is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// The binding recorded for this source, still being the grant the read
/// selected and still naming the resource it is about to read.
pub fn admit_remote_view_binding(
    binding: &DataAccessGrant,
    selected: &DataAccessGrant,
    source: &GrantSourceBinding,
    resource: &str,
) -> Result<(), AgentFailure> {
    if binding.id() != selected.id()
        || binding.authority() != selected.authority()
        || binding.source() != source
        || !binding
            .scope()
            .resources()
            .iter()
            .any(|item| item.as_str() == resource)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// A stored remote dependency this Person could still be shown at all.
pub fn remote_dependency_live(
    dependency: &ContextDependency,
    person_id: PersonId,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AgentFailure> {
    dependency
        .validate()
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if dependency.person_id() != person_id
        || dependency.source().person_id() != person_id
        || dependency.operation() != GrantOperation::Read
        || dependency.purpose() != GrantPurpose::Assistant
        || dependency.source().execution_owner().as_str().is_empty()
        || now >= dependency.expires_at()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// The grant a stored remote dependency was recorded under, still admitting it,
/// and the single resource it admits.
pub fn remote_dependency_resource<'a>(
    grant: &'a DataAccessGrant,
    dependency: &ContextDependency,
) -> Result<&'a str, AgentFailure> {
    if grant.authority() != dependency.grant_authority()
        || grant.source() != dependency.source()
        || grant.state() != GrantState::Active
        || grant.review_required()
        || !grant.scope().operations().contains(&GrantOperation::Read)
        || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(dependency.consumer())
        || grant.scope().resources().len() != 1
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(grant.scope().resources()[0].as_str())
}

/// The producer, source and recipient a stored remote dependency named, still
/// the ones answering.
pub fn remote_dependency_source_admits(
    dependency: &ContextDependency,
    reference: &RemoteViewSourceReference,
    connection_revision: u64,
    recipient: &str,
) -> Result<(), AgentFailure> {
    if reference.source_authority != dependency.source().source_authority()
        || reference.execution_owner != dependency.source().execution_owner().as_str()
        || reference.connection_revision != connection_revision
    {
        return Err(AgentFailure::PolicyDenied);
    }
    match dependency.processing() {
        ProcessingRestriction::ApprovedRecipient {
            recipient: approved,
            ..
        } if approved == recipient => {}
        _ => return Err(AgentFailure::PolicyDenied),
    }
    Ok(())
}

/// The binding recorded for the dependency's source, still the one it names.
pub fn remote_dependency_binding_matches(
    consumer_policy: ConsumerPolicyAuthority,
    authority: GrantAuthority,
    dependency: &ContextDependency,
) -> Result<(), AgentFailure> {
    if consumer_policy != dependency.consumer_policy() || authority != dependency.grant_authority()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
