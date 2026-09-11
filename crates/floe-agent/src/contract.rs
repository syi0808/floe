use std::future::Future;

use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentFailure, InferencePolicyDecision, ModelPlacement, SessionProtection};

pub const AGENT_VERSION: u32 = 1;

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
    pub usage: AgentUsage,
    #[serde(default)]
    pub model_attempts: Vec<crate::ModelAttemptRecord>,
    #[serde(default)]
    pub capability_executions: Vec<CapabilityExecution>,
    #[serde(default)]
    pub delegation_executions: Vec<DelegationExecution>,
    #[serde(default)]
    pub pending_output: Option<Vec<ModelStep>>,
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
                provider:
                    floe_domain::CalendarProvider::EventKit
                    | floe_domain::CalendarProvider::Google
                    | floe_domain::CalendarProvider::Microsoft
                    | floe_domain::CalendarProvider::Android,
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
pub struct CapabilityExecution {
    pub scope_id: Uuid,
    pub turn_id: Uuid,
    pub call_id: Uuid,
    pub capability_id: String,
    pub input: String,
    pub state: CapabilityExecutionState,
    pub result: Option<Result<String, AgentFailure>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ProviderReplay>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationExecution {
    pub turn_id: Uuid,
    pub task_id: Uuid,
    pub agent_id: String,
    pub message: String,
    pub state: DelegationExecutionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<crate::A2ATask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ProviderReplay>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationExecutionState {
    Started,
    Settled,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderReplay {
    pub call_ids: Vec<String>,
    pub preamble: String,
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
    #[serde(default)]
    pub model_attempts: u32,
    #[serde(default)]
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
    pub placement: ModelPlacement,
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
        task: crate::A2ATask,
    },
}

impl AgentMessage {
    pub fn turn_id(&self) -> Uuid {
        match self {
            Self::Compaction { turn_id, .. }
            | Self::Preamble { turn_id, .. }
            | Self::User { turn_id, .. }
            | Self::Assistant { turn_id, .. }
            | Self::Capability { turn_id, .. }
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
        record: crate::ModelAttemptRecord,
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
    pub usage: crate::UsageLedger,
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
    pub usage: crate::UsageLedger,
    pub replay: Vec<ModelReplay>,
    pub schema_version: u32,
    pub prompt: crate::PromptAssembly,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub policy: InferencePolicyDecision,
    pub context: crate::AgentContext,
    pub messages: Vec<AgentMessage>,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub active_agents: Vec<crate::AgentCard>,
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

    pub fn context_envelope(&self) -> Result<ContextEnvelope, AgentFailure> {
        self.prompt.validate()?;
        let (history, current_turn) = self.model_conversation();
        if !current_turn.iter().any(|message| message["role"] == "user") {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(ContextEnvelope {
            schema_version: AGENT_VERSION,
            stable_instructions: self.prompt.clone(),
            scoped_instructions: ScopedInstructions {
                purpose: self.policy.purpose.clone(),
                available_capabilities: self.capabilities.clone(),
                active_experts: self.active_agents.clone(),
            },
            contextual_data: ContextualData {
                projection_version: self.context.projection_version,
                memories: self.context.memories.clone(),
                evidence: self.context.evidence.clone(),
            },
            conversation: ConversationContext {
                history,
                current_turn,
            },
            runtime: RuntimeContext {
                max_output_bytes: self.max_output_bytes.min(16384),
            },
            manifest: ContextManifest {
                prompt_components: self
                    .prompt
                    .components
                    .iter()
                    .map(|component| PromptManifestEntry {
                        kind: component.kind,
                        source: component.source.clone(),
                        revision: component.revision,
                    })
                    .collect(),
                evidence: self
                    .context
                    .evidence
                    .iter()
                    .map(|evidence| EvidenceManifestEntry {
                        source_handle: evidence.source_handle.clone(),
                        data_class: evidence.data_class,
                        expires_at_unix_ms: evidence.expires_at_unix_ms,
                    })
                    .collect(),
                memories: self
                    .context
                    .memories
                    .iter()
                    .map(|memory| MemoryManifestEntry {
                        target_id: memory.target_id,
                        revision: memory.revision,
                        source_refs: memory.source_refs.clone(),
                    })
                    .collect(),
                agent_cards: self
                    .active_agents
                    .iter()
                    .map(|card| AgentCardManifestEntry {
                        id: card.id.clone(),
                        version: card.version.clone(),
                    })
                    .collect(),
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEnvelope {
    pub schema_version: u32,
    pub stable_instructions: crate::PromptAssembly,
    pub scoped_instructions: ScopedInstructions,
    pub contextual_data: ContextualData,
    pub conversation: ConversationContext,
    pub runtime: RuntimeContext,
    pub manifest: ContextManifest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextualData {
    pub projection_version: u32,
    pub memories: Vec<crate::ContextMemory>,
    pub evidence: Vec<crate::ContextEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedInstructions {
    pub purpose: String,
    pub available_capabilities: Vec<CapabilityDescriptor>,
    pub active_experts: Vec<crate::AgentCard>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationContext {
    pub history: Vec<serde_json::Value>,
    pub current_turn: Vec<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeContext {
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextManifest {
    pub prompt_components: Vec<PromptManifestEntry>,
    pub evidence: Vec<EvidenceManifestEntry>,
    pub memories: Vec<MemoryManifestEntry>,
    pub agent_cards: Vec<AgentCardManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryManifestEntry {
    pub target_id: Uuid,
    pub revision: u64,
    pub source_refs: Vec<crate::LearningEvidenceRef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCardManifestEntry {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptManifestEntry {
    pub kind: crate::PromptComponentKind,
    pub source: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceManifestEntry {
    pub source_handle: String,
    pub data_class: crate::DataClass,
    pub expires_at_unix_ms: u64,
}

fn model_messages(message: &AgentMessage, include_capability: bool) -> Vec<serde_json::Value> {
    match message {
        AgentMessage::Compaction { summary, .. } => vec![serde_json::json!({
            "role": "assistant",
            "content": summary,
        })],
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
        AgentMessage::Capability { .. } | AgentMessage::Preamble { .. } => vec![],
        AgentMessage::Delegation { task, .. } if include_capability => vec![
            serde_json::json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": task.id,
                    "type": "function",
                    "function": {
                        "name": "floe.a2a.delegate",
                        "arguments": {
                            "agent_id": task.agent_id,
                            "message": task.history.first().and_then(|message| message.text().ok()).unwrap_or_default(),
                        }
                    }
                }]
            }),
            serde_json::json!({
                "role": "tool",
                "tool_call_id": task.id,
                "capability_id": "floe.a2a.delegate",
                "status": if task.state == crate::A2ATaskState::Completed { "success" } else { "error" },
                "content": task,
            }),
        ],
        AgentMessage::Delegation { .. } => vec![],
    }
}

fn embedded_json(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.into()))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelStep {
    Preamble {
        text: String,
    },
    Answer {
        text: String,
    },
    Call {
        capability_id: String,
        input: String,
    },
    Delegate {
        agent_id: String,
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelResponse {
    pub replay: Option<ProviderReplay>,
    pub schema_version: u32,
    pub output: Vec<ModelStep>,
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

impl ModelResponse {
    pub fn call_count(&self) -> usize {
        self.output
            .iter()
            .filter(|step| matches!(step, ModelStep::Call { .. } | ModelStep::Delegate { .. }))
            .count()
    }

    pub fn capability_call_count(&self) -> usize {
        self.output
            .iter()
            .filter(|step| matches!(step, ModelStep::Call { .. }))
            .count()
    }

    pub fn delegation_count(&self) -> usize {
        self.output
            .iter()
            .filter(|step| matches!(step, ModelStep::Delegate { .. }))
            .count()
    }

    pub fn replay_for(&self, call_index: usize) -> Result<Option<ProviderReplay>, AgentFailure> {
        self.replay
            .clone()
            .map(|mut replay| {
                replay.provider_call_id = replay
                    .call_ids
                    .get(call_index)
                    .ok_or(AgentFailure::InvalidModelOutput)?
                    .clone();
                Ok(replay)
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_projects_only_the_summary_to_the_model() {
        let message = AgentMessage::Compaction {
            turn_id: Uuid::new_v4(),
            summary: "Earlier conversation summary".into(),
            recovery: SessionRecoveryPointer {
                archive_id: Uuid::new_v4(),
                source_revision: 4,
                through_turn_id: Uuid::new_v4(),
                archived_message_count: 6,
            },
        };

        assert_eq!(
            model_messages(&message, false),
            vec![serde_json::json!({
                "role": "assistant",
                "content": "Earlier conversation summary",
            })]
        );
    }
}
