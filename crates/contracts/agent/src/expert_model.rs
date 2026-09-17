//! The one bounded model call an Expert makes on its own assignment.
//!
//! An Expert reasons once over context it was authorized for and returns one
//! answer. It does not drive a conversation, call capabilities, delegate, or
//! carry a Session: those belong to whoever runs the root turn. Everything on
//! the other side of this port — which provider answers, how an attempt is
//! retried, and who accounts for the tokens it spent — belongs to the model's
//! owner, and no Expert names any of it.

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use floe_context_contract::{DataClass, ModelPlacement};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};

use crate::{
    AgentContext, InferencePolicyDecision, ModelReplay, ProviderReplay, ports::BoxFuture,
    prompts::PromptAssembly,
};

/// One assignment, with the context and the bounds it must be answered under.
pub struct ExpertModelCall {
    pub person_id: PersonId,
    /// The Expert invocation this call belongs to.
    pub invocation_id: Uuid,
    pub prompt: PromptAssembly,
    pub policy: InferencePolicyDecision,
    pub context: AgentContext,
    pub assignment: String,
    pub max_output_bytes: usize,
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

/// The single answer the call produced, and what it cost.
pub struct ExpertModelAnswer {
    pub schema_version: u32,
    pub answer: String,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

/// The model an Expert reasons on.
pub trait ExpertModel: Sync {
    /// Where this model runs. An Expert whose judgment needs one placement
    /// refuses rather than silently accepting the other.
    fn placement(&self) -> ModelPlacement;

    /// Answer one assignment.
    ///
    /// Anything but a single answer is `InvalidModelOutput`: the caller asked
    /// one question and is owed one reply.
    fn answer<'a>(
        &'a self,
        call: ExpertModelCall,
    ) -> BoxFuture<'a, Result<ExpertModelAnswer, AgentFailure>>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub read_only: bool,
    pub output_data_class: DataClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

/// What one reasoning step produced.
///
/// There is no delegation here. An Expert answers its own assignment; handing
/// work to another agent belongs to whoever asked for this one.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertStep {
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
}

/// What an Expert's own reasoning has said or done so far.
///
/// This is the Expert's working transcript, not a Session: it starts at the
/// task it was given and ends when it answers. Nothing here outlives the
/// invocation, and no Session state reaches it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertTranscriptEntry {
    Task {
        text: String,
    },
    Preamble {
        text: String,
    },
    Capability {
        call_id: Uuid,
        capability_id: String,
        input: String,
        result: String,
    },
}

/// One step of an Expert's own reasoning.
pub struct ExpertReasoningStep {
    pub person_id: PersonId,
    pub invocation_id: Uuid,
    pub prompt: PromptAssembly,
    pub policy: InferencePolicyDecision,
    pub context: AgentContext,
    pub transcript: Vec<ExpertTranscriptEntry>,
    /// The capabilities the Expert is willing to be asked for on this step. An
    /// empty list is how it says it has none left to give.
    pub capabilities: Vec<CapabilityDescriptor>,
    pub replay: Vec<ModelReplay>,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

/// What the model did with one step.
pub struct ExpertStepOutcome {
    pub schema_version: u32,
    pub steps: Vec<ExpertStep>,
    pub replay: Option<ProviderReplay>,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

impl ExpertStepOutcome {
    /// How many of the step's outputs ask for a capability to be run.
    pub fn call_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, ExpertStep::Call { .. }))
            .count()
    }

    /// The replay receipt for one call in this step, if the provider gave one.
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

/// A model an Expert reasons with over several steps, calling its own bounded
/// capabilities in between.
///
/// The Expert decides what it offers and what it does with each answer. Whose
/// provider runs the step, how a failed attempt is recovered and which ledger
/// it is charged to stay on this side of the port, exactly as for one answer.
pub trait ExpertReasoner: ExpertModel {
    fn step<'a>(
        &'a self,
        step: ExpertReasoningStep,
    ) -> BoxFuture<'a, Result<ExpertStepOutcome, AgentFailure>>;
}

/// What counts as source-derived history in a transcript.
///
/// An owner that reads a source — through its own capability, or through an
/// Expert it delegated to — knows which ids those are. Conversation knows only
/// that once such a result is in the history, everything answered after it may
/// be derived from it, and that a compaction summary carries no provenance at
/// all.
pub trait SourceHistoryBoundary: Sync {
    /// A capability result that carried source data into the transcript.
    fn capability_carries_source(&self, capability_id: &str) -> bool;

    /// A delegated Expert that answered with source data.
    fn delegation_carries_source(
        &self,
        agent_id: &str,
        completed: bool,
        has_artifacts: bool,
    ) -> bool;
}
