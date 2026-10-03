//! Package-owned role, source-tool schemas and final judgment on captured evidence.
use floe_agent_contract::{AgentFailure, Artifact, DelegationRequest, OwnerActor, ToolCall};
use serde_json::Value;

use crate::{ExpertAdmissionIdentity, ExpertExecutionSelection, ExpertPrivateState};

#[derive(Clone, Debug)]
pub struct ExpertProgramRequest {
    pub actor: OwnerActor,
    pub request: DelegationRequest,
    pub admission: ExpertAdmissionIdentity,
    pub selection: ExpertExecutionSelection,
    pub private_state: ExpertPrivateState,
    pub now_unix_ms: i64,
}

#[derive(Clone, Debug)]
pub struct ExpertToolSpec {
    pub requirement_key: String,
    pub description: String,
    pub input_schema: String,
}

#[derive(Clone, Debug)]
pub struct ExpertProgramSpec {
    pub prompt: floe_agent_contract::prompts::PromptAssembly,
    pub output_contract: String,
    pub tools: Vec<ExpertToolSpec>,
}

#[derive(Clone, Debug)]
pub struct ExpertToolObservation {
    pub call: ToolCall,
    pub requirement_key: String,
    pub payload: Value,
    pub coverage: floe_agent_contract::DependencyCoverage,
}

pub struct ExpertFinalOutput {
    pub payload: floe_agent_runtime::ValidatedFinalPayload,
    pub settlement: Option<floe_agent_contract::EndpointSettlement>,
}

pub trait ExpertProgram: Send + Sync {
    fn specification(&self, request: &ExpertProgramRequest) -> Result<ExpertProgramSpec, AgentFailure>;
    fn finalize(&self, request: &ExpertProgramRequest, observations: &[ExpertToolObservation],
        text: &str, artifacts: &[Artifact]) -> Result<ExpertFinalOutput, AgentFailure>;
}
