use std::fmt;

use floe_context_contract::{GrantAuthority, GrantScope, GrantSourceBinding, GrantValidationError};
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::{DataAccessGrant, GrantTransitionError};
use floe_context_contract::GrantId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessGrantMutation {
    Review { scope: GrantScope },
    Activate { scope: GrantScope },
    ReviewActive { scope: GrantScope },
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

/// The reviewed grant expectation a Calendar mutation runs under.
/// Both halves must be present or both absent; a half expectation is invalid
/// input, never a fresh review.
pub fn validate_grant_expectation(
    expected_id: Option<floe_context_contract::GrantId>,
    expected_authority: Option<GrantAuthority>,
) -> Result<Option<(floe_context_contract::GrantId, GrantAuthority)>, floe_kernel::AgentFailure> {
    match (expected_id, expected_authority) {
        (None, None) => Ok(None),
        (Some(id), Some(authority)) if id.is_valid() && authority.is_valid() => {
            Ok(Some((id, authority)))
        }
        _ => Err(floe_kernel::AgentFailure::InvalidInput),
    }
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
        AccessGrantMutation::Review { scope } => grant
            .review(expected, scope)
            .map_err(GrantPolicyError::Transition),
        AccessGrantMutation::Activate { scope } => grant
            .activate_review(expected, scope)
            .map_err(GrantPolicyError::Transition),
        AccessGrantMutation::ReviewActive { scope } => grant
            .review_active(expected, scope)
            .map_err(GrantPolicyError::Transition),
        AccessGrantMutation::Pause => grant.pause(expected).map_err(GrantPolicyError::Transition),
        AccessGrantMutation::Revoke => grant.revoke(expected).map_err(GrantPolicyError::Transition),
    }
}
