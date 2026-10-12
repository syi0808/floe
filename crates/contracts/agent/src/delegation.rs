use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{AgentContext, AgentFailure, Artifact, DependencyCoverage, TaskId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Submitted,
    Working,
    Completed,
    Blocked,
    Failed,
    Rejected,
    Cancelled,
    TimedOut,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSnapshot {
    pub task_id: TaskId,
    pub parent_run_id: Option<Uuid>,
    pub principal: String,
    pub agent_id: String,
    pub definition_revision: u64,
    pub state: TaskState,
    pub result: Option<String>,
    pub artifacts: Vec<Artifact>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub blockage: Option<crate::TaskBlockage>,
}

impl TaskSnapshot {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        let result_valid = match self.state {
            TaskState::Blocked => {
                self.result.is_none()
                    && self.issue.is_none()
                    && self.artifacts.is_empty()
                    && self.blockage.is_some()
                    && self.coverage != DependencyCoverage::Unknown
            }
            TaskState::Completed => {
                self.result
                    .as_deref()
                    .is_some_and(|result| crate::message::bounded(result, maximum_bytes))
                    && self.issue.is_none()
                    && self.coverage != DependencyCoverage::Unknown
            }
            TaskState::Submitted | TaskState::Working => {
                self.result.is_none() && self.issue.is_none()
            }
            _ => self.result.is_none(),
        };
        let mut artifact_ids = std::collections::HashSet::new();
        if !self.task_id.is_valid()
            || self.principal.trim().is_empty()
            || self.agent_id.trim().is_empty()
            || self.definition_revision == 0
            || !result_valid
            || (self.state == TaskState::Blocked) != self.blockage.is_some()
            || self
                .blockage
                .as_ref()
                .is_some_and(|blockage| blockage.validate().is_err())
            || self
                .result
                .as_deref()
                .is_some_and(|result| result.len() > maximum_bytes)
            || self
                .artifacts
                .iter()
                .any(|artifact| artifact.validate(maximum_bytes).is_err())
            || self
                .artifacts
                .iter()
                .any(|artifact| !artifact_ids.insert(artifact.artifact_id))
            || self
                .artifacts
                .iter()
                .any(|artifact| match (&self.coverage, &artifact.coverage) {
                    (_, DependencyCoverage::Unknown) => true,
                    (_, DependencyCoverage::Independent) => false,
                    (
                        DependencyCoverage::Dependent {
                            dependencies: report,
                        },
                        DependencyCoverage::Dependent {
                            dependencies: artifact,
                        },
                    ) => !artifact
                        .iter()
                        .all(|dependency| report.contains(dependency)),
                    _ => true,
                })
            || self.coverage.validate().is_err()
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

/// Maximum device identity bytes on a delegation execution context: the same
/// limit connector device validation enforces.
pub const MAX_DELEGATION_DEVICE_ID_BYTES: usize = 128;

/// Maximum serialized bytes of one delegation execution context. The bound
/// keeps DelegationIntent and validated-batch journal entries sized for
/// durable persistence.
pub const MAX_DELEGATION_EXECUTION_CONTEXT_BYTES: usize = 96 * 1024;

/// The explicit secret-free host context one delegation executes under.
///
/// session/device/context/output-bound only: no saved connection, token,
/// base URL, model profile, recipient/consent, provider route, or
/// cancellation/deadline (ExecutionScope owns those). The Manager binds the
/// exact context to the validated batch before dispatch, journals it on the
/// DelegationIntent, and every replay check covers it, so a crash followed by
/// changed host state can never reuse a prior Task result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationContextInput {
    pub session_id: Uuid,
    pub device_id: String,
    pub agent_context: AgentContext,
    pub max_output_bytes: usize,
}

impl DelegationContextInput {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil()
            || self.device_id.trim().is_empty()
            || self.device_id.len() > MAX_DELEGATION_DEVICE_ID_BYTES
            || self.max_output_bytes == 0
            || self.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.agent_context.validate()?;
        if serde_json::to_vec(self)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > MAX_DELEGATION_EXECUTION_CONTEXT_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }

    pub fn bind_projection(
        self,
        projection_coverage: DependencyCoverage,
        projected: &crate::ContextualData,
    ) -> Result<DelegationExecutionContext, AgentFailure> {
        self.validate()?;
        // Pin exactly the live context used by this model attempt, not the
        // Run's initial memory snapshot. A later batch must not resurrect a
        // memory forgotten by an earlier Tool. Stored batches remain immutable.
        let mut agent_context = self.agent_context;
        agent_context.projection_version = projected.projection_version;
        agent_context.memories = projected.memories.clone();
        agent_context.optional_context_issues = projected.optional_context_issues.clone();
        agent_context.evidence = projected.evidence.clone();
        let context = DelegationExecutionContext {
            session_id: self.session_id,
            device_id: self.device_id,
            agent_context,
            max_output_bytes: self.max_output_bytes,
            projection_coverage,
        };
        context.validate()?;
        Ok(context)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationExecutionContext {
    pub session_id: Uuid,
    pub device_id: String,
    pub agent_context: AgentContext,
    pub max_output_bytes: usize,
    pub projection_coverage: DependencyCoverage,
}

impl DelegationExecutionContext {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil()
            || self.device_id.trim().is_empty()
            || self.device_id.len() > MAX_DELEGATION_DEVICE_ID_BYTES
            || self.max_output_bytes == 0
            || self.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.agent_context.validate()?;
        self.projection_coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if self.projection_coverage == DependencyCoverage::Unknown {
            return Err(AgentFailure::PolicyDenied);
        }
        if serde_json::to_vec(self)
            .map(|encoded| encoded.len() > MAX_DELEGATION_EXECUTION_CONTEXT_BYTES)
            .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationRequest {
    pub task_id: TaskId,
    pub parent_run_id: Option<Uuid>,
    pub principal: String,
    pub invocation_key: crate::InvocationKey,
    pub selected_agent_id: String,
    pub selected_definition_revision: u64,
    pub message: String,
    pub context_refs: Vec<String>,
    pub execution_context: DelegationExecutionContext,
}

/// One canonical delegation request digest: principal/parent linkage, selected
/// agent + definition revision, assignment message, context refs, and the
/// explicit execution context. Stable TaskId and InvocationKey stay separate
/// identity fields and are checked exactly alongside this digest.
///
/// Task admission/replay, Engine replay verification, and Conversation
/// recovery all use this digest, so a changed device/session/context conflicts
/// rather than reusing a prior Task result.
pub fn delegation_request_digest(request: &DelegationRequest) -> [u8; 32] {
    let encoded = serde_json::to_vec(&(
        &request.principal,
        &request.parent_run_id,
        &request.selected_agent_id,
        &request.selected_definition_revision,
        &request.message,
        &request.context_refs,
        &request.execution_context,
    ))
    .unwrap_or_default();
    Sha256::digest(&encoded).into()
}

/// Context references carried on one delegation: bounded count, bounded bytes
/// each. Every layer that accepts model-produced references — provider
/// decode, legacy and canonical validation, the journal exchange — enforces
/// this same bound and maps a violation to its own failure.
pub fn valid_context_refs(references: &[String]) -> bool {
    references.len() <= crate::MAX_CONTEXT_REFS
        && references
            .iter()
            .all(|reference| reference.len() <= crate::MAX_OUTPUT_BYTES)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReceipt {
    pub task_id: TaskId,
    pub snapshot: TaskSnapshot,
    pub replay: Option<crate::ReplayReceipt>,
    pub execution: crate::TaskExecutionEvidence,
}

impl TaskReceipt {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.snapshot.validate(maximum_bytes)?;
        if serde_json::to_vec(self)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > crate::MAX_TASK_RECEIPT_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        if self.task_id != self.snapshot.task_id {
            return Err(AgentFailure::InvalidInput);
        }
        match &self.execution {
            crate::TaskExecutionEvidence::Admitted(receipt) => {
                receipt.validate(maximum_bytes)?;
                if receipt.snapshot != self.snapshot {
                    return Err(AgentFailure::Conflict);
                }
            }
            crate::TaskExecutionEvidence::Unadmitted => {
                if self.snapshot.state != TaskState::Rejected
                    || self.snapshot.issue.is_none()
                    || !self.snapshot.artifacts.is_empty()
                    || self.snapshot.coverage != DependencyCoverage::Independent
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod live_projection_tests {
    use super::*;

    #[test]
    fn fresh_delegation_pins_projected_memories_instead_of_initial_run_snapshot() {
        let memory = crate::ContextMemory {
            target_id: Uuid::new_v4(),
            revision: 1,
            kind: crate::PersonalMemoryKind::Preference,
            statement: "Prefers tea".into(),
            epistemic_status: floe_context_contract::EpistemicStatus::Fact,
            confidence_millis: 1000,
            observed_at_unix_ms: 1,
            valid_from_unix_ms: None,
            valid_until_unix_ms: None,
            source_refs: vec![floe_context_contract::LearningEvidenceRef {
                session_id: Uuid::new_v4(),
                turn_id: Uuid::new_v4(),
            }],
        };
        let input = DelegationContextInput {
            session_id: Uuid::new_v4(),
            device_id: "test-device".into(),
            max_output_bytes: 1024,
            agent_context: AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![memory.clone()],
                optional_context_issues: vec![],
                evidence: vec![],
            },
        };
        let empty = crate::ContextualData {
            projection_version: 1,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        };
        let after_forget = input
            .clone()
            .bind_projection(DependencyCoverage::Independent, &empty)
            .unwrap();
        assert!(after_forget.agent_context.memories.is_empty());
        let mut current = memory;
        current.revision = 2;
        current.statement = "Prefers coffee".into();
        let fresh = crate::ContextualData {
            memories: vec![current.clone()],
            ..empty
        };
        let new_batch = input
            .bind_projection(DependencyCoverage::Independent, &fresh)
            .unwrap();
        assert_eq!(new_batch.agent_context.memories, vec![current]);
        assert!(
            after_forget.agent_context.memories.is_empty(),
            "an already pinned batch remains immutable"
        );
    }
}
