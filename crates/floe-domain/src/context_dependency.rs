use std::{collections::BTreeMap, num::NonZeroU64};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    DataAccessGrant, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation,
    GrantPurpose, GrantSourceBinding, GrantState, PersonId, ProcessingRestriction, ResourceHandle,
};

pub const MAX_CONTEXT_DEPENDENCIES: usize = 64;
pub const MAX_CONTEXT_DEPENDENCY_BYTES: usize = 64 * 1024;
pub const MAX_QUERY_FINGERPRINT_BYTES: usize = 4 * 1024;
pub const MAX_DEPENDENCY_LIFETIME: Duration = Duration::hours(1);

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
        crate::ConnectionId::try_new(source.connection_id().as_str().to_owned())
            .map_err(|_| ContextDependencyError::InvalidScope)?;
        if source.person_id() != person_id {
            return Err(ContextDependencyError::PersonMismatch);
        }
        if resources.is_empty() || resources.len() > crate::MAX_RESOURCE_HANDLES {
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DependencyCoverage {
    Unknown,
    Independent,
    Dependent {
        dependencies: Vec<ContextDependency>,
    },
}

impl Default for DependencyCoverage {
    fn default() -> Self {
        Self::Unknown
    }
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
                    {
                        if existing.0 != bytes {
                            return Err(ContextDependencyError::Conflict);
                        }
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

#[derive(Clone, Debug, Default)]
pub struct CoverageAccumulator {
    coverage: Option<DependencyCoverage>,
}

impl CoverageAccumulator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_stored(coverage: DependencyCoverage) -> Result<Self, ContextDependencyError> {
        coverage.validate()?;
        Ok(Self {
            coverage: Some(coverage),
        })
    }
    pub fn coverage(&self) -> DependencyCoverage {
        self.coverage.clone().unwrap_or(DependencyCoverage::Unknown)
    }
    pub fn record_host_dependency(
        &mut self,
        dependency: ContextDependency,
    ) -> Result<(), ContextDependencyError> {
        let incoming = DependencyCoverage::dependent(dependency)?;
        self.coverage = Some(match self.coverage.take() {
            Some(current) => current.merge(&incoming)?,
            None => incoming,
        });
        Ok(())
    }
    pub fn record_host_independent(&mut self) -> Result<(), ContextDependencyError> {
        self.coverage = Some(match self.coverage.take() {
            Some(current) => current.merge(&DependencyCoverage::Independent)?,
            None => DependencyCoverage::Independent,
        });
        Ok(())
    }
    pub fn mark_unknown(&mut self) {
        self.coverage = Some(DependencyCoverage::Unknown);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayRequest {
    pub now: DateTime<Utc>,
    pub source: crate::SourceAuthority,
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
    if dependency.person_id != dependency.source.person_id()
        || dependency.grant_id != grant.id()
        || dependency.grant_authority != grant.authority()
        || dependency.source != *grant.source()
        || dependency.source.source_authority() != request.source
        || dependency.consumer_policy != request.consumer_policy
        || dependency.operation != request.operation
        || dependency.purpose != request.purpose
        || dependency.consumer != request.consumer
        || dependency.processing != request.processing
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
            .all(|item| dependency.resources.contains(item))
        || !requested_categories
            .iter()
            .all(|item| dependency.categories.contains(item))
        || grant.state() != GrantState::Active
        || grant.review_required()
        || !dependency
            .resources
            .iter()
            .all(|item| grant.scope().resources().contains(item))
        || !dependency
            .categories
            .iter()
            .all(|item| grant.scope().categories().contains(item))
        || !grant.scope().operations().contains(&dependency.operation)
        || !grant.scope().purposes().contains(&dependency.purpose)
        || !grant.scope().consumers().contains(&dependency.consumer)
        || grant.scope().processing() != &dependency.processing
        || !trust.source_is_current(dependency.source())
        || !trust.consumer_policy_is_current(dependency.consumer_policy)
        || !trust.lease_is_current(dependency, &request)
    {
        return Err(ContextDependencyError::Unauthorized);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dependency(observation_id: Uuid, fingerprint: &[u8]) -> ContextDependency {
        let person = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            crate::ConnectionId::try_new("connection").unwrap(),
            crate::ConnectorId::try_new("connector").unwrap(),
            crate::ExecutionOwnerId::try_new("owner").unwrap(),
            crate::SourceAuthority::new(),
        )
        .unwrap();
        ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("calendar/a").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            observation_id,
            fingerprint.to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 5, 0).unwrap(),
        )
        .unwrap()
    }

    struct ReplayTrustFixture {
        source: bool,
        policy: bool,
        lease: bool,
    }

    impl ReplayTrust for ReplayTrustFixture {
        fn source_is_current(&self, _: &GrantSourceBinding) -> bool {
            self.source
        }

        fn consumer_policy_is_current(&self, _: ConsumerPolicyAuthority) -> bool {
            self.policy
        }

        fn lease_is_current(&self, _: &ContextDependency, _: &ReplayRequest) -> bool {
            self.lease
        }
    }

    fn replay_fixture() -> (ContextDependency, DataAccessGrant, ReplayRequest) {
        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            crate::ConnectionId::try_new("connection").unwrap(),
            crate::ConnectorId::try_new("connector").unwrap(),
            crate::ExecutionOwnerId::try_new("owner").unwrap(),
            crate::SourceAuthority::new(),
        )
        .unwrap();
        let scope = crate::GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let mut grant = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            source.clone(),
            scope.clone(),
        )
        .unwrap();
        grant
            .activate_review(grant.authority(), source.clone(), scope)
            .unwrap();
        let policy = ConsumerPolicyAuthority::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let dependency = ContextDependency::try_new(
            person_id,
            grant.id(),
            grant.authority(),
            source.clone(),
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            policy,
            Uuid::new_v4(),
            b"query".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + chrono::Duration::minutes(5),
        )
        .unwrap();
        let request = ReplayRequest {
            now: now + chrono::Duration::minutes(1),
            source: source.source_authority(),
            consumer_policy: policy,
            resources: dependency.resources.clone(),
            categories: dependency.categories.clone(),
            operation: dependency.operation,
            purpose: dependency.purpose,
            consumer: dependency.consumer.clone(),
            processing: dependency.processing.clone(),
        };
        (dependency, grant, request)
    }

    #[test]
    fn missing_is_unknown_and_unknown_dominates() {
        assert_eq!(DependencyCoverage::default(), DependencyCoverage::Unknown);
        assert_eq!(
            DependencyCoverage::Independent
                .merge(&DependencyCoverage::Unknown)
                .unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[test]
    fn fresh_accumulator_establishes_coverage() {
        let mut accumulator = CoverageAccumulator::new();
        assert_eq!(accumulator.coverage(), DependencyCoverage::Unknown);
        accumulator.record_host_independent().unwrap();
        assert_eq!(accumulator.coverage(), DependencyCoverage::Independent);
        let mut dependent = CoverageAccumulator::new();
        dependent
            .record_host_dependency(dependency(Uuid::new_v4(), b"host"))
            .unwrap();
        assert!(matches!(
            dependent.coverage(),
            DependencyCoverage::Dependent { .. }
        ));
        accumulator.mark_unknown();
        accumulator.record_host_independent().unwrap();
        assert_eq!(accumulator.coverage(), DependencyCoverage::Unknown);
    }

    #[test]
    fn dependent_union_is_canonical_and_idempotent() {
        let first = dependency(Uuid::new_v4(), b"one");
        let second = dependency(Uuid::new_v4(), b"two");
        let left = DependencyCoverage::dependent(first.clone()).unwrap();
        let right = DependencyCoverage::dependent(second).unwrap();
        let merged = left.merge(&right).unwrap();
        assert_eq!(merged.merge(&left).unwrap(), merged);
        assert!(merged.as_persisted_bytes().is_ok());
    }

    #[test]
    fn same_identity_with_changed_payload_is_corruption() {
        let id = Uuid::new_v4();
        let left = dependency(id, b"one");
        let mut right = left.clone();
        right.query_fingerprint = b"two".to_vec();
        assert_eq!(
            DependencyCoverage::dependent(left)
                .unwrap()
                .merge(&DependencyCoverage::dependent(right).unwrap()),
            Err(ContextDependencyError::Conflict)
        );
    }

    #[test]
    fn deserialize_rejects_noncanonical_order() {
        let mut first = dependency(Uuid::new_v4(), b"one");
        let mut second = dependency(Uuid::new_v4(), b"two");
        if first.canonical_bytes().unwrap() < second.canonical_bytes().unwrap() {
            std::mem::swap(&mut first, &mut second);
        }
        let coverage = DependencyCoverage::Dependent {
            dependencies: vec![first.clone(), second],
        };
        assert!(coverage.validate().is_err());
        first.query_fingerprint = vec![0; MAX_QUERY_FINGERPRINT_BYTES + 1];
        assert!(
            ContextDependency::try_new(
                first.person_id,
                first.grant_id,
                first.grant_authority,
                first.source,
                first.resources,
                first.categories,
                first.operation,
                first.purpose,
                first.consumer,
                first.processing,
                first.consumer_policy,
                first.observation_id,
                first.query_fingerprint,
                first.lease_invocation_id,
                first.process_incarnation_id,
                first.observed_at,
                first.expires_at,
            )
            .is_err()
        );
    }

    #[test]
    fn replay_admission_requires_current_exact_authorities_and_intent() {
        let (dependency, grant, request) = replay_fixture();
        let trust = ReplayTrustFixture {
            source: true,
            policy: true,
            lease: true,
        };
        assert!(
            admit_replay(&dependency, &grant, request.clone(), &trust).is_ok(),
            "{:?}",
            admit_replay(&dependency, &grant, request.clone(), &trust)
        );
        let mut wrong_consumer = request.clone();
        wrong_consumer.consumer = GrantConsumer::builtin("other").unwrap();
        assert_eq!(
            admit_replay(&dependency, &grant, wrong_consumer, &trust),
            Err(ContextDependencyError::Unauthorized)
        );
        assert_eq!(
            admit_replay(
                &dependency,
                &grant,
                request.clone(),
                &ReplayTrustFixture {
                    source: true,
                    policy: true,
                    lease: false,
                }
            ),
            Err(ContextDependencyError::Unauthorized)
        );
        assert_eq!(
            admit_replay(
                &dependency,
                &grant,
                ReplayRequest {
                    source: crate::SourceAuthority::new(),
                    ..request.clone()
                },
                &trust
            ),
            Err(ContextDependencyError::Unauthorized)
        );
    }

    #[test]
    fn replay_admission_rejects_paused_revoked_and_expired_records() {
        let (dependency, mut grant, request) = replay_fixture();
        let trust = ReplayTrustFixture {
            source: true,
            policy: true,
            lease: true,
        };
        let (_, paused, _) = replay_fixture();
        assert_eq!(
            admit_replay(
                &dependency,
                &DataAccessGrant::new(
                    grant.id(),
                    grant.authority_owner(),
                    paused.source().clone(),
                    paused.scope().clone()
                )
                .unwrap(),
                request.clone(),
                &trust
            ),
            Err(ContextDependencyError::Unauthorized)
        );
        grant.revoke(grant.authority()).unwrap();
        assert_eq!(
            admit_replay(&dependency, &grant, request.clone(), &trust),
            Err(ContextDependencyError::Unauthorized)
        );
        let (dependency, grant, request) = replay_fixture();
        let mut expanded = grant.clone();
        let expanded_scope = crate::GrantScope::try_new(
            vec![
                ResourceHandle::try_new("calendar/main").unwrap(),
                ResourceHandle::try_new("calendar/other").unwrap(),
            ],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        expanded
            .activate_review(
                expanded.authority(),
                expanded.source().clone(),
                expanded_scope,
            )
            .unwrap();
        assert_eq!(
            admit_replay(&dependency, &expanded, request.clone(), &trust),
            Err(ContextDependencyError::Unauthorized)
        );
        let mut expired = dependency.clone();
        expired.observed_at = request.now - chrono::Duration::minutes(10);
        expired.expires_at = request.now - chrono::Duration::minutes(5);
        assert!(validate_stored_dependency(&expired).is_ok());
        assert_eq!(
            validate_dependency_freshness(&expired, request.now),
            Err(ContextDependencyError::Expired)
        );
    }
}
