use std::io::{self, Write};

use floe_agent_contract::AgentFailure;
use floe_context_contract::{
    AuthorizedSourceBinding, ContextDependency, GrantScope, validate_stored_dependency,
};
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
    bindings: Vec<AuthorizedSourceBinding>,
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
        Self::try_new_bound(
            vec![AuthorizedSourceBinding { dependency, scope }],
            payload,
            deadline,
            reservation,
        )
    }

    pub fn try_new_bound(
        bindings: Vec<AuthorizedSourceBinding>,
        payload: Payload,
        deadline: Instant,
        reservation: SourceLeaseReservation,
    ) -> Result<Self, AgentFailure> {
        if bindings.is_empty() || bindings.len() > floe_context_contract::MAX_CONTEXT_DEPENDENCIES {
            return Err(AgentFailure::InvalidInput);
        }
        for binding in &bindings {
            validate_stored_dependency(&binding.dependency)
                .map_err(|_| AgentFailure::InvalidInput)?;
            validate_source_scope(&binding.dependency, &binding.scope)?;
        }
        if deadline <= Instant::now() {
            return Err(AgentFailure::StaleContext);
        }
        let first = &bindings[0].dependency;
        if bindings.iter().any(|binding| {
            binding.dependency.person_id() != first.person_id()
                || binding.dependency.process_incarnation_id() != first.process_incarnation_id()
        }) {
            return Err(AgentFailure::InvalidInput);
        }
        reservation.validate_binding(first.person_id(), first.process_incarnation_id())?;
        bounded_serialized_size(&payload, reservation.byte_allowance())?;
        if deadline <= Instant::now() {
            return Err(AgentFailure::StaleContext);
        }
        Ok(Self {
            bindings,
            payload,
            deadline,
            _reservation: reservation,
        })
    }

    pub fn bindings(&self) -> &[AuthorizedSourceBinding] {
        &self.bindings
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

/// What any held read was admitted under, whatever it carries.
impl<Payload: Serialize> floe_context_contract::HeldGrant for SourceView<Payload> {
    fn bindings(&self) -> &[AuthorizedSourceBinding] {
        &self.bindings
    }
}

/// A held source read, as any reader outside Context sees it.
impl floe_context_contract::AuthorizedRead for SourceView<serde_json::Value> {
    fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    fn is_fresh(&self) -> bool {
        self.deadline > Instant::now()
    }
}
