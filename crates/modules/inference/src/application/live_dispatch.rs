//! One selected live handoff using the same Access and budget owners as
//! generation. Caller observation is separate from the bounded owner lifetime.
use crate::{
    AdmittedDispatchTarget, CanonicalLiveStart, InferenceService, LIVE_PRESENTATION_CONSUMER,
    LIVE_PRESENTATION_PURPOSE, LiveDispatchCustody, LiveDispatchIntent, LiveModelProvider,
    LiveSessionControl, LiveStartRequest, PreparedLiveSelection, PreparedLiveTransport,
};
use floe_access::{
    DependencyResolver, GatewayAdmission, ModelDispatchFence, ModelDispatchRequest,
    ModelDispatchTarget, admit_model_dispatch, consume_model_dispatch, revalidate_model_dispatch,
};
use floe_agent_contract::AgentFailure;
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetAttempt, ModelAttemptReceipt, ModelReservationCeiling},
};
use floe_model_contract::{LiveModelPlan, LiveUsageObservation};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;

// Host scheduling bounds, not zero-latency external revocation guarantees.
const REVALIDATE_INTERVAL: Duration = Duration::from_millis(250);
const REVALIDATE_TIMEOUT: Duration = Duration::from_millis(500);

/// Retains one fenced transport and budget reservation. No raw transport is
/// exposed: eventual event/append release must use guarded owned operations.
pub struct AuthorizedLiveSession<Session: LiveSessionControl, Resolver, Authority> {
    plan: LiveModelPlan,
    transport: Arc<Session>,
    fence: Arc<ModelDispatchFence<Resolver, Authority>>,
    scope: ExecutionScope,
    preparation_cancellation: Cancellation,
    owner_cancellation: Cancellation,
    attempt: Option<BudgetAttempt>,
    monitor: JoinHandle<()>,
    failure: Arc<Mutex<Option<AgentFailure>>>,
}
impl<S: LiveSessionControl, R, A> AuthorizedLiveSession<S, R, A> {
    pub fn plan(&self) -> &LiveModelPlan {
        &self.plan
    }
    pub fn failure(&self) -> Option<AgentFailure> {
        *self.failure.lock().unwrap()
    }
    pub fn finish(
        mut self,
        usage: LiveUsageObservation,
    ) -> Result<ModelAttemptReceipt, AgentFailure> {
        // Accounting remains independent of source/result release permissions.
        self.transport.fence();
        self.monitor.abort();
        usage.validate().map_err(|_| AgentFailure::InvalidInput)?;
        self.attempt
            .take()
            .ok_or(AgentFailure::Conflict)?
            .settle_observed(usage.tokens, usage.cost_micros)
    }
}
impl<S: LiveSessionControl, R: DependencyResolver, A: GatewayAdmission>
    AuthorizedLiveSession<S, R, A>
{
    pub async fn revalidate(&self) -> Result<(), AgentFailure> {
        if let Some(failure) = self.failure() {
            return Err(failure);
        }
        let result = if self.preparation_cancellation.is_cancelled()
            || self.owner_cancellation.is_cancelled()
        {
            Err(AgentFailure::Cancelled)
        } else {
            bounded_revalidate(&self.fence, &self.scope).await
        };
        if let Err(failure) = result {
            // Publish the causal failure before waking cancellation observers
            // or entering the local adapter fence. A derivative cancellation
            // must not replace a source-review/expiry failure with "cancelled".
            self.failure.lock().unwrap().get_or_insert(failure);
            self.scope
                .cancellation()
                .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
            self.transport.fence();
        }
        result
    }
}
impl<S: LiveSessionControl, R, A> Drop for AuthorizedLiveSession<S, R, A> {
    fn drop(&mut self) {
        self.scope
            .cancellation()
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        // Local fencing precedes accounting/storage settlement. An unsettled
        // dispatched attempt drops to unknown, never a zero-usage claim.
        self.transport.fence();
        self.monitor.abort();
    }
}

// Covers cancellation/panic between a returned transport and ready publication.
struct PendingLiveSession<S: LiveSessionControl>(Option<Arc<S>>);
impl<S: LiveSessionControl> Drop for PendingLiveSession<S> {
    fn drop(&mut self) {
        if let Some(session) = &self.0 {
            session.fence();
        }
    }
}

