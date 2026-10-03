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
