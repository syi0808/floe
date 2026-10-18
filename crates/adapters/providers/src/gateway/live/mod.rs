//! Paired live control + private native WebRTC signaling. Credentials remain in
//! the existing Gateway store; Inference owns selection and source authority.
mod control;
mod session;
mod wire;
use crate::gateway::{
    credentials::GatewayCredentialStore, http::GatewayHttpTransport, inference_wire,
};
use control::{Control, TicketLease, success};
use floe_access::ModelDispatchTarget;
use floe_agent_contract::{AgentFailure, BoxFuture, OwnerActor, ProcessingBoundary};
use floe_execution::ExecutionScope;
use floe_inference::{
    AdmittedDispatchTarget, CanonicalLiveStart, LIVE_PRESENTATION_CONSUMER,
    LIVE_PRESENTATION_PURPOSE, LiveModelPlanRequest, LiveModelProvider, LocalAvailabilityReason,
    LocalObservation, ModelObservationError, ObservedLiveModelCapability, PreparedLiveModelProfile,
    PreparedLiveTransport, PrimaryAbsence, PrimaryObservation,
};
use floe_model_contract::{LiveModelCapabilities, LiveTransportLimits};
use floe_native::live_media::{LiveMediaBinding, LiveMediaRegistry};
pub use session::GatewayLiveSession;
use std::sync::Arc;
use tokio::time::Instant;

pub struct GatewayLiveModelProvider {
    store: GatewayCredentialStore,
    media: Arc<dyn LiveMediaRegistry>,
}
impl GatewayLiveModelProvider {
    pub fn new(store: GatewayCredentialStore, media: Arc<dyn LiveMediaRegistry>) -> Self {
        Self { store, media }
    }
    async fn observe(
        &self,
        request: &LiveModelPlanRequest,
        scope: &ExecutionScope,
    ) -> Result<
        PrimaryObservation<PreparedLiveModelProfile<PreparedGatewayLive>>,
        ModelObservationError,
    > {
        request
            .validate()
            .map_err(|_| ModelObservationError::InvalidIdentity)?;
        let Some(connection) = self
            .store
            .load(
                &request.actor.person_id.to_string(),
                &request.actor.device_id,
            )
            .await
            .map_err(super::inference::credential_observation)?
        else {
            return Ok(PrimaryObservation::Absent(
                PrimaryAbsence::NoGatewayConfigured,
            ));
        };
        let control = Arc::new(Control {
            store: self.store.clone(),
            connection,
            http: GatewayHttpTransport::new().map_err(super::inference::observation_failure)?,
        });
        let (status, bytes) = control
            .get("/v1/live-model", scope.deadline(), scope.cancellation())
            .await
            .map_err(super::inference::observation_failure)?;
        success(status, &bytes).map_err(super::inference::observation_failure)?;
        let capability: wire::Capability =
            wire::decode(&bytes).map_err(|_| ModelObservationError::InvalidInventory)?;
        capability
            .validate()
            .map_err(|_| ModelObservationError::InvalidInventory)?;
        match capability.status.as_str() {
            "not_configured" => {
                return Ok(PrimaryObservation::Absent(
                    PrimaryAbsence::PurposeNotConfigured,
                ));
            }
            "disabled" => return Ok(PrimaryObservation::Absent(PrimaryAbsence::PurposeDisabled)),
            _ => {}
        }
        let profile = capability
            .profile
            .ok_or(ModelObservationError::InvalidInventory)?;
        if !profile
            .capabilities
            .includes(&request.required_capabilities)
        {
            return Err(ModelObservationError::CapabilityUnavailable);
        }
        let revision = capability
            .capability_revision
            .ok_or(ModelObservationError::InvalidInventory)?;
        let bytes = control
            .post(
                "/v1/live/prepare",
                &serde_json::json!({"schema_version":1,"capability_revision":revision}),
                scope.deadline(),
                scope.cancellation(),
            )
            .await
            .map_err(super::inference::observation_failure)?;
        let ticket: wire::Ticket =
            wire::decode(&bytes).map_err(|_| ModelObservationError::InvalidInventory)?;
        wire::id(&ticket.ticket_id).map_err(|_| ModelObservationError::InvalidInventory)?;
        let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
        if ticket.schema_version != 1
            || ticket.capability_revision != revision
            || ticket.expires_at_unix_ms <= now
            || ticket.expires_at_unix_ms > now.saturating_add(35_000)
        {
            return Err(ModelObservationError::InvalidInventory);
        }
        let binding = control.connection.binding.clone();
        let binding_digest = control.connection.binding_digest();
        let selection_commitment = inference_wire::selection_commitment(&revision)?;
        let expires_at_unix_ms = ticket.expires_at_unix_ms;
        let ticket = Arc::new(TicketLease {
            control,
            id: ticket.ticket_id,
        });
        Ok(PrimaryObservation::Available(PreparedLiveModelProfile {
            capability: ObservedLiveModelCapability {
                purpose: LIVE_PRESENTATION_PURPOSE.into(),
                consumer: LIVE_PRESENTATION_CONSUMER.into(),
                capabilities: profile.capabilities.clone(),
                limits: profile.limits.clone(),
                boundary: ProcessingBoundary::Gateway,
                binding_digest,
                selection_commitment,
            },
            target: ModelDispatchTarget::Gateway { expected: binding },
            transport: PreparedGatewayLive {
                ticket,
                revision,
                actor: request.actor.clone(),
                capabilities: profile.capabilities,
                limits: profile.limits,
                media: self.media.clone(),
                expires_at_unix_ms,
            },
        }))
    }
}
// A preparation is intentionally short-lived; clock checking does not extend the
// server ticket. The server remains authoritative if clocks drift.

