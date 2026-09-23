use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod assembly;
mod authorized_read;
pub mod calendar;
mod evidence;
mod memory;
mod source_access;
pub mod views;

pub use assembly::{OptionalSource, acquire_optional_source, record_source_issue};
pub use authorized_read::{AuthorizedRead, HeldGrant};
pub use calendar::{CalendarProvider, CalendarReadAccessStamp, CalendarScope};
pub use evidence::{ContextEvidence, MAX_CONTEXT_EVIDENCE, MAX_CONTEXT_EVIDENCE_BYTES};
pub use floe_kernel::PersonId;
pub use memory::{
    ContextMemory, EpistemicStatus, LearningEvidenceRef, MAX_CONTEXT_MEMORIES,
    MAX_CONTEXT_MEMORY_BYTES, MemoryContextSnapshot, PersonalMemoryKind,
};
pub use source_access::{
    SourceAccessRequirement, SourceAccessRequirementKind, SourceReadOutcome, SourceUnavailable,
};
pub use views::*;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAuthority {
    incarnation: Uuid,
    epoch: NonZeroU64,
}

impl SourceAuthority {
    pub fn new() -> Self {
        Self {
            incarnation: Uuid::new_v4(),
            epoch: NonZeroU64::MIN,
        }
    }

    pub fn is_valid(self) -> bool {
        !self.incarnation.is_nil()
    }

    pub fn incarnation(self) -> Uuid {
        self.incarnation
    }

    pub fn epoch(self) -> NonZeroU64 {
        self.epoch
    }

    pub fn from_parts(incarnation: Uuid, epoch: NonZeroU64) -> Option<Self> {
        (!incarnation.is_nil()).then_some(Self { incarnation, epoch })
    }

    pub fn advance(self) -> Option<Self> {
        Some(Self {
            incarnation: self.incarnation,
            epoch: self.epoch.checked_add(1)?,
        })
    }
}

