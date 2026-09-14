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

#[cfg(test)]
mod tests {
    use super::*;
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_kernel::PersonId;
    use uuid::Uuid;

    fn fixture() -> (ContextDependency, GrantScope) {
        let person = PersonId::new();
        let consumer = GrantConsumer::builtin("source-reader").unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("source/item").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![consumer.clone()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let dependency = ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            GrantSourceBinding::try_new(
                person,
                ConnectionId::try_new("connection").unwrap(),
                ConnectorId::try_new("connector").unwrap(),
                ExecutionOwnerId::try_new("device").unwrap(),
                SourceAuthority::new(),
            )
            .unwrap(),
            scope.resources().to_vec(),
            scope.categories().to_vec(),
            GrantOperation::Read,
            GrantPurpose::Assistant,
            consumer,
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"query".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Utc::now(),
            Utc::now() + chrono::Duration::minutes(1),
        )
        .unwrap();
        (dependency, scope)
    }

    #[test]
    fn either_clock_expiry_or_changed_authority_blocks_without_erasing_lineage() {
        let lineage = ConsumedLineage::default();
        let (dependency, scope) = fixture();
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        lineage
            .record(dependency.clone(), scope.clone(), deadline)
            .unwrap();
        assert_eq!(
            lineage.validate(
                dependency.observed_at(),
                Instant::now(),
                |actual, actual_scope| actual == &dependency && actual_scope == &scope
            ),
            Ok(())
        );
        assert_eq!(
            lineage.validate(dependency.observed_at(), deadline, |_, _| true),
            Err(AgentFailure::StaleContext)
        );
        assert_eq!(
            lineage.validate(dependency.expires_at(), Instant::now(), |_, _| true),
            Err(AgentFailure::StaleContext)
        );
        assert_eq!(
            lineage.validate(dependency.observed_at(), Instant::now(), |_, _| false),
            Err(AgentFailure::StaleContext)
        );
        assert_eq!(lineage.dependencies().unwrap(), vec![dependency]);
    }

    #[test]
    fn mismatched_scope_is_rejected_without_recording_lineage() {
        let lineage = ConsumedLineage::default();
        let (dependency, scope) = fixture();
        let different_scope = GrantScope::try_new(
            scope.resources().to_vec(),
            scope.categories().to_vec(),
            scope.operations().to_vec(),
            scope.purposes().to_vec(),
            vec![GrantConsumer::builtin("different-reader").unwrap()],
            scope.processing().clone(),
        )
        .unwrap();
        assert_eq!(
            lineage.record(
                dependency,
                different_scope,
                Instant::now() + std::time::Duration::from_secs(20)
            ),
            Err(AgentFailure::InvalidInput)
        );
        assert!(lineage.dependencies().unwrap().is_empty());
    }

    #[test]
    fn repeated_observation_cannot_extend_the_consumed_deadline() {
        let lineage = ConsumedLineage::default();
        let (dependency, scope) = fixture();
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        lineage
            .record(dependency.clone(), scope.clone(), deadline)
            .unwrap();
        lineage
            .record(dependency.clone(), scope.clone(), deadline)
            .unwrap();
        assert_eq!(
            lineage.record(
                dependency.clone(),
                scope,
                deadline + std::time::Duration::from_secs(20)
            ),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(lineage.dependencies().unwrap(), vec![dependency.clone()]);
        assert_eq!(
            lineage.validate(dependency.observed_at(), deadline, |_, _| true),
            Err(AgentFailure::StaleContext)
        );
        let empty = ConsumedLineage::default();
        assert_eq!(
            empty.validate(Utc::now(), Instant::now(), |_, _| panic!(
                "unused source must not be authorized"
            )),
            Ok(())
        );
    }
}