async fn bounded_revalidate<R: DependencyResolver, A: GatewayAdmission>(
    fence: &ModelDispatchFence<R, A>,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    scope
        .run(async {
            tokio::time::timeout(REVALIDATE_TIMEOUT, revalidate_model_dispatch(fence))
                .await
                .map_err(|_| AgentFailure::DeadlineExceeded)?
        })
        .await
}
fn monitor<S, R, A>(
    transport: Arc<S>,
    fence: Arc<ModelDispatchFence<R, A>>,
    scope: ExecutionScope,
    preparation_cancellation: Cancellation,
    owner_cancellation: Cancellation,
    failure: Arc<Mutex<Option<AgentFailure>>>,
) -> JoinHandle<()>
where
    S: LiveSessionControl + Send + Sync + 'static,
    R: DependencyResolver + 'static,
    A: GatewayAdmission + 'static,
{
    tokio::spawn(async move {
        let terminal = loop {
            tokio::select! {
                _ = scope.cancellation().cancelled() => break AgentFailure::Cancelled,
                _ = preparation_cancellation.cancelled() => break AgentFailure::Cancelled,
                _ = owner_cancellation.cancelled() => break AgentFailure::Interrupted,
                _ = tokio::time::sleep_until(scope.deadline()) => break AgentFailure::DeadlineExceeded,
                _ = tokio::time::sleep(REVALIDATE_INTERVAL) => {}
            }
            // Retirement/preparation cancellation must also interrupt a source
            // authorization read that is already in flight.
            let checked = tokio::select! {
                biased;
                _ = owner_cancellation.cancelled() => Err(AgentFailure::Interrupted),
                _ = preparation_cancellation.cancelled() => Err(AgentFailure::Cancelled),
                _ = scope.cancellation().cancelled() => Err(AgentFailure::Cancelled),
                _ = tokio::time::sleep_until(scope.deadline()) => Err(AgentFailure::DeadlineExceeded),
                result = bounded_revalidate(&fence, &scope) => result,
            };
            if let Err(error) = checked {
                break error;
            }
        };
        failure.lock().unwrap().get_or_insert(terminal);
        scope
            .cancellation()
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        transport.fence();
    })
}

