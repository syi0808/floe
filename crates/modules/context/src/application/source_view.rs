use std::io::{self, Write};

use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, GrantScope, validate_stored_dependency};
use serde::Serialize;
use tokio::time::Instant;

use super::leases::{MAX_LEASE_BYTES, SourceLeaseReservation};

struct BoundedByteCounter {
    allowance: usize,
    bytes_written: usize,
    exceeded: bool,
}

impl BoundedByteCounter {
    fn new(allowance: usize) -> Self {
        Self {
            allowance,
            bytes_written: 0,
            exceeded: false,
        }
    }

    fn bytes_written(&self) -> usize {
        self.bytes_written
    }

    fn exceeded(&self) -> bool {
        self.exceeded
    }
}

impl Write for BoundedByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let Some(next) = self.bytes_written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "source view byte budget exceeded",
            ));
        };
        if next > self.allowance {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "source view byte budget exceeded",
            ));
        }
        self.bytes_written = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

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
        validate_source_scope(&dependency, &scope)?;
        if deadline <= Instant::now() {
            return Err(AgentFailure::StaleContext);
        }
        reservation.validate_binding(
            dependency.person_id(),
            dependency.process_incarnation_id(),
        )?;
        bounded_serialized_size(&payload, reservation.byte_allowance())?;
        if deadline <= Instant::now() {
            return Err(AgentFailure::StaleContext);
        }
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

pub(crate) fn bounded_serialized_size(
    payload: &impl Serialize,
    allowance: usize,
) -> Result<usize, AgentFailure> {
    let mut payload_writer = BoundedByteCounter::new(allowance);
    let serialization_result = serde_json::to_writer(&mut payload_writer, payload);
    if payload_writer.exceeded() {
        return Err(AgentFailure::BudgetExceeded);
    }
    serialization_result.map_err(|_| AgentFailure::InvalidInput)?;
    let payload_size = payload_writer.bytes_written();
    if payload_size == 0 || payload_size > MAX_LEASE_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload_size)
}

pub(crate) fn validate_source_scope(
    dependency: &ContextDependency,
    scope: &GrantScope,
) -> Result<(), AgentFailure> {
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
    Ok(())
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
    use serde::ser::{Error as _, SerializeSeq, Serializer};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use uuid::Uuid;

    struct RepeatingPayload {
        attempts: Arc<AtomicUsize>,
        count: usize,
    }

    impl Serialize for RepeatingPayload {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut sequence = serializer.serialize_seq(Some(self.count))?;
            for _ in 0..self.count {
                self.attempts.fetch_add(1, Ordering::Relaxed);
                sequence.serialize_element(&"x")?;
            }
            sequence.end()
        }
    }

    struct FailingPayload;

    impl Serialize for FailingPayload {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(S::Error::custom("serialization failed"))
        }
    }

    struct SlowPayload(Arc<AtomicUsize>);

    impl Serialize for SlowPayload {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            self.0.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(120));
            serializer.serialize_str("payload")
        }
    }

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

    #[test]
    fn quota_stops_counting_serializer_without_output_buffer() {
        let (registry, dependency, scope) = fixture();
        let attempts = Arc::new(AtomicUsize::new(0));
        let reservation = registry.reserve(dependency.person_id(), 1).unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency,
                scope,
                RepeatingPayload {
                    attempts: Arc::clone(&attempts),
                    count: 1_000_000,
                },
                Instant::now() + std::time::Duration::from_secs(5),
                reservation,
            ),
            Err(AgentFailure::BudgetExceeded)
        ));
        assert!(attempts.load(Ordering::Relaxed) < 1_000_000);
    }

    #[test]
    fn malformed_serialization_releases_reservation() {
        let (registry, dependency, scope) = fixture();
        let person = dependency.person_id();
        let reservation = registry.reserve(person, 32).unwrap();
        assert!(matches!(
            SourceView::try_new(
                dependency,
                scope,
                FailingPayload,
                Instant::now() + std::time::Duration::from_secs(5),
                reservation,
            ),
            Err(AgentFailure::InvalidInput)
        ));
        assert!(registry.reserve(person, MAX_LEASE_BYTES).is_ok());
    }

    #[test]
    fn serialization_crossing_deadline_is_stale_and_releases_reservation() {
        let (registry, dependency, scope) = fixture();
        let person = dependency.person_id();
        let reservation = registry.reserve(person, 32).unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        assert!(matches!(
            SourceView::try_new(
                dependency,
                scope,
                SlowPayload(Arc::clone(&attempts)),
                Instant::now() + std::time::Duration::from_millis(100),
                reservation,
            ),
            Err(AgentFailure::StaleContext)
        ));
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        assert!(registry.reserve(person, MAX_LEASE_BYTES).is_ok());
    }
}

/// A held source read, as any reader outside Context sees it.
impl floe_context_contract::AuthorizedRead for SourceView<serde_json::Value> {
    fn dependency(&self) -> &ContextDependency {
        &self.dependency
    }

    fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    fn is_fresh(&self) -> bool {
        self.deadline > Instant::now()
    }
}
