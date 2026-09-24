use std::future::Future;

use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::{AgentFailure, ModelPlacement, SessionProtection};

use floe_inference::ModelStep;

use floe_agent_contract::{AGENT_VERSION, CapabilityExecution, ProviderReplay};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSession {
    pub schema_version: u32,
    pub id: Uuid,
    pub person_id: PersonId,
    pub scope: Option<AgentSessionScope>,
    pub revision: u64,
    pub data_classes: Vec<floe_agent_contract::DataClass>,
    pub messages: Vec<AgentMessage>,
    pub usage: AgentUsage,
    pub model_attempts: Vec<floe_inference::ModelAttemptRecord>,
    pub capability_executions: Vec<CapabilityExecution>,
    pub delegation_executions: Vec<DelegationExecution>,
    pub pending_output: Option<Vec<ModelStep>>,
    pub active_turn: Option<Uuid>,
    pub last_outcome: Option<AgentOutcome>,
    pub continuation: Option<AgentContinuation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentSessionScope {
    Calendar {
        setup_id: Uuid,
        provider: floe_agent_contract::CalendarProvider,
    },
}

impl AgentSessionScope {
    pub fn data_class(self) -> floe_agent_contract::DataClass {
        match self {
            Self::Calendar {
                provider: floe_agent_contract::CalendarProvider::Fixture,
                ..
            } => floe_agent_contract::DataClass::Synthetic,
            Self::Calendar {
                provider:
                    floe_agent_contract::CalendarProvider::EventKit
                    | floe_agent_contract::CalendarProvider::Google
                    | floe_agent_contract::CalendarProvider::Microsoft
                    | floe_agent_contract::CalendarProvider::Android,
                ..
            } => floe_agent_contract::DataClass::Personal,
        }
    }
}

impl AgentSession {
    pub fn new(person_id: PersonId) -> Self {
        Self {
            schema_version: AGENT_VERSION,
            id: Uuid::new_v4(),
            person_id,
            scope: None,
            revision: 0,
            data_classes: vec![floe_agent_contract::DataClass::Personal],
            messages: vec![],
            usage: AgentUsage::default(),
            model_attempts: vec![],
            capability_executions: vec![],
            delegation_executions: vec![],
            pending_output: None,
            active_turn: None,
            last_outcome: None,
            continuation: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationExecution {
    pub turn_id: Uuid,
    pub task_id: Uuid,
    pub agent_id: String,
    pub message: String,
    pub state: DelegationExecutionState,
    pub task: Option<floe_experts::A2ATask>,
    pub replay: Option<ProviderReplay>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationExecutionState {
    Started,
    Settled,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentUsage {
    pub model_attempts: u32,
    pub estimated_tokens: u64,
    pub iterations: u32,
    pub capability_calls: u32,
    pub tokens: u64,
    pub cost_micros: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentContinuation {
    pub turn_id: Uuid,
    pub level: u8,
    pub usage: AgentUsage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentMessage {
    Compaction {
        turn_id: Uuid,
        summary: String,
        recovery: SessionRecoveryPointer,
    },
    Preamble {
        turn_id: Uuid,
        text: String,
    },
    User {
        turn_id: Uuid,
        text: String,
    },
    Assistant {
        turn_id: Uuid,
        text: String,
    },
    Capability {
        turn_id: Uuid,
        call_id: Uuid,
        capability_id: String,
        input: String,
        result: Result<String, AgentFailure>,
    },
    Delegation {
        turn_id: Uuid,
        task: floe_experts::A2ATask,
    },
    Interaction {
        turn_id: Uuid,
        interaction_id: Uuid,
        interaction_kind: floe_agent_contract::UserInteractionKind,
    },
}

impl AgentMessage {
    /// Whether this message carries something a source read could have produced.
    ///
    /// An answer, a capability result that succeeded, and a delegation that
    /// completed may all be derived from what a source said; the read behind
    /// them has to still hold when they are committed. A preamble, a compaction
    /// pointer, an interaction reference, and what the Person themselves said
    /// do not: the interaction message is bare metadata, and authoritative
    /// status always loads from the interaction row.
    pub fn may_derive_from_source(&self) -> bool {
        match self {
            Self::Assistant { .. } | Self::Capability { result: Ok(_), .. } => true,
            Self::Delegation { task, .. } => task.state == floe_experts::A2ATaskState::Completed,
            Self::Compaction { .. }
            | Self::Preamble { .. }
            | Self::User { .. }
            | Self::Interaction { .. }
            | Self::Capability { result: Err(_), .. } => false,
        }
    }

    pub fn turn_id(&self) -> Uuid {
        match self {
            Self::Compaction { turn_id, .. }
            | Self::Preamble { turn_id, .. }
            | Self::User { turn_id, .. }
            | Self::Assistant { turn_id, .. }
            | Self::Capability { turn_id, .. }
            | Self::Interaction { turn_id, .. }
            | Self::Delegation { turn_id, .. } => *turn_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRecoveryPointer {
    pub archive_id: Uuid,
    pub source_revision: u64,
    pub through_turn_id: Uuid,
    pub archived_message_count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentOutcome {
    Completed,
    Halted { reason: AgentFailure },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEvent {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub event: AgentEventKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEventKind {
    Started,
    ModelAttempt {
        record: floe_inference::ModelAttemptRecord,
    },
    ModelStarted {
        iteration: u32,
        placement: ModelPlacement,
    },
    CapabilityStarted {
        call_id: Uuid,
        capability_id: String,
    },
    DelegationStarted {
        task_id: Uuid,
        agent_id: String,
    },
    MessageCommitted {
        message: AgentMessage,
        revision: u64,
    },
    Finished {
        outcome: AgentOutcome,
        revision: u64,
    },
}

pub trait SessionStore {
    fn protection(&self) -> SessionProtection;

    fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> impl Future<Output = Result<AgentSession, AgentFailure>> + Send;

    fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> impl Future<Output = Result<(), AgentFailure>> + Send;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentBudget {
    pub max_iterations: u32,
    pub max_capability_calls: u32,
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub max_output_bytes: usize,
    pub max_context_bytes: usize,
    pub max_session_bytes: usize,
    pub deadline_ms: u64,
}

impl Default for AgentBudget {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            max_capability_calls: 100,
            max_tokens: 409_600,
            max_cost_micros: 50_000,
            max_output_bytes: 16_384,
            max_context_bytes: 1_048_576,
            max_session_bytes: 2_097_152,
            deadline_ms: 300_000,
        }
    }
}

impl AgentBudget {
    pub fn expanded(self, level: u8) -> Option<Self> {
        if level > 3 {
            return None;
        }
        let mut expanded = self;
        for _ in 0..level {
            expanded.max_iterations = expanded.max_iterations.checked_mul(7)?.div_ceil(4);
            expanded.max_capability_calls =
                expanded.max_capability_calls.checked_mul(7)?.div_ceil(4);
            expanded.max_tokens = expanded.max_tokens.checked_mul(7)?.div_ceil(4);
            expanded.max_cost_micros = expanded.max_cost_micros.checked_mul(7)?.div_ceil(4);
            expanded.deadline_ms = expanded.deadline_ms.checked_mul(7)?.div_ceil(4);
        }
        Some(expanded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_manager_budget_supports_long_running_turns() {
        let budget = AgentBudget::default();
        assert_eq!(budget.max_iterations, 100);
        assert_eq!(budget.max_capability_calls, 100);
        assert_eq!(budget.max_tokens, 409_600);
        assert_eq!(budget.max_context_bytes, 1_048_576);
        assert_eq!(budget.max_session_bytes, 2_097_152);
        assert_eq!(budget.deadline_ms, 300_000);
        assert_eq!(budget.expanded(1).unwrap().max_iterations, 175);
        assert_eq!(budget.expanded(2).unwrap().max_iterations, 307);
        assert_eq!(budget.expanded(3).unwrap().max_iterations, 538);
        assert!(budget.expanded(4).is_none());
    }
}
