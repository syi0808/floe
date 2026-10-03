use std::sync::{Arc, OnceLock};

use floe_agent_contract::{AgentFailure, CancelReason};
use floe_context_contract::{
    ContextDependencyError, GrantOperation, PersonId, SourceReadOutcome,
    validate_dependency_freshness,
};

use crate::ports::source_reader::SourceReadRequestParts;
use crate::{
    MAX_LEASE_BYTES, SelectedSourceReader, SourceLeaseRegistry, SourceRead, SourceReadRequest,
    SourceReader, SourceView,
};

static SOURCE_LEASES: OnceLock<Arc<SourceLeaseRegistry>> = OnceLock::new();

pub struct ContextService<'a> {
    source_reader: Option<&'a dyn SourceReader>,
    leases: Arc<SourceLeaseRegistry>,
}

impl<'a> ContextService<'a> {
    pub fn new(source_reader: Option<&'a dyn SourceReader>) -> Self {
        Self {
            source_reader,
            leases: Arc::clone(SOURCE_LEASES.get_or_init(|| Arc::new(SourceLeaseRegistry::new()))),
        }
    }

    pub fn prepare(&self, person_id: PersonId) -> Result<PreparedContext<'a>, AgentFailure> {
        if person_id.0.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(PreparedContext {
            person_id,
            source_reader: self.source_reader,
            leases: Arc::clone(&self.leases),
        })
    }
}

pub struct PreparedContext<'a> {
    person_id: PersonId,
    source_reader: Option<&'a dyn SourceReader>,
    leases: Arc<SourceLeaseRegistry>,
}

impl PreparedContext<'_> {
    pub fn source_request(
        &self,
        source_id: impl Into<String>,
        consumer: floe_context_contract::GrantConsumer,
        purpose: floe_context_contract::GrantPurpose,
        query: serde_json::Value,
        deadline: tokio::time::Instant,
        cancellation: floe_agent_contract::Cancellation,
    ) -> Result<SourceReadRequest, AgentFailure> {
        SourceReadRequest::try_new(SourceReadRequestParts {
            person_id: self.person_id,
            source_id: source_id.into(),
            consumer,
            purpose,
            query,
            deadline,
            cancellation,
            process_incarnation_id: self.leases.process_incarnation(),
        })
    }

    pub async fn read_source(
        &self,
        request: &SourceReadRequest,
    ) -> Result<SourceReadOutcome<SourceView<serde_json::Value>>, AgentFailure> {
        let reader = self
            .source_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        self.read_source_from(request, reader.read(request)).await
    }

    pub async fn read_selected_source(
        &self,
        request: &SourceReadRequest,
        reader: &dyn SelectedSourceReader,
        selected: &[floe_context_contract::SourceSelectionReference],
    ) -> Result<SourceReadOutcome<SourceView<serde_json::Value>>, AgentFailure> {
        self.read_source_from(request, reader.read_selected(request, selected))
            .await
    }

    async fn read_source_from(
        &self,
        request: &SourceReadRequest,
        observation: impl std::future::Future<
            Output = Result<SourceReadOutcome<SourceRead>, AgentFailure>,
        >,
    ) -> Result<SourceReadOutcome<SourceView<serde_json::Value>>, AgentFailure> {
        if request.person_id() != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        check_window(request)?;
        let outcome = tokio::select! {
            biased;
            _ = request.cancellation().cancelled() => return Err(cancelled(request)),
            _ = tokio::time::sleep_until(request.deadline()) => return Err(AgentFailure::DeadlineExceeded),
            observation = observation => observation?,
        };
        check_window(request)?;
        let source_read = match outcome {
            SourceReadOutcome::Ready(read) => read,
            SourceReadOutcome::Unavailable(reason) => {
                return Ok(SourceReadOutcome::Unavailable(reason));
            }
            SourceReadOutcome::NeedsUserAction(blockers) => {
                blockers
                    .validate()
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                for blocker in blockers.blockers() {
                    // A reader reports only what this consumer asked to read.
                    if blocker.consumer() != request.consumer()
                        || blocker.purpose() != request.purpose()
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                }
                return Ok(SourceReadOutcome::NeedsUserAction(blockers));
            }
        };
        if source_read.source() != request.source() || source_read.bindings().is_empty() {
            return Err(AgentFailure::PolicyDenied);
        }
        let now = chrono::Utc::now();
        let mut effective_deadline = request.deadline();
        for binding in source_read.bindings() {
            let dependency = &binding.dependency;
            validate_dependency_freshness(dependency, now).map_err(|error| match error {
                ContextDependencyError::Expired => AgentFailure::StaleContext,
                ContextDependencyError::Unauthorized => AgentFailure::PolicyDenied,
                _ => AgentFailure::VaultUnavailable,
            })?;
            super::source_view::validate_source_scope(dependency, &binding.scope)
                .map_err(|_| AgentFailure::PolicyDenied)?;
            if dependency.person_id() != self.person_id
                || dependency.operation() != GrantOperation::Read
                || dependency.purpose() != request.purpose()
                || dependency.consumer() != request.consumer()
                || dependency.process_incarnation_id() != request.process_incarnation_id()
                || dependency.query_fingerprint() != request.query_fingerprint()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let wall_remaining = dependency
                .expires_at()
                .signed_duration_since(now)
                .to_std()
                .map_err(|_| AgentFailure::StaleContext)?;
            effective_deadline = effective_deadline.min(
                tokio::time::Instant::now()
                    .checked_add(wall_remaining)
                    .ok_or(AgentFailure::StaleContext)?,
            );
        }
        let (payload, bindings) = source_read.into_parts();
        let payload_size = super::source_view::bounded_serialized_size(&payload, MAX_LEASE_BYTES)?;
        let reservation = self.leases.reserve(self.person_id, payload_size)?;
        SourceView::try_new_bound(bindings, payload, effective_deadline, reservation)
            .map(SourceReadOutcome::Ready)
    }
}

fn check_window(request: &SourceReadRequest) -> Result<(), AgentFailure> {
    if request.cancellation().is_cancelled() {
        return Err(cancelled(request));
    }
    if request.deadline() <= tokio::time::Instant::now() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

fn cancelled(request: &SourceReadRequest) -> AgentFailure {
    if request.cancellation().reason() == Some(CancelReason::Deadline) {
        AgentFailure::DeadlineExceeded
    } else {
        AgentFailure::Cancelled
    }
}
