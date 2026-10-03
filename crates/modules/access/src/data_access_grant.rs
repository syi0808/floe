use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use floe_context_contract::{
    GrantAuthority, GrantId, GrantScope, GrantSourceBinding, GrantValidationError,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantState {
    Paused,
    Active,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataAccessGrant {
    id: GrantId,
    authority_owner: Uuid,
    source: GrantSourceBinding,
    scope: GrantScope,
    authority: GrantAuthority,
    state: GrantState,
    review_required: bool,
}

impl DataAccessGrant {
    pub fn new(
        id: GrantId,
        authority_owner: Uuid,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<Self, GrantValidationError> {
        if !id.is_valid() || authority_owner.is_nil() {
            return Err(GrantValidationError::Identity);
        }
        source.validate()?;
        scope.validate()?;
        Ok(Self {
            id,
            authority_owner,
            source,
            scope,
            authority: GrantAuthority::new(),
            state: GrantState::Paused,
            review_required: true,
        })
    }
    pub fn id(&self) -> GrantId {
        self.id
    }
    pub fn authority_owner(&self) -> Uuid {
        self.authority_owner
    }
    pub fn source(&self) -> &GrantSourceBinding {
        &self.source
    }
    pub fn scope(&self) -> &GrantScope {
        &self.scope
    }
    pub fn authority(&self) -> GrantAuthority {
        self.authority
    }
    pub fn state(&self) -> GrantState {
        self.state
    }
    pub fn review_required(&self) -> bool {
        self.review_required
    }
    pub fn validate(&self) -> Result<(), GrantValidationError> {
        if !self.id.is_valid() || self.authority_owner.is_nil() || !self.authority.is_valid() {
            return Err(GrantValidationError::Identity);
        }
        if self.state == GrantState::Active && self.review_required {
            return Err(GrantValidationError::InvalidState);
        }
        self.source.validate()?;
        self.scope.validate()
    }
    pub fn activate_review(
        &mut self,
        expected: GrantAuthority,
        scope: GrantScope,
    ) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        scope.validate().map_err(GrantTransitionError::Invalid)?;
        if self.state == GrantState::Revoked {
            return Err(GrantTransitionError::Terminal);
        }
        if self.state == GrantState::Active && self.scope == scope {
            return Ok(false);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.scope = scope;
        self.state = GrantState::Active;
        self.review_required = false;
        Ok(true)
    }

    pub fn review_active(
        &mut self,
        expected: GrantAuthority,
        scope: GrantScope,
    ) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        scope.validate().map_err(GrantTransitionError::Invalid)?;
        if self.state == GrantState::Revoked {
            return Err(GrantTransitionError::Terminal);
        }
        if self.state != GrantState::Active {
            return Err(GrantTransitionError::Conflict);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.scope = scope;
        self.review_required = false;
        Ok(true)
    }
    pub fn review(
        &mut self,
        expected: GrantAuthority,
        scope: GrantScope,
    ) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        scope.validate().map_err(GrantTransitionError::Invalid)?;
        if self.state == GrantState::Revoked {
            return Err(GrantTransitionError::Terminal);
        }
        if self.state != GrantState::Paused {
            return Err(GrantTransitionError::Conflict);
        }
        if !self.review_required && self.scope == scope {
            return Ok(false);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.scope = scope;
        self.review_required = false;
        Ok(true)
    }
    pub fn pause(&mut self, expected: GrantAuthority) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        if self.state == GrantState::Revoked {
            return Err(GrantTransitionError::Terminal);
        }
        if self.state == GrantState::Paused {
            return Ok(false);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.state = GrantState::Paused;
        Ok(true)
    }
    /// Invalidate a reviewed source configuration without authorizing its new
    /// physical resources. Every live grant requires a fresh explicit review.
    pub fn invalidate_source(&mut self, expected: GrantAuthority) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        if self.state == GrantState::Revoked { return Err(GrantTransitionError::Terminal); }
        self.authority = self.authority.advance().ok_or(GrantTransitionError::Overflow)?;
        self.state = GrantState::Paused;
        self.review_required = true;
        Ok(true)
    }
    pub fn revoke(&mut self, expected: GrantAuthority) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        if self.state == GrantState::Revoked {
            return Ok(false);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.state = GrantState::Revoked;
        Ok(true)
    }
    fn check_expected(&self, expected: GrantAuthority) -> Result<(), GrantTransitionError> {
        self.validate().map_err(GrantTransitionError::Invalid)?;
        if !expected.is_valid() {
            return Err(GrantTransitionError::Invalid(
                GrantValidationError::Identity,
            ));
        }
        (expected == self.authority)
            .then_some(())
            .ok_or(GrantTransitionError::Conflict)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GrantTransitionError {
    #[error("stale grant authority")]
    Conflict,
    #[error("revoked grant is terminal")]
    Terminal,
    #[error("grant epoch exhausted")]
    Overflow,
    #[error("invalid grant: {0}")]
    Invalid(GrantValidationError),
}
