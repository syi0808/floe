use std::sync::Mutex;

use chrono::{DateTime, Utc};
use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, GrantScope, validate_stored_dependency};
use tokio::time::Instant;

#[derive(Default)]
pub struct ConsumedLineage {
    entries: Mutex<Vec<ConsumedSource>>,
}

struct ConsumedSource {
    dependency: ContextDependency,
    scope: GrantScope,
    deadline: Instant,
}

impl ConsumedLineage {
    pub fn record(
        &self,
        dependency: ContextDependency,
        scope: GrantScope,
        deadline: Instant,
    ) -> Result<(), AgentFailure> {
        validate_stored_dependency(&dependency).map_err(|_| AgentFailure::InvalidInput)?;
        scope.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if scope.resources() != dependency.resources()
            || scope.categories() != dependency.categories()
            || !scope.operations().contains(&dependency.operation())
            || !scope.purposes().contains(&dependency.purpose())
            || !scope.consumers().contains(dependency.consumer())
            || scope.processing() != dependency.processing()
        {
            return Err(AgentFailure::InvalidInput);
        }
        if deadline <= Instant::now() {
            return Err(AgentFailure::StaleContext);
        }
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if let Some(existing) = entries.iter().find(|entry| {
            entry.dependency.person_id() == dependency.person_id()
                && entry.dependency.observation_id() == dependency.observation_id()
        }) {
            return if existing.dependency == dependency
                && existing.scope == scope
                && existing.deadline == deadline
            {
                Ok(())
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        entries.push(ConsumedSource {
            dependency,
            scope,
            deadline,
        });
        Ok(())
    }

    pub fn dependencies(&self) -> Result<Vec<ContextDependency>, AgentFailure> {
        self.entries
            .lock()
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| entry.dependency.clone())
                    .collect()
            })
            .map_err(|_| AgentFailure::CapabilityUnavailable)
    }

    pub fn validate(
        &self,
        wall_now: DateTime<Utc>,
        monotonic_now: Instant,
        authorized: impl Fn(&ContextDependency, &GrantScope) -> bool,
    ) -> Result<(), AgentFailure> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        for entry in entries.iter() {
            if entry.dependency.expires_at() <= wall_now
                || entry.deadline <= monotonic_now
                || !authorized(&entry.dependency, &entry.scope)
            {
                return Err(AgentFailure::StaleContext);
            }
        }
        Ok(())
    }
}
