//! Long-lived model capabilities. No provider wire, audio frames or credentials.
use crate::{ModelBindingDigest, ModelContractError, ModelSelectionCommitment, ProcessingBoundary};
use floe_kernel::OwnerActor;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveModelCapability {
    DuplexAudio,
    FinalInputTranscript,
    FinalOutputTranscript,
    ClientDelegation,
    ContextAppend,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LiveModelCapabilities(pub Vec<LiveModelCapability>);
impl LiveModelCapabilities {
    pub fn conversational() -> Self {
        Self(vec![
            LiveModelCapability::DuplexAudio,
            LiveModelCapability::FinalInputTranscript,
            LiveModelCapability::FinalOutputTranscript,
            LiveModelCapability::ClientDelegation,
            LiveModelCapability::ContextAppend,
        ])
    }
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if self.0.is_empty() || self.0.len() > 5 || self.0.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ModelContractError::Invalid);
        }
        Ok(())
    }
    pub fn includes(&self, required: &Self) -> bool {
        required
            .0
            .iter()
            .all(|capability| self.0.contains(capability))
    }
}

/// Effective bounds enforced by the prepared adapter/host, not a claim about
/// the provider model's context window, remote expiry or billing policy.
/// Zero never means unlimited. Provider capacity may remain unknown separately.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveTransportLimits {
    pub initial_context_bytes: u64,
    /// Maximum whole logical append payload; provider frame chunking is private
    /// adapter policy and may use a smaller wire-frame bound.
    pub append_bytes: u64,
    pub local_session_duration_ms: u64,
}
impl LiveTransportLimits {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if [
            self.initial_context_bytes,
            self.append_bytes,
            self.local_session_duration_ms,
        ]
        .iter()
        .any(|value| *value == 0 || *value > 9_007_199_254_740_991)
        {
            return Err(ModelContractError::Bounds);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_capabilities_have_no_generation_chat_or_tool_requirement() {
        let caps = LiveModelCapabilities::conversational();
        caps.validate().unwrap();
        assert!(caps.includes(&LiveModelCapabilities(vec![
            LiveModelCapability::ClientDelegation
        ])));
        assert!(!LiveModelCapabilities(vec![LiveModelCapability::DuplexAudio]).includes(&caps));
        let json = serde_json::to_string(&caps).unwrap();
        assert!(!json.contains("tool_proposals"));
        assert_eq!(
            serde_json::from_str::<LiveModelCapabilities>(&json).unwrap(),
            caps
        );
    }
    #[test]
    fn duplicate_or_unordered_capabilities_are_invalid() {
        for caps in [
            vec![],
            vec![LiveModelCapability::DuplexAudio; 2],
            vec![
                LiveModelCapability::ContextAppend,
                LiveModelCapability::DuplexAudio,
            ],
        ] {
            assert_eq!(
                LiveModelCapabilities(caps).validate(),
                Err(ModelContractError::Invalid)
            );
        }
    }
    #[test]
    fn unknown_limits_are_not_unlimited_and_wire_rejects_provider_fields() {
        let mut limits = LiveTransportLimits {
            initial_context_bytes: 16_384,
            append_bytes: 500,
            local_session_duration_ms: 60_000,
        };
        limits.validate().unwrap();
        limits.append_bytes = 0;
        assert_eq!(limits.validate(), Err(ModelContractError::Bounds));
        limits.append_bytes = u64::MAX;
        assert_eq!(limits.validate(), Err(ModelContractError::Bounds));
        assert!(serde_json::from_str::<LiveTransportLimits>(r#"{"initial_context_bytes":1,"append_bytes":1,"local_session_duration_ms":1,"token":"not-an-owner-field"}"#).is_err());
    }
}

/// A non-secret planning snapshot, not a microphone or dispatch capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveModelPlan {
    pub operation_id: Uuid,
    pub actor: OwnerActor,
    pub capabilities: LiveModelCapabilities,
    pub limits: LiveTransportLimits,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
    pub selection_commitment: ModelSelectionCommitment,
}

impl LiveModelPlan {
    pub fn validate(&self) -> Result<(), floe_kernel::AgentFailure> {
        self.actor.validate()?;
        if self.operation_id.is_nil() || self.binding_digest.0 == [0; 32] {
            return Err(floe_kernel::AgentFailure::InvalidInput);
        }
        self.capabilities
            .validate()
            .map_err(|_| floe_kernel::AgentFailure::InvalidInput)?;
        self.limits
            .validate()
            .map_err(|_| floe_kernel::AgentFailure::InvalidInput)?;
        self.selection_commitment.validate()
    }
}

/// Bounded provider-facing context text. Authorization/provenance is carried
/// separately and must never be serialized wholesale to the provider.
#[derive(Clone, Eq, PartialEq)]
pub struct LiveContextPayload(String);
pub const MAX_LIVE_CONTEXT_BYTES: usize = 2 * 1024 * 1024;
impl LiveContextPayload {
    pub fn new(value: String) -> Result<Self, ModelContractError> {
        if value.trim().is_empty() {
            return Err(ModelContractError::Invalid);
        }
        if value.len() > MAX_LIVE_CONTEXT_BYTES {
            return Err(ModelContractError::Bounds);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn byte_len(&self) -> usize {
        self.0.len()
    }
}
impl std::fmt::Debug for LiveContextPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveContextPayload")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}
