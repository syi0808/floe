use chrono::{DateTime, Utc};
use floe_context_contract::{
    ConsumerPolicyAuthority, ContextDependency, ContextDependencyError, GrantConsumer,
    GrantDataCategory, GrantOperation, GrantPurpose, GrantSourceBinding, ProcessingRestriction,
    ResourceHandle, SourceAuthority, validate_dependency_freshness,
};

use crate::{DataAccessGrant, GrantState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayRequest {
    pub now: DateTime<Utc>,
    pub source: SourceAuthority,
    pub consumer_policy: ConsumerPolicyAuthority,
    pub resources: Vec<ResourceHandle>,
    pub categories: Vec<GrantDataCategory>,
    pub operation: GrantOperation,
    pub purpose: GrantPurpose,
    pub consumer: GrantConsumer,
    pub processing: ProcessingRestriction,
}

pub trait ReplayTrust {
    fn source_is_current(&self, source: &GrantSourceBinding) -> bool;
    fn consumer_policy_is_current(&self, authority: ConsumerPolicyAuthority) -> bool;
    fn lease_is_current(&self, dependency: &ContextDependency, request: &ReplayRequest) -> bool;
}

pub fn admit_replay(
    dependency: &ContextDependency,
    grant: &DataAccessGrant,
    request: ReplayRequest,
    trust: &impl ReplayTrust,
) -> Result<(), ContextDependencyError> {
    validate_dependency_freshness(dependency, request.now)?;
    let mut requested_resources = request.resources.clone();
    let mut requested_categories = request.categories.clone();
    requested_resources.sort();
    requested_categories.sort();
    if dependency.person_id() != dependency.source().person_id()
        || dependency.grant_id() != grant.id()
        || dependency.grant_authority() != grant.authority()
        || *dependency.source() != *grant.source()
        || dependency.source().source_authority() != request.source
        || dependency.consumer_policy() != request.consumer_policy
        || dependency.operation() != request.operation
        || dependency.purpose() != request.purpose
        || *dependency.consumer() != request.consumer
        || *dependency.processing() != request.processing
        || requested_resources.is_empty()
        || requested_resources
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        || requested_categories.is_empty()
        || requested_categories
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        || !requested_resources
            .iter()
            .all(|item| dependency.resources().contains(item))
        || !requested_categories
            .iter()
            .all(|item| dependency.categories().contains(item))
        || grant.state() != GrantState::Active
        || grant.review_required()
        || !dependency
            .resources()
            .iter()
            .all(|item| grant.scope().resources().contains(item))
        || !dependency
            .categories()
            .iter()
            .all(|item| grant.scope().categories().contains(item))
        || !grant.scope().operations().contains(&dependency.operation())
        || !grant.scope().purposes().contains(&dependency.purpose())
        || !grant.scope().consumers().contains(dependency.consumer())
        || grant.scope().processing() != dependency.processing()
        || !trust.source_is_current(dependency.source())
        || !trust.consumer_policy_is_current(dependency.consumer_policy())
        || !trust.lease_is_current(dependency, &request)
    {
        return Err(ContextDependencyError::Unauthorized);
    }
    Ok(())
}
