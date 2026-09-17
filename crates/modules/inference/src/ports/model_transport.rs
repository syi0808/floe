//! The transport one approved model attempt is dispatched over.
//!
//! Inference owns the attempt, the retry and the ledger; a transport owns the
//! wire. It receives an immutable input and the route evidence that admitted it,
//! and it returns the steps, the usage and the provider replay unchanged. It
//! never sees a Session, a Task or a mutable ledger.

use std::future::Future;

use floe_agent_contract::{
    AgentCard, AgentContext, AgentFailure, CapabilityDescriptor, ContextEnvelope,
    InferencePolicyDecision, ModelPlacement, ModelReplay, ProviderReplay,
    prompts::PromptAssembly,
};
use floe_execution::Cancellation;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

/// One step a model produced.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelStep {
    Preamble {
        text: String,
    },
    Answer {
        text: String,
    },
    Call {
        capability_id: String,
        input: String,
    },
    Delegate {
        agent_id: String,
        message: String,
    },
}

/// What a transport is given for one attempt.
#[derive(Clone)]
pub struct ModelTransportRequest {
    pub schema_version: u32,
    /// Which attempt this is, so a replayed call is recognised as the same one.
    pub attempt_id: Uuid,
    pub prompt: PromptAssembly,
    pub policy: InferencePolicyDecision,
    pub context: AgentContext,
    /// The immutable model input; no Session or Task travels in it.
    pub envelope: ContextEnvelope,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub active_agents: Vec<AgentCard>,
    pub replay: Vec<ModelReplay>,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

/// What a transport returns, without loss.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTransportResponse {
    pub replay: Option<ProviderReplay>,
    pub schema_version: u32,
    pub output: Vec<ModelStep>,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

pub trait ModelTransport {
    fn placement(&self) -> ModelPlacement;

    fn generate(
        &self,
        request: ModelTransportRequest,
    ) -> impl Future<Output = Result<ModelTransportResponse, AgentFailure>> + Send;
}

/// A borrowed transport is still a transport, so a caller can hand one out
/// without giving up ownership of it.
impl<Transport: ModelTransport + Sync + ?Sized> ModelTransport for &Transport {
    fn placement(&self) -> ModelPlacement {
        (**self).placement()
    }

    fn generate(
        &self,
        request: ModelTransportRequest,
    ) -> impl Future<Output = Result<ModelTransportResponse, AgentFailure>> + Send {
        (**self).generate(request)
    }
}
