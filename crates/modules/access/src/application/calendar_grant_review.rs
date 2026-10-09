//! Access-owned classification of Calendar grant state for source review.

use crate::{DataAccessGrant, GrantState};
use floe_context_contract::{GrantSourceBinding, ObservedGrant, SourceAccessRequirementKind};
use floe_kernel::AgentFailure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarGrantReviewClassification {
    pub reason: SourceAccessRequirementKind,
    pub observed: Option<ObservedGrant>,
}

/// Classify the current non-revoked Calendar grant for an exact source.
/// Revoked grants are absence; multiple live matches fail closed.
pub fn classify_calendar_grant_review(
    grants: &[DataAccessGrant],
    source: &GrantSourceBinding,
) -> Result<CalendarGrantReviewClassification, AgentFailure> {
    let mut matching = grants
        .iter()
        .filter(|grant| grant.source() == source && grant.state() != GrantState::Revoked);
    let grant = matching.next();
    if matching.next().is_some() {
        return Err(AgentFailure::PolicyDenied);
    }

    let observed = grant
        .map(|grant| {
            ObservedGrant::try_new(grant.id(), grant.authority())
                .map_err(|_| AgentFailure::StaleContext)
        })
        .transpose()?;
    let reason = match grant {
        None => SourceAccessRequirementKind::EnableObserve,
        Some(grant) if grant.state() == GrantState::Paused => {
            SourceAccessRequirementKind::EnableObserve
        }
        Some(_) => SourceAccessRequirementKind::ReviewChangedSource,
    };

    Ok(CalendarGrantReviewClassification { reason, observed })
}

#[cfg(test)]
mod tests {
    use super::classify_calendar_grant_review;
    use crate::{DataAccessGrant, GrantState};
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ObservedGrant,
        ProcessingRestriction, ResourceHandle, SourceAccessRequirementKind,
    };
    use floe_kernel::{AgentFailure, PersonId};
    use uuid::Uuid;

    fn test_source(person_id: PersonId, owner: &str) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("calendar.connection").unwrap(),
            ConnectorId::try_new("calendar.apple").unwrap(),
            ExecutionOwnerId::try_new(owner).unwrap(),
        )
        .unwrap()
    }

    fn scope() -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar.connection").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("fixture.expert").unwrap()],
            ProcessingRestriction::DeviceOnly,
        )
        .unwrap()
    }

    fn grant(source: &GrantSourceBinding, state: GrantState) -> DataAccessGrant {
        let scope = scope();
        let mut grant = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            source.clone(),
            scope.clone(),
        )
        .unwrap();
        match state {
            GrantState::Paused => {}
            GrantState::Active => {
                grant.activate_review(grant.authority(), scope).unwrap();
            }
            GrantState::Revoked => {
                grant.revoke(grant.authority()).unwrap();
            }
        }
        grant
    }

    #[test]
    fn absence_and_revoked_or_other_source_grants_require_observe_without_an_observation() {
        let person_id = PersonId::new();
        let source = test_source(person_id, "device.one");
        let revoked = grant(&source, GrantState::Revoked);
        let other_source = test_source(person_id, "device.two");

        let classified = classify_calendar_grant_review(&[], &source).unwrap();
        assert_eq!(
            classified.reason,
            SourceAccessRequirementKind::EnableObserve
        );
        assert_eq!(classified.observed, None);

        let classified = classify_calendar_grant_review(&[revoked], &source).unwrap();
        assert_eq!(
            classified.reason,
            SourceAccessRequirementKind::EnableObserve
        );
        assert_eq!(classified.observed, None);

        let classified =
            classify_calendar_grant_review(&[grant(&other_source, GrantState::Active)], &source)
                .unwrap();
        assert_eq!(
            classified.reason,
            SourceAccessRequirementKind::EnableObserve
        );
        assert_eq!(classified.observed, None);
    }

    #[test]
    fn paused_and_active_grants_preserve_the_exact_observed_authority() {
        for (state, reason) in [
            (
                GrantState::Paused,
                SourceAccessRequirementKind::EnableObserve,
            ),
            (
                GrantState::Active,
                SourceAccessRequirementKind::ReviewChangedSource,
            ),
        ] {
            let source = test_source(PersonId::new(), "device.one");
            let grant = grant(&source, state);
            let expected = ObservedGrant::try_new(grant.id(), grant.authority()).unwrap();

            let classified = classify_calendar_grant_review(&[grant], &source).unwrap();
            assert_eq!(classified.reason, reason);
            assert_eq!(classified.observed, Some(expected));
        }
    }

    #[test]
    fn multiple_non_revoked_grants_for_the_exact_source_fail_closed() {
        let source = test_source(PersonId::new(), "device.one");
        let grants = [
            grant(&source, GrantState::Paused),
            grant(&source, GrantState::Active),
        ];

        assert_eq!(
            classify_calendar_grant_review(&grants, &source),
            Err(AgentFailure::PolicyDenied)
        );
    }
}
