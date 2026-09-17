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
    ProcessingRestriction,
};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};

use crate::{DataAccessGrant, GrantState};

/// What one read needs the Person to have granted.
pub struct PersonalReadRequirement<'a> {
    /// The source the read is bound to.
    pub source: &'a GrantSourceBinding,
    /// The resource handle inside that source.
    pub resource: &'a str,
    /// Who the read is for.
    pub consumer: &'a GrantConsumer,
    /// The grant must name the same source authority too, not only the same
    /// Person, connector, connection and execution owner.
    pub same_authority: bool,
    /// A second live grant on the same source is a review the Person owes,
    /// rather than a choice this read may make on their behalf.
    pub reject_ambiguous: bool,
}

/// Whether a grant is bound to the same source identity: the same Person,
/// connector, connection and execution owner. The source authority advances on
/// its own, so it is not part of the identity.
fn binds_source(grant: &DataAccessGrant, source: &GrantSourceBinding) -> bool {
    let binding = grant.source();
    binding.person_id() == source.person_id()
        && binding.connector() == source.connector()
        && binding.connection_id() == source.connection_id()
        && binding.execution_owner() == source.execution_owner()
}

/// The single live grant that admits this read.
pub fn active_read_grant(
    grants: &[DataAccessGrant],
    requirement: &PersonalReadRequirement<'_>,
) -> Result<DataAccessGrant, AgentFailure> {
    let source = requirement.source;
    let mut live = grants.iter().filter(|grant| {
        grant.state() != GrantState::Revoked
            && if requirement.same_authority {
                grant.source() == source
            } else {
                binds_source(grant, source)
            }
    });
    let grant = live.next().ok_or(AgentFailure::AccessReviewRequired)?;
    if requirement.reject_ambiguous && live.next().is_some() {
        return Err(AgentFailure::AccessReviewRequired);
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
        || grant.scope().processing() != &ProcessingRestriction::LocalOnly
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

/// The single active grant that admits reading one resource for this Person.
///
/// The caller says which sources are admissible and what resource handle each
/// must carry; Access decides the rest — the grant has to be active, not
/// awaiting review, and scoped to a read for the assistant by this consumer.
/// Nothing matching is a review the Person owes; more than one is a conflict
/// they have to resolve, not a choice a read may make on their behalf.
pub fn active_resource_grant(
    grants: &[DataAccessGrant],
    person_id: floe_kernel::PersonId,
    consumer: &GrantConsumer,
    required_resource: impl Fn(&GrantSourceBinding) -> Option<String>,
) -> Result<DataAccessGrant, AgentFailure> {
    let mut admitted = grants.iter().filter(|grant| {
        grant.source().person_id() == person_id
            && grant.state() == GrantState::Active
            && !grant.review_required()
            && grant.scope().operations().contains(&GrantOperation::Read)
            && grant.scope().purposes().contains(&GrantPurpose::Assistant)
            && grant.scope().consumers().contains(consumer)
            && grant.scope().resources().len() == 1
            && required_resource(grant.source())
                .is_some_and(|resource| grant.scope().resources()[0].as_str() == resource)
    });
    let grant = admitted.next().ok_or(AgentFailure::AccessReviewRequired)?;
    if admitted.next().is_some() {
        return Err(AgentFailure::Conflict);
    }
    Ok(grant.clone())
}

#[cfg(test)]
mod tests {
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantId, GrantScope, ResourceHandle,
        SourceAuthority,
    };
    use floe_kernel::PersonId;
    use uuid::Uuid;

    use super::*;

    fn consumer() -> GrantConsumer {
        GrantConsumer::builtin("assistant").unwrap()
    }

    fn source(person_id: PersonId, owner: &str) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("attention.macos.local").unwrap(),
            ConnectorId::try_new("attention.macos").unwrap(),
            ExecutionOwnerId::try_new(owner).unwrap(),
            SourceAuthority::new(),
        )
        .unwrap()
    }

    fn scope(resource: &str) -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new(resource).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![consumer()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    fn active(source: GrantSourceBinding, resource: &str) -> DataAccessGrant {
        let mut grant = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            source.clone(),
            scope(resource),
        )
        .unwrap();
        grant
            .activate_review(grant.authority(), source, scope(resource))
            .unwrap();
        grant
    }

    fn requirement<'a>(source: &'a GrantSourceBinding, consumer: &'a GrantConsumer) -> PersonalReadRequirement<'a> {
        PersonalReadRequirement {
            source,
            resource: "attention.coarse",
            consumer,
            same_authority: false,
            reject_ambiguous: false,
        }
    }

    #[test]
    fn a_read_needs_a_grant_the_person_actually_left_for_it() {
        let person_id = PersonId::new();
        let consumer = consumer();
        let bound = source(person_id, "device:this");
        let grants = [active(bound.clone(), "attention.coarse")];
        assert!(active_read_grant(&grants, &requirement(&bound, &consumer)).is_ok());

        // A grant for a different resource, or for another device's execution
        // owner, is not this read's grant.
        let other_resource = [active(bound.clone(), "wellbeing.derived")];
        assert_eq!(
            active_read_grant(&other_resource, &requirement(&bound, &consumer)),
            Err(AgentFailure::AccessReviewRequired)
        );
        let other_device = [active(source(person_id, "device:other"), "attention.coarse")];
        assert_eq!(
            active_read_grant(&other_device, &requirement(&bound, &consumer)),
            Err(AgentFailure::AccessReviewRequired)
        );
    }

    #[test]
    fn a_second_live_grant_is_a_review_only_where_the_read_says_so() {
        let person_id = PersonId::new();
        let consumer = consumer();
        let bound = source(person_id, "device:this");
        let grants = [
            active(bound.clone(), "attention.coarse"),
            active(bound.clone(), "attention.coarse"),
        ];
        assert!(active_read_grant(&grants, &requirement(&bound, &consumer)).is_ok());
        assert_eq!(
            active_read_grant(
                &grants,
                &PersonalReadRequirement {
                    reject_ambiguous: true,
                    ..requirement(&bound, &consumer)
                }
            ),
            Err(AgentFailure::AccessReviewRequired)
        );
    }

    #[test]
    fn a_grant_that_changed_under_the_read_does_not_carry_it() {
        let person_id = PersonId::new();
        let bound = source(person_id, "device:this");
        let before = active(bound.clone(), "attention.coarse");
        assert_eq!(grant_unchanged(&before, &before.clone()), Ok(()));

        let mut after = before.clone();
        after
            .review_active(after.authority(), bound, scope("attention.coarse"))
            .unwrap();
        assert_eq!(
            grant_unchanged(&before, &after),
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            grant_unchanged(&before, &active(source(person_id, "device:this"), "attention.coarse")),
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn the_reviewed_device_subject_has_to_be_the_one_that_answered() {
        let reviewed = "a".repeat(64);
        let other = "b".repeat(64);
        assert_eq!(subject_unchanged(&reviewed, &reviewed, &reviewed), Ok(()));
        assert_eq!(
            subject_unchanged(&reviewed, &other, &reviewed),
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            subject_unchanged(&reviewed, &reviewed, &other),
            Err(AgentFailure::AccessReviewRequired)
        );
        assert!(valid_subject_fingerprint(&reviewed));
        assert!(!valid_subject_fingerprint(&"A".repeat(64)));
        assert!(!valid_subject_fingerprint(&"a".repeat(63)));
    }

    #[test]
    fn a_resource_read_takes_the_one_admitted_grant_and_refuses_a_tie() {
        let person_id = PersonId::new();
        let consumer = consumer();
        let bound = source(person_id, "device:this");
        let resource = |_: &GrantSourceBinding| Some("attention.coarse".to_owned());
        assert!(
            active_resource_grant(
                &[active(bound.clone(), "attention.coarse")],
                person_id,
                &consumer,
                resource
            )
            .is_ok()
        );
        assert_eq!(
            active_resource_grant(&[], person_id, &consumer, resource),
            Err(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(
            active_resource_grant(
                &[
                    active(bound.clone(), "attention.coarse"),
                    active(source(person_id, "device:other"), "attention.coarse"),
                ],
                person_id,
                &consumer,
                resource
            ),
            Err(AgentFailure::Conflict)
        );
        // A source the caller does not admit contributes no grant at all.
        assert_eq!(
            active_resource_grant(
                &[active(bound, "attention.coarse")],
                person_id,
                &consumer,
                |_| None
            ),
            Err(AgentFailure::AccessReviewRequired)
        );
    }
}

/// The query one feasibility grant admits.
///
/// The Person reviewed a specific event, at a specific destination, in a
/// specific window. A read that asks for anything else is not the one they
/// granted.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FeasibilityGrantQuery {
    pub event_handle: String,
    pub evidence_handles: Vec<String>,
    pub destination_latitude: f64,
    pub destination_longitude: f64,
    pub event_start_unix_ms: i64,
    pub event_end_unix_ms: i64,
    pub travel_mode: String,
}

impl FeasibilityGrantQuery {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.event_handle.is_empty()
            || self.event_handle.len() > 128
            || self.event_handle.chars().any(char::is_whitespace)
            || self.evidence_handles.is_empty()
            || self.evidence_handles.len() > 8
            || self
                .evidence_handles
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.evidence_handles.iter().any(|handle| {
                handle.is_empty() || handle.len() > 128 || handle.chars().any(char::is_whitespace)
            })
            || !self.destination_latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.destination_latitude)
            || !self.destination_longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.destination_longitude)
            || self.event_start_unix_ms < 0
            || self.event_end_unix_ms <= self.event_start_unix_ms
            || self.event_end_unix_ms - self.event_start_unix_ms > 86_400_000
            || !matches!(
                self.travel_mode.as_str(),
                "automobile" | "transit" | "walking"
            )
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
