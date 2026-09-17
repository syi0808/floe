use chrono::{DateTime, Utc};
use floe_context_contract::ContextDependency;
use floe_kernel::AgentFailure;
use floe_kernel::PersonId;
use serde::Serialize;
use uuid::Uuid;

use crate::CalendarReadAccessAdmission;

#[cfg(test)]
use floe_context::SourceLeaseRegistry;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct CalendarLeaseKey {
    pub invocation_id: Uuid,
    pub person_id: PersonId,
    pub handle: Uuid,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub max_items: usize,
    pub max_bytes: usize,
}

pub fn calendar_lease_dependency(
    invocation_id: Uuid,
    process_incarnation: Uuid,
    observation_id: Uuid,
    admission: &CalendarReadAccessAdmission,
    key: &CalendarLeaseKey,
    observed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<ContextDependency, AgentFailure> {
    let query_fingerprint = serde_json::to_vec(key).map_err(|_| AgentFailure::InvalidInput)?;
    ContextDependency::try_new(
        admission.person_id,
        admission.grant_id,
        admission.grant_authority,
        admission.source.clone(),
        admission.scope.resources().to_vec(),
        admission.scope.categories().to_vec(),
        admission.operation,
        admission.purpose,
        admission.consumer.clone(),
        admission.processing.clone(),
        admission.consumer_policy,
        observation_id,
        query_fingerprint,
        invocation_id,
        process_incarnation,
        observed_at,
        expires_at,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_context_contract::{
        ConnectorId, ConsumerPolicyAuthority, GrantAuthority, GrantConsumer, GrantDataCategory,
        GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
        ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use tokio::time::Instant;

    fn sample_dependency(process_incarnation: Uuid) -> ContextDependency {
        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            floe_context_contract::ConnectionId::new(),
            ConnectorId::try_new("calendar.fixture").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("fixture-device").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let consumer = GrantConsumer::builtin("calendar.expert").unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![consumer.clone()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let admission = CalendarReadAccessAdmission::remote(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            scope,
            ConsumerPolicyAuthority::new(),
            consumer,
            ProcessingRestriction::LocalOnly,
        );
        let now = Utc::now();
        let key = CalendarLeaseKey {
            invocation_id: Uuid::new_v4(),
            person_id,
            handle: Uuid::new_v4(),
            device_id: "fixture-device".into(),
            calendar_ids: vec!["calendar/main".into()],
            range_start_unix_ms: now.timestamp_millis(),
            range_end_unix_ms: (now + chrono::Duration::hours(1)).timestamp_millis(),
            timezone_offset_seconds: 0,
            end_timezone_offset_seconds: None,
            max_items: 16,
            max_bytes: 4096,
        };
        calendar_lease_dependency(
            Uuid::new_v4(),
            process_incarnation,
            Uuid::new_v4(),
            &admission,
            &key,
            now,
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }

    #[test]
    fn retained_observation_requires_exact_dependency_identity() {
        let registry = SourceLeaseRegistry::new();
        let dependency = sample_dependency(registry.process_incarnation());
        let expiry = Instant::now() + std::time::Duration::from_secs(5);
        registry
            .retain_observation(dependency.clone(), "a".repeat(64), expiry)
            .unwrap();
        assert!(registry.observation(&dependency).is_ok());
        assert_eq!(
            registry.retain_observation(
                dependency.clone(),
                "a".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            SourceLeaseRegistry::new().observation(&dependency),
            Err(AgentFailure::StaleContext)
        );

        let changed = ContextDependency::try_new(
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
            b"different-query".to_vec(),
            dependency.lease_invocation_id(),
            dependency.process_incarnation_id(),
            dependency.observed_at(),
            dependency.expires_at(),
        )
        .unwrap();
        assert_eq!(
            registry.observation(&changed),
            Err(AgentFailure::StaleContext)
        );
    }

    #[tokio::test]
    async fn retained_observation_expires_without_reopen_repair() {
        let registry = SourceLeaseRegistry::new();
        let dependency = sample_dependency(registry.process_incarnation());
        registry
            .retain_observation(
                dependency.clone(),
                "b".repeat(64),
                Instant::now() + std::time::Duration::from_millis(10),
            )
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            registry.observation(&dependency),
            Err(AgentFailure::StaleContext)
        );
    }
}
