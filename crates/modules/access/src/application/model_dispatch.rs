use crate::{
    DependencyAuthorization, DependencyResolver, GatewayAdmission, ModelDispatchRequest,
    ModelDispatchTarget,
};
use chrono::Utc;
use floe_context_contract::{DataClass, DependencyCoverage};
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use std::sync::Arc;

/// Owns the original resolver/authority and execution scope so a live call can
/// retain the same fence across awaits. Ownership never extends its deadline or
/// detaches cancellation. Generation and live dispatch share this one policy.
#[must_use = "consume the permit at the provider handoff fence"]
pub struct ModelDispatchPermit<Resolver, Authority> {
    request: ModelDispatchRequest,
    resolver: Arc<Resolver>,
    authority: Arc<Authority>,
    scope: ExecutionScope,
}
#[must_use = "revalidate the fence before releasing the response"]
pub struct ModelDispatchFence<Resolver, Authority> {
    request: ModelDispatchRequest,
    resolver: Arc<Resolver>,
    authority: Arc<Authority>,
    scope: ExecutionScope,
}
impl<Resolver, Authority> ModelDispatchFence<Resolver, Authority> {
    pub fn target(&self) -> (&[u8; 32], &ModelDispatchTarget) {
        (&self.request.binding_digest, &self.request.target)
    }
}

