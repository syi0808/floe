//! Which grant a personal-source read runs under, and whether it still holds
//! when the read comes back.
//!
//! A native read takes time, and the Person can revoke, re-review or re-bind
//! their grant while it is in flight. Access answers three things for every such
//! read: which single live grant admits it, that the grant is still that same
//! grant afterwards, and that the device subject the Person reviewed is the one
//! that answered. A reader composes the acquisition; it does not decide any of
//! this.

use floe_context_contract::{
    GrantConsumer, GrantDataCategory, GrantOperation, GrantPurpose, GrantSourceBinding,
};
use floe_kernel::AgentFailure;

use crate::{DataAccessGrant, GrantState};

/// What one read needs the Person to have granted.
pub struct PersonalReadRequirement<'a> {
    /// The source the read is bound to.
    pub source: &'a GrantSourceBinding,
    /// The resource handle inside that source.
    pub resource: &'a str,
    /// Who the read is for.
    pub consumer: &'a GrantConsumer,
    /// A second live grant on the same source is a review the Person owes,
    /// rather than a choice this read may make on their behalf.
    pub reject_ambiguous: bool,
}

/// The single live grant that admits this read.
pub fn active_read_grant(
    grants: &[DataAccessGrant],
    requirement: &PersonalReadRequirement<'_>,
) -> Result<DataAccessGrant, AgentFailure> {
    let source = requirement.source;
    let mut live = grants
        .iter()
        .filter(|grant| grant.state() != GrantState::Revoked && grant.source() == source);
    let grant = live.next().ok_or(AgentFailure::AccessReviewRequired)?;
    if requirement.reject_ambiguous && live.next().is_some() {
        return Err(AgentFailure::Conflict);
    }
    if grant.state() != GrantState::Active
        || grant.review_required()
        || !grant
            .scope()
            .resources()
            .iter()
            .any(|resource| resource.as_str() == requirement.resource)
        || !grant
            .scope()
            .categories()
            .contains(&GrantDataCategory::Derived)
        || !grant.scope().operations().contains(&GrantOperation::Read)
        || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(requirement.consumer)
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(grant.clone())
}

/// That the grant a read started under is the grant it finished under.
///
/// A new identity, a new authority or a re-bound source all mean the Person
/// changed something mid-read, and the result is not theirs to keep.
pub fn grant_unchanged(
    before: &DataAccessGrant,
    after: &DataAccessGrant,
) -> Result<(), AgentFailure> {
    if after.id() != before.id()
        || after.authority() != before.authority()
        || after.source() != before.source()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// The shape a device subject fingerprint has to have to be compared at all.
pub fn valid_subject_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// That the device subject the Person reviewed is the one that answered, both
/// before the read and after it.
pub fn subject_unchanged(reviewed: &str, before: &str, after: &str) -> Result<(), AgentFailure> {
    if before != reviewed || after != reviewed {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(())
}

pub fn attention_consumer(value: &str) -> Result<GrantConsumer, AgentFailure> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        || GrantConsumer::builtin(value).is_err()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    GrantConsumer::builtin(value).map_err(|_| AgentFailure::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::{PersonalReadRequirement, active_read_grant};
    use crate::DataAccessGrant;
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
        ResourceHandle,
    };
    use floe_kernel::{AgentFailure, PersonId};
    use uuid::Uuid;

    #[test]
    fn ambiguous_live_grants_remain_a_conflict_for_history_reauthorization() {
        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("fixture.connection").unwrap(),
            ConnectorId::try_new("fixture.connector").unwrap(),
            ExecutionOwnerId::try_new("fixture.device").unwrap(),
        )
        .unwrap();
        let consumer = GrantConsumer::builtin("fixture.manager").unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("fixture.resource").unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![consumer.clone()],
            ProcessingRestriction::DeviceOnly,
        )
        .unwrap();
        let active_grant = || {
            let mut grant = DataAccessGrant::new(
                GrantId::new(),
                Uuid::new_v4(),
                source.clone(),
                scope.clone(),
            )
            .unwrap();
            grant
                .activate_review(grant.authority(), scope.clone())
                .unwrap();
            grant
        };
        let requirement = PersonalReadRequirement {
            source: &source,
            resource: "fixture.resource",
            consumer: &consumer,
            reject_ambiguous: true,
        };

        assert_eq!(
            active_read_grant(&[active_grant(), active_grant()], &requirement),
            Err(AgentFailure::Conflict),
            "ambiguous authority is not silently omitted as stale history"
        );
    }
}
