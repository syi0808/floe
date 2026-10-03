use crate::{DataAccessGrant, GrantState, RemoteViewAuthorizationExpectation};
use floe_context_contract::{
    GrantConsumer, GrantOperation, GrantPurpose, ResourceHandle, split_connection_view_resource,
};
use floe_kernel::{AgentFailure, PersonId};

/// Grant policy for a signed source read. Producer challenge decoding and
/// signature verification remain at the Gateway boundary; row CAS remains Vault.
pub fn validate_authorization_grant(
    grant: &DataAccessGrant,
    person_id: PersonId,
    expected: &RemoteViewAuthorizationExpectation,
    purpose: GrantPurpose,
    consumer: &GrantConsumer,
) -> Result<(), AgentFailure> {
    grant.validate().map_err(|_| AgentFailure::PolicyDenied)?;
    let [resource] = expected.resources.as_slice() else {
        return Err(AgentFailure::PolicyDenied);
    };
    let resource =
        ResourceHandle::try_new(resource.clone()).map_err(|_| AgentFailure::PolicyDenied)?;
    split_connection_view_resource(&resource, &grant.source().connection_id())
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if grant.state() != GrantState::Active
        || grant.review_required()
        || grant.source().person_id() != person_id
        || grant.id().as_uuid().to_string() != expected.grant_id
        || grant.source().connection_id().as_str() != expected.source_connection
        || grant.source().connector().as_str() != expected.source_connector
        || grant.source().execution_owner().as_str() != expected.source_execution_owner
        || grant.authority().incarnation().to_string() != expected.grant_incarnation
        || grant.authority().access_epoch().get() != expected.grant_epoch
        || !grant.scope().operations().contains(&GrantOperation::Read)
        || grant.scope().resources() != [resource]
        || !grant.scope().purposes().contains(&purpose)
        || !grant.scope().consumers().contains(consumer)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
