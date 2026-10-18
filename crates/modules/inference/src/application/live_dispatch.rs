//! One selected live handoff using the same Access and budget owners as
//! generation. Caller observation is separate from the bounded owner lifetime.
#[cfg(test)]
use crate::LiveSessionControl;
use crate::{
    AdmittedDispatchTarget, CanonicalLiveStart, InferenceService, LIVE_PRESENTATION_CONSUMER,
    LIVE_PRESENTATION_PURPOSE, LiveDelivery, LiveDispatchCustody, LiveDispatchIntent,
    LiveEventTransport, LiveModelProvider, LiveStartRequest, PreparedLiveSelection,
    PreparedLiveTransport,
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
/// exposed: event release is guarded and retained; append remains a separate
/// integration gate with cumulative source provenance.
pub struct AuthorizedLiveSession {
    plan: LiveModelPlan,
    transport: Arc<dyn LiveEventTransport>,
    fence: Arc<dyn LiveSourceFence>,
    scope: ExecutionScope,
    preparation_cancellation: Cancellation,
    owner_cancellation: Cancellation,
    attempt: Option<BudgetAttempt>,
    monitor: JoinHandle<()>,
    failure: Arc<Mutex<Option<AgentFailure>>>,
    pending_delivery: Option<LiveDelivery>,
    acknowledged_delivery: u64,
    terminal_usage: Option<LiveUsageObservation>,
}
/// Canonical ledger custody, including an overrun that must not erase usage.
#[derive(Clone, Copy, Debug)]
pub struct LiveSettlement {
    pub receipt: ModelAttemptReceipt,
    pub failure: Option<AgentFailure>,
}
impl AuthorizedLiveSession {
    pub fn plan(&self) -> &LiveModelPlan {
        &self.plan
    }
    pub fn failure(&self) -> Option<AgentFailure> {
        *self.failure.lock().unwrap()
    }
    fn fail(&self, failure: AgentFailure) -> AgentFailure {
        // Publish the causal failure before derivative cancellation/local fence.
        let cause = *self.failure.lock().unwrap().get_or_insert(failure);
        self.scope
            .cancellation()
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        self.transport.fence();
        cause
    }
    pub fn settle_usage(
        &mut self,
        usage: LiveUsageObservation,
    ) -> Result<LiveSettlement, AgentFailure> {
        // Accounting remains independent of source/result release permissions.
        self.transport.fence();
        self.monitor.abort();
        usage.validate().map_err(|_| AgentFailure::InvalidInput)?;
        let usage = match self.terminal_usage {
            Some(observed) => LiveUsageObservation {
                tokens: observed.tokens.or(usage.tokens),
                cost_micros: observed.cost_micros.or(usage.cost_micros),
            },
            None => usage,
        };
        let result = self
            .attempt
            .take()
            .ok_or(AgentFailure::Conflict)?
            .settle_observed(usage.tokens, usage.cost_micros);
        match result {
            Ok(receipt) => Ok(LiveSettlement {
                receipt,
                failure: None,
            }),
            Err(failure) => self
                .scope
                .budget()
                .model_attempt_receipt(self.plan.operation_id)
                .map(|receipt| LiveSettlement {
                    receipt,
                    failure: Some(failure),
                })
                .ok_or(failure),
        }
    }
    pub fn finish(
        mut self,
        usage: LiveUsageObservation,
    ) -> Result<ModelAttemptReceipt, AgentFailure> {
        let settlement = self.settle_usage(usage)?;
        match settlement.failure {
            Some(failure) => Err(failure),
            None => Ok(settlement.receipt),
        }
    }
}
impl AuthorizedLiveSession {
    pub async fn revalidate(&self) -> Result<(), AgentFailure> {
        if let Some(failure) = self.failure() {
            return Err(failure);
        }
        let result = if self.owner_cancellation.is_cancelled() {
            Err(AgentFailure::Interrupted)
        } else if self.preparation_cancellation.is_cancelled() {
            Err(AgentFailure::Cancelled)
        } else {
            bounded_revalidate(&self.fence, &self.scope).await
        };
        match result {
            Err(failure) => Err(self.fail(failure)),
            // A concurrent monitor may have retired the call while this read
            // succeeded. A later successful read cannot revive that call.
            Ok(()) => self.failure().map_or(Ok(()), Err),
        }
    }
}
impl AuthorizedLiveSession {
    /// Source-authorized observation, retained locally until the owner confirms
    /// custody. A dropped observer future never clears this pending delivery.
    pub async fn receive(&mut self) -> Result<LiveDelivery, AgentFailure> {
        if self.attempt.is_none() {
            return Err(AgentFailure::Conflict);
        }
        self.revalidate().await?;
        if self.pending_delivery.is_none() {
            if self.terminal_usage.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let observation = self.scope.child_scope(self.scope.deadline(), 0, 0, None);
            let delivery = match observation.run(self.transport.observe(&observation)).await {
                Ok(delivery) => delivery,
                Err(error) => {
                    return Err(self.fail(error));
                }
            };
            if delivery.validate().is_err() || delivery.sequence != self.acknowledged_delivery + 1 {
                return Err(self.fail(AgentFailure::InvalidInput));
            }
            if let floe_model_contract::LiveProviderEvent::Ended { usage, .. } = &delivery.event {
                // Accounting is retained even if source release is later denied.
                // Local fencing does not fabricate a native Stop acknowledgement.
                self.terminal_usage = Some(*usage);
                self.transport.fence();
            }
            self.pending_delivery = Some(delivery);
        }
        self.revalidate().await?;
        Ok(self.pending_delivery.as_ref().unwrap().clone())
    }
    /// Call only after canonical local custody. ACK loss retains the same local
    /// head, allowing an exact transport retry; it never advances to later input.
    pub async fn acknowledge(&mut self, sequence: u64) -> Result<(), AgentFailure> {
        if sequence == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        if sequence <= self.acknowledged_delivery {
            return Ok(());
        }
        if self.pending_delivery.as_ref().map(|d| d.sequence) != Some(sequence) {
            return Err(AgentFailure::Conflict);
        }
        let observation = self.scope.child_scope(self.scope.deadline(), 0, 0, None);
        observation
            .run(self.transport.acknowledge(sequence, &observation))
            .await?;
        self.acknowledged_delivery = sequence;
        self.pending_delivery = None;
        Ok(())
    }
}

impl Drop for AuthorizedLiveSession {
    fn drop(&mut self) {
        self.scope
            .cancellation()
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        // Local fencing precedes accounting/storage settlement. An unsettled
        // dispatched attempt drops to unknown, never a zero-usage claim.
        self.transport.fence();
        self.monitor.abort();
        // A terminal observation already received from the transport remains
        // known in the Execution ledger even if its observer drops this guard.
        if let Some(usage) = self.terminal_usage
            && let Some(attempt) = self.attempt.take()
        {
            let _ = attempt.settle_observed(usage.tokens, usage.cost_micros);
        }
    }
}

// Covers cancellation/panic between a returned transport and ready publication.
struct PendingLiveSession(Option<Arc<dyn LiveEventTransport>>);
impl Drop for PendingLiveSession {
    fn drop(&mut self) {
        if let Some(session) = &self.0 {
            session.fence();
        }
    }
}

// Private type erasure retains the one Access-owned fence. Voice consumers do
// not import resolver/Gateway policy types or get a second authorization path.
trait LiveSourceFence: Send + Sync {
    fn revalidate(&self) -> floe_execution::BoxFuture<'_, Result<(), AgentFailure>>;
}
impl<R: DependencyResolver, A: GatewayAdmission> LiveSourceFence for ModelDispatchFence<R, A> {
    fn revalidate(&self) -> floe_execution::BoxFuture<'_, Result<(), AgentFailure>> {
        Box::pin(revalidate_model_dispatch(self))
    }
}
async fn bounded_revalidate(
    fence: &Arc<dyn LiveSourceFence>,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    let observation = scope.child_scope(scope.deadline(), 0, 0, None);
    observation
        .run(async {
            tokio::time::timeout(REVALIDATE_TIMEOUT, fence.revalidate())
                .await
                .map_err(|_| AgentFailure::DeadlineExceeded)?
        })
        .await
}
fn monitor(
    transport: Arc<dyn LiveEventTransport>,
    fence: Arc<dyn LiveSourceFence>,
    scope: ExecutionScope,
    preparation_cancellation: Cancellation,
    owner_cancellation: Cancellation,
    failure: Arc<Mutex<Option<AgentFailure>>>,
) -> JoinHandle<()> {
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
    ) -> Result<AuthorizedLiveSession, AgentFailure> {
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
    ) -> Result<AuthorizedLiveSession, AgentFailure> {
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
        let session: Arc<dyn LiveEventTransport> =
            Arc::new(transport.start(canonical, admitted).await?);
        let fence: Arc<dyn LiveSourceFence> = fence;
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
            pending_delivery: None,
            acknowledged_delivery: 0,
            terminal_usage: None,
        })
    }
}

#[cfg(test)]
#[path = "live_dispatch_tests.rs"]
mod tests;
