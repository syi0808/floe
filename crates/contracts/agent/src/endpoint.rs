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

#[derive(Clone)]
pub struct EndpointInvocation {
    pub request: DelegationRequest,
    pub request_digest: [u8; 32],
    pub execution: crate::TaskExecutionKey,
    pub journal: std::sync::Arc<dyn crate::ExecutionJournal>,
    pub resources: std::sync::Arc<EndpointResources>,
}

/// Task-owned leases outlive endpoint return and stay held through terminal acknowledgement.
#[derive(Default)]
pub struct EndpointResources {
    held: std::sync::Mutex<Vec<Box<dyn Send>>>,
}
impl EndpointResources {
    pub fn retain(&self, resource: Box<dyn Send>) -> Result<(), AgentFailure> {
        let mut held = self.held.lock().map_err(|_| AgentFailure::StorageUnavailable)?;
        if held.len() >= 64 { return Err(AgentFailure::BudgetExceeded); }
        held.push(resource);
        Ok(())
    }
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
        let artifact_coverage_valid =
            self.artifacts
                .iter()
                .all(|artifact| match (&self.coverage, &artifact.coverage) {
                    (_, DependencyCoverage::Unknown) => false,
                    (_, DependencyCoverage::Independent) => true,
                    (
                        DependencyCoverage::Dependent {
                            dependencies: report,
                        },
                        DependencyCoverage::Dependent {
                            dependencies: artifact,
                        },
                    ) => artifact
                        .iter()
                        .all(|dependency| report.contains(dependency)),
                    _ => false,
                });
        let mut artifact_ids = std::collections::HashSet::new();
        if self.task_id != invocation.request.task_id
            || self.principal != invocation.request.principal
            || self.agent_id != invocation.request.selected_agent_id
            || self.definition_revision != invocation.request.selected_definition_revision
            || self.result.trim().is_empty()
            || self.result.len() > maximum_bytes
            || self.coverage == DependencyCoverage::Unknown
            || self.coverage.validate().is_err()
            || !artifact_coverage_valid
            || self
                .artifacts
                .iter()
                .any(|artifact| !artifact_ids.insert(artifact.artifact_id))
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
    ) -> BoxFuture<'a, Result<ExpertExecutionOutcome, AgentFailure>>;
}

#[derive(Clone, Debug)]
pub struct ExpertBlockReport {
    pub task_id: TaskId,
    pub principal: String,
    pub agent_id: String,
    pub definition_revision: u64,
    pub coverage: DependencyCoverage,
    pub blockage: crate::TaskBlockage,
}

#[derive(Clone, Debug)]
pub enum ExpertExecutionOutcome {
    Completed(ExpertReport),
    Blocked(ExpertBlockReport),
}

impl ExpertBlockReport {
    pub fn validate(&self, invocation: &EndpointInvocation) -> Result<(), AgentFailure> {
        self.blockage.validate()?;
        self.coverage.validate().map_err(|_| AgentFailure::InvalidModelOutput)?;
        if self.task_id != invocation.request.task_id
            || self.principal != invocation.request.principal
            || self.agent_id != invocation.request.selected_agent_id
            || self.definition_revision != invocation.request.selected_definition_revision
            || self.coverage == DependencyCoverage::Unknown
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}
