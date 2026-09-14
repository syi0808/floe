use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

pub const MAX_LIVE_LEASES: usize = 64;
pub const MAX_LEASE_BYTES: usize = 4 * 1024 * 1024;

struct PersonLeaseUsage {
    leases: usize,
    bytes: usize,
}

struct LeaseReservationDrop {
    registry: Arc<SourceLeaseRegistry>,
    person_id: PersonId,
    bytes: usize,
}

impl Drop for LeaseReservationDrop {
    fn drop(&mut self) {
        if let Ok(mut usage) = self.registry.usage.lock()
            && let Some(person_usage) = usage.get_mut(&self.person_id)
        {
            person_usage.leases = person_usage.leases.saturating_sub(1);
            person_usage.bytes = person_usage.bytes.saturating_sub(self.bytes);
            if person_usage.leases == 0 {
                usage.remove(&self.person_id);
            }
        }
    }
}

pub struct SourceLeaseReservation {
    _drop: Arc<LeaseReservationDrop>,
}

impl SourceLeaseReservation {
    pub(crate) fn validate_binding(
        &self,
        person_id: PersonId,
        process_incarnation_id: Uuid,
    ) -> Result<(), AgentFailure> {
        if self._drop.person_id != person_id
            || self._drop.registry.process_incarnation() != process_incarnation_id
        {
            return Err(AgentFailure::StaleContext);
        }
        Ok(())
    }

    pub(crate) fn byte_allowance(&self) -> usize {
        self._drop.bytes
    }
}

pub struct SourceLeaseRegistry {
    process_incarnation: Uuid,
    usage: Mutex<HashMap<PersonId, PersonLeaseUsage>>,
    evidence: Mutex<HashMap<(PersonId, Uuid), LiveObservationEvidence>>,
}

struct LiveObservationEvidence {
    dependency: ContextDependency,
    subject_fingerprint: String,
    expires_at: Instant,
    bytes: usize,
}

impl SourceLeaseRegistry {
    pub fn new() -> Self {
        Self {
            process_incarnation: Uuid::new_v4(),
            usage: Mutex::new(HashMap::new()),
            evidence: Mutex::new(HashMap::new()),
        }
    }

    pub fn process_incarnation(&self) -> Uuid {
        self.process_incarnation
    }

    pub fn reserve(
        self: &Arc<Self>,
        person_id: PersonId,
        bytes: usize,
    ) -> Result<SourceLeaseReservation, AgentFailure> {
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
        Ok(SourceLeaseReservation {
            _drop: Arc::new(LeaseReservationDrop {
                registry: Arc::clone(self),
                person_id,
                bytes,
            }),
        })
    }

    pub fn retain_observation(
        &self,
        dependency: ContextDependency,
        subject_fingerprint: String,
        expires_at: Instant,
    ) -> Result<(), AgentFailure> {
        if subject_fingerprint.len() != 64
            || subject_fingerprint
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit())
            || expires_at <= Instant::now()
        {
            return Err(AgentFailure::InvalidInput);
        }
        dependency
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if dependency.process_incarnation_id() != self.process_incarnation {
            return Err(AgentFailure::StaleContext);
        }
        let dependency_bytes = serde_json::to_vec(&dependency)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len();
        let bytes = dependency_bytes
            .checked_add(subject_fingerprint.len())
            .ok_or(AgentFailure::BudgetExceeded)?;
        let key = (dependency.person_id(), dependency.observation_id());
        let mut evidence = self
            .evidence
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let now = Instant::now();
        evidence.retain(|_, item| item.expires_at > now);
        if evidence.contains_key(&key) {
            return Err(AgentFailure::Conflict);
        }
        let person_count = evidence
            .values()
            .filter(|item| item.dependency.person_id() == dependency.person_id())
            .count();
        let person_bytes = evidence
            .values()
            .filter(|item| item.dependency.person_id() == dependency.person_id())
            .map(|item| item.bytes)
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
                subject_fingerprint,
                expires_at,
                bytes,
            },
        );
        Ok(())
    }

    pub fn observation(
        &self,
        dependency: &ContextDependency,
    ) -> Result<(ContextDependency, String), AgentFailure> {
        dependency
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if dependency.process_incarnation_id() != self.process_incarnation {
            return Err(AgentFailure::StaleContext);
        }
        let key = (dependency.person_id(), dependency.observation_id());
        let mut evidence = self
            .evidence
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let now = Instant::now();
        evidence.retain(|_, item| item.expires_at > now);
        let item = evidence.get(&key).ok_or(AgentFailure::StaleContext)?;
        if item.dependency != *dependency {
            return Err(AgentFailure::StaleContext);
        }
        Ok((item.dependency.clone(), item.subject_fingerprint.clone()))
    }
}

