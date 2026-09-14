use floe_context_contract as context;

pub use floe_access::{ReplayRequest, ReplayTrust, admit_replay};

pub use context::{
    ConsumerPolicyAuthority, ContextDependency, ContextDependencyError, DependencyCoverage,
    MAX_CONTEXT_DEPENDENCIES, MAX_CONTEXT_DEPENDENCY_BYTES, MAX_QUERY_FINGERPRINT_BYTES,
    validate_dependency_freshness, validate_stored_dependency,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DataAccessGrant, PersonId};
    use chrono::{TimeZone, Utc};
    use floe_context_contract::{
        GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    };
    use uuid::Uuid;

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
            resources: dependency.resources().to_vec(),
            categories: dependency.categories().to_vec(),
            operation: dependency.operation(),
            purpose: dependency.purpose(),
            consumer: dependency.consumer().clone(),
            processing: dependency.processing().clone(),
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
        let mut encoded = serde_json::to_value(&left).unwrap();
        encoded["query_fingerprint"] = serde_json::json!([116, 119, 111]);
        let right: ContextDependency = serde_json::from_value(encoded).unwrap();
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
        if serde_json::to_vec(&first).unwrap() < serde_json::to_vec(&second).unwrap() {
            std::mem::swap(&mut first, &mut second);
        }
        let coverage = DependencyCoverage::Dependent {
            dependencies: vec![first.clone(), second],
        };
        assert!(coverage.validate().is_err());
        let mut encoded = serde_json::to_value(&first).unwrap();
        encoded["query_fingerprint"] = serde_json::json!(vec![0; MAX_QUERY_FINGERPRINT_BYTES + 1]);
        let oversized: ContextDependency = serde_json::from_value(encoded).unwrap();
        assert!(oversized.validate().is_err());
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
        let expired = ContextDependency::try_new(
            dependency.person_id(),
            dependency.grant_id(),
            dependency.grant_authority(),
            dependency.source().clone(),
            dependency.resources().to_vec(),
            dependency.categories().to_vec(),
            dependency.operation(),
            dependency.purpose(),
            dependency.consumer().clone(),
            dependency.processing().clone(),
            dependency.consumer_policy(),
            dependency.observation_id(),
            dependency.query_fingerprint().to_vec(),
            dependency.lease_invocation_id(),
            dependency.process_incarnation_id(),
            request.now - chrono::Duration::minutes(10),
            request.now - chrono::Duration::minutes(5),
        )
        .unwrap();
        assert!(validate_stored_dependency(&expired).is_ok());
        assert_eq!(
            validate_dependency_freshness(&expired, request.now),
            Err(ContextDependencyError::Expired)
        );
    }
}
