use std::fmt;

use floe_context_contract::{GrantAuthority, GrantScope, GrantSourceBinding, GrantValidationError};
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::{DataAccessGrant, GrantTransitionError};
use floe_context_contract::{GrantId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessGrantMutation {
    Review {
        source: GrantSourceBinding,
        scope: GrantScope,
    },
    Activate {
        source: GrantSourceBinding,
        scope: GrantScope,
    },
    ReviewActive {
        source: GrantSourceBinding,
        scope: GrantScope,
    },
    Pause,
    Revoke,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GrantPolicyError {
    Unauthorized,
    Invalid(GrantValidationError),
    Transition(GrantTransitionError),
}

impl fmt::Display for GrantPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => formatter.write_str("grant owner is not authorized"),
            Self::Invalid(error) => write!(formatter, "invalid grant: {error}"),
            Self::Transition(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for GrantPolicyError {}

pub fn create_grant(
    id: GrantId,
    person_id: PersonId,
    authority_owner: Uuid,
    source: GrantSourceBinding,
    scope: GrantScope,
) -> Result<DataAccessGrant, GrantPolicyError> {
    if source.person_id() != person_id {
        return Err(GrantPolicyError::Unauthorized);
    }
    DataAccessGrant::new(id, authority_owner, source, scope).map_err(GrantPolicyError::Invalid)
}

pub fn authorize_grant(
    grant: &DataAccessGrant,
    person_id: PersonId,
    authority_owner: Uuid,
) -> Result<(), GrantPolicyError> {
    grant.validate().map_err(GrantPolicyError::Invalid)?;
    if grant.source().person_id() != person_id || grant.authority_owner() != authority_owner {
        return Err(GrantPolicyError::Unauthorized);
    }
    Ok(())
}

pub fn apply_grant_mutation(
    grant: &mut DataAccessGrant,
    person_id: PersonId,
    authority_owner: Uuid,
    expected: GrantAuthority,
    mutation: AccessGrantMutation,
) -> Result<bool, GrantPolicyError> {
    authorize_grant(grant, person_id, authority_owner)?;
    match mutation {
        AccessGrantMutation::Review { source, scope } => grant
            .review(expected, source, scope)
            .map_err(GrantPolicyError::Transition),
        AccessGrantMutation::Activate { source, scope } => grant
            .activate_review(expected, source, scope)
            .map_err(GrantPolicyError::Transition),
        AccessGrantMutation::ReviewActive { source, scope } => grant
            .review_active(expected, source, scope)
            .map_err(GrantPolicyError::Transition),
        AccessGrantMutation::Pause => grant.pause(expected).map_err(GrantPolicyError::Transition),
        AccessGrantMutation::Revoke => grant.revoke(expected).map_err(GrantPolicyError::Transition),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory, GrantOperation, GrantPurpose, ProcessingRestriction, ResourceHandle, SourceAuthority};

    fn fixture() -> (PersonId, DataAccessGrant, GrantScope) {
        let person = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let grant = create_grant(
            GrantId::new(),
            person,
            Uuid::new_v4(),
            source,
            scope.clone(),
        )
        .unwrap();
        (person, grant, scope)
    }

    #[test]
    fn mutation_checks_person_owner_and_epoch() {
        let (person, mut grant, scope) = fixture();
        let owner = grant.authority_owner();
        let source = grant.source().clone();
        let initial_authority = grant.authority();
        let initial = grant.clone();
        assert_eq!(
            apply_grant_mutation(
                &mut grant,
                PersonId::new(),
                owner,
                initial_authority,
                AccessGrantMutation::Activate {
                    source: source.clone(),
                    scope: scope.clone()
                },
            ),
            Err(GrantPolicyError::Unauthorized)
        );
        assert_eq!(grant, initial);
        assert_eq!(
            apply_grant_mutation(
                &mut grant,
                person,
                Uuid::new_v4(),
                initial_authority,
                AccessGrantMutation::Revoke,
            ),
            Err(GrantPolicyError::Unauthorized)
        );
        assert_eq!(grant, initial);
        let active_authority = grant.authority();
        assert!(
            apply_grant_mutation(
                &mut grant,
                person,
                owner,
                active_authority,
                AccessGrantMutation::Activate { source, scope },
            )
            .unwrap()
        );
        let active = grant.clone();
        assert_eq!(
            apply_grant_mutation(
                &mut grant,
                person,
                owner,
                GrantAuthority::new(),
                AccessGrantMutation::Pause,
            ),
            Err(GrantPolicyError::Transition(GrantTransitionError::Conflict))
        );
        assert_eq!(grant, active);
    }
}
