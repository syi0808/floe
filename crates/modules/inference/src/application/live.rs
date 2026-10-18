//! Live planning only. A prepared selection is not microphone or dispatch authority.
use crate::{
    InferenceService, LIVE_PRESENTATION_CONSUMER, LIVE_PRESENTATION_PURPOSE, LiveModelPlanRequest,
    LiveModelProvider, PreparedLiveModelProfile,
};
use floe_agent_contract::{AgentFailure, ProcessingBoundary};
use floe_execution::ExecutionScope;
use floe_model_contract::LiveModelPlan;
#[cfg(test)]
use floe_model_contract::{LiveModelCapabilities, LiveTransportLimits};
use uuid::Uuid;

/// Non-clone and privately constructed. Inspection is not a dispatch token.
pub struct PreparedLiveSelection<T> {
    pub(super) plan: LiveModelPlan,
    pub(super) target: floe_access::ModelDispatchTarget,
    pub(super) transport: T,
    pub(super) preparation_cancellation: floe_execution::Cancellation,
    pub(super) preparation_deadline: tokio::time::Instant,
}
impl<T> PreparedLiveSelection<T> {
    pub fn plan(&self) -> &LiveModelPlan {
        &self.plan
    }
}

impl<P: LiveModelProvider, R, A> InferenceService<P, R, A> {
    pub async fn prepare_live(
        &self,
        request: LiveModelPlanRequest,
        scope: &ExecutionScope,
    ) -> Result<PreparedLiveSelection<P::Transport>, AgentFailure> {
        request.validate()?;
        scope
            .run(async {
                let (profile, boundary) = super::selection::select_primary_or_local(
                    self.provider.observe_primary_live(&request, scope),
                    || self.provider.observe_local_live(&request, scope),
                )
                .await?;
                let PreparedLiveModelProfile {
                    capability,
                    target,
                    transport,
                } = profile;
                capability
                    .capabilities
                    .validate()
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                capability
                    .limits
                    .validate()
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                capability.selection_commitment.validate()?;
                if capability.purpose != LIVE_PRESENTATION_PURPOSE
                    || capability.consumer != LIVE_PRESENTATION_CONSUMER
                    || capability.boundary != boundary
                    || capability.binding_digest.0 == [0; 32]
                    || !capability
                        .capabilities
                        .includes(&request.required_capabilities)
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                match (&target, boundary) {
                    (floe_access::ModelDispatchTarget::Device, ProcessingBoundary::Device) => {}
                    (
                        floe_access::ModelDispatchTarget::Gateway { expected },
                        ProcessingBoundary::Gateway,
                    ) => {
                        expected.validate()?;
                        if expected.person_id != request.actor.person_id.to_string()
                            || expected.device_id != request.actor.device_id
                            || expected.binding_digest() != capability.binding_digest.0
                        {
                            return Err(AgentFailure::PolicyDenied);
                        }
                    }
                    _ => return Err(AgentFailure::PolicyDenied),
                }
                Ok(PreparedLiveSelection {
                    plan: LiveModelPlan {
                        operation_id: Uuid::new_v4(),
                        actor: request.actor,
                        capabilities: capability.capabilities,
                        limits: capability.limits,
                        boundary,
                        binding_digest: capability.binding_digest,
                        selection_commitment: capability.selection_commitment,
                    },
                    target,
                    transport,
                    preparation_cancellation: scope.cancellation().clone(),
                    preparation_deadline: scope.deadline(),
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LocalObservation, ModelObservationError, ObservedLiveModelCapability, PrimaryAbsence,
        PrimaryObservation,
    };
    use floe_access::{ModelDispatchTarget, VerifiedGatewayBinding};
    use floe_agent_contract::{
        BoxFuture, ModelBindingDigest, ModelSelectionCommitment, OwnerActor, PersonId, TraceContext,
    };
    use floe_execution::{
        Cancellation,
        budget::{BudgetConfig, BudgetLedger, ModelUsage},
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    struct LiveHandle {
        drops: Arc<AtomicUsize>,
    }
    impl Drop for LiveHandle {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct Provider {
        primary: bool,
        corrupt: u8,
        primary_error: Option<ModelObservationError>,
        local_calls: AtomicUsize,
        drops: Arc<AtomicUsize>,
    }
    impl Provider {
        fn profile(
            &self,
            request: &LiveModelPlanRequest,
            boundary: ProcessingBoundary,
        ) -> PreparedLiveModelProfile<LiveHandle> {
            let mut expected = VerifiedGatewayBinding {
                person_id: request.actor.person_id.to_string(),
                device_id: request.actor.device_id.clone(),
                client_id: "fixture-client".into(),
                producer_instance: "fixture-producer".into(),
                producer_key_fingerprint: "a".repeat(64),
                producer_audience: "fixture-audience".into(),
                enrollment_id: "fixture-enrollment".into(),
                credential_generation: 1,
            };
            if self.corrupt == 1 {
                expected.device_id = "foreign-device".into();
            }
            let target = match boundary {
                ProcessingBoundary::Device => ModelDispatchTarget::Device,
                ProcessingBoundary::Gateway => ModelDispatchTarget::Gateway {
                    expected: expected.clone(),
                },
            };
            let mut capability = ObservedLiveModelCapability {
                purpose: LIVE_PRESENTATION_PURPOSE.into(),
                consumer: LIVE_PRESENTATION_CONSUMER.into(),
                capabilities: LiveModelCapabilities::conversational(),
                limits: LiveTransportLimits {
                    initial_context_bytes: 16_384,
                    append_bytes: 500,
                    local_session_duration_ms: 60_000,
                },
                boundary,
                binding_digest: ModelBindingDigest(if self.primary {
                    expected.binding_digest()
                } else {
                    [7; 32]
                }),
                selection_commitment: ModelSelectionCommitment([8; 32]),
            };
            match self.corrupt {
                2 => capability.binding_digest = ModelBindingDigest([6; 32]),
                3 => capability.capabilities.0.pop().map(|_| ()).unwrap(),
                4 => capability.consumer = "floe.manager".into(),
                5 => capability.purpose = "everyday_assistance".into(),
                6 => capability.limits.append_bytes = 0,
                7 => capability.boundary = ProcessingBoundary::Device,
                _ => {}
            }
            PreparedLiveModelProfile {
                capability,
                target,
                transport: LiveHandle {
                    drops: self.drops.clone(),
                },
            }
        }
    }
    impl LiveModelProvider for Provider {
        type Transport = LiveHandle;
        fn observe_primary_live<'a>(
            &'a self,
            request: &'a LiveModelPlanRequest,
            _: &'a ExecutionScope,
        ) -> BoxFuture<
            'a,
            Result<PrimaryObservation<PreparedLiveModelProfile<LiveHandle>>, ModelObservationError>,
        > {
            Box::pin(async move {
                if let Some(error) = self.primary_error {
                    return Err(error);
                }
                Ok(if self.primary {
                    PrimaryObservation::Available(
                        self.profile(request, ProcessingBoundary::Gateway),
                    )
                } else {
                    PrimaryObservation::Absent(PrimaryAbsence::NoGatewayConfigured)
                })
            })
        }
        fn observe_local_live<'a>(
            &'a self,
            request: &'a LiveModelPlanRequest,
            _: &'a ExecutionScope,
        ) -> BoxFuture<
            'a,
            Result<LocalObservation<PreparedLiveModelProfile<LiveHandle>>, ModelObservationError>,
        > {
            Box::pin(async move {
                self.local_calls.fetch_add(1, Ordering::SeqCst);
                Ok(LocalObservation::Available(
                    self.profile(request, ProcessingBoundary::Device),
                ))
            })
        }
    }
    fn request() -> LiveModelPlanRequest {
        LiveModelPlanRequest {
            actor: OwnerActor {
                person_id: PersonId::new(),
                device_id: "fixture-device".into(),
                runtime_epoch: 1,
            },
            required_capabilities: LiveModelCapabilities::conversational(),
        }
    }
    fn scope() -> ExecutionScope {
        let ledger = BudgetLedger::new(BudgetConfig::new(1, 1), ModelUsage::default());
        ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(2),
            ledger.root_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }
    fn service(primary: bool, corrupt: u8) -> InferenceService<Provider, (), ()> {
        InferenceService::new(
            Provider {
                primary,
                corrupt,
                primary_error: None,
                local_calls: AtomicUsize::new(0),
                drops: Arc::new(AtomicUsize::new(0)),
            },
            (),
            (),
        )
    }
    #[tokio::test]
    async fn prepared_live_selection_pins_actor_and_owns_one_transport_without_dispatch() {
        for primary in [true, false] {
            let service = service(primary, 0);
            let request = request();
            let selected = service
                .prepare_live(request.clone(), &scope())
                .await
                .unwrap();
            assert_eq!(selected.plan.actor, request.actor);
            assert_eq!(
                selected.plan.boundary,
                if primary {
                    ProcessingBoundary::Gateway
                } else {
                    ProcessingBoundary::Device
                }
            );
            assert_eq!(
                service.provider.local_calls.load(Ordering::SeqCst),
                usize::from(!primary)
            );
            assert_eq!(service.provider.drops.load(Ordering::SeqCst), 0);
            assert_eq!(selected.target.is_gateway(), primary);
            assert!(!selected.preparation_cancellation.is_cancelled());
            assert!(selected.preparation_deadline > tokio::time::Instant::now());
            assert_eq!(Arc::strong_count(&selected.transport.drops), 2);
            drop(selected);
            assert_eq!(service.provider.drops.load(Ordering::SeqCst), 1);
        }
    }
    #[tokio::test]
    async fn invalid_available_primary_is_rejected_without_local_fallback() {
        for corrupt in 1..=7 {
            let service = service(true, corrupt);
            let result = service.prepare_live(request(), &scope()).await;
            assert!(
                matches!(result, Err(AgentFailure::PolicyDenied)),
                "invalid profile case {corrupt}"
            );
            assert_eq!(service.provider.local_calls.load(Ordering::SeqCst), 0);
            assert_eq!(service.provider.drops.load(Ordering::SeqCst), 1);
        }
    }
    #[tokio::test]
    async fn live_failure_does_not_open_a_hidden_fallback_route() {
        let mut service = service(true, 0);
        service.provider.primary_error = Some(ModelObservationError::QuotaExceeded);
        assert!(matches!(
            service.prepare_live(request(), &scope()).await,
            Err(AgentFailure::QuotaExceeded)
        ));
        assert_eq!(service.provider.local_calls.load(Ordering::SeqCst), 0);
        assert_eq!(service.provider.drops.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn malformed_actor_never_reaches_provider_observation() {
        let service = service(false, 0);
        let mut request = request();
        request.actor.runtime_epoch = 0;
        assert!(matches!(
            service.prepare_live(request, &scope()).await,
            Err(AgentFailure::InvalidInput)
        ));
        assert_eq!(service.provider.local_calls.load(Ordering::SeqCst), 0);
        assert_eq!(service.provider.drops.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn cancelled_preparation_never_observes_local() {
        let service = service(false, 0);
        let scope = scope();
        scope.cancellation().cancel();
        assert!(matches!(
            service.prepare_live(request(), &scope).await,
            Err(AgentFailure::Cancelled)
        ));
        assert_eq!(service.provider.local_calls.load(Ordering::SeqCst), 0);
        assert_eq!(service.provider.drops.load(Ordering::SeqCst), 0);
    }
}
