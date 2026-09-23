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
    AgentContext, InferencePolicyDecision, ModelReplay, ProviderReplay, ports::BoxFuture,
    prompts::PromptAssembly,
};

/// The consumer delegated built-in Experts share on canonical Inference.
///
/// Experts run under the everyday-assistance purpose with their own consumer,
/// never as `conversation.root`: the root profile stays root-only while the
/// product purpose still describes delegated Expert work.
pub const EXPERT_INFERENCE_CONSUMER: &str = "experts.builtin";

/// What execution class an Expert requires.
///
/// Experts state the class; Inference selects the provider. An Expert whose
/// judgment needs one class states it and lets dispatch fail closed when no
/// profile satisfies it, rather than silently accepting another class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpertModelRequirement {
    Any,
    DeviceOnly,
    RemoteOnly,
}

/// One assignment, with the context and the bounds it must be answered under.
pub struct ExpertModelCall {
    pub person_id: PersonId,
    /// The Expert invocation this call belongs to.
    pub invocation_id: Uuid,
    pub prompt: PromptAssembly,
    pub policy: InferencePolicyDecision,
    pub context: AgentContext,
    pub assignment: String,
    /// What execution class this call requires. Carried as intent; the model
    /// owner maps it to an execution constraint, never to a provider.
    pub requirement: ExpertModelRequirement,
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
    /// Answer one assignment.
    ///
    /// Anything but a single answer is `InvalidModelOutput`: the caller asked
    /// one question and is owed one reply. The call's requirement states what
    /// execution class the Expert needs; which provider satisfies it is the
    /// model owner's answer, never the Expert's.
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
    /// What execution class this step requires. Carried as intent; the model
    /// owner maps it to an execution constraint, never to a provider.
    pub requirement: ExpertModelRequirement,
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

#[cfg(test)]
mod observation_tests {
    use super::*;

    #[test]
    fn failed_capability_observations_are_bounded_and_typed() {
        assert_eq!(
            ExpertCapabilityObservation::Unavailable {
                reason_code: "".into(),
            }
            .validate(4096),
            Err(AgentFailure::InvalidModelOutput)
        );
        assert_eq!(
            ExpertCapabilityObservation::NeedsUserAction {
                interaction: crate::UserInteractionRef {
                    interaction_id: Uuid::nil(),
                    kind: crate::UserInteractionKind::SourceAccess,
                    status: crate::UserInteractionStatus::Pending,
                },
                summary: "Calendar access needs approval".into(),
            }
            .validate(4096),
            Err(AgentFailure::InvalidModelOutput)
        );
        assert!(ExpertCapabilityObservation::Unavailable {
            reason_code: "temporarily_unavailable".into(),
        }.validate(4096).is_ok());
    }
}
