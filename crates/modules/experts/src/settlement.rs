//! What a delegated Expert hands back so its Task can be settled exactly once.
//!
//! An endpoint may finish its work and still fail to record it. The settlement
//! carries only its assignment-local private-state transition and result.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::ContextDependency;
use floe_agent_contract::{AgentFailure, EndpointSettlement};

use crate::{ExpertAdmissionIdentity, ExpertPrivateState};

/// Prepare the assignment-local transition after package judgment; storage performs its exact CAS.
pub fn prepare_expert_completion(request: &crate::ExpertProgramRequest, result: &str)
    -> Result<(ExpertPrivateState, EndpointSettlement), AgentFailure>
{
    if request.private_state.schema_version != request.state_schema_version
        || request.private_state.last_invocation_id == Some(request.request.invocation_key.as_uuid())
        || result.trim().is_empty() || result.len() > request.request.execution_context.max_output_bytes
    { return Err(AgentFailure::Conflict); }
    let dependencies = match &request.coverage {
        floe_agent_contract::DependencyCoverage::Independent => vec![],
        floe_agent_contract::DependencyCoverage::Dependent { dependencies } => dependencies.clone(),
        floe_agent_contract::DependencyCoverage::Unknown => return Err(AgentFailure::PolicyDenied),
    };
    let mut next = request.private_state.clone();
    next.revision = next.revision.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
    next.completed_invocations = next.completed_invocations.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
    next.last_invocation_id = Some(request.request.invocation_key.as_uuid());
    let settlement = ExpertSettlement::new(request.admission.package.id.clone(), request.admission.clone(),
        request.private_state.revision, next.clone(), request.request.invocation_key.as_uuid(),
        dependencies, result.to_owned()).into_endpoint_settlement()?;
    Ok((next, settlement))
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertSettlement {
    schema_version: u32,
    owner: String,
    pub admission: ExpertAdmissionIdentity,
    pub expected_private_state_revision: u64,
    pub next_private_state: ExpertPrivateState,
    pub invocation_id: Uuid,
    pub dependencies: Vec<ContextDependency>,
    pub task_result: String,
}

impl ExpertSettlement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: impl Into<String>,
        admission: ExpertAdmissionIdentity,
        expected_private_state_revision: u64,
        next_private_state: ExpertPrivateState,
        invocation_id: Uuid,
        dependencies: Vec<ContextDependency>,
        task_result: String,
    ) -> Self {
        Self {
            schema_version: 2,
            owner: owner.into(),
            admission,
            expected_private_state_revision,
            next_private_state,
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
        if decoded.schema_version != 2
            || decoded.owner != owner
            || serde_json::to_string(&decoded).map_err(|_| AgentFailure::InvalidModelOutput)?
                != settlement.payload()
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(decoded)
    }
}

