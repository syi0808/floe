//! The root Run's model boundary, satisfied by an Inference transport.
//!
//! Conversation owns the Session and therefore owns turning its messages into
//! the immutable input a transport may see. The transport owns the wire; it
//! never receives the Session, the Task or the ledger this request carries.

use floe_agent_contract::{AgentFailure, ModelPlacement};
use floe_inference::{ModelTransport, ModelTransportRequest};
use uuid::Uuid;

use crate::turn::{ModelRequest, ModelResponse, ModelRunner};

pub struct TransportModelRunner<Transport> {
    transport: Transport,
}

impl<Transport> TransportModelRunner<Transport> {
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }
}

/// The immutable input one attempt is dispatched with.
///
/// Everything a transport may not see — the ledger, the Session id, the
/// capability journal — is left behind here rather than trimmed downstream.
pub fn transport_request(
    request: &ModelRequest,
    attempt_id: Uuid,
) -> Result<ModelTransportRequest, AgentFailure> {
    Ok(ModelTransportRequest {
        schema_version: request.schema_version,
        attempt_id,
        prompt: request.prompt.clone(),
        policy: request.policy.clone(),
        context: request.context.clone(),
        envelope: request.context_envelope()?,
        capabilities: request.capabilities.clone(),
        active_agents: request.active_agents.clone(),
        replay: request.replay.clone(),
        remaining_tokens: request.remaining_tokens,
        remaining_cost_micros: request.remaining_cost_micros,
        max_output_bytes: request.max_output_bytes,
        deadline: request.deadline,
        cancellation: request.cancellation.clone(),
    })
}

impl<Transport: ModelTransport + Sync> ModelRunner for TransportModelRunner<Transport> {
    fn placement(&self) -> ModelPlacement {
        self.transport.placement()
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        let attempt_id = Uuid::new_v4();
        let response = self
            .transport
            .generate(transport_request(&request, attempt_id)?)
            .await?;
        Ok(ModelResponse {
            replay: response.replay,
            schema_version: response.schema_version,
            output: response.output,
            used_tokens: response.used_tokens,
            cost_micros: response.cost_micros,
        })
    }
}
