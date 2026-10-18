//! Live I/O is a selected model transport, independent of generation loops.
//! Native/Go adapters retain SDP, credentials and media handles privately.
use crate::AdmittedDispatchTarget;
use floe_agent_contract::{AgentFailure, OwnerActor};
use floe_context_contract::LiveContextProjection;
use floe_execution::{BoxFuture, Cancellation, ExecutionScope, budget::ModelReservationCeiling};
use floe_model_contract::{LiveContextPayload, LiveModelPlan};
use tokio::time::Instant;
use uuid::Uuid;

/// Correlation for the Voice owner's already-persisted start. A digest is not
/// authority: the owner port compares it with canonical intent and native state.
#[derive(Clone, Debug)]
pub struct LiveDispatchIntent {
    pub actor: OwnerActor,
    pub voice_session_id: Uuid,
    pub surface_session_id: Uuid,
    pub media_lease_id: Uuid,
    pub plan: LiveModelPlan,
    pub projection_digest: [u8; 32],
    pub reservation: ModelReservationCeiling,
}
impl LiveDispatchIntent {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        self.plan.validate()?;
        self.reservation.validate()?;
        if self.actor != self.plan.actor
            || self.voice_session_id.is_nil()
            || self.surface_session_id.is_nil()
            || self.media_lease_id.is_nil()
            || self.projection_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
pub trait LiveDispatchCustody: Send + Sync {
    /// The canonical Voice generation's lifetime, independent of UI/request
    /// cancellation. Retirement must fence even an independent/local model call.
    fn lifetime_cancellation(&self, actor: &OwnerActor) -> Result<Cancellation, AgentFailure>;
    /// Returns true only for a fresh durable dispatch transition. An existing
    /// transition, response loss or restart can never authorize another start.
    fn claim<'a>(
        &'a self,
        intent: &'a LiveDispatchIntent,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>>;
}

pub struct LiveStartRequest {
    pub voice_session_id: Uuid,
    pub surface_session_id: Uuid,
    pub media_lease_id: Uuid,
    pub projection: LiveContextProjection,
    pub reservation: ModelReservationCeiling,
}
/// Only bounded provider input crosses this boundary, never source proofs or
/// the projection's internal provenance manifest. No raw media or credential.
#[derive(Clone, Debug)]
pub struct CanonicalLiveStart {
    pub actor: OwnerActor,
    pub voice_session_id: Uuid,
    pub media_lease_id: Uuid,
    pub plan: LiveModelPlan,
    pub context: LiveContextPayload,
    pub reservation: ModelReservationCeiling,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}
pub trait PreparedLiveTransport: Send + 'static {
    type Session: LiveSessionControl + Send + Sync + 'static;
    /// Pure local preflight. No provider I/O, microphone start or credential
    /// mutation is permitted before the durable custody and Access fences.
    fn validate_start(&self, request: &CanonicalLiveStart) -> Result<(), AgentFailure>;
    fn start(
        self,
        request: CanonicalLiveStart,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'static, Result<Self::Session, AgentFailure>>;
}
pub trait LiveSessionControl {
    /// Immediate local fence; it must not wait for network or storage. The
    /// composed media/control adapter permanently fences this lease against
    /// late unmute/write commands, then settles actual stop receipts separately. This is not proof of provider close or zero usage.
    fn fence(&self);
}