pub async fn admit_model_dispatch<R: DependencyResolver, A: GatewayAdmission>(
    request: ModelDispatchRequest,
    resolver: Arc<R>,
    authority: Arc<A>,
    scope: ExecutionScope,
) -> Result<ModelDispatchPermit<R, A>, AgentFailure> {
    authorize_request(&request, resolver.as_ref(), authority.as_ref(), &scope).await?;
    Ok(ModelDispatchPermit {
        request,
        resolver,
        authority,
        scope,
    })
}
pub async fn consume_model_dispatch<R: DependencyResolver, A: GatewayAdmission>(
    permit: ModelDispatchPermit<R, A>,
) -> Result<ModelDispatchFence<R, A>, AgentFailure> {
    let ModelDispatchPermit {
        request,
        resolver,
        authority,
        scope,
    } = permit;
    authorize_request(&request, resolver.as_ref(), authority.as_ref(), &scope).await?;
    Ok(ModelDispatchFence {
        request,
        resolver,
        authority,
        scope,
    })
}
pub async fn revalidate_model_dispatch<R: DependencyResolver, A: GatewayAdmission>(
    fence: &ModelDispatchFence<R, A>,
) -> Result<(), AgentFailure> {
    // Revalidation is a read observation. Cancelling or dropping its nested
    // resolver/Gateway future must not cancel the retained dispatch lifetime.
    let observation = fence.scope.child_scope(fence.scope.deadline(), 0, 0, None);
    let mut request = fence.request.clone();
    request.cancellation = fence.request.cancellation.child_scope();
    authorize_request(
        &request,
        fence.resolver.as_ref(),
        fence.authority.as_ref(),
        &observation,
    )
    .await
}
async fn authorize_request<R: DependencyResolver, A: GatewayAdmission>(
    request: &ModelDispatchRequest,
    resolver: &R,
    authority: &A,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    request.validate()?;
    if request.cancellation.is_cancelled() || scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if request.deadline > scope.deadline() {
        return Err(AgentFailure::InvalidInput);
    }
    if tokio::time::Instant::now() >= request.deadline
        || tokio::time::Instant::now() >= scope.deadline()
    {
        return Err(AgentFailure::DeadlineExceeded);
    }
    if request.input_data_classes.iter().any(|class| {
        matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw)
            || (*class == DataClass::TemporaryAiContext && request.target.is_gateway())
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    match &request.coverage {
        DependencyCoverage::Unknown if request.target.is_gateway() => {
            Err(AgentFailure::PolicyDenied)
        }
        DependencyCoverage::Unknown | DependencyCoverage::Independent => Ok(()),
        DependencyCoverage::Dependent { dependencies } => {
            let authorization = DependencyAuthorization {
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            };
            for dependency in dependencies {
                if dependency.person_id() != request.person_id
                    || dependency.expires_at() <= Utc::now()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                resolver.authorize(dependency, &authorization).await?;
                if dependency.source().connector().as_str() == crate::WELLBEING_CONNECTOR {
                    dependency
                        .validate_health_transform(&request.device_id, Utc::now())
                        .map_err(|_| AgentFailure::PolicyDenied)?;
                    if !request
                        .input_data_classes
                        .contains(&DataClass::HighlySensitive)
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                }
                if request.target.is_gateway()
                    && !dependency
                        .processing()
                        .admits_gateway(dependency.categories())
                {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
            Ok(())
        }
    }?;
    // Revalidate exact confirmed-memory revisions after potentially slow source
    // reads. A captured projection cannot keep forgotten memory alive.
    if !request.memories.is_empty() {
        resolver
            .authorize_memories(
                request.person_id,
                &request.memories,
                &DependencyAuthorization {
                    deadline: request.deadline,
                    cancellation: request.cancellation.clone(),
                },
            )
            .await?;
    }
    // Source revalidation may await native/provider I/O. Reload the exact model
    // transport after those awaits, immediately at this handoff/release fence.
    if let ModelDispatchTarget::Gateway { expected } = &request.target {
        if authority.admit(expected, scope).await? != *expected {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    if request.cancellation.is_cancelled() || scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= request.deadline
        || tokio::time::Instant::now() >= scope.deadline()
    {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod memory_fence_tests {
    use super::*;
    use floe_context_contract::{ContextDependency, MemoryRevisionReference};
    use floe_execution::{
        BoxFuture, Cancellation,
        budget::{BudgetConfig, BudgetLedger, ModelUsage},
    };
    use floe_kernel::{PersonId, TraceContext};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use uuid::Uuid;

    struct MemoryResolver {
        stall: AtomicBool,
        live: AtomicBool,
        reads: AtomicUsize,
    }
    impl DependencyResolver for MemoryResolver {
        fn authorize<'a>(
            &'a self,
            _: &'a ContextDependency,
            _: &'a DependencyAuthorization,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async { Err(AgentFailure::PolicyDenied) })
        }
        fn authorize_memories<'a>(
            &'a self,
            _: PersonId,
            references: &'a [MemoryRevisionReference],
            authorization: &'a DependencyAuthorization,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async move {
                if self.stall.load(Ordering::SeqCst) {
                    return floe_execution::tasks::run_bounded(
                        std::future::pending(),
                        authorization.deadline,
                        &authorization.cancellation,
                    )
                    .await;
                }
                assert_eq!(references.len(), 1);
                self.reads.fetch_add(1, Ordering::SeqCst);
                if self.live.load(Ordering::SeqCst) {
                    Ok(())
                } else {
                    Err(AgentFailure::StaleContext)
                }
            })
        }
    }
    struct NoGateway;
    impl GatewayAdmission for NoGateway {
        fn admit<'a>(
            &'a self,
            _: &'a crate::VerifiedGatewayBinding,
            _: &'a ExecutionScope,
        ) -> BoxFuture<'a, Result<crate::VerifiedGatewayBinding, AgentFailure>> {
            Box::pin(async { panic!("device dispatch cannot consult Gateway") })
        }
    }
    fn scope() -> ExecutionScope {
        ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            BudgetLedger::new(BudgetConfig::new(64, 64), ModelUsage::default()).root_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }
    fn request(scope: &ExecutionScope) -> ModelDispatchRequest {
        ModelDispatchRequest {
            person_id: PersonId::new(),
            device_id: "test-device".into(),
            plan_id: Uuid::new_v4(),
            binding_digest: [1; 32],
            projection_ref: Uuid::new_v4(),
            projection_revision: 1,
            coverage: DependencyCoverage::Independent,
            memories: vec![MemoryRevisionReference {
                target_id: Uuid::new_v4(),
                revision: 1,
            }],
            input_data_classes: vec![DataClass::Personal],
            purpose: "test".into(),
            consumer: "manager".into(),
            target: ModelDispatchTarget::Device,
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        }
    }
    #[tokio::test]
    async fn owned_fence_crosses_tasks_without_detaching_original_cancellation() {
        let (fence, cancellation, resolver) = {
            let scope = scope();
            let cancellation = scope.cancellation().clone();
            let resolver = Arc::new(MemoryResolver {
                live: AtomicBool::new(true),
                reads: AtomicUsize::new(0),
                stall: AtomicBool::new(false),
            });
            let permit = admit_model_dispatch(
                request(&scope),
                resolver.clone(),
                Arc::new(NoGateway),
                scope,
            )
            .await
            .unwrap();
            (
                consume_model_dispatch(permit).await.unwrap(),
                cancellation,
                resolver,
            )
        };
        let fence = tokio::spawn(async move {
            revalidate_model_dispatch(&fence).await.unwrap();
            fence
        })
        .await
        .unwrap();
        resolver.live.store(false, Ordering::SeqCst);
        assert_eq!(
            revalidate_model_dispatch(&fence).await,
            Err(AgentFailure::StaleContext)
        );
        resolver.live.store(true, Ordering::SeqCst);
        cancellation.cancel_with_reason(floe_execution::CancelReason::User);
        assert_eq!(
            revalidate_model_dispatch(&fence).await,
            Err(AgentFailure::Cancelled)
        );
    }

    #[tokio::test]
    async fn forgotten_memory_is_checked_at_admission_handoff_and_response_release() {
        let scope = scope();
        let resolver = Arc::new(MemoryResolver {
            live: AtomicBool::new(true),
            reads: AtomicUsize::new(0),
            stall: AtomicBool::new(false),
        });
        let permit = admit_model_dispatch(
            request(&scope),
            resolver.clone(),
            Arc::new(NoGateway),
            scope.clone(),
        )
        .await
        .unwrap();
        resolver.live.store(false, Ordering::SeqCst);
        assert!(matches!(
            consume_model_dispatch(permit).await,
            Err(AgentFailure::StaleContext)
        ));
        assert!(matches!(
            admit_model_dispatch(
                request(&scope),
                resolver.clone(),
                Arc::new(NoGateway),
                scope.clone()
            )
            .await,
            Err(AgentFailure::StaleContext)
        ));
        resolver.live.store(true, Ordering::SeqCst);
        let permit = admit_model_dispatch(
            request(&scope),
            resolver.clone(),
            Arc::new(NoGateway),
            scope.clone(),
        )
        .await
        .unwrap();
        let fence = consume_model_dispatch(permit).await.unwrap();
        resolver.live.store(false, Ordering::SeqCst);
        assert_eq!(
            revalidate_model_dispatch(&fence).await,
            Err(AgentFailure::StaleContext)
        );
        assert_eq!(resolver.reads.load(Ordering::SeqCst), 6);
        let mut empty = request(&scope);
        empty.memories.clear();
        let permit =
            admit_model_dispatch(empty, resolver.clone(), Arc::new(NoGateway), scope.clone())
                .await
                .unwrap();
        let fence = consume_model_dispatch(permit).await.unwrap();
        revalidate_model_dispatch(&fence).await.unwrap();
        assert_eq!(
            resolver.reads.load(Ordering::SeqCst),
            6,
            "memory-free projections are unaffected"
        );
    }
    #[test]
    fn duplicate_and_invalid_memory_references_are_rejected() {
        let scope = scope();
        let mut request = request(&scope);
        request.memories.push(request.memories[0].clone());
        assert_eq!(request.validate(), Err(AgentFailure::InvalidInput));
        request.memories.pop();
        request.memories[0].revision = 0;
        assert_eq!(request.validate(), Err(AgentFailure::InvalidInput));
    }
    #[tokio::test]
    async fn dropped_nested_revalidation_cannot_cancel_original_dispatch_scope() {
        let scope = scope();
        let resolver = Arc::new(MemoryResolver {
            live: AtomicBool::new(true),
            reads: AtomicUsize::new(0),
            stall: AtomicBool::new(false),
        });
        let permit = admit_model_dispatch(
            request(&scope),
            resolver.clone(),
            Arc::new(NoGateway),
            scope.clone(),
        )
        .await
        .unwrap();
        let fence = consume_model_dispatch(permit).await.unwrap();
        resolver.stall.store(true, Ordering::SeqCst);
        let mut observation = Box::pin(revalidate_model_dispatch(&fence));
        let polled = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(observation.as_mut(), cx))
        })
        .await;
        assert!(polled.is_pending());
        drop(observation);
        assert!(!scope.cancellation().is_cancelled());
        resolver.stall.store(false, Ordering::SeqCst);
        revalidate_model_dispatch(&fence).await.unwrap();
    }
    #[tokio::test]
    async fn revalidation_retains_a_distinct_original_request_cancellation() {
        let scope = scope();
        let request_cancellation = Cancellation::new();
        let mut selected = request(&scope);
        selected.cancellation = request_cancellation.clone();
        let resolver = Arc::new(MemoryResolver {
            live: AtomicBool::new(true),
            reads: AtomicUsize::new(0),
            stall: AtomicBool::new(false),
        });
        let permit = admit_model_dispatch(selected, resolver, Arc::new(NoGateway), scope.clone())
            .await
            .unwrap();
        let fence = consume_model_dispatch(permit).await.unwrap();
        request_cancellation.cancel();
        assert_eq!(
            revalidate_model_dispatch(&fence).await,
            Err(AgentFailure::Cancelled)
        );
        assert!(!scope.cancellation().is_cancelled());
    }
}
