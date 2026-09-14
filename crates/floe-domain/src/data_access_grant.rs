use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory,
    GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, GrantValidationError,
    MAX_CONNECTOR_ID_BYTES, MAX_CONSUMER_ID_BYTES, MAX_CONSUMERS, MAX_EXECUTION_OWNER_BYTES,
    MAX_RESOURCE_HANDLE_BYTES, MAX_RESOURCE_HANDLES, MAX_SCOPE_BYTES, ProcessingRestriction,
    ResourceHandle,
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
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        self.validate_transition_source(&source)?;
        scope.validate().map_err(GrantTransitionError::Invalid)?;
        if self.state == GrantState::Revoked {
            return Err(GrantTransitionError::Terminal);
        }
        if self.state == GrantState::Active && self.source == source && self.scope == scope {
            return Ok(false);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.source = source;
        self.scope = scope;
        self.state = GrantState::Active;
        self.review_required = false;
        Ok(true)
    }

    pub fn review_active(
        &mut self,
        expected: GrantAuthority,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        self.validate_transition_source(&source)?;
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
        self.source = source;
        self.scope = scope;
        self.review_required = false;
        Ok(true)
    }
    pub fn review(
        &mut self,
        expected: GrantAuthority,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<bool, GrantTransitionError> {
        self.check_expected(expected)?;
        self.validate_transition_source(&source)?;
        scope.validate().map_err(GrantTransitionError::Invalid)?;
        if self.state == GrantState::Revoked {
            return Err(GrantTransitionError::Terminal);
        }
        if self.state != GrantState::Paused {
            return Err(GrantTransitionError::Conflict);
        }
        if !self.review_required && self.source == source && self.scope == scope {
            return Ok(false);
        }
        self.authority = self
            .authority
            .advance()
            .ok_or(GrantTransitionError::Overflow)?;
        self.source = source;
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
    fn validate_transition_source(
        &self,
        source: &GrantSourceBinding,
    ) -> Result<(), GrantTransitionError> {
        source.validate().map_err(GrantTransitionError::Invalid)?;
        if !self.source.same_identity(source) {
            return Err(GrantTransitionError::Identity);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GrantTransitionError {
    #[error("stale grant authority")]
    Conflict,
    #[error("grant identity mismatch")]
    Identity,
    #[error("revoked grant is terminal")]
    Terminal,
    #[error("grant epoch exhausted")]
    Overflow,
    #[error("invalid grant: {0}")]
    Invalid(GrantValidationError),
}
#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use crate::{PersonId, SourceAuthority};

    use super::*;

    fn scope() -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    #[test]
    fn scope_is_explicit_and_bounded() {
        assert!(ResourceHandle::try_new("*").is_err());
        assert!(
            GrantScope::try_new(
                vec![],
                vec![GrantDataCategory::Metadata],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Scheduling],
                vec![GrantConsumer::builtin("x").unwrap()],
                ProcessingRestriction::LocalOnly
            )
            .is_err()
        );
        let first = GrantScope::try_new(
            vec![
                ResourceHandle::try_new("calendar/z").unwrap(),
                ResourceHandle::try_new("calendar/a").unwrap(),
            ],
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            vec![GrantOperation::Suggestion, GrantOperation::Read],
            vec![GrantPurpose::Summarization, GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let second = GrantScope::try_new(
            vec![
                ResourceHandle::try_new("calendar/a").unwrap(),
                ResourceHandle::try_new("calendar/z").unwrap(),
            ],
            vec![GrantDataCategory::Content, GrantDataCategory::Metadata],
            vec![GrantOperation::Read, GrantOperation::Suggestion],
            vec![GrantPurpose::Scheduling, GrantPurpose::Summarization],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        assert_eq!(first, second);
        let recipient_first = GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::approved_recipient(
                "device-export",
                vec![GrantDataCategory::Content, GrantDataCategory::Metadata],
            )
            .unwrap(),
        )
        .unwrap();
        let recipient_second = GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Content, GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::approved_recipient(
                "device-export",
                vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(recipient_first, recipient_second);
        assert!(
            GrantScope::try_new(
                vec![
                    ResourceHandle::try_new("x").unwrap(),
                    ResourceHandle::try_new("x").unwrap()
                ],
                vec![GrantDataCategory::Metadata],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Scheduling],
                vec![GrantConsumer::builtin("x").unwrap()],
                ProcessingRestriction::LocalOnly
            )
            .is_err()
        );
    }

    #[test]
    fn review_transitions_are_checked_and_terminal() {
        let person = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("mac-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let connection_id = source.connection_id();
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source.clone(), scope()).unwrap();
        assert!(grant.review_required());
        let stamp = grant.authority();
        assert!(
            grant
                .activate_review(stamp, source.clone(), scope())
                .unwrap()
        );
        let active = grant.authority();
        assert!(!grant.review_required());
        assert!(!grant.activate_review(active, source, scope()).unwrap());
        let alternate_connection = GrantSourceBinding::try_new(
            person,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("mac-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        assert_eq!(
            grant.activate_review(active, alternate_connection, scope()),
            Err(GrantTransitionError::Identity)
        );
        let alternate_connector = GrantSourceBinding::try_new(
            person,
            connection_id.clone(),
            ConnectorId::try_new("other-calendar").unwrap(),
            ExecutionOwnerId::try_new("mac-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        assert_eq!(
            grant.activate_review(active, alternate_connector, scope()),
            Err(GrantTransitionError::Identity)
        );
        let alternate_owner = GrantSourceBinding::try_new(
            person,
            connection_id.clone(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("other-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        assert_eq!(
            grant.activate_review(active, alternate_owner, scope()),
            Err(GrantTransitionError::Identity)
        );
        let alternate_person = GrantSourceBinding::try_new(
            PersonId::new(),
            connection_id,
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("mac-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        assert_eq!(
            grant.activate_review(active, alternate_person, scope()),
            Err(GrantTransitionError::Identity)
        );
        assert!(grant.revoke(active).unwrap());
        assert_eq!(
            grant.activate_review(grant.authority(), grant.source().clone(), scope()),
            Err(GrantTransitionError::Terminal)
        );
    }

    #[test]
    fn grant_authority_rejects_nil_and_never_wraps() {
        assert!(GrantAuthority::from_parts(Uuid::nil(), NonZeroU64::MIN).is_none());
        let exhausted = GrantAuthority::from_parts(Uuid::new_v4(), NonZeroU64::MAX).unwrap();
        assert!(exhausted.advance().is_none());
    }

    #[test]
    fn deserialized_nested_values_are_revalidated_before_transitions() {
        let person = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let mut source_value = serde_json::to_value(&source).unwrap();
        source_value["connector"] = serde_json::json!("");
        let decoded_source: GrantSourceBinding = serde_json::from_value(source_value).unwrap();
        assert!(decoded_source.validate().is_err());
        let mut owner_value = serde_json::to_value(&source).unwrap();
        owner_value["execution_owner"] = serde_json::json!("*");
        let decoded_owner: GrantSourceBinding = serde_json::from_value(owner_value).unwrap();
        assert!(decoded_owner.validate().is_err());

        let mut scope_value = serde_json::to_value(scope()).unwrap();
        scope_value["resources"] = serde_json::json!(["*"]);
        scope_value["processing"] = serde_json::json!({
            "approved_recipient": {"recipient": "*", "categories": ["metadata"]}
        });
        let decoded_scope: GrantScope = serde_json::from_value(scope_value).unwrap();
        assert!(decoded_scope.validate().is_err());
        let mut recipient_value = serde_json::to_value(scope()).unwrap();
        recipient_value["processing"] = serde_json::json!({
            "approved_recipient": {"recipient": "", "categories": ["metadata"]}
        });
        let decoded_recipient: GrantScope = serde_json::from_value(recipient_value).unwrap();
        assert!(decoded_recipient.validate().is_err());

        let grant = DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source, scope()).unwrap();
        let mut grant_value = serde_json::to_value(&grant).unwrap();
        grant_value["authority"]["incarnation"] = serde_json::json!(Uuid::nil());
        let mut decoded_grant: DataAccessGrant = serde_json::from_value(grant_value).unwrap();
        assert_eq!(
            decoded_grant.pause(decoded_grant.authority()),
            Err(GrantTransitionError::Invalid(
                GrantValidationError::Identity
            ))
        );
        let valid_grant = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            decoded_grant.source().clone(),
            scope(),
        )
        .unwrap();
        let mut inconsistent_value = serde_json::to_value(&valid_grant).unwrap();
        inconsistent_value["state"] = serde_json::json!("active");
        inconsistent_value["review_required"] = serde_json::json!(true);
        let mut inconsistent_grant: DataAccessGrant =
            serde_json::from_value(inconsistent_value).unwrap();
        assert_eq!(
            inconsistent_grant.validate(),
            Err(GrantValidationError::InvalidState)
        );
        assert_eq!(
            inconsistent_grant.pause(inconsistent_grant.authority()),
            Err(GrantTransitionError::Invalid(
                GrantValidationError::InvalidState
            ))
        );
        assert!(serde_json::from_str::<GrantScope>(r#"{"resources":[],"unknown":true}"#).is_err());
        assert!(
            serde_json::from_str::<GrantAuthority>(&format!(
                r#"{{"incarnation":"{}","access_epoch":0}}"#,
                Uuid::new_v4()
            ))
            .is_err()
        );
    }
}