impl Default for SourceLeaseRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };

    fn dependency(process_incarnation: Uuid) -> ContextDependency {
        dependency_for(process_incarnation, PersonId::new(), Uuid::new_v4())
    }

    fn dependency_for(
        process_incarnation: Uuid,
        person_id: PersonId,
        observation_id: Uuid,
    ) -> ContextDependency {
        let consumer = GrantConsumer::builtin("source.fixture").unwrap();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            GrantSourceBinding::try_new(
                person_id,
                ConnectionId::new(),
                ConnectorId::try_new("fixture.connector").unwrap(),
                ExecutionOwnerId::try_new("fixture-device").unwrap(),
                SourceAuthority::new(),
            )
            .unwrap(),
            vec![ResourceHandle::try_new("source/item").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            consumer,
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            observation_id,
            b"fixture-query".to_vec(),
            Uuid::new_v4(),
            process_incarnation,
            Utc::now(),
            Utc::now() + chrono::Duration::minutes(1),
        )
        .unwrap()
    }

    #[test]
    fn reservation_quota_is_per_person_and_raii_releases() {
        let registry = Arc::new(SourceLeaseRegistry::new());
        let person = PersonId::new();
        let reservations = (0..MAX_LIVE_LEASES)
            .map(|_| registry.reserve(person, 1).unwrap())
            .collect::<Vec<_>>();
        assert!(matches!(
            registry.reserve(person, 1),
            Err(AgentFailure::BudgetExceeded)
        ));
        drop(reservations);
        assert!(registry.reserve(person, 1).is_ok());
    }

    #[test]
    fn reservation_bytes_are_isolated_per_person() {
        let registry = Arc::new(SourceLeaseRegistry::new());
        let first_person = PersonId::new();
        let second_person = PersonId::new();
        let first = registry.reserve(first_person, MAX_LEASE_BYTES).unwrap();
        assert!(matches!(
            registry.reserve(first_person, 1),
            Err(AgentFailure::BudgetExceeded)
        ));
        assert!(registry.reserve(second_person, MAX_LEASE_BYTES).is_ok());
        drop(first);
        assert!(registry.reserve(first_person, 1).is_ok());
    }

    #[test]
    fn retained_evidence_uses_stored_bytes_and_isolates_people() {
        let registry = SourceLeaseRegistry::new();
        let first_person = PersonId::new();
        let second_person = PersonId::new();
        let first_observation = Uuid::new_v4();
        let first = dependency_for(
            registry.process_incarnation(),
            first_person,
            first_observation,
        );
        let expected_bytes = serde_json::to_vec(&first).unwrap().len() + 64;
        registry
            .retain_observation(
                first,
                "a".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        let first_key = (first_person, first_observation);
        assert_eq!(
            registry
                .evidence
                .lock()
                .unwrap()
                .get(&first_key)
                .unwrap()
                .bytes,
            expected_bytes
        );
        registry
            .evidence
            .lock()
            .unwrap()
            .get_mut(&first_key)
            .unwrap()
            .bytes = MAX_LEASE_BYTES - 1;

        let second = dependency_for(registry.process_incarnation(), first_person, Uuid::new_v4());
        assert!(matches!(
            registry.retain_observation(
                second.clone(),
                "b".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::BudgetExceeded)
        ));
        let other = dependency_for(
            registry.process_incarnation(),
            second_person,
            Uuid::new_v4(),
        );
        registry
            .retain_observation(
                other,
                "c".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();

        registry
            .evidence
            .lock()
            .unwrap()
            .get_mut(&first_key)
            .unwrap()
            .expires_at = Instant::now() - std::time::Duration::from_secs(1);
        registry
            .retain_observation(
                second,
                "b".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
    }

    #[test]
    fn retained_evidence_count_is_per_person_and_expiry_frees_capacity() {
        let registry = SourceLeaseRegistry::new();
        let first_person = PersonId::new();
        let second_person = PersonId::new();
        let first_observation = Uuid::new_v4();
        for observation_id in
            std::iter::once(first_observation).chain((1..MAX_LIVE_LEASES).map(|_| Uuid::new_v4()))
        {
            registry
                .retain_observation(
                    dependency_for(registry.process_incarnation(), first_person, observation_id),
                    "d".repeat(64),
                    Instant::now() + std::time::Duration::from_secs(5),
                )
                .unwrap();
        }
        assert!(matches!(
            registry.retain_observation(
                dependency_for(registry.process_incarnation(), first_person, Uuid::new_v4()),
                "e".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::BudgetExceeded)
        ));
        registry
            .retain_observation(
                dependency_for(
                    registry.process_incarnation(),
                    second_person,
                    Uuid::new_v4(),
                ),
                "f".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        registry
            .evidence
            .lock()
            .unwrap()
            .get_mut(&(first_person, first_observation))
            .unwrap()
            .expires_at = Instant::now() - std::time::Duration::from_secs(1);
        registry
            .retain_observation(
                dependency_for(registry.process_incarnation(), first_person, Uuid::new_v4()),
                "e".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
    }

    #[test]
    fn observation_requires_exact_identity_and_process() {
        let registry = SourceLeaseRegistry::new();
        let dependency = dependency(registry.process_incarnation());
        registry
            .retain_observation(
                dependency.clone(),
                "a".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            registry.observation(&dependency).unwrap(),
            (dependency.clone(), "a".repeat(64))
        );
        assert_eq!(
            registry.retain_observation(
                dependency.clone(),
                "a".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::Conflict)
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
            b"changed-query".to_vec(),
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
        let other_registry = SourceLeaseRegistry::new();
        assert_eq!(
            other_registry.retain_observation(
                dependency,
                "b".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            ),
            Err(AgentFailure::StaleContext)
        );
    }

    #[tokio::test]
    async fn observation_expires_monotonically() {
        let registry = SourceLeaseRegistry::new();
        let dependency = dependency(registry.process_incarnation());
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
        registry
            .retain_observation(
                dependency,
                "b".repeat(64),
                Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
    }
}
