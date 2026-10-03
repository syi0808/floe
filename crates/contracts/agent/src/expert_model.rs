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

use floe_context_contract::DataClass;
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};

use crate::{
    AgentContext, InferencePolicyDecision, ModelReplay, ProviderReplay, SourceProjectionReview, ports::BoxFuture,
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
#[derive(Clone, Debug)]
pub struct ExpertModelAnswer {
    pub schema_version: u32,
    pub answer: String,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

/// What one Expert model call produced: the single answer, or the exact
/// dispatch the model owner blocked on before any transmission.
///
/// `Blocked` is a distinct typed expected completion, not a failure: the
/// Expert reports a blocked-domain judgment and stops, while the trusted
/// host publishes the requirement's card and attaches the durable refs.
/// Hard failures (policy prohibition, transport errors, invalid input)
/// stay `Err(AgentFailure)`.
#[derive(Clone, Debug)]
pub enum ExpertModelOutcome {
    Answered(ExpertModelAnswer),
    Blocked(SourceProjectionReview),
}

/// The model an Expert reasons on.
pub trait ExpertModel: Sync {
    /// Answer one assignment.
    ///
    /// Anything but a single answer is `InvalidModelOutput`: the caller asked
    /// one question and is owed one reply. The shared planner selects the model
    /// before source projection; `Blocked` carries a pre-attempt source review.
    fn answer<'a>(
        &'a self,
        call: ExpertModelCall,
    ) -> BoxFuture<'a, Result<ExpertModelOutcome, AgentFailure>>;
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

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != crate::AGENT_SCHEMA_VERSION
            || self.id.trim().is_empty()
            || self.version.trim().is_empty()
            || self
                .input_schema
                .as_ref()
                .is_some_and(|schema| !schema.is_object())
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
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
        observation: ExpertCapabilityObservation,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertCapabilityObservation {
    Success {
        result: String,
    },
    Unavailable {
        reason_code: String,
    },
    NeedsUserAction {
        interaction: crate::UserInteractionRef,
        summary: String,
    },
}

impl ExpertCapabilityObservation {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        let valid = match self {
            Self::Success { result } => crate::message::bounded(result, maximum_bytes),
            Self::Unavailable { reason_code } => crate::message::bounded(reason_code, 128),
            Self::NeedsUserAction {
                interaction,
                summary,
            } => {
                interaction.validate().is_ok()
                    && crate::message::bounded(summary, maximum_bytes.min(512))
            }
        };
        if valid {
            Ok(())
        } else {
            Err(AgentFailure::InvalidModelOutput)
        }
    }
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
#[derive(Clone, Debug)]
pub struct ExpertStepOutcome {
    pub schema_version: u32,
    pub steps: Vec<ExpertStep>,
    pub replay: Option<ProviderReplay>,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

impl ExpertStepOutcome {
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

    /// How many of the step's outputs ask for a capability to be run.
    pub fn call_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, ExpertStep::Call { .. }))
            .count()
    }
}

/// What one Expert reasoning step produced: the step outcome, or the exact
/// dispatch the model owner blocked on before any transmission.
///
/// Same contract as [`ExpertModelOutcome`]: the Expert reports a
/// blocked-domain judgment and stops; the trusted host publishes the card.
#[derive(Clone, Debug)]
pub enum ExpertStepResult {
    Stepped(ExpertStepOutcome),
    Blocked(SourceProjectionReview),
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
    ) -> BoxFuture<'a, Result<ExpertStepResult, AgentFailure>>;
}
