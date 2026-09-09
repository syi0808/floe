use std::future::Future;

use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{InferencePolicyDecision, ModelPlacement};

pub const AGENT_VERSION: u32 = 1;
pub const AGENT_SYSTEM_INSTRUCTIONS: &str = include_str!("../prompts/manager.txt");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCommand {
    pub schema_version: u32,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSession {
    pub schema_version: u32,
    pub id: Uuid,
    pub person_id: PersonId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<AgentSessionScope>,
    pub revision: u64,
    pub data_classes: Vec<crate::DataClass>,
    pub messages: Vec<AgentMessage>,
    #[serde(default)]
    pub capability_executions: Vec<CapabilityExecution>,
    pub active_turn: Option<Uuid>,
    pub last_outcome: Option<AgentOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<AgentContinuation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentSessionScope {
    Calendar {
        setup_id: Uuid,
        provider: floe_domain::CalendarProvider,
    },
}

impl AgentSessionScope {
    pub fn data_class(self) -> crate::DataClass {
        match self {
            Self::Calendar {
                provider: floe_domain::CalendarProvider::Fixture,
                ..
            } => crate::DataClass::Synthetic,
            Self::Calendar {
                provider: floe_domain::CalendarProvider::EventKit,
                ..
            } => crate::DataClass::Personal,
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
            data_classes: vec![crate::DataClass::Personal],
            messages: vec![],
            capability_executions: vec![],
            active_turn: None,
            last_outcome: None,
            continuation: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityExecution {
    pub turn_id: Uuid,
    pub call_id: Uuid,
    pub capability_id: String,
    pub input: String,
    pub state: CapabilityExecutionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ProviderReplay>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderReplay {
    pub gateway: String,
    pub purpose: String,
    pub external: bool,
    pub source: String,
    pub provider_call_id: String,
    pub items: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ModelReplay {
    pub call_id: Uuid,
    pub replay: ProviderReplay,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityExecutionState {
    Started,
    Settled,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentUsage {
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
    pub placement: ModelPlacement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentMessage {
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
}

impl AgentMessage {
    pub fn turn_id(&self) -> Uuid {
        match self {
            Self::User { turn_id, .. }
            | Self::Assistant { turn_id, .. }
            | Self::Capability { turn_id, .. } => *turn_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFailure {
    UnsupportedVersion,
    InvalidInput,
    NotFound,
    Conflict,
    StorageUnavailable,
    VaultUnavailable,
    PolicyDenied,
    ConsentRequired,
    ModelUnavailable,
    LocalModelUnavailable,
    ServerModelUnavailable,
    CredentialExpired,
    QuotaExceeded,
    InvalidModelOutput,
    LocalModelInvalidOutput,
    ServerModelInvalidOutput,
    CapabilityDenied,
    CapabilityUnavailable,
    StaleContext,
    BudgetExceeded,
    Stalled,
    Cancelled,
    DeadlineExceeded,
    Interrupted,
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
    ModelStarted {
        iteration: u32,
        placement: ModelPlacement,
    },
    CapabilityStarted {
        call_id: Uuid,
        capability_id: String,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionProtection {
    SyntheticOnly,
    Encrypted,
    KeyUnavailable,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub read_only: bool,
    pub output_data_class: crate::DataClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

pub struct CapabilityInvocation {
    pub schema_version: u32,
    pub call_id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub capability_id: String,
    pub input: String,
    pub max_output_bytes: usize,
    pub deadline: tokio::time::Instant,
    pub cancellation: crate::Cancellation,
}

pub trait CapabilityHost {
    fn descriptors(&self, person_id: PersonId) -> Vec<CapabilityDescriptor>;

    fn invoke(
        &self,
        invocation: CapabilityInvocation,
    ) -> impl Future<Output = Result<String, AgentFailure>> + Send;
}

#[derive(Clone)]
pub struct ModelRequest {
    pub replay: Vec<ModelReplay>,
    pub schema_version: u32,
    pub system_instructions: &'static str,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub policy: InferencePolicyDecision,
    pub context: crate::AgentContext,
    pub messages: Vec<AgentMessage>,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: tokio::time::Instant,
    pub cancellation: crate::Cancellation,
}

impl ModelRequest {
    pub fn conversation_messages(&self) -> (Vec<&AgentMessage>, Vec<&AgentMessage>) {
        self.messages
            .iter()
            .partition(|message| message.turn_id() != self.turn_id)
    }

    pub fn model_conversation(&self) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
        let (history, current_turn) = self.conversation_messages();
        let history = history
            .into_iter()
            .flat_map(|message| model_messages(message, false))
            .collect();
        let current_turn = current_turn
            .into_iter()
            .flat_map(|message| model_messages(message, true))
            .collect();
        (history, current_turn)
    }
}

fn model_messages(message: &AgentMessage, include_capability: bool) -> Vec<serde_json::Value> {
    match message {
        AgentMessage::User { text, .. } => vec![serde_json::json!({
            "role": "user",
            "content": text,
        })],
        AgentMessage::Assistant { text, .. } => vec![serde_json::json!({
            "role": "assistant",
            "content": text,
        })],
        AgentMessage::Capability {
            call_id,
            capability_id,
            input,
            result,
            ..
        } if include_capability => {
            let call = serde_json::json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": capability_id,
                        "arguments": embedded_json(input),
                    }
                }]
            });
            let output = match result {
                Ok(output) => serde_json::json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "capability_id": capability_id,
                    "status": "success",
                    "content": embedded_json(output),
                }),
                Err(failure) => serde_json::json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "capability_id": capability_id,
                    "status": "error",
                    "failure": failure,
                }),
            };
            vec![call, output]
        }
        AgentMessage::Capability { .. } => vec![],
    }
}

fn embedded_json(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.into()))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelStep {
    Answer {
        text: String,
    },
    Call {
        capability_id: String,
        input: String,
    },
}

pub struct ModelResponse {
    pub replay: Option<ProviderReplay>,
    pub schema_version: u32,
    pub step: ModelStep,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

pub trait ModelRunner {
    fn placement(&self) -> ModelPlacement;

    fn generate(
        &self,
        request: ModelRequest,
    ) -> impl Future<Output = Result<ModelResponse, AgentFailure>> + Send;
}
