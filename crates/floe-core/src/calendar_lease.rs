use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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

pub(crate) const MAX_LIVE_LEASES: usize = 64;
pub(crate) const MAX_LEASE_BYTES: usize = 4 * 1024 * 1024;

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

struct PersonLeaseUsage {
    leases: usize,
    bytes: usize,
}

struct LeaseReservationDrop {
    registry: Arc<CalendarLeaseRegistry>,
    person_id: PersonId,
    bytes: usize,
}

impl Drop for LeaseReservationDrop {
    fn drop(&mut self) {
        if let Ok(mut usage) = self.registry.usage.lock() {
            if let Some(person_usage) = usage.get_mut(&self.person_id) {
                person_usage.leases = person_usage.leases.saturating_sub(1);
                person_usage.bytes = person_usage.bytes.saturating_sub(self.bytes);
                if person_usage.leases == 0 {
                    usage.remove(&self.person_id);
                }
            }
        }
    }
}

pub(crate) struct LeaseReservation {
    _drop: Arc<LeaseReservationDrop>,
}

pub(crate) struct CalendarLeaseRegistry {
    process_incarnation: Uuid,
    usage: Mutex<HashMap<PersonId, PersonLeaseUsage>>,
    evidence: Mutex<HashMap<(PersonId, Uuid), LiveObservationEvidence>>,
}

struct LiveObservationEvidence {
    dependency: CalendarLeaseDependencies,
    native_subject_fingerprint: String,
    expires_at: Instant,
}

impl CalendarLeaseRegistry {
    pub(crate) fn new() -> Self {
        Self {
            process_incarnation: Uuid::new_v4(),
            usage: Mutex::new(HashMap::new()),
            evidence: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn process_incarnation(&self) -> Uuid {
        self.process_incarnation
    }

    pub(crate) fn reserve(
        self: &Arc<Self>,
        person_id: PersonId,
        bytes: usize,
    ) -> Result<LeaseReservation, AgentFailure> {
        if bytes == 0 || bytes > MAX_LEASE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut usage = self
            .usage
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let person_usage = usage.entry(person_id).or_insert(PersonLeaseUsage {
            leases: 0,
            bytes: 0,
        });
        let Some(next_leases) = person_usage.leases.checked_add(1) else {
            return Err(AgentFailure::BudgetExceeded);
        };
        let Some(next_bytes) = person_usage.bytes.checked_add(bytes) else {
            return Err(AgentFailure::BudgetExceeded);
        };
        if next_leases > MAX_LIVE_LEASES || next_bytes > MAX_LEASE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        person_usage.leases = next_leases;
        person_usage.bytes = next_bytes;
        drop(usage);
        Ok(LeaseReservation {
            _drop: Arc::new(LeaseReservationDrop {
                registry: Arc::clone(self),
                person_id,
                bytes,
            }),
        })
    }

    pub(crate) fn retain_observation(
        &self,
        dependency: CalendarLeaseDependencies,
        native_subject_fingerprint: String,
        expires_at: Instant,
    ) -> Result<(), AgentFailure> {
        if native_subject_fingerprint.len() != 64
            || native_subject_fingerprint
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit())
            || expires_at <= Instant::now()
        {
            return Err(AgentFailure::InvalidInput);
        }
        let key = (dependency.person_id, dependency.observation_id);
        let bytes = serde_json::to_vec(&dependency)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            .checked_add(native_subject_fingerprint.len())
            .ok_or(AgentFailure::BudgetExceeded)?;
        let mut evidence = self
            .evidence
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        evidence.retain(|_, item| item.expires_at > Instant::now());
        if evidence.contains_key(&key) {
            return Err(AgentFailure::Conflict);
        }
        let person_count = evidence
            .values()
            .filter(|item| item.dependency.person_id == dependency.person_id)
            .count();
        let person_bytes = evidence
            .values()
            .filter(|item| item.dependency.person_id == dependency.person_id)
            .filter_map(|item| {
                serde_json::to_vec(&item.dependency)
                    .ok()
                    .map(|payload| payload.len() + item_subject_bytes(item))
            })
            .try_fold(0usize, usize::checked_add)
            .ok_or(AgentFailure::BudgetExceeded)?;
        if person_count >= MAX_LIVE_LEASES
            || person_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_LEASE_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        evidence.insert(
            key,
            LiveObservationEvidence {
                dependency,
                native_subject_fingerprint,
                expires_at,
            },
        );
        Ok(())
    }

    pub(crate) fn observation(
        &self,
        dependency: &ContextDependency,
    ) -> Result<(CalendarLeaseDependencies, String), AgentFailure> {
        let key = (dependency.person_id(), dependency.observation_id());
        let mut evidence = self
            .evidence
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        evidence.retain(|_, item| item.expires_at > Instant::now());
        let item = evidence.get(&key).ok_or(AgentFailure::StaleContext)?;
        if item.dependency.dependency != *dependency {
            return Err(AgentFailure::StaleContext);
        }
        Ok((
            item.dependency.clone(),
            item.native_subject_fingerprint.clone(),
        ))
    }
}

fn item_subject_bytes(item: &LiveObservationEvidence) -> usize {
    item.native_subject_fingerprint.len()
}

pub(crate) struct CalendarLeaseEntry {
    pub(crate) _key: CalendarLeaseKey,
    pub(crate) dependencies: CalendarLeaseDependencies,
    pub(crate) view: floe_agent::ExpertTimelineView,
    pub(crate) expires_at: Instant,
    pub(crate) _reservation: LeaseReservation,
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

    fn sample_dependency() -> CalendarLeaseDependencies {
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
            Uuid::new_v4(),
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
        let registry = CalendarLeaseRegistry::new();
        let dependency = sample_dependency();
        let expiry = Instant::now() + std::time::Duration::from_secs(5);
        registry
            .retain_observation(dependency.clone(), "a".repeat(64), expiry)
            .unwrap();
        assert!(registry.observation(&dependency.dependency).is_ok());
        assert_eq!(
            registry.retain_observation(
                dependency.clone(),
                "a".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            CalendarLeaseRegistry::new().observation(&dependency.dependency),
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
        let registry = CalendarLeaseRegistry::new();
        let dependency = sample_dependency();
        registry
            .retain_observation(
                dependency.clone(),
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