impl<P, R, A> InferenceService<P, R, A>
where
    P: LiveModelProvider,
    P::Transport: PreparedLiveTransport,
    R: DependencyResolver + 'static,
    A: GatewayAdmission + 'static,
{
    pub async fn start_live(
        &self,
        prepared: PreparedLiveSelection<P::Transport>,
        request: LiveStartRequest,
        custody: &dyn LiveDispatchCustody,
        scope: &ExecutionScope,
    ) -> Result<
        AuthorizedLiveSession<<P::Transport as PreparedLiveTransport>::Session, R, A>,
        AgentFailure,
    > {
        let PreparedLiveSelection {
            plan,
            target,
            transport,
            preparation_cancellation,
            preparation_deadline,
        } = prepared;
        plan.validate()?;
        request.reservation.validate()?;
        request.projection.validate_for_plan(
            &plan,
            request.voice_session_id,
            request.surface_session_id,
        )?;
        if request.media_lease_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if request.projection.payload.byte_len() as u64 > plan.limits.initial_context_bytes {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        if preparation_cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let now = tokio::time::Instant::now();
        let deadline = now
            .checked_add(Duration::from_millis(plan.limits.local_session_duration_ms))
            .ok_or(AgentFailure::InvalidInput)?
            .min(scope.deadline())
            .min(preparation_deadline);
        if now >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let lifetime = scope.child_scope(
            deadline,
            request.reservation.tokens,
            request.reservation.cost_micros,
            None,
        );
        let owner_cancellation = custody.lifetime_cancellation(&plan.actor)?;
        let preparation_guard = preparation_cancellation.child_scope();
        let future = self.dispatch_live(
            plan,
            target,
            transport,
            request,
            custody,
            &lifetime,
            preparation_cancellation,
            owner_cancellation.clone(),
        );
        tokio::select! {
            biased;
            _ = owner_cancellation.cancelled() => Err(AgentFailure::Interrupted),
            result = floe_execution::tasks::run_bounded(lifetime.run(future), preparation_deadline, &preparation_guard) => result,
        }
    }

    async fn dispatch_live(
        &self,
        plan: LiveModelPlan,
        target: ModelDispatchTarget,
        transport: P::Transport,
        request: LiveStartRequest,
        custody: &dyn LiveDispatchCustody,
        lifetime: &ExecutionScope,
        preparation_cancellation: Cancellation,
        owner_cancellation: Cancellation,
    ) -> Result<
        AuthorizedLiveSession<<P::Transport as PreparedLiveTransport>::Session, R, A>,
        AgentFailure,
    > {
        let intent = LiveDispatchIntent {
            actor: plan.actor.clone(),
            voice_session_id: request.voice_session_id,
            surface_session_id: request.surface_session_id,
            media_lease_id: request.media_lease_id,
            plan: plan.clone(),
            projection_digest: request.projection.digest()?,
            reservation: request.reservation,
        };
        intent.validate()?;
        let dispatch = ModelDispatchRequest {
            person_id: plan.actor.person_id,
            device_id: plan.actor.device_id.clone(),
            plan_id: plan.operation_id,
            binding_digest: plan.binding_digest.0,
            projection_ref: request.projection.projection_id,
            projection_revision: request.projection.projection_revision,
            coverage: request.projection.coverage.clone(),
            memories: request.projection.memories.clone(),
            input_data_classes: request.projection.input_data_classes.clone(),
            purpose: LIVE_PRESENTATION_PURPOSE.into(),
            consumer: LIVE_PRESENTATION_CONSUMER.into(),
            target,
            deadline: lifetime.deadline(),
            cancellation: lifetime.cancellation().clone(),
        };
        let permit = admit_model_dispatch(
            dispatch,
            self.resolver.clone(),
            self.authority.clone(),
            lifetime.clone(),
        )
        .await?;
        let mut tokens = request.reservation.tokens;
        let mut cost = request.reservation.cost_micros;
        let mut attempt =
            lifetime
                .budget()
                .begin_model_attempt(plan.operation_id, &mut tokens, &mut cost)?;
        let canonical = CanonicalLiveStart {
            actor: plan.actor.clone(),
            voice_session_id: request.voice_session_id,
            media_lease_id: request.media_lease_id,
            plan: plan.clone(),
            context: request.projection.payload,
            reservation: ModelReservationCeiling {
                tokens,
                cost_micros: cost,
            },
            deadline: lifetime.deadline(),
            cancellation: lifetime.cancellation().clone(),
        };
        transport.validate_start(&canonical)?;
        if !custody.claim(&intent, lifetime).await? {
            return Err(AgentFailure::Conflict);
        }
        // Source changes while waiting for durable custody cannot cross handoff.
        let fence = Arc::new(consume_model_dispatch(permit).await?);
        let admitted = AdmittedDispatchTarget::from_consumed(&fence);
        if !admitted.matches(&plan.binding_digest, plan.boundary) {
            return Err(AgentFailure::PolicyDenied);
        }
        attempt.mark_dispatched();
        let session = Arc::new(transport.start(canonical, admitted).await?);
        let mut pending = PendingLiveSession(Some(session.clone()));
        bounded_revalidate(&fence, lifetime).await?;
        if preparation_cancellation.is_cancelled() || owner_cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let failure = Arc::new(Mutex::new(None));
        let monitor = monitor(
            session.clone(),
            fence.clone(),
            lifetime.clone(),
            preparation_cancellation.clone(),
            owner_cancellation.clone(),
            failure.clone(),
        );
        pending.0.take();
        Ok(AuthorizedLiveSession {
            plan,
            transport: session,
            fence,
            scope: lifetime.clone(),
            preparation_cancellation,
            owner_cancellation,
            attempt: Some(attempt),
            monitor,
            failure,
        })
    }
}

#[cfg(test)]
#[path = "live_dispatch_tests.rs"]
mod tests;