impl LiveModelProvider for GatewayLiveModelProvider {
    type Transport = PreparedGatewayLive;
    fn observe_primary_live<'a>(
        &'a self,
        request: &'a LiveModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<
            PrimaryObservation<PreparedLiveModelProfile<Self::Transport>>,
            ModelObservationError,
        >,
    > {
        Box::pin(self.observe(request, scope))
    }
    fn observe_local_live<'a>(
        &'a self,
        _: &'a LiveModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<LocalObservation<PreparedLiveModelProfile<Self::Transport>>, ModelObservationError>,
    > {
        Box::pin(async {
            Ok(LocalObservation::Unavailable(
                LocalAvailabilityReason::Unsupported,
            ))
        })
    }
}
pub struct PreparedGatewayLive {
    ticket: Arc<TicketLease>,
    revision: String,
    actor: OwnerActor,
    capabilities: LiveModelCapabilities,
    limits: LiveTransportLimits,
    media: Arc<dyn LiveMediaRegistry>,
    expires_at_unix_ms: u64,
}
impl PreparedLiveTransport for PreparedGatewayLive {
    type Session = GatewayLiveSession;
    fn validate_start(&self, request: &CanonicalLiveStart) -> Result<(), AgentFailure> {
        request.plan.validate()?;
        if request.actor != self.actor
            || request.plan.actor != self.actor
            || request.voice_session_id.is_nil()
            || request.media_lease_id.is_nil()
            || request.plan.boundary != ProcessingBoundary::Gateway
            || request.plan.binding_digest != self.ticket.control.connection.binding_digest()
            || request.plan.selection_commitment
                != inference_wire::selection_commitment(&self.revision)
                    .map_err(AgentFailure::from)?
            || request.plan.limits != self.limits
            || request.plan.capabilities != self.capabilities
            || request.context.byte_len() as u64 > self.limits.initial_context_bytes
            || request.cancellation.is_cancelled()
            || request.deadline <= Instant::now()
            || chrono::Utc::now().timestamp_millis().max(0) as u64 >= self.expires_at_unix_ms
        {
            return Err(AgentFailure::PolicyDenied);
        }
        session::data_classes(&request.input_data_classes)?;
        let binding = LiveMediaBinding {
            actor: request.actor.clone(),
            voice_session_id: request.voice_session_id,
            media_lease_id: request.media_lease_id,
            plan_id: request.plan.operation_id,
        };
        let lease = self.media.lease(&binding)?;
        if lease.binding() != &binding {
            return Err(AgentFailure::PolicyDenied);
        }
        lease.validate_current()
    }
    fn start(
        self,
        request: CanonicalLiveStart,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'static, Result<Self::Session, AgentFailure>> {
        Box::pin(async move {
            self.validate_start(&request)?;
            if !target.matches(&request.plan.binding_digest, ProcessingBoundary::Gateway) {
                return Err(AgentFailure::PolicyDenied);
            }
            session::start(self, request).await
        })
    }
}

#[cfg(test)]
mod tests;
