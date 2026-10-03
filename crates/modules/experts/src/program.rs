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
    /// Final judgment receives the exact context of the acknowledged model projection.
    pub context: floe_agent_contract::AgentContext,
    pub coverage: floe_agent_contract::DependencyCoverage,
    pub started_at_unix_ms: i64,
    pub state_schema_version: u32,
    pub data_class: floe_agent_contract::DataClass,
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
    pub outcome: ExpertSourceObservation,
}

#[derive(Clone, Debug)]
pub enum ExpertSourceObservation {
    Ready { payload: Value, coverage: floe_agent_contract::DependencyCoverage },
    Unavailable { reason: floe_context_contract::SourceUnavailable },
}
impl ExpertToolObservation {
    pub fn coverage(&self) -> floe_agent_contract::DependencyCoverage {
        match &self.outcome {
            ExpertSourceObservation::Ready { coverage, .. } => coverage.clone(),
            ExpertSourceObservation::Unavailable { .. } => floe_agent_contract::DependencyCoverage::Independent,
        }
    }
}

pub struct ExpertFinalOutput {
    pub payload: floe_agent_contract::ValidatedFinalPayload,
    pub settlement: Option<floe_agent_contract::EndpointSettlement>,
}

pub trait ExpertProgram: Send + Sync {
    fn specification(&self, request: &ExpertProgramRequest) -> Result<ExpertProgramSpec, AgentFailure>;
    fn finalize(&self, request: &ExpertProgramRequest, observations: &[ExpertToolObservation],
        text: &str, artifacts: &[Artifact]) -> Result<ExpertFinalOutput, AgentFailure>;
}
