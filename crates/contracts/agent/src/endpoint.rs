use serde::{Deserialize, Serialize};

use crate::{
    AgentFailure, Artifact, BoxFuture, DelegationRequest, DependencyCoverage, ExecutionScope,
    TaskId,
};

pub const MAX_ENDPOINT_SETTLEMENT_BYTES: usize = 384 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointSettlement {
    owner: String,
    payload: String,
}

impl EndpointSettlement {
    pub fn try_new(
        owner: impl Into<String>,
        payload: impl Into<String>,
    ) -> Result<Self, AgentFailure> {
        let settlement = Self {
            owner: owner.into(),
            payload: payload.into(),
        };
        settlement.validate()?;
        Ok(settlement)
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn payload(&self) -> &str {
        &self.payload
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.owner.trim().is_empty()
            || self.owner.len() > 128
            || self.payload.is_empty()
            || self.payload.len() > MAX_ENDPOINT_SETTLEMENT_BYTES
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

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
    #[serde(skip)]
    pub settlement: Option<EndpointSettlement>,
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
                .settlement
                .as_ref()
                .is_some_and(|settlement| settlement.validate().is_err())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_is_bounded_and_omitted_from_the_public_report_shape() {
        assert_eq!(
            EndpointSettlement::try_new("schedule", "x".repeat(MAX_ENDPOINT_SETTLEMENT_BYTES + 1)),
            Err(AgentFailure::InvalidModelOutput)
        );
        let report = ExpertReport {
            task_id: TaskId::new(),
            principal: "person-a".into(),
            agent_id: "floe.builtin.schedule".into(),
            definition_revision: 1,
            result: "result".into(),
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            settlement: Some(EndpointSettlement::try_new("schedule", "{}").unwrap()),
        };
        let encoded = serde_json::to_value(report).unwrap();
        assert!(encoded.get("settlement").is_none());
    }
}
