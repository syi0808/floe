//! Exact paired live schema; signaling/credentials are never product contracts.
use floe_agent_contract::AgentFailure;
use floe_model_contract::{LiveModelCapabilities, LiveProviderEvent, LiveTransportLimits};
use serde::Deserialize;
use uuid::Uuid;

pub(super) const MAX_REQUEST: usize = 16 * 1024 * 1024;
pub(super) const MAX_RESPONSE: usize = 512 * 1024;
pub(super) fn decode<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, AgentFailure> {
    crate::gateway::json::strict_json_bytes(data, MAX_RESPONSE)
        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    serde_json::from_slice(data).map_err(|_| AgentFailure::ServerModelInvalidOutput)
}
pub(super) fn hex(value: &str) -> bool {
    crate::gateway::inference_wire::valid_hex(value, 64)
}
pub(super) fn id(value: &str) -> Result<Uuid, AgentFailure> {
    let id = Uuid::parse_str(value).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    if id.is_nil() || id.to_string() != value {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    Ok(id)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capability {
    pub schema_version: u32,
    pub purpose: String,
    pub status: String,
    pub capability_revision: Option<String>,
    pub profile: Option<Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    pub provider_id: String,
    pub model_id: String,
    pub endpoint: String,
    pub capabilities: LiveModelCapabilities,
    pub limits: LiveTransportLimits,
    pub protocol_source: String,
    pub experimental: bool,
}
impl Capability {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let invalid = AgentFailure::ServerModelInvalidOutput;
        if self.schema_version != 1 || self.purpose != "live_presentation" {
            return Err(invalid);
        }
        match self.status.as_str() {
            "not_configured" | "disabled"
                if self.capability_revision.is_none() && self.profile.is_none() =>
            {
                Ok(())
            }
            "available" => {
                let revision = self.capability_revision.as_deref().ok_or(invalid)?;
                let p = self.profile.as_ref().ok_or(invalid)?;
                if !hex(revision)
                    || p.provider_id != "codex_oauth"
                    || p.model_id != "gpt-live-1-codex"
                    || p.endpoint != "https://chatgpt.com/backend-api/codex"
                    || !p.experimental
                    || p.protocol_source != "openai/codex@0df4aa67781ae45a3c97befa4db48b734afe46d9"
                {
                    return Err(invalid);
                }
                p.capabilities.validate().map_err(|_| invalid)?;
                p.limits.validate().map_err(|_| invalid)?;
                if p.limits.initial_context_bytes > 2 * 1024 * 1024
                    || p.limits.append_bytes > 65_536
                    || p.limits.local_session_duration_ms > 3_600_000
                {
                    return Err(invalid);
                }
                Ok(())
            }
            _ => Err(invalid),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ticket {
    pub schema_version: u32,
    pub ticket_id: String,
    pub capability_revision: String,
    pub expires_at_unix_ms: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Status {
    pub schema_version: u32,
    pub ticket_id: String,
    pub phase: String,
    pub request_id: Option<String>,
    pub expires_at_unix_ms: u64,
    pub attempted: bool,
    pub failure: Option<String>,
    pub answer: Option<String>,
}
impl Status {
    pub fn validate(&self, ticket: &str, request: &str) -> Result<(), AgentFailure> {
        if self.schema_version != 1
            || self.ticket_id != ticket
            || self.request_id.as_deref() != Some(request)
            || self.expires_at_unix_ms == 0
            || self.expires_at_unix_ms > 9_007_199_254_740_991
            || !matches!(
                self.phase.as_str(),
                "starting" | "ready" | "failed" | "closing" | "closed_local"
            )
            || self
                .failure
                .as_ref()
                .is_some_and(|f| f.is_empty() || f.len() > 128)
            || self.answer.as_ref().is_some_and(|a| a.len() > 65_536)
        {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        if self.phase == "ready" && (!self.attempted || self.failure.is_some()) {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Event {
    pub schema_version: u32,
    pub ticket_id: String,
    pub sequence: u64,
    pub digest: String,
    pub event: LiveProviderEvent,
}
impl Event {
    pub fn validate(&self, ticket: &str) -> Result<(), AgentFailure> {
        if self.schema_version != 1
            || self.ticket_id != ticket
            || self.sequence == 0
            || self.sequence > 9_007_199_254_740_991
            || !hex(&self.digest)
        {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        // Native activity and locally synthesized disconnection are not Go wire facts.
        if matches!(
            self.event,
            LiveProviderEvent::InputActivity { .. }
                | LiveProviderEvent::OutputActivity { .. }
                | LiveProviderEvent::Ended { .. }
        ) {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        self.event
            .validate()
            .map_err(|_| AgentFailure::ServerModelInvalidOutput)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ack {
    pub schema_version: u32,
    pub acknowledged_sequence: u64,
    pub digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Write {
    pub schema_version: u32,
    pub operation_id: String,
    pub phase: String,
    pub attempted: bool,
    pub completed_frames: u64,
    pub failure: Option<String>,
}
impl Write {
    pub fn validate(&self, operation: &str) -> Result<(), AgentFailure> {
        if self.schema_version != 1
            || self.operation_id != operation
            || self.completed_frames > 1024
            || !matches!(
                self.phase.as_str(),
                "pending" | "written" | "not_sent" | "uncertain"
            )
            || self
                .failure
                .as_ref()
                .is_some_and(|f| f.is_empty() || f.len() > 128)
        {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        if self.phase == "written"
            && (!self.attempted || self.completed_frames == 0 || self.failure.is_some())
            || self.phase == "not_sent" && (self.attempted || self.completed_frames != 0)
            || self.phase == "uncertain" && !self.attempted
        {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        Ok(())
    }
}
