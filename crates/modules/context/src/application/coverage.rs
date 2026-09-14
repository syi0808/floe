use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Mutex,
};

use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, ContextDependencyError, DependencyCoverage};
use uuid::Uuid;

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
        self.coverage.clone().unwrap_or_default()
    }

    pub fn record_host_dependency(
        &mut self,
        dependency: ContextDependency,
    ) -> Result<(), ContextDependencyError> {
        let incoming = DependencyCoverage::dependent(dependency)?;
        let merged = match &self.coverage {
            Some(current) => current.merge(&incoming)?,
            None => incoming,
        };
        self.coverage = Some(merged);
        Ok(())
    }

    pub fn record_host_independent(&mut self) -> Result<(), ContextDependencyError> {
        let merged = match &self.coverage {
            Some(current) => current.merge(&DependencyCoverage::Independent)?,
            None => DependencyCoverage::Independent,
        };
        self.coverage = Some(merged);
        Ok(())
    }

    pub fn mark_unknown(&mut self) {
        self.coverage = Some(DependencyCoverage::Unknown);
    }
}

pub struct CoverageRegistry {
    coverage: Mutex<HashMap<Uuid, CoverageAccumulator>>,
    result_coverage: Mutex<HashMap<(Uuid, Uuid), CoverageAccumulator>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageMessageFact {
    pub turn_id: Uuid,
    pub existing_turn: bool,
    pub is_user: bool,
    pub result_id: Option<Uuid>,
}

impl CoverageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_turn(&self, turn_id: Uuid) -> Result<bool, AgentFailure> {
        self.coverage
            .lock()
            .map(|coverage| coverage.contains_key(&turn_id))
            .map_err(|_| AgentFailure::VaultUnavailable)
    }

    pub fn record_dependency(
        &self,
        turn_id: Uuid,
        dependency: ContextDependency,
        stored: Option<DependencyCoverage>,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut coverage = self
            .coverage
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let mut accumulator = match coverage.get(&turn_id) {
            Some(accumulator) => accumulator.clone(),
            None => match stored {
                Some(stored) => CoverageAccumulator::from_stored(stored)
                    .map_err(|_| AgentFailure::VaultUnavailable)?,
                None => CoverageAccumulator::new(),
            },
        };
        accumulator
            .record_host_dependency(dependency)
            .map_err(|_| AgentFailure::InvalidInput)?;
        coverage.insert(turn_id, accumulator);
        Ok(())
    }

    pub fn record_result_dependency(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut result_coverage = self
            .result_coverage
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key = (turn_id, result_id);
        let mut accumulator = result_coverage.get(&key).cloned().unwrap_or_default();
        accumulator
            .record_host_dependency(dependency)
            .map_err(|_| AgentFailure::InvalidInput)?;
        result_coverage.insert(key, accumulator);
        Ok(())
    }

    pub fn record_result_independent(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut result_coverage = self
            .result_coverage
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key = (turn_id, result_id);
        let mut accumulator = result_coverage.get(&key).cloned().unwrap_or_default();
        accumulator
            .record_host_independent()
            .map_err(|_| AgentFailure::InvalidInput)?;
        result_coverage.insert(key, accumulator);
        Ok(())
    }

    pub fn turn_coverage(&self, turn_id: Uuid) -> Result<Option<DependencyCoverage>, AgentFailure> {
        self.coverage
            .lock()
            .map(|coverage| coverage.get(&turn_id).map(CoverageAccumulator::coverage))
            .map_err(|_| AgentFailure::VaultUnavailable)
    }

    pub fn result_coverage(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
    ) -> Result<Option<DependencyCoverage>, AgentFailure> {
        self.result_coverage
            .lock()
            .map(|coverage| {
                coverage
                    .get(&(turn_id, result_id))
                    .map(CoverageAccumulator::coverage)
            })
            .map_err(|_| AgentFailure::VaultUnavailable)
    }

    pub fn fold_messages(
        &self,
        messages: &[CoverageMessageFact],
        initial: &BTreeMap<Uuid, DependencyCoverage>,
    ) -> Result<BTreeMap<Uuid, DependencyCoverage>, AgentFailure> {
        if messages.iter().any(|message| {
            message.turn_id.is_nil() || message.result_id.is_some_and(|id| id.is_nil())
        }) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut coverage = self
            .coverage
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let result_coverage = self
            .result_coverage
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let mut next = coverage.clone();
        let mut fresh_turns = HashSet::new();
        let mut initialized = HashSet::new();
        for message in messages {
            let accumulator = if let Some(accumulator) = next.get(&message.turn_id) {
                accumulator.clone()
            } else if message.existing_turn {
                CoverageAccumulator::from_stored(
                    initial
                        .get(&message.turn_id)
                        .cloned()
                        .ok_or(AgentFailure::VaultUnavailable)?,
                )
                .map_err(|_| AgentFailure::VaultUnavailable)?
            } else {
                fresh_turns.insert(message.turn_id);
                CoverageAccumulator::new()
            };
            next.entry(message.turn_id).or_insert(accumulator);
            let first_message = initialized.insert(message.turn_id);
            if first_message && fresh_turns.contains(&message.turn_id) && message.is_user {
                next.get_mut(&message.turn_id)
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .record_host_independent()
                    .map_err(|_| AgentFailure::InvalidInput)?;
            }
            if let Some(result_id) = message.result_id {
                match result_coverage
                    .get(&(message.turn_id, result_id))
                    .map(CoverageAccumulator::coverage)
                {
                    Some(DependencyCoverage::Dependent { dependencies }) => {
                        let accumulator = next
                            .get_mut(&message.turn_id)
                            .ok_or(AgentFailure::VaultUnavailable)?;
                        for dependency in dependencies {
                            accumulator
                                .record_host_dependency(dependency)
                                .map_err(|_| AgentFailure::InvalidInput)?;
                        }
                    }
                    Some(DependencyCoverage::Independent) => next
                        .get_mut(&message.turn_id)
                        .ok_or(AgentFailure::VaultUnavailable)?
                        .record_host_independent()
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    Some(DependencyCoverage::Unknown) | None => next
                        .get_mut(&message.turn_id)
                        .ok_or(AgentFailure::VaultUnavailable)?
                        .mark_unknown(),
                }
            }
        }
        let snapshot = next
            .iter()
            .map(|(turn_id, accumulator)| (*turn_id, accumulator.coverage()))
            .collect();
        *coverage = next;
        Ok(snapshot)
    }
}

