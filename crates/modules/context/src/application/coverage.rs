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

/// The coverage one committed message stands on.
///
/// A message that consumed nothing, from a run where a source was nonetheless
/// observed, cannot be shown to be independent of it; one where nothing was
/// observed at all can.
pub fn message_coverage(
    dependencies: Vec<ContextDependency>,
    source_was_observed: bool,
) -> Result<DependencyCoverage, AgentFailure> {
    if dependencies.is_empty() {
        if source_was_observed {
            return Ok(DependencyCoverage::Unknown);
        }
        let mut accumulator = CoverageAccumulator::new();
        accumulator
            .record_host_independent()
            .map_err(|_| AgentFailure::InvalidInput)?;
        return Ok(accumulator.coverage());
    }
    let mut accumulator = CoverageAccumulator::new();
    for dependency in dependencies {
        accumulator
            .record_host_dependency(dependency)
            .map_err(|_| AgentFailure::InvalidInput)?;
    }
    Ok(accumulator.coverage())
}
