//! The one bounded model call an Expert makes on its own assignment.
//!
//! An Expert reasons once over context it was authorized for and returns one
//! answer. It does not drive a conversation, call capabilities, delegate, or
//! carry a Session: those belong to whoever runs the root turn. Everything on
//! the other side of this port — which provider answers, how an attempt is
//! retried, and who accounts for the tokens it spent — belongs to the model's
//! owner, and no Expert names any of it.

use tokio::time::Instant;
use uuid::Uuid;

use floe_context_contract::ModelPlacement;
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};

use crate::{AgentContext, InferencePolicyDecision, ports::BoxFuture, prompts::PromptAssembly};

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
