use chrono::{DateTime, Utc};
use floe_agent::AgentFailure;
use floe_domain::{
    ConsumerPolicyAuthority, ContextDependency, GrantAuthority, GrantConsumer, GrantId,
    GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, PersonId, ProcessingRestriction,
};
use serde::Serialize;
use tokio::time::Instant;
use uuid::Uuid;

use crate::calendar_view::CalendarReadAccessAdmission;

use floe_context::SourceLeaseReservation;
#[cfg(test)]
use floe_context::SourceLeaseRegistry;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct CalendarLeaseKey {
    pub(crate) invocation_id: Uuid,
    pub(crate) person_id: PersonId,
    pub(crate) handle: Uuid,
    pub(crate) device_id: String,
    pub(crate) calendar_ids: Vec<String>,
    pub(crate) range_start_unix_ms: i64,
    pub(crate) range_end_unix_ms: i64,
    pub(crate) timezone_offset_seconds: i32,
    pub(crate) end_timezone_offset_seconds: Option<i32>,
    pub(crate) max_items: usize,
    pub(crate) max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CalendarLeaseDependencies {
    pub invocation_id: Uuid,
    pub process_incarnation: Uuid,
    pub observation_id: Uuid,
    pub person_id: PersonId,
    pub grant_id: GrantId,
    pub grant_authority: GrantAuthority,
    pub source: GrantSourceBinding,
    pub scope: GrantScope,
    pub consumer_policy: ConsumerPolicyAuthority,
    pub operation: GrantOperation,
    pub purpose: GrantPurpose,
    pub consumer: GrantConsumer,
    pub processing: ProcessingRestriction,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub max_items: usize,
    pub max_bytes: usize,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub dependency: ContextDependency,
}

impl CalendarLeaseDependencies {
    pub(crate) fn from_admission(
        invocation_id: Uuid,
        process_incarnation: Uuid,
        observation_id: Uuid,
        admission: &CalendarReadAccessAdmission,
        key: &CalendarLeaseKey,
        observed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, AgentFailure> {
        let query_fingerprint = serde_json::to_vec(key).map_err(|_| AgentFailure::InvalidInput)?;
        let dependency = ContextDependency::try_new(
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
        .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(Self {
            invocation_id,
            process_incarnation,
            observation_id,
            person_id: admission.person_id,
            grant_id: admission.grant_id,
            grant_authority: admission.grant_authority,
            source: admission.source.clone(),
            scope: admission.scope.clone(),
            consumer_policy: admission.consumer_policy,
            operation: admission.operation,
            purpose: admission.purpose,
            consumer: admission.consumer.clone(),
            processing: admission.processing.clone(),
            range_start_unix_ms: key.range_start_unix_ms,
            range_end_unix_ms: key.range_end_unix_ms,
            timezone_offset_seconds: key.timezone_offset_seconds,
            end_timezone_offset_seconds: key.end_timezone_offset_seconds,
            max_items: key.max_items,
            max_bytes: key.max_bytes,
            observed_at,
            expires_at,
            dependency,
        })
    }
}

pub(crate) struct CalendarLeaseEntry {
    pub(crate) _key: CalendarLeaseKey,
    pub(crate) dependencies: CalendarLeaseDependencies,
    pub(crate) view: floe_agent::ExpertTimelineView,
    pub(crate) expires_at: Instant,
    pub(crate) _reservation: SourceLeaseReservation,
}

impl CalendarLeaseEntry {
    pub(crate) fn is_fresh(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_domain::{ConnectorId, GrantDataCategory, ResourceHandle, SourceAuthority};

    fn sample_dependency(process_incarnation: Uuid) -> CalendarLeaseDependencies {
        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            floe_domain::ConnectionId::new(),
            ConnectorId::try_new("calendar.fixture").unwrap(),
            floe_domain::ExecutionOwnerId::try_new("fixture-device").unwrap(),
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
        CalendarLeaseDependencies::from_admission(
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
            .retain_observation(dependency.dependency.clone(), "a".repeat(64), expiry)
            .unwrap();
        assert!(registry.observation(&dependency.dependency).is_ok());
        assert_eq!(
            registry.retain_observation(
                dependency.dependency.clone(),
                "a".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            SourceLeaseRegistry::new().observation(&dependency.dependency),
            Err(AgentFailure::StaleContext)
        );

        let changed = ContextDependency::try_new(
            dependency.dependency.person_id(),
            dependency.dependency.grant_id(),
            dependency.dependency.grant_authority(),
            dependency.dependency.source().clone(),
            dependency.dependency.resources().to_vec(),
            dependency.dependency.categories().to_vec(),
            dependency.dependency.operation(),
            dependency.dependency.purpose(),
            dependency.dependency.consumer().clone(),
            dependency.dependency.processing().clone(),
            dependency.dependency.consumer_policy(),
            dependency.dependency.observation_id(),
            b"different-query".to_vec(),
            dependency.dependency.lease_invocation_id(),
            dependency.dependency.process_incarnation_id(),
            dependency.dependency.observed_at(),
            dependency.dependency.expires_at(),
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
                dependency.dependency.clone(),
                "b".repeat(64),
                Instant::now() + std::time::Duration::from_millis(10),
            )
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            registry.observation(&dependency.dependency),
            Err(AgentFailure::StaleContext)
        );
    }
}
