use std::future::Future;

use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::{AgentFailure, ModelPlacement, SessionProtection};

pub use floe_inference::ModelStep;

pub use floe_agent_contract::{
    AgentCardManifestEntry, CapabilityDescriptor, CapabilityExecution, CapabilityExecutionState,
    ContextEnvelope, ContextManifest, ContextualData, EvidenceManifestEntry, MemoryManifestEntry,
    ModelReplay, PromptManifestEntry, ProviderReplay, RuntimeContext, ScopedInstructions,
};

use floe_agent_contract::{
    Artifact as ContractArtifact, ArtifactPart as ContractArtifactPart, DelegationRequest,
    DependencyCoverage, InvocationKey, ModelConversation, ModelConversationEntry, OutcomeIssue,
    TaskId, TaskReceipt, TaskSnapshot, TaskState, ToolCall, ToolResult,
};

use floe_context::InferencePolicyDecision;

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
        task: floe_experts::A2ATask,
    },
}

impl AgentMessage {
    /// Whether this message carries something a source read could have produced.
    ///
    /// An answer, a capability result that succeeded, and a delegation that
    /// completed may all be derived from what a source said; the read behind
    /// them has to still hold when they are committed. A preamble, a compaction
    /// pointer, and what the Person themselves said do not.
    pub fn may_derive_from_source(&self) -> bool {
        match self {
            Self::Assistant { .. } | Self::Capability { result: Ok(_), .. } => true,
            Self::Delegation { task, .. } => task.state == floe_experts::A2ATaskState::Completed,
            Self::Compaction { .. }
            | Self::Preamble { .. }
            | Self::User { .. }
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

pub struct CapabilityInvocation {
    pub usage: crate::turn::UsageLedger,
    pub schema_version: u32,
    pub call_id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub capability_id: String,
    pub input: String,
    pub max_output_bytes: usize,
    pub deadline: tokio::time::Instant,
    pub cancellation: floe_execution::Cancellation,
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
    pub usage: crate::turn::UsageLedger,
    pub replay: Vec<ModelReplay>,
    pub schema_version: u32,
    pub prompt: floe_knowledge::prompts::PromptAssembly,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub policy: InferencePolicyDecision,
    pub context: floe_context::AgentContext,
    pub messages: Vec<AgentMessage>,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub active_agents: Vec<floe_experts::AgentCard>,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: tokio::time::Instant,
    pub cancellation: floe_execution::Cancellation,
}

impl ModelRequest {
    pub fn conversation_messages(&self) -> (Vec<&AgentMessage>, Vec<&AgentMessage>) {
        self.messages
            .iter()
            .partition(|message| message.turn_id() != self.turn_id)
    }

    pub fn model_conversation(&self) -> (Vec<ModelConversationEntry>, Vec<ModelConversationEntry>) {
        let (history, current_turn) = self.conversation_messages();
        let principal = self.person_id.to_string();
        let history = history
            .into_iter()
            .flat_map(|message| model_entries(message, false, &principal))
            .collect();
        let current_turn = current_turn
            .into_iter()
            .flat_map(|message| model_entries(message, true, &principal))
            .collect();
        (history, current_turn)
    }

    pub fn context_envelope(&self) -> Result<ContextEnvelope, AgentFailure> {
        self.context.validate()?;
        self.prompt.validate()?;
        let (history, current_turn) = self.model_conversation();
        if !current_turn
            .iter()
            .any(|entry| matches!(entry, ModelConversationEntry::User { .. }))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(ContextEnvelope {
            schema_version: AGENT_VERSION,
            stable_instructions: self.prompt.clone(),
            scoped_instructions: ScopedInstructions {
                purpose: self.policy.purpose.clone(),
                response_contract: String::new(),
                available_capabilities: self.capabilities.clone(),
                active_experts: self.active_agents.clone(),
                correction: None,
            },
            contextual_data: ContextualData {
                projection_version: self.context.projection_version,
                memories: self.context.memories.clone(),
                optional_context_issues: self.context.optional_context_issues.clone(),
                evidence: self.context.evidence.clone(),
            },
            conversation: ModelConversation {
                history,
                current_turn,
            },
            runtime: RuntimeContext {
                max_output_bytes: self.max_output_bytes.min(16384),
            },
            manifest: context_manifest(&self.prompt, &self.context, &self.active_agents),
        })
    }
}

/// The manifest half of an envelope: what prompt, evidence, memories, and
/// agent cards went into it. Shared by the legacy envelope builder and the
/// transitional model projection so both describe the same inputs.
pub fn context_manifest(
    prompt: &floe_agent_contract::prompts::PromptAssembly,
    context: &floe_agent_contract::AgentContext,
    active_agents: &[floe_agent_contract::AgentCard],
) -> ContextManifest {
    ContextManifest {
        prompt_components: prompt
            .components
            .iter()
            .map(|component| PromptManifestEntry {
                kind: component.kind,
                source: component.source.clone(),
                revision: component.revision,
            })
            .collect(),
        evidence: context
            .evidence
            .iter()
            .map(|evidence| EvidenceManifestEntry {
                source_handle: evidence.source_handle.clone(),
                data_class: evidence.data_class,
                expires_at_unix_ms: evidence.expires_at_unix_ms,
            })
            .collect(),
        memories: context
            .memories
            .iter()
            .map(|memory| MemoryManifestEntry {
                target_id: memory.target_id,
                revision: memory.revision,
                source_refs: memory.source_refs.clone(),
            })
            .collect(),
        agent_cards: active_agents
            .iter()
            .map(|card| AgentCardManifestEntry {
                id: card.id.clone(),
                version: card.version.clone(),
            })
            .collect(),
    }
}

/// Legacy session messages as typed model input. Text carries over exactly;
/// history keeps its old shape (capability and delegation evidence stays in
/// the current turn only, preambles never cross). Identity fields the old path
/// never recorded are derived deterministically from the call or task id, and
/// coverage the old message never carried reads as unknown: these entries are
/// model input only, never journaled or replayed.
fn model_entries(
    message: &AgentMessage,
    include_capability: bool,
    principal: &str,
) -> Vec<ModelConversationEntry> {
    match message {
        AgentMessage::Compaction { summary, .. } => vec![ModelConversationEntry::Assistant {
            message_id: message.turn_id(),
            text: summary.clone(),
        }],
        AgentMessage::User { text, .. } => vec![ModelConversationEntry::User {
            message_id: message.turn_id(),
            text: text.clone(),
        }],
        AgentMessage::Assistant { text, .. } => vec![ModelConversationEntry::Assistant {
            message_id: message.turn_id(),
            text: text.clone(),
        }],
        AgentMessage::Capability {
            call_id,
            capability_id,
            input,
            result,
            ..
        } if include_capability => vec![ModelConversationEntry::ToolExchange {
            call: ToolCall {
                call_id: *call_id,
                invocation_key: InvocationKey::from_uuid(*call_id)
                    .unwrap_or_else(InvocationKey::new),
                tool_id: capability_id.clone(),
                definition_revision: 1,
                input: input.clone(),
            },
            result: match result {
                Ok(output) => ToolResult {
                    call_id: *call_id,
                    text: output.clone(),
                    artifacts: vec![],
                    coverage: DependencyCoverage::Unknown,
                    issue: None,
                },
                Err(failure) => ToolResult {
                    call_id: *call_id,
                    text: format!("unavailable: {failure:?}"),
                    artifacts: vec![],
                    coverage: DependencyCoverage::Unknown,
                    issue: Some(OutcomeIssue {
                        failure: *failure,
                        retryable: false,
                    }),
                },
            },
        }],
        AgentMessage::Capability { .. } | AgentMessage::Preamble { .. } => vec![],
        AgentMessage::Delegation { task, .. } if include_capability => {
            vec![legacy_delegation_exchange(task, principal)]
        }
        AgentMessage::Delegation { .. } => vec![],
    }
}

fn legacy_delegation_exchange(
    task: &floe_experts::A2ATask,
    principal: &str,
) -> ModelConversationEntry {
    let task_id = TaskId::from_uuid(task.id).unwrap_or_else(TaskId::new);
    let message = task
        .history
        .first()
        .and_then(|message| message.text().ok())
        .unwrap_or_default()
        .to_owned();
    ModelConversationEntry::DelegationExchange {
        request: DelegationRequest {
            task_id,
            parent_run_id: None,
            principal: principal.to_owned(),
            invocation_key: InvocationKey::from_uuid(task.id)
                .unwrap_or_else(InvocationKey::new),
            selected_agent_id: task.agent_id.clone(),
            selected_definition_revision: 1,
            message,
            context_refs: vec![],
        },
        receipt: TaskReceipt {
            task_id,
            snapshot: TaskSnapshot {
                task_id,
                parent_run_id: None,
                principal: principal.to_owned(),
                agent_id: task.agent_id.clone(),
                definition_revision: 1,
                state: match task.state {
                    floe_experts::A2ATaskState::Submitted => TaskState::Submitted,
                    floe_experts::A2ATaskState::Working => TaskState::Working,
                    floe_experts::A2ATaskState::Completed => TaskState::Completed,
                    floe_experts::A2ATaskState::Failed => TaskState::Failed,
                    floe_experts::A2ATaskState::Cancelled => TaskState::Cancelled,
                    floe_experts::A2ATaskState::Rejected => TaskState::Rejected,
                },
                result: task.result_text().ok().map(str::to_owned),
                artifacts: task
                    .artifacts
                    .iter()
                    .map(|artifact| ContractArtifact {
                        artifact_id: artifact.artifact_id,
                        name: artifact.name.clone(),
                        parts: artifact
                            .parts
                            .iter()
                            .map(|part| match part {
                                floe_experts::A2APart::Text { text } => {
                                    ContractArtifactPart::Text { text: text.clone() }
                                }
                                floe_experts::A2APart::Data { media_type, data } => {
                                    ContractArtifactPart::Data {
                                        media_type: media_type.clone(),
                                        data: data.clone(),
                                    }
                                }
                            })
                            .collect(),
                        coverage: DependencyCoverage::Unknown,
                    })
                    .collect(),
                coverage: DependencyCoverage::Unknown,
                issue: task.failure,
            },
            replay: None,
        },
    }
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

    fn history_start(
        &self,
        _messages: &[AgentMessage],
        _current_turn: Uuid,
        _max_bytes: usize,
    ) -> Result<usize, AgentFailure> {
        Ok(0)
    }

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
            model_entries(&message, false, "person:test"),
            vec![ModelConversationEntry::Assistant {
                message_id: message.turn_id(),
                text: "Earlier conversation summary".into(),
            }]
        );
    }

    #[test]
    fn a_capability_result_is_structured_current_evidence_not_a_stored_record() {
        let call_id = Uuid::new_v4();
        let message = AgentMessage::Capability {
            turn_id: Uuid::new_v4(),
            call_id,
            capability_id: "fixture.read".into(),
            input: r#"{"day":"today"}"#.into(),
            result: Ok(r#"{"summary":"One meeting at 10:00"}"#.into()),
        };

        let projected = model_entries(&message, true, "person:test");

        assert!(matches!(
            projected.as_slice(),
            [ModelConversationEntry::ToolExchange { call, result }]
                if call.call_id == call_id
                    && call.tool_id == "fixture.read"
                    && call.input.contains("today")
                    && result.call_id == call_id
                    && result.text.contains("One meeting at 10:00")
                    && result.issue.is_none()
        ));
    }

    #[test]
    fn an_earlier_turn_becomes_history_and_leaves_its_evidence_behind() {
        let previous_turn = Uuid::new_v4();
        let current_turn = Uuid::new_v4();
        let messages = vec![
            AgentMessage::User {
                turn_id: previous_turn,
                text: "The earlier question".into(),
            },
            AgentMessage::Capability {
                turn_id: previous_turn,
                call_id: Uuid::new_v4(),
                capability_id: "fixture.read".into(),
                input: "old".into(),
                result: Ok("stale private evidence".into()),
            },
            AgentMessage::Assistant {
                turn_id: previous_turn,
                text: "The earlier answer".into(),
            },
            AgentMessage::User {
                turn_id: current_turn,
                text: "Summarize this fixture".into(),
            },
        ];

        let (history, current): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|message| message.turn_id() != current_turn);
        let history: Vec<_> = history
            .into_iter()
            .flat_map(|message| model_entries(message, false, "person:test"))
            .collect();
        let current: Vec<_> = current
            .into_iter()
            .flat_map(|message| model_entries(message, true, "person:test"))
            .collect();

        // The earlier turn's question and answer are history; the evidence its
        // capability call stood on is not carried forward with them.
        assert!(matches!(
            history.as_slice(),
            [
                ModelConversationEntry::User { text, .. },
                ModelConversationEntry::Assistant { text: answer, .. }
            ] if text == "The earlier question" && answer == "The earlier answer"
        ));
        assert!(
            !serde_json::to_string(&history)
                .unwrap()
                .contains("stale private evidence")
        );
        assert!(matches!(
            current.as_slice(),
            [ModelConversationEntry::User { text, .. }]
                if text == "Summarize this fixture"
        ));
    }

    #[test]
    fn a_capability_message_is_withheld_when_the_turn_may_not_carry_one() {
        let message = AgentMessage::Capability {
            turn_id: Uuid::new_v4(),
            call_id: Uuid::new_v4(),
            capability_id: "fixture.read".into(),
            input: r#"{"day":"today"}"#.into(),
            result: Ok(r#"{"summary":"One meeting at 10:00"}"#.into()),
        };

        assert!(model_entries(&message, false, "person:test").is_empty());
    }
}
