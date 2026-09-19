//! Canonical provider port: non-secret profile facts plus an opaque prepared
//! transport whose credential/endpoint internals stay in the adapter.
//!
//! Inference observes profiles, plans a route and dispatches through Access.
//! It never sees a bearer, a base URL or an arbitrary endpoint.

use floe_agent_contract::{AgentFailure, AllowedCatalog, ContextEnvelope, ModelStep};
use floe_context_contract::DataClass;
use floe_execution::Cancellation;
use tokio::time::Instant;
use uuid::Uuid;

use crate::api::ModelProfile;

/// What one approved model attempt needs. No Session, Task, ledger,
/// bearer or endpoint travels here.
#[derive(Clone, Debug)]
pub struct CanonicalModelRequest {
    pub attempt_id: Uuid,
    pub envelope: ContextEnvelope,
    pub catalog: AllowedCatalog,
    pub input_data_classes: Vec<DataClass>,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

impl CanonicalModelRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.attempt_id.is_nil()
            || self.remaining_tokens == 0
            || self.max_output_bytes == 0
            || self.max_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.envelope
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(())
    }
}

/// What prepared transport returns, without loss.
#[derive(Clone, Debug)]
pub struct CanonicalModelResponse {
    pub output: Vec<ModelStep>,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

impl CanonicalModelResponse {
    pub fn validate(&self, max_output_bytes: usize) -> Result<(), AgentFailure> {
        if self.output.is_empty()
            || self.output.len() > 16
            || self
                .output
                .iter()
                .any(|step| !valid_step(step, max_output_bytes))
        {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        Ok(())
    }
}

fn valid_step(step: &ModelStep, max_output_bytes: usize) -> bool {
    match step {
        ModelStep::Preamble { text } | ModelStep::Answer { text, .. } => {
            !text.trim().is_empty() && text.len() <= max_output_bytes
        }
        ModelStep::CallTool {
            tool_id,
            definition_revision,
            input,
        } => {
            !tool_id.is_empty()
                && *definition_revision > 0
                && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(input).is_ok()
        }
        ModelStep::Delegate {
            agent_id,
            definition_revision,
            message,
            context_refs,
        } => {
            !agent_id.is_empty()
                && *definition_revision > 0
                && !message.trim().is_empty()
                && message.len() <= max_output_bytes
                && floe_agent_contract::valid_context_refs(context_refs)
        }
    }
}

/// One prepared attempt dispatch. The secret fields stay in the adapter.
pub trait PreparedModelTransport: Sync {
    fn generate(
        &self,
        request: CanonicalModelRequest,
    ) -> impl std::future::Future<Output = Result<CanonicalModelResponse, AgentFailure>> + Send;
}

/// Non-secret profile facts paired with the opaque capability that can run them.
pub struct PreparedModelProfile<Prepared> {
    pub profile: ModelProfile,
    pub transport: Prepared,
}

/// Canonical profile observation. Async because a server purpose inventory
/// is one HTTP round trip; a device profile is immediately available.
pub trait ModelProvider: Sync {
    type Prepared: PreparedModelTransport + Send;

    fn observe_profiles(
        &self,
    ) -> impl std::future::Future<Output = Vec<PreparedModelProfile<Self::Prepared>>> + Send;
}
