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
    pub input_data_classes: Vec<floe_context_contract::DataClass>,
    pub reservation: ModelReservationCeiling,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}
pub trait PreparedLiveTransport: Send + 'static {
    type Session: LiveEventTransport + Send + Sync + 'static;
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

/// A call-local retained transport observation, not a Conversation identity.
/// The adapter keeps this exact value until acknowledgement; no crash-durability
/// or provider acceptance is implied by its sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveDelivery {
    pub sequence: u64,
    pub event: floe_model_contract::LiveProviderEvent,
}
impl LiveDelivery {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.sequence == 0 || self.sequence > 9_007_199_254_740_991 {
            return Err(AgentFailure::InvalidInput);
        }
        self.event
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)
    }
}
/// A real transport boundary. Observe is cancellation-safe: until ACK, repeated
/// observation returns the same head. ACK is exact, ordered and replay-safe even
/// after response loss. One continuously owned adapter reader supplies it; UI
/// request cancellation cannot cancel that reader or consume a pending event.
pub trait LiveEventTransport: LiveSessionControl + Send + Sync {
    /// Pure whole-batch codec preflight, before durable intent or provider I/O.
    fn validate_append(
        &self,
        payload: &floe_model_contract::LiveAppendPayload,
    ) -> Result<(), AgentFailure>;
    /// One shot on this exact call. The owner retains the durable intent if this
    /// future is interrupted; a dropped response cannot justify recreating it.
    fn append<'a>(
        &'a self,
        payload: &'a floe_model_contract::LiveAppendPayload,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;

    fn observe<'a>(
        &'a self,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<LiveDelivery, AgentFailure>>;
    fn acknowledge<'a>(
        &'a self,
        sequence: u64,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
}

/// One Context-owned result projection for an already selected call. No raw
/// caller text can be substituted for the projection payload at this boundary.
pub struct LiveAppendRequest {
    pub operation_id: Uuid,
    pub kind: floe_model_contract::LiveAppendKind,
    pub delegation_id: Option<String>,
    pub projection: LiveContextProjection,
}
#[derive(Clone, Debug)]
pub struct LiveAppendIntent {
    pub actor: OwnerActor,
    pub voice_session_id: Uuid,
    pub surface_session_id: Uuid,
    pub plan_id: Uuid,
    pub source: floe_context_contract::LiveProjectionSource,
    pub projection_digest: [u8; 32],
    pub payload: floe_model_contract::LiveAppendPayload,
}
impl LiveAppendIntent {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        self.source.validate()?;
        self.payload
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if self.voice_session_id.is_nil()
            || self.surface_session_id.is_nil()
            || self.plan_id.is_nil()
            || self.projection_digest == [0; 32]
            || self.source.through().session_id != self.surface_session_id
            || !matches!(
                self.source,
                floe_context_contract::LiveProjectionSource::SurfaceResult { .. }
            )
            || matches!(
                self.payload.kind,
                floe_model_contract::LiveAppendKind::Instruction
            )
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
pub trait LiveAppendCustody: Send + Sync {
    /// Fresh durable intent only. Exact replay, uncertain commit and restart
    /// never authorize another provider write. The Voice owner binds the result
    /// and delegation to canonical Work before returning true.
    fn claim_append<'a>(
        &'a self,
        intent: &'a LiveAppendIntent,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>>;
}
/// Local transport evidence only: neither variant proves remote acceptance or
/// audio playback. An uncertain write must never be blindly retried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveAppendWrite {
    Written,
    Uncertain(AgentFailure),
}
