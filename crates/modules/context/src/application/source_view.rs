use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, GrantScope, validate_stored_dependency};
use serde::Serialize;
use tokio::time::Instant;

use super::leases::{MAX_LEASE_BYTES, SourceLeaseReservation};

pub struct SourceView<Payload: Serialize> {
    dependency: ContextDependency,
    scope: GrantScope,
    payload: Payload,
    deadline: Instant,
    _reservation: SourceLeaseReservation,
}

impl<Payload: Serialize> SourceView<Payload> {
    pub fn try_new(
        dependency: ContextDependency,
        scope: GrantScope,
        payload: Payload,
        deadline: Instant,
        reservation: SourceLeaseReservation,
    ) -> Result<Self, AgentFailure> {
        validate_stored_dependency(&dependency).map_err(|_| AgentFailure::InvalidInput)?;
        scope.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if dependency
            .resources()
            .iter()
            .any(|resource| !scope.resources().contains(resource))
            || dependency
                .categories()
                .iter()
                .any(|category| !scope.categories().contains(category))
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
        let payload_bytes = serde_json::to_vec(&payload).map_err(|_| AgentFailure::InvalidInput)?;
        let payload_size = payload_bytes.len();
        if payload_size == 0 || payload_size > MAX_LEASE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        reservation.validate_binding(
            dependency.person_id(),
            dependency.process_incarnation_id(),
            payload_size,
        )?;
        Ok(Self {
            dependency,
            scope,
            payload,
            deadline,
            _reservation: reservation,
        })
    }

    pub fn dependency(&self) -> &ContextDependency {
        &self.dependency
    }

    pub fn scope(&self) -> &GrantScope {
        &self.scope
    }

    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    pub fn is_fresh(&self) -> bool {
        self.deadline > Instant::now()
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
    use std::sync::Arc;
    use uuid::Uuid;

    fn fixture() -> (
        Arc<super::super::leases::SourceLeaseRegistry>,
        ContextDependency,
        GrantScope,
    ) {
        let registry = Arc::new(super::super::leases::SourceLeaseRegistry::new());
        let person = floe_context_contract::PersonId::new();
        let consumer = GrantConsumer::builtin("source.fixture").unwrap();
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
                ConnectionId::new(),
                ConnectorId::try_new("fixture.connector").unwrap(),
                ExecutionOwnerId::try_new("fixture-device").unwrap(),
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
            b"fixture-query".to_vec(),
            Uuid::new_v4(),
            registry.process_incarnation(),
            Utc::now(),
            Utc::now() + chrono::Duration::minutes(1),
        )
        .unwrap();
        (registry, dependency, scope)
    }

    #[test]
    fn accepts_canonical_dependency_and_selected_scope() {
        let (registry, dependency, scope) = fixture();
        let reservation = registry.reserve(dependency.person_id(), 32).unwrap();
        let view = SourceView::try_new(
            dependency.clone(),
            scope.clone(),
            "payload".to_owned(),
            Instant::now() + std::time::Duration::from_secs(5),
            reservation,
        )
        .unwrap();
        assert_eq!(view.dependency(), &dependency);
        assert_eq!(view.scope(), &scope);
        assert_eq!(view.payload(), "payload");
        assert!(view.is_fresh());
    }

    #[test]
    fn rejects_scope_that_does_not_cover_dependency() {
        let (registry, dependency, scope) = fixture();
        let narrower_scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("other/item").unwrap()],
            scope.categories().to_vec(),
            scope.operations().to_vec(),
            scope.purposes().to_vec(),
            scope.consumers().to_vec(),
            scope.processing().clone(),
        )
        .unwrap();
        let reservation = registry.reserve(dependency.person_id(), 32).unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency,
                narrower_scope,
                "payload".to_owned(),
                Instant::now() + std::time::Duration::from_secs(5),
                reservation,
            ),
            Err(AgentFailure::InvalidInput)
        ));
    }

    #[test]
    fn shared_view_keeps_its_reservation_until_the_last_owner_drops() {
        let (registry, dependency, scope) = fixture();
        let person = dependency.person_id();
        let reservation = registry.reserve(person, MAX_LEASE_BYTES).unwrap();
        let view = Arc::new(
            SourceView::try_new(
                dependency,
                scope,
                "payload".to_owned(),
                Instant::now() + std::time::Duration::from_secs(5),
                reservation,
            )
            .unwrap(),
        );
        let shared = Arc::clone(&view);
        drop(view);
        assert!(matches!(
            registry.reserve(person, 1),
            Err(AgentFailure::BudgetExceeded)
        ));
        assert_eq!(shared.payload(), "payload");
        drop(shared);
        assert!(registry.reserve(person, MAX_LEASE_BYTES).is_ok());
    }

    #[test]
    fn rejects_stale_deadline_and_unbound_reservation() {
        let (registry, dependency, scope) = fixture();
        let reservation = registry.reserve(dependency.person_id(), 32).unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency.clone(),
                scope.clone(),
                "payload".to_owned(),
                Instant::now() - std::time::Duration::from_secs(1),
                reservation,
            ),
            Err(AgentFailure::StaleContext)
        ));
        let other_person_reservation = registry
            .reserve(floe_context_contract::PersonId::new(), 32)
            .unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency.clone(),
                scope.clone(),
                "payload".to_owned(),
                Instant::now() + std::time::Duration::from_secs(5),
                other_person_reservation,
            ),
            Err(AgentFailure::StaleContext)
        ));
        let other_registry = Arc::new(super::super::leases::SourceLeaseRegistry::new());
        let reservation = other_registry.reserve(dependency.person_id(), 32).unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency,
                scope,
                "payload".to_owned(),
                Instant::now() + std::time::Duration::from_secs(5),
                reservation,
            ),
            Err(AgentFailure::StaleContext)
        ));
    }

    #[test]
    fn rejects_payload_larger_than_reservation() {
        let (registry, dependency, scope) = fixture();
        let reservation = registry.reserve(dependency.person_id(), 1).unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency,
                scope,
                "payload".to_owned(),
                Instant::now() + std::time::Duration::from_secs(5),
                reservation,
            ),
            Err(AgentFailure::BudgetExceeded)
        ));
    }
}
