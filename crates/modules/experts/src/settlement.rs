//! What a delegated Expert hands back so its Task can be settled exactly once.
//!
//! An endpoint may finish its work and still fail to record it. The settlement
//! carries the registry state it staged and the result it produced, so the Task
//! owner can commit both together, or commit neither.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::ContextDependency;
use floe_agent_contract::{AgentFailure, EndpointSettlement, TaskId, TaskSnapshot};

use crate::RegistrySnapshot;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertSettlement {
    schema_version: u32,
    owner: String,
    pub expected_registry_revision: u64,
    pub staged_registry: RegistrySnapshot,
    pub assignment_id: Uuid,
    pub invocation_id: Uuid,
    pub dependencies: Vec<ContextDependency>,
    pub task_result: String,
}

impl ExpertSettlement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: impl Into<String>,
        expected_registry_revision: u64,
        staged_registry: RegistrySnapshot,
        assignment_id: Uuid,
        invocation_id: Uuid,
        dependencies: Vec<ContextDependency>,
        task_result: String,
    ) -> Self {
        Self {
            schema_version: 1,
            owner: owner.into(),
            expected_registry_revision,
            staged_registry,
            assignment_id,
            invocation_id,
            dependencies,
            task_result,
        }
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn into_endpoint_settlement(self) -> Result<EndpointSettlement, AgentFailure> {
        let owner = self.owner.clone();
        let payload = serde_json::to_string(&self).map_err(|_| AgentFailure::StorageUnavailable)?;
        EndpointSettlement::try_new(owner, payload)
    }

    /// Decode a settlement this owner produced.
    ///
    /// The payload must round-trip exactly: a settlement that re-encodes to
    /// something else is not the one that was signed off.
    pub fn from_endpoint_settlement(
        settlement: &EndpointSettlement,
        owner: &str,
    ) -> Result<Self, AgentFailure> {
        settlement.validate()?;
        if settlement.owner() != owner {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let decoded: Self = serde_json::from_str(settlement.payload())
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
        if decoded.schema_version != 1
            || decoded.owner != owner
            || serde_json::to_string(&decoded).map_err(|_| AgentFailure::InvalidModelOutput)?
                != settlement.payload()
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(decoded)
    }
}

/// One settled Expert Task, ready to be committed with its staged registry.
pub struct ExpertTaskCompletion {
    pub settlement: ExpertSettlement,
    pub task_id: TaskId,
    pub expected_task_revision: u64,
    pub executor_generation: u64,
    pub task_snapshot: TaskSnapshot,
}
