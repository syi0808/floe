use floe_context_contract::ContextDependency;
use floe_kernel::AgentFailure;

use crate::{DataAccessGrant, GrantState};

pub fn validate_grant_dependency(
    grant: &DataAccessGrant,
    dependency: &ContextDependency,
) -> Result<(), AgentFailure> {
    if grant.id() != dependency.grant_id()
        || grant.state() != GrantState::Active
        || grant.review_required()
        || grant.authority() != dependency.grant_authority()
        || grant.source() != dependency.source()
        || dependency
            .resources()
            .iter()
            .any(|resource| !grant.scope().resources().contains(resource))
        || dependency
            .categories()
            .iter()
            .any(|category| !grant.scope().categories().contains(category))
        || !grant.scope().operations().contains(&dependency.operation())
        || !grant.scope().purposes().contains(&dependency.purpose())
        || !grant.scope().consumers().contains(dependency.consumer())
        || grant.scope().processing() != dependency.processing()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
