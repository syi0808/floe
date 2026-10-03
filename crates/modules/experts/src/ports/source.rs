use floe_agent_contract::{AgentFailure, AgentContext, BoxFuture, DependencyCoverage,
    ExecutionScope, ModelProjectionOutcome, ModelProjectionRequest, OwnerActor, TaskExecutionKey, ToolCall};
use floe_context_contract::SourceReadOutcome;

pub struct ExpertSourceRequest {
    pub actor: OwnerActor,
    pub execution: TaskExecutionKey,
    pub admission: crate::ExpertAdmissionIdentity,
    pub requirement: crate::AdmittedRequirementSelection,
    pub call: ToolCall,
    pub max_output_bytes: usize,
}

/// Keeps actual authorized source leases alive until the Task finishes.
pub struct ExpertSourceRead {
    pub payload: serde_json::Value,
    pub coverage: DependencyCoverage,
    _retention: Box<dyn Send>,
}
impl ExpertSourceRead {
    pub fn new(payload: serde_json::Value, coverage: DependencyCoverage, retention: Box<dyn Send>)
        -> Result<Self, AgentFailure>
    {
        coverage.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if coverage == DependencyCoverage::Unknown { return Err(AgentFailure::PolicyDenied); }
        Ok(Self { payload, coverage, _retention: retention })
    }
}

pub trait ExpertSourcePort: Send + Sync {
    fn read<'a>(&'a self, request: ExpertSourceRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<SourceReadOutcome<ExpertSourceRead>, AgentFailure>>;
}

pub struct ExpertProjectionRequest {
    pub actor: OwnerActor,
    pub execution: TaskExecutionKey,
    pub request: ModelProjectionRequest,
    pub context: AgentContext,
    pub prompt: floe_agent_contract::prompts::PromptAssembly,
    pub observations: Vec<crate::ExpertToolObservation>,
}

pub trait ExpertProjectionPort: Send + Sync {
    fn project<'a>(&'a self, request: ExpertProjectionRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>>;
}