impl Default for CoverageRegistry {
    fn default() -> Self {
        Self {
            coverage: Mutex::new(HashMap::new()),
            result_coverage: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use floe_context_contract::{
        ConsumerPolicyAuthority, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    };
    use floe_kernel::PersonId;

    use super::*;

    fn dependency(observation_id: Uuid, fingerprint: &[u8]) -> ContextDependency {
        let person = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            floe_context_contract::ConnectionId::try_new("connection").unwrap(),
            floe_context_contract::ConnectorId::try_new("connector").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("owner").unwrap(),
            floe_context_contract::SourceAuthority::new(),
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
    fn rejected_merge_preserves_accumulated_coverage() {
        let observation_id = Uuid::new_v4();
        let first = dependency(observation_id, b"one");
        let mut encoded = serde_json::to_value(&first).unwrap();
        encoded["query_fingerprint"] = serde_json::json!([116, 119, 111]);
        let conflicting: ContextDependency = serde_json::from_value(encoded).unwrap();
        let mut accumulator = CoverageAccumulator::new();
        accumulator.record_host_dependency(first).unwrap();
        let before = accumulator.coverage();
        assert_eq!(
            accumulator.record_host_dependency(conflicting),
            Err(ContextDependencyError::Conflict)
        );
        assert!(matches!(
            accumulator.coverage(),
            DependencyCoverage::Dependent { .. }
        ));
        assert_eq!(accumulator.coverage(), before);
    }

    #[test]
    fn registry_rejected_result_merge_does_not_mutate_existing_result() {
        let registry = CoverageRegistry::new();
        let turn_id = Uuid::new_v4();
        let result_id = Uuid::new_v4();
        let observation_id = Uuid::new_v4();
        let first = dependency(observation_id, b"one");
        let mut encoded = serde_json::to_value(&first).unwrap();
        encoded["query_fingerprint"] = serde_json::json!([116, 119, 111]);
        let conflicting: ContextDependency = serde_json::from_value(encoded).unwrap();
        registry
            .record_result_dependency(turn_id, result_id, first)
            .unwrap();
        let before = registry.result_coverage(turn_id, result_id).unwrap();
        assert_eq!(
            registry.record_result_dependency(turn_id, result_id, conflicting),
            Err(AgentFailure::InvalidInput)
        );
        assert!(matches!(
            registry.result_coverage(turn_id, result_id).unwrap(),
            Some(DependencyCoverage::Dependent { .. })
        ));
        assert_eq!(registry.result_coverage(turn_id, result_id).unwrap(), before);
    }

    #[test]
    fn registry_folds_multiple_messages_for_existing_turn() {
        let registry = CoverageRegistry::new();
        let turn_id = Uuid::new_v4();
        let messages = [
            CoverageMessageFact {
                turn_id,
                existing_turn: true,
                is_user: false,
                result_id: None,
            },
            CoverageMessageFact {
                turn_id,
                existing_turn: true,
                is_user: false,
                result_id: None,
            },
        ];
        let mut initial = BTreeMap::new();
        initial.insert(turn_id, DependencyCoverage::Independent);
        assert_eq!(
            registry.fold_messages(&messages, &initial).unwrap(),
            BTreeMap::from([(turn_id, DependencyCoverage::Independent)])
        );
    }

    #[test]
    fn failed_fold_does_not_publish_a_partially_classified_turn() {
        let registry = CoverageRegistry::new();
        let fresh = Uuid::new_v4();
        let messages = [
            CoverageMessageFact {
                turn_id: fresh,
                existing_turn: false,
                is_user: true,
                result_id: None,
            },
            CoverageMessageFact {
                turn_id: Uuid::new_v4(),
                existing_turn: true,
                is_user: false,
                result_id: None,
            },
        ];
        assert_eq!(
            registry.fold_messages(&messages, &BTreeMap::new()),
            Err(AgentFailure::VaultUnavailable)
        );
        assert_eq!(registry.turn_coverage(fresh).unwrap(), None);
        assert_eq!(
            registry.record_result_independent(fresh, Uuid::nil()),
            Err(AgentFailure::InvalidInput)
        );
        let invalid = [CoverageMessageFact {
            turn_id: Uuid::nil(),
            existing_turn: false,
            is_user: true,
            result_id: None,
        }];
        assert_eq!(
            registry.fold_messages(&invalid, &BTreeMap::new()),
            Err(AgentFailure::InvalidInput)
        );
    }
}
