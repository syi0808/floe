use serde::{Deserialize, Serialize};

use crate::{
    AgentFailure, Artifact, BoxFuture, DelegationRequest, DependencyCoverage, ExecutionScope,
    TaskId,
};

#[derive(Clone, Debug)]
pub struct EndpointInvocation {
    pub request: DelegationRequest,
    pub request_digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertReport {
    pub task_id: TaskId,
    pub principal: String,
    pub agent_id: String,
    pub definition_revision: u64,
    pub result: String,
    pub artifacts: Vec<Artifact>,
    pub coverage: DependencyCoverage,
}

impl ExpertReport {
    pub fn validate(
        &self,
        invocation: &EndpointInvocation,
        maximum_bytes: usize,
    ) -> Result<(), AgentFailure> {
        if self.task_id != invocation.request.task_id
            || self.principal != invocation.request.principal
            || self.agent_id != invocation.request.selected_agent_id
            || self.definition_revision != invocation.request.selected_definition_revision
            || self.result.trim().is_empty()
            || self.result.len() > maximum_bytes
            || self.coverage == DependencyCoverage::Unknown
            || self.coverage.validate().is_err()
            || self
                .artifacts
                .iter()
                .any(|artifact| artifact.validate(maximum_bytes).is_err())
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

pub trait AgentEndpoint: Send + Sync {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>>;
}