impl Default for SourceAuthority {
    fn default() -> Self {
        Self::new()
    }
}
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
pub const MAX_CONTEXT_DEPENDENCIES: usize = 64;
pub const MAX_CONTEXT_DEPENDENCY_BYTES: usize = 64 * 1024;
pub const MAX_QUERY_FINGERPRINT_BYTES: usize = 4 * 1024;
pub const MAX_DEPENDENCY_LIFETIME: Duration = Duration::hours(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    Memory,
    Tasks,
    Notes,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextIssueReason {
    Unavailable,
    Denied,
    BudgetExceeded,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextIssue {
    pub source: ContextSource,
    pub reason: ContextIssueReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerPolicyAuthority {
    incarnation: Uuid,
    epoch: NonZeroU64,
}

impl ConsumerPolicyAuthority {
    pub fn from_parts(incarnation: Uuid, epoch: NonZeroU64) -> Option<Self> {
        (!incarnation.is_nil()).then_some(Self { incarnation, epoch })
    }

    pub fn new() -> Self {
        Self {
            incarnation: Uuid::new_v4(),
            epoch: NonZeroU64::MIN,
        }
    }

    pub fn incarnation(self) -> Uuid {
        self.incarnation
    }

    pub fn epoch(self) -> NonZeroU64 {
        self.epoch
    }

    pub fn is_valid(self) -> bool {
        !self.incarnation.is_nil()
    }

    pub fn advance(self) -> Option<Self> {
        self.epoch
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(|epoch| Self {
                incarnation: self.incarnation,
                epoch,
            })
    }
}

impl Default for ConsumerPolicyAuthority {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextDependency {
    person_id: PersonId,
    grant_id: GrantId,
    grant_authority: GrantAuthority,
    source: GrantSourceBinding,
    resources: Vec<ResourceHandle>,
    categories: Vec<GrantDataCategory>,
    operation: GrantOperation,
    purpose: GrantPurpose,
    consumer: GrantConsumer,
    processing: ProcessingRestriction,
    consumer_policy: ConsumerPolicyAuthority,
    observation_id: Uuid,
    query_fingerprint: Vec<u8>,
    lease_invocation_id: Uuid,
    process_incarnation_id: Uuid,
    observed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl ContextDependency {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        person_id: PersonId,
        grant_id: GrantId,
        grant_authority: GrantAuthority,
        source: GrantSourceBinding,
        mut resources: Vec<ResourceHandle>,
        mut categories: Vec<GrantDataCategory>,
        operation: GrantOperation,
        purpose: GrantPurpose,
        consumer: GrantConsumer,
        processing: ProcessingRestriction,
        consumer_policy: ConsumerPolicyAuthority,
        observation_id: Uuid,
        query_fingerprint: Vec<u8>,
        lease_invocation_id: Uuid,
        process_incarnation_id: Uuid,
        observed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, ContextDependencyError> {
        if person_id.0.is_nil()
            || !grant_id.is_valid()
            || !grant_authority.is_valid()
            || !consumer_policy.is_valid()
            || observation_id.is_nil()
            || lease_invocation_id.is_nil()
            || process_incarnation_id.is_nil()
        {
            return Err(ContextDependencyError::InvalidIdentity);
        }
        source
            .validate()
            .map_err(|_| ContextDependencyError::InvalidScope)?;
        ConnectionId::try_new(source.connection_id().as_str().to_owned())
            .map_err(|_| ContextDependencyError::InvalidScope)?;
        if source.person_id() != person_id {
            return Err(ContextDependencyError::PersonMismatch);
        }
        if resources.is_empty() || resources.len() > MAX_RESOURCE_HANDLES {
            return Err(ContextDependencyError::InvalidScope);
        }
        if categories.is_empty() {
            return Err(ContextDependencyError::InvalidScope);
        }
        for resource in &resources {
            ResourceHandle::try_new(resource.as_str().to_owned())
                .map_err(|_| ContextDependencyError::InvalidScope)?;
        }
        resources.sort();
        if resources.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContextDependencyError::InvalidScope);
        }
        categories.sort();
        if categories.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContextDependencyError::InvalidScope);
        }
        match &consumer {
            GrantConsumer::Builtin(name) => GrantConsumer::builtin(name.clone()),
            GrantConsumer::Extension(name) => GrantConsumer::extension(name.clone()),
        }
        .map_err(|_| ContextDependencyError::InvalidScope)?;
        if let ProcessingRestriction::ApprovedRecipient {
            categories: allowed,
            ..
        } = &processing
            && (allowed.is_empty()
                || allowed
                    .iter()
                    .any(|category| !categories.contains(category))
                || allowed.windows(2).any(|pair| pair[0] >= pair[1]))
        {
            return Err(ContextDependencyError::InvalidScope);
        }
        if let ProcessingRestriction::ApprovedRecipient { recipient, .. } = &processing {
            GrantConsumer::builtin(recipient.clone())
                .map_err(|_| ContextDependencyError::InvalidScope)?;
        }
        if !expires_at.gt(&observed_at)
            || expires_at.signed_duration_since(observed_at) > MAX_DEPENDENCY_LIFETIME
        {
            return Err(ContextDependencyError::Expiry);
        }
        if query_fingerprint.is_empty() || query_fingerprint.len() > MAX_QUERY_FINGERPRINT_BYTES {
            return Err(ContextDependencyError::Fingerprint);
        }
        let dependency = Self {
            person_id,
            grant_id,
            grant_authority,
            source,
            resources,
            categories,
            operation,
            purpose,
            consumer,
            processing,
            consumer_policy,
            observation_id,
            query_fingerprint,
            lease_invocation_id,
            process_incarnation_id,
            observed_at,
            expires_at,
        };
        dependency.validate_serialized_size()?;
        Ok(dependency)
    }

    pub fn validate(&self) -> Result<(), ContextDependencyError> {
        let rebuilt = Self::try_new(
            self.person_id,
            self.grant_id,
            self.grant_authority,
            self.source.clone(),
            self.resources.clone(),
            self.categories.clone(),
            self.operation,
            self.purpose,
            self.consumer.clone(),
            self.processing.clone(),
            self.consumer_policy,
            self.observation_id,
            self.query_fingerprint.clone(),
            self.lease_invocation_id,
            self.process_incarnation_id,
            self.observed_at,
            self.expires_at,
        )?;
        (rebuilt == *self)
            .then_some(())
            .ok_or(ContextDependencyError::NonCanonical)
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }
    pub fn grant_id(&self) -> GrantId {
        self.grant_id
    }
    pub fn grant_authority(&self) -> GrantAuthority {
        self.grant_authority
    }
    pub fn source(&self) -> &GrantSourceBinding {
        &self.source
    }
    pub fn resources(&self) -> &[ResourceHandle] {
        &self.resources
    }
    pub fn categories(&self) -> &[GrantDataCategory] {
        &self.categories
    }
    pub fn operation(&self) -> GrantOperation {
        self.operation
    }
    pub fn purpose(&self) -> GrantPurpose {
        self.purpose
    }
    pub fn consumer(&self) -> &GrantConsumer {
        &self.consumer
    }
    pub fn processing(&self) -> &ProcessingRestriction {
        &self.processing
    }
    pub fn consumer_policy(&self) -> ConsumerPolicyAuthority {
        self.consumer_policy
    }
    pub fn observation_id(&self) -> Uuid {
        self.observation_id
    }
    pub fn query_fingerprint(&self) -> &[u8] {
        &self.query_fingerprint
    }
    pub fn lease_invocation_id(&self) -> Uuid {
        self.lease_invocation_id
    }
    pub fn process_incarnation_id(&self) -> Uuid {
        self.process_incarnation_id
    }
    pub fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    fn validate_serialized_size(&self) -> Result<(), ContextDependencyError> {
        let bytes = serde_json::to_vec(self).map_err(|_| ContextDependencyError::Corrupt)?;
        (bytes.len() <= MAX_CONTEXT_DEPENDENCY_BYTES)
            .then_some(())
            .ok_or(ContextDependencyError::TooLarge)
    }

    fn identity(&self) -> Result<Vec<u8>, ContextDependencyError> {
        serde_json::to_vec(&(
            self.observation_id,
            self.grant_id,
            self.grant_authority,
            self.source.source_authority(),
        ))
        .map_err(|_| ContextDependencyError::Corrupt)
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, ContextDependencyError> {
        serde_json::to_vec(self).map_err(|_| ContextDependencyError::Corrupt)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DependencyCoverage {
    #[default]
    Unknown,
    Independent,
    Dependent {
        dependencies: Vec<ContextDependency>,
    },
}

impl DependencyCoverage {
    pub fn dependent(dependency: ContextDependency) -> Result<Self, ContextDependencyError> {
        let coverage = Self::Dependent {
            dependencies: vec![dependency],
        };
        coverage.validate()?;
        Ok(coverage)
    }

    pub fn validate(&self) -> Result<(), ContextDependencyError> {
        match self {
            Self::Unknown | Self::Independent => Ok(()),
            Self::Dependent { dependencies } => {
                if dependencies.is_empty() || dependencies.len() > MAX_CONTEXT_DEPENDENCIES {
                    return Err(ContextDependencyError::DependencyCount);
                }
                let mut identities = BTreeMap::new();
                let mut previous = None;
                let mut total = 0usize;
                for dependency in dependencies {
                    dependency.validate()?;
                    let bytes = dependency.canonical_bytes()?;
                    total = total
                        .checked_add(bytes.len())
                        .ok_or(ContextDependencyError::TooLarge)?;
                    if total > MAX_CONTEXT_DEPENDENCY_BYTES {
                        return Err(ContextDependencyError::TooLarge);
                    }
                    if previous.is_some_and(|item: Vec<u8>| item >= bytes) {
                        return Err(ContextDependencyError::NonCanonical);
                    }
                    previous = Some(bytes.clone());
                    if identities.insert(dependency.identity()?, bytes).is_some() {
                        return Err(ContextDependencyError::Conflict);
                    }
                }
                Ok(())
            }
        }
    }

    pub fn merge(&self, incoming: &Self) -> Result<Self, ContextDependencyError> {
        self.validate()?;
        incoming.validate()?;
        match (self, incoming) {
            (Self::Unknown, _) | (_, Self::Unknown) => Ok(Self::Unknown),
            (Self::Independent, Self::Independent) => Ok(Self::Independent),
            (Self::Independent, Self::Dependent { dependencies })
            | (Self::Dependent { dependencies }, Self::Independent) => {
                let mut all = dependencies.clone();
                all.sort_by_cached_key(|dependency| {
                    dependency.canonical_bytes().unwrap_or_default()
                });
                Self::Dependent {
                    dependencies: all.clone(),
                }
                .validate()?;
                Ok(Self::Dependent { dependencies: all })
            }
            (
                Self::Dependent { dependencies: left },
                Self::Dependent {
                    dependencies: right,
                },
            ) => {
                let mut by_identity = BTreeMap::new();
                for dependency in left.iter().chain(right) {
                    let bytes = dependency.canonical_bytes()?;
                    if let Some(existing) = by_identity
                        .insert(dependency.identity()?, (bytes.clone(), dependency.clone()))
                        && existing.0 != bytes
                    {
                        return Err(ContextDependencyError::Conflict);
                    }
                }
                let mut dependencies = by_identity
                    .into_values()
                    .map(|(_, dependency)| dependency)
                    .collect::<Vec<_>>();
                dependencies.sort_by_cached_key(|dependency| {
                    dependency.canonical_bytes().unwrap_or_default()
                });
                let merged = Self::Dependent { dependencies };
                merged.validate()?;
                Ok(merged)
            }
        }
    }

    pub fn as_persisted_bytes(&self) -> Result<Vec<u8>, ContextDependencyError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ContextDependencyError::Corrupt)?;
        (bytes.len() <= MAX_CONTEXT_DEPENDENCY_BYTES)
            .then_some(bytes)
            .ok_or(ContextDependencyError::TooLarge)
    }

    pub fn from_persisted_bytes(bytes: &[u8]) -> Result<Self, ContextDependencyError> {
        if bytes.is_empty() || bytes.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
            return Err(ContextDependencyError::Corrupt);
        }
        let coverage: Self =
            serde_json::from_slice(bytes).map_err(|_| ContextDependencyError::Corrupt)?;
        coverage
            .validate()
            .map_err(|_| ContextDependencyError::Corrupt)?;
        Ok(coverage)
    }
}
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ContextDependencyError {
    #[error("invalid dependency identity")]
    InvalidIdentity,
    #[error("dependency person mismatch")]
    PersonMismatch,
    #[error("invalid dependency scope")]
    InvalidScope,
    #[error("dependency expiry is invalid")]
    Expiry,
    #[error("dependency fingerprint is invalid")]
    Fingerprint,
    #[error("dependency is too large")]
    TooLarge,
    #[error("dependency count is invalid")]
    DependencyCount,
    #[error("dependency is not canonical")]
    NonCanonical,
    #[error("dependency identity has conflicting payload")]
    Conflict,
    #[error("dependency payload is corrupt")]
    Corrupt,
    #[error("dependency has expired")]
    Expired,
    #[error("dependency replay is unauthorized")]
    Unauthorized,
}

pub fn validate_stored_dependency(
    dependency: &ContextDependency,
) -> Result<(), ContextDependencyError> {
    dependency.validate()
}

pub fn validate_dependency_freshness(
    dependency: &ContextDependency,
    now: DateTime<Utc>,
) -> Result<(), ContextDependencyError> {
    validate_stored_dependency(dependency)?;
    if now < dependency.observed_at || now >= dependency.expires_at {
        return Err(ContextDependencyError::Expired);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    Synthetic,
    Personal,
    TemporaryAiContext,
    HighlySensitive,
    DeviceOnlyRaw,
    Credential,
}

/// Whether a consumer may read one source right now, and what stands in the
/// way when it may not.
///
/// A source nobody bound, a source whose connection is down and a source this
/// consumer was never granted are different answers. Only the consumer that
/// asked can decide what each one means for its own judgment: an optional
/// enrichment may be skipped, a source the judgment depends on may not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceGrant {
    Granted,
    /// Nothing in the current setup binds this source for this consumer.
    NotConfigured,
    /// The source is bound but disabled or unreachable right now.
    Unavailable,
    /// The source is bound and available, but this consumer holds no grant.
    Denied,
}

impl SourceGrant {
    pub fn is_granted(self) -> bool {
        matches!(self, Self::Granted)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPlacement {
    DeviceLocal,
    Remote,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferConsent {
    NotGranted,
    Granted,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            PersonId::new(),
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("device").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap()
    }

    fn admit_dependency(
        source: GrantSourceBinding,
        processing: ProcessingRestriction,
    ) -> Result<ContextDependency, ContextDependencyError> {
        let observed_at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        ContextDependency::try_new(
            source.person_id(),
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("calendar").unwrap()],
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("manager").unwrap(),
            processing,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"synthetic-query".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            observed_at,
            observed_at + Duration::minutes(5),
        )
    }

    #[test]
    fn dependency_constructor_revalidates_deserialized_connection_identity() {
        let source = source();
        assert!(admit_dependency(source.clone(), ProcessingRestriction::LocalOnly).is_ok());
        let mut encoded = serde_json::to_value(source).unwrap();
        encoded["connection_id"] = serde_json::json!("");
        let invalid_source = serde_json::from_value(encoded).unwrap();
        assert_eq!(
            admit_dependency(invalid_source, ProcessingRestriction::LocalOnly),
            Err(ContextDependencyError::InvalidScope)
        );
    }

    #[test]
    fn dependency_constructor_rejects_noncanonical_recipient_categories() {
        let source = source();
        let processing = ProcessingRestriction::ApprovedRecipient {
            recipient: "model".into(),
            categories: vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        };
        assert!(admit_dependency(source.clone(), processing).is_ok());
        let reversed = ProcessingRestriction::ApprovedRecipient {
            recipient: "model".into(),
            categories: vec![GrantDataCategory::Content, GrantDataCategory::Metadata],
        };
        assert_eq!(
            admit_dependency(source, reversed),
            Err(ContextDependencyError::InvalidScope)
        );
    }

    #[test]
    fn authority_changes_without_reusing_an_incarnation() {
        let authority = SourceAuthority::new();
        let advanced = authority.advance().unwrap();
        assert!(authority.is_valid());
        assert_eq!(authority.incarnation(), advanced.incarnation());
        assert_eq!(advanced.epoch().get(), 2);
        assert_ne!(
            SourceAuthority::new().incarnation(),
            authority.incarnation()
        );
    }

    #[test]
    fn exhaustion_never_wraps() {
        let exhausted = SourceAuthority::from_parts(Uuid::new_v4(), NonZeroU64::MAX).unwrap();
        assert_eq!(exhausted.advance(), None);
        assert!(SourceAuthority::from_parts(Uuid::nil(), exhausted.epoch()).is_none());
    }
}
