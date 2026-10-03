use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentFailure, PreparedModelPlan, SourceProjectionReview, TaskId, TaskSnapshot, TaskState};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecutionKey {
    pub task_id: TaskId,
    pub execution_id: Uuid,
    pub executor_generation: u64,
}

impl TaskExecutionKey {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.task_id.is_valid() || self.execution_id.is_nil() || self.executor_generation == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecutionReceiptRef {
    pub execution: TaskExecutionKey,
    pub task_revision: u64,
    pub journal_revision: u64,
    pub digest: [u8; 32],
}

impl TaskExecutionReceiptRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.execution.validate()?;
        if self.task_revision < 2 || self.journal_revision > 512 || self.digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedModelAttempt {
    pub attempt_id: Uuid,
    pub reservation_ceiling: floe_execution::budget::ModelReservationCeiling,
    pub accounting: floe_execution::budget::ModelAccounting,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskModelAccounting {
    pub usage: floe_execution::budget::ModelUsage,
    pub attempt_refs: Vec<Uuid>,
    pub unresolved_attempts: Vec<UnresolvedModelAttempt>,
}

impl TaskModelAccounting {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let maximum = floe_execution::budget::MAX_MODEL_ATTEMPTS_PER_SCOPE;
        let mut attempts = std::collections::HashSet::new();
        let mut unresolved = std::collections::HashSet::new();
        if self.attempt_refs.len() > maximum
            || self.usage.attempts as usize != self.attempt_refs.len()
            || self.usage.estimated_tokens > self.usage.tokens
            || self.usage.estimated_cost_micros > self.usage.cost_micros
            || self.attempt_refs.iter().any(|id| id.is_nil() || !attempts.insert(*id))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        for attempt in &self.unresolved_attempts {
            attempt.reservation_ceiling.validate()?;
            if !attempts.contains(&attempt.attempt_id)
                || !unresolved.insert(attempt.attempt_id)
                || attempt.accounting.observed_tokens.is_some()
                || attempt.accounting.observed_cost_micros.is_some()
                || !attempt.accounting.unknown_tokens
                || !attempt.accounting.unknown_cost
            {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecutionReceipt {
    pub reference: TaskExecutionReceiptRef,
    pub journal_digest: [u8; 32],
    pub snapshot: TaskSnapshot,
    pub accounting: TaskModelAccounting,
}

impl TaskExecutionReceipt {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.reference.validate()?;
        self.snapshot.validate(maximum_bytes)?;
        self.accounting.validate()?;
        if self.reference.execution.task_id != self.snapshot.task_id
            || self.journal_digest == [0; 32]
            || matches!(self.snapshot.state, TaskState::Submitted | TaskState::Working)
            || (matches!(self.snapshot.state, TaskState::Completed | TaskState::Blocked)
                && !self.accounting.unresolved_attempts.is_empty())
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "receipt", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskExecutionEvidence {
    Unadmitted,
    Admitted(TaskExecutionReceipt),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskBlockage {
    SourceRead {
        tool_call_id: Uuid,
        blockers: floe_context_contract::SourceAccessBlockers,
    },
    ModelProjection {
        plan: PreparedModelPlan,
        review: SourceProjectionReview,
    },
    Binding {
        requirement_keys: Vec<String>,
    },
}

impl TaskBlockage {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::SourceRead { tool_call_id, blockers } => {
                if tool_call_id.is_nil() { return Err(AgentFailure::InvalidInput); }
                blockers.validate().map_err(|_| AgentFailure::InvalidInput)
            }
            Self::ModelProjection { plan, review } => {
                plan.validate()?;
                review.validate()
            }
            Self::Binding { requirement_keys } => {
                let mut keys = std::collections::HashSet::new();
                if requirement_keys.is_empty() || requirement_keys.len() > 64
                    || requirement_keys.iter().any(|key| key.trim().is_empty()
                        || key.len() > 128 || !keys.insert(key))
                {
                    return Err(AgentFailure::InvalidInput);
                }
                Ok(())
            }
        }
    }
}
