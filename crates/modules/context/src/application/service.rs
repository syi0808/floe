use std::sync::{Arc, OnceLock};

use floe_agent_contract::{AgentFailure, CancelReason};
use floe_context_contract::{
    ContextDependencyError, GrantOperation, PersonId, validate_dependency_freshness,
};

use crate::ports::source_reader::SourceReadRequestParts;
use crate::{MAX_LEASE_BYTES, SourceLeaseRegistry, SourceReadRequest, SourceReader, SourceView};

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
    ) -> Result<SourceView<serde_json::Value>, AgentFailure> {
        if request.person_id() != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        check_window(request)?;
        let reader = self
            .source_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let source_read = tokio::select! {
            biased;
            _ = request.cancellation().cancelled() => return Err(cancelled(request)),
            _ = tokio::time::sleep_until(request.deadline()) => return Err(AgentFailure::DeadlineExceeded),
            observation = reader.read(request) => observation?,
        };
        check_window(request)?;
        let dependency = source_read.dependency();
        validate_dependency_freshness(dependency, chrono::Utc::now()).map_err(
            |error| match error {
                ContextDependencyError::Expired => AgentFailure::StaleContext,
                ContextDependencyError::Unauthorized => AgentFailure::PolicyDenied,
                _ => AgentFailure::VaultUnavailable,
            },
        )?;
        super::source_view::validate_source_scope(dependency, source_read.scope())
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if source_read.source() != request.source()
            || dependency.person_id() != self.person_id
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != request.purpose()
            || dependency.consumer() != request.consumer()
            || dependency.process_incarnation_id() != request.process_incarnation_id()
            || dependency.query_fingerprint() != request.query_fingerprint()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let now = chrono::Utc::now();
        let wall_remaining = dependency
            .expires_at()
            .signed_duration_since(now)
            .to_std()
            .map_err(|_| AgentFailure::StaleContext)?;
        let effective_deadline = request.deadline().min(
            tokio::time::Instant::now()
                .checked_add(wall_remaining)
                .ok_or(AgentFailure::StaleContext)?,
        );
        let (payload, dependency, scope) = source_read.into_parts();
        let payload_size = super::source_view::bounded_serialized_size(&payload, MAX_LEASE_BYTES)?;
        let reservation = self.leases.reserve(self.person_id, payload_size)?;
        SourceView::try_new(dependency, scope, payload, effective_deadline, reservation)
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::Utc;
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantPurpose, GrantSourceBinding,
        ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use uuid::Uuid;

    use super::*;

    struct FixtureReader {
        calls: Arc<AtomicUsize>,
        wrong_binding: bool,
    }

    impl SourceReader for FixtureReader {
        fn read<'a>(
            &'a self,
            request: &'a SourceReadRequest,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<crate::SourceRead, AgentFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::Relaxed);
                Ok(crate::SourceRead::new(
                    if self.wrong_binding {
                        crate::SourceKey::try_new("other.view").unwrap()
                    } else {
                        request.source().clone()
                    },
                    serde_json::json!({"items": []}),
                    dependency(request, self.wrong_binding),
                    scope(request.consumer().clone()),
                ))
            })
        }
    }

    fn dependency(
        request: &SourceReadRequest,
        wrong_binding: bool,
    ) -> floe_context_contract::ContextDependency {
        let now = Utc::now();
        floe_context_contract::ContextDependency::try_new(
            request.person_id(),
            GrantId::new(),
            GrantAuthority::new(),
            GrantSourceBinding::try_new(
                request.person_id(),
                ConnectionId::new(),
                ConnectorId::try_new("fixture.connector").unwrap(),
                ExecutionOwnerId::try_new("fixture-owner").unwrap(),
                SourceAuthority::new(),
            )
            .unwrap(),
            vec![ResourceHandle::try_new("fixture/item").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            request.consumer().clone(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            if wrong_binding {
                vec![9; 32]
            } else {
                request.query_fingerprint().to_vec()
            },
            Uuid::new_v4(),
            if wrong_binding {
                Uuid::new_v4()
            } else {
                request.process_incarnation_id()
            },
            now,
            now + chrono::Duration::minutes(1),
        )
        .unwrap()
    }

    fn scope(consumer: GrantConsumer) -> floe_context_contract::GrantScope {
        floe_context_contract::GrantScope::try_new(
            vec![ResourceHandle::try_new("fixture/item").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![consumer],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    fn request(prepared: &PreparedContext<'_>, consumer: GrantConsumer) -> SourceReadRequest {
        prepared
            .source_request(
                "fixture.view",
                consumer,
                GrantPurpose::Assistant,
                serde_json::json!({"limit": 10}),
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                floe_agent_contract::Cancellation::new(),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn preparation_is_lazy_and_read_validates_the_observation() {
        let person_id = PersonId::new();
        let consumer = GrantConsumer::builtin("fixture.expert").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let reader = FixtureReader {
            calls: Arc::clone(&calls),
            wrong_binding: false,
        };
        let prepared = ContextService::new(Some(&reader))
            .prepare(person_id)
            .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let observation = prepared
            .read_source(&request(&prepared, consumer))
            .await
            .unwrap();
        assert_eq!(observation.payload(), &serde_json::json!({"items": []}));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn concurrent_small_views_reserve_their_measured_bytes() {
        let person_id = PersonId::new();
        let consumer = GrantConsumer::builtin("fixture.expert").unwrap();
        let reader = FixtureReader {
            calls: Arc::new(AtomicUsize::new(0)),
            wrong_binding: false,
        };
        let prepared = ContextService::new(Some(&reader))
            .prepare(person_id)
            .unwrap();
        let first = prepared
            .read_source(&request(&prepared, consumer.clone()))
            .await
            .unwrap();
        let second = prepared
            .read_source(&request(&prepared, consumer))
            .await
            .unwrap();
        assert_eq!(first.payload(), second.payload());
    }

    #[tokio::test]
    async fn cancellation_and_person_mismatch_do_not_reach_the_reader() {
        let person_id = PersonId::new();
        let consumer = GrantConsumer::builtin("fixture.expert").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let reader = FixtureReader {
            calls: Arc::clone(&calls),
            wrong_binding: false,
        };
        let prepared = ContextService::new(Some(&reader))
            .prepare(person_id)
            .unwrap();
        let cancelled = request(&prepared, consumer.clone());
        cancelled.cancellation().cancel();
        assert_eq!(
            prepared.read_source(&cancelled).await.err(),
            Some(AgentFailure::Cancelled)
        );
        let other_prepared = ContextService::new(Some(&reader))
            .prepare(PersonId::new())
            .unwrap();
        assert_eq!(
            prepared
                .read_source(&request(&other_prepared, consumer))
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn mismatched_process_and_query_binding_is_rejected() {
        let person_id = PersonId::new();
        let consumer = GrantConsumer::builtin("fixture.expert").unwrap();
        let reader = FixtureReader {
            calls: Arc::new(AtomicUsize::new(0)),
            wrong_binding: true,
        };
        let prepared = ContextService::new(Some(&reader))
            .prepare(person_id)
            .unwrap();
        assert_eq!(
            prepared
                .read_source(&request(&prepared, consumer))
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
