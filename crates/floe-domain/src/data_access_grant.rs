use std::{collections::BTreeSet, num::NonZeroU64};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{PersonId, SourceAuthority};

pub const MAX_RESOURCE_HANDLES: usize = 128;
pub const MAX_RESOURCE_HANDLE_BYTES: usize = 256;
pub const MAX_CONNECTOR_ID_BYTES: usize = 128;
pub const MAX_EXECUTION_OWNER_BYTES: usize = 256;
pub const MAX_CONSUMERS: usize = 32;
pub const MAX_CONSUMER_ID_BYTES: usize = 128;
pub const MAX_SCOPE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct GrantId(Uuid);

impl GrantId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn from_uuid(value: Uuid) -> Option<Self> {
        (!value.is_nil()).then_some(Self(value))
    }
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn is_valid(self) -> bool {
        !self.0.is_nil()
    }
}

impl Default for GrantId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ConnectionId(String);

impl ConnectionId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, GrantValidationError> {
        let value = value.into();
        validate_identifier(&value, 256, "connection")?;
        Ok(Self(value))
    }
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn from_uuid(value: Uuid) -> Option<Self> {
        (!value.is_nil()).then(|| Self(value.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ConnectorId(String);

impl ConnectorId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, GrantValidationError> {
        let value = value.into();
        validate_identifier(&value, MAX_CONNECTOR_ID_BYTES, "connector")?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ExecutionOwnerId(String);

impl ExecutionOwnerId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, GrantValidationError> {
        let value = value.into();
        validate_identifier(&value, MAX_EXECUTION_OWNER_BYTES, "execution owner")?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantOperation {
    Read,
    Suggestion,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantPurpose {
    Assistant,
    Scheduling,
    Summarization,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantDataCategory {
    Metadata,
    Content,
    Derived,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub enum GrantConsumer {
    #[serde(rename = "builtin")]
    Builtin(String),
    #[serde(rename = "extension")]
    Extension(String),
}

impl GrantConsumer {
    pub fn builtin(value: impl Into<String>) -> Result<Self, GrantValidationError> {
        Self::new(Self::Builtin(value.into()))
    }
    pub fn extension(value: impl Into<String>) -> Result<Self, GrantValidationError> {
        Self::new(Self::Extension(value.into()))
    }
    fn new(value: Self) -> Result<Self, GrantValidationError> {
        let name = match &value {
            Self::Builtin(name) | Self::Extension(name) => name,
        };
        validate_identifier(name, MAX_CONSUMER_ID_BYTES, "consumer")?;
        Ok(value)
    }
    pub fn identifier(&self) -> &str {
        match self {
            Self::Builtin(value) | Self::Extension(value) => value,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ProcessingRestriction {
    #[serde(rename = "local_only")]
    LocalOnly,
    #[serde(rename = "approved_recipient")]
    ApprovedRecipient {
        recipient: String,
        categories: Vec<GrantDataCategory>,
    },
}

impl ProcessingRestriction {
    pub fn approved_recipient(
        value: impl Into<String>,
        categories: Vec<GrantDataCategory>,
    ) -> Result<Self, GrantValidationError> {
        let value = value.into();
        validate_identifier(&value, MAX_CONSUMER_ID_BYTES, "recipient")?;
        if categories.is_empty() {
            return Err(GrantValidationError::MissingScope);
        }
        ensure_unique(&categories, "category")?;
        Ok(Self::ApprovedRecipient {
            recipient: value,
            categories,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ResourceHandle(String);

impl ResourceHandle {
    pub fn try_new(value: impl Into<String>) -> Result<Self, GrantValidationError> {
        let value = value.into();
        if value == "*" {
            return Err(GrantValidationError::Wildcard("resource"));
        }
        validate_identifier(&value, MAX_RESOURCE_HANDLE_BYTES, "resource")?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantScope {
    resources: Vec<ResourceHandle>,
    categories: Vec<GrantDataCategory>,
    operations: Vec<GrantOperation>,
    purposes: Vec<GrantPurpose>,
    consumers: Vec<GrantConsumer>,
    processing: ProcessingRestriction,
}

impl GrantScope {
    pub fn try_new(
        resources: Vec<ResourceHandle>,
        categories: Vec<GrantDataCategory>,
        operations: Vec<GrantOperation>,
        purposes: Vec<GrantPurpose>,
        consumers: Vec<GrantConsumer>,
        processing: ProcessingRestriction,
    ) -> Result<Self, GrantValidationError> {
        if resources.is_empty() || resources.len() > MAX_RESOURCE_HANDLES {
            return Err(GrantValidationError::ResourceCount);
        }
        if categories.is_empty()
            || operations.is_empty()
            || purposes.is_empty()
            || consumers.is_empty()
            || consumers.len() > MAX_CONSUMERS
        {
            return Err(GrantValidationError::MissingScope);
        }
        ensure_unique(&resources, "resource")?;
        ensure_unique(&categories, "category")?;
        ensure_unique(&operations, "operation")?;
        ensure_unique(&purposes, "purpose")?;
        ensure_unique(&consumers, "consumer")?;
        for resource in &resources {
            ResourceHandle::try_new(resource.0.clone())?;
        }
        for consumer in &consumers {
            GrantConsumer::new(consumer.clone())?;
        }
        let processing = canonical_processing(processing, &categories)?;
        let mut resources = resources;
        let mut categories = categories;
        let mut operations = operations;
        let mut purposes = purposes;
        let mut consumers = consumers;
        resources.sort();
        categories.sort();
        operations.sort();
        purposes.sort();
        consumers.sort();
        let scope = Self {
            resources,
            categories,
            operations,
            purposes,
            consumers,
            processing,
        };
        let encoded_size = serde_json::to_vec(&scope)
            .map_err(|_| GrantValidationError::TooLarge("scope"))?
            .len();
        if encoded_size > MAX_SCOPE_BYTES {
            return Err(GrantValidationError::TooLarge("scope"));
        }
        Ok(scope)
    }
    pub fn resources(&self) -> &[ResourceHandle] {
        &self.resources
    }
    pub fn categories(&self) -> &[GrantDataCategory] {
        &self.categories
    }
    pub fn operations(&self) -> &[GrantOperation] {
        &self.operations
    }
    pub fn purposes(&self) -> &[GrantPurpose] {
        &self.purposes
    }
    pub fn consumers(&self) -> &[GrantConsumer] {
        &self.consumers
    }
    pub fn processing(&self) -> &ProcessingRestriction {
        &self.processing
    }
    pub fn validate(&self) -> Result<(), GrantValidationError> {
        let canonical = Self::try_new(
            self.resources.clone(),
            self.categories.clone(),
            self.operations.clone(),
            self.purposes.clone(),
            self.consumers.clone(),
            self.processing.clone(),
        )?;
        (canonical == *self)
            .then_some(())
            .ok_or(GrantValidationError::InvalidIdentifier(
                "non-canonical scope",
            ))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantSourceBinding {
    person_id: PersonId,
    connection_id: ConnectionId,
    connector: ConnectorId,
    execution_owner: ExecutionOwnerId,
    source_authority: SourceAuthority,
}

impl GrantSourceBinding {
    pub fn try_new(
        person_id: PersonId,
        connection_id: ConnectionId,
        connector: ConnectorId,
        execution_owner: ExecutionOwnerId,
        source_authority: SourceAuthority,
    ) -> Result<Self, GrantValidationError> {
        if person_id.0.is_nil() || !source_authority.is_valid() {
            return Err(GrantValidationError::Identity);
        }
        ConnectorId::try_new(connector.0.clone())?;
        ExecutionOwnerId::try_new(execution_owner.0.clone())?;
        Ok(Self {
            person_id,
            connection_id: connection_id.clone(),
            connector,
            execution_owner,
            source_authority,
        })
    }
    pub fn person_id(&self) -> PersonId {
        self.person_id
    }
    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id.clone()
    }
    pub fn connector(&self) -> &ConnectorId {
        &self.connector
    }
    pub fn execution_owner(&self) -> &ExecutionOwnerId {
        &self.execution_owner
    }
    pub fn source_authority(&self) -> SourceAuthority {
        self.source_authority
    }
    pub fn same_identity(&self, other: &Self) -> bool {
        self.person_id == other.person_id
            && self.connection_id == other.connection_id
            && self.connector == other.connector
            && self.execution_owner == other.execution_owner
    }
    pub fn validate(&self) -> Result<(), GrantValidationError> {
        Self::try_new(
            self.person_id,
            self.connection_id.clone(),
            self.connector.clone(),
            self.execution_owner.clone(),
            self.source_authority,
        )
        .map(|_| ())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantAuthority {
    incarnation: Uuid,
    access_epoch: NonZeroU64,
}

impl GrantAuthority {
    pub fn new() -> Self {
        Self {
            incarnation: Uuid::new_v4(),
            access_epoch: NonZeroU64::MIN,
        }
    }
    pub fn from_parts(incarnation: Uuid, access_epoch: NonZeroU64) -> Option<Self> {
        (!incarnation.is_nil()).then_some(Self {
            incarnation,
            access_epoch,
        })
    }
    pub fn incarnation(self) -> Uuid {
        self.incarnation
    }
    pub fn access_epoch(self) -> NonZeroU64 {
        self.access_epoch
    }
    pub fn advance(self) -> Option<Self> {
        Some(Self {
            incarnation: self.incarnation,
            access_epoch: self.access_epoch.checked_add(1)?,
        })
    }
    pub fn is_valid(self) -> bool {
        !self.incarnation.is_nil()
    }
}

impl Default for GrantAuthority {
    fn default() -> Self {
        Self::new()
    }
}

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

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GrantValidationError {
    #[error("invalid identity")]
    Identity,
    #[error("blank or invalid {0}")]
    InvalidIdentifier(&'static str),
    #[error("{0} is too large")]
    TooLarge(&'static str),
    #[error("wildcard {0} is unsupported")]
    Wildcard(&'static str),
    #[error("resource count is invalid")]
    ResourceCount,
    #[error("scope member is missing")]
    MissingScope,
    #[error("duplicate {0}")]
    Duplicate(&'static str),
    #[error("invalid grant state")]
    InvalidState,
}

fn validate_identifier(
    value: &str,
    limit: usize,
    name: &'static str,
) -> Result<(), GrantValidationError> {
    if value.trim() != value
        || value.is_empty()
        || value.chars().any(char::is_control)
        || Uuid::parse_str(value).is_ok_and(|identifier| identifier.is_nil())
    {
        return Err(GrantValidationError::InvalidIdentifier(name));
    }
    if value.len() > limit {
        return Err(GrantValidationError::TooLarge(name));
    }
    if value.contains('*') {
        return Err(GrantValidationError::Wildcard(name));
    }
    Ok(())
}

fn canonical_processing(
    processing: ProcessingRestriction,
    scope_categories: &[GrantDataCategory],
) -> Result<ProcessingRestriction, GrantValidationError> {
    match processing {
        ProcessingRestriction::LocalOnly => Ok(ProcessingRestriction::LocalOnly),
        ProcessingRestriction::ApprovedRecipient {
            recipient,
            mut categories,
        } => {
            validate_identifier(&recipient, MAX_CONSUMER_ID_BYTES, "recipient")?;
            if categories.is_empty() {
                return Err(GrantValidationError::MissingScope);
            }
            ensure_unique(&categories, "category")?;
            if !categories
                .iter()
                .all(|category| scope_categories.contains(category))
            {
                return Err(GrantValidationError::MissingScope);
            }
            categories.sort();
            Ok(ProcessingRestriction::ApprovedRecipient {
                recipient,
                categories,
            })
        }
    }
}

fn ensure_unique<T: Ord>(values: &[T], name: &'static str) -> Result<(), GrantValidationError> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .all(|value| seen.insert(value))
        .then_some(())
        .ok_or(GrantValidationError::Duplicate(name))
}

#[cfg(test)]
mod tests {
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
        let exhausted = GrantAuthority {
            incarnation: Uuid::new_v4(),
            access_epoch: NonZeroU64::MAX,
        };
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
