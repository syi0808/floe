use floe_agent_contract::{
    AllowedCatalog, DelegationExecutionContext, DelegationPort, ModelPort, ModelProjectionPort,
    ReplayReceipt, RoleSpec, ToolPort,
};
use floe_agent_runtime::FinalPayloadValidator;
use floe_execution::{Cancellation, budget::BudgetConfig};
use floe_kernel::{AgentFailure, CommandId, RunId};
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{CanonicalTurnIntent, ProfileSelection, TurnMode};

pub const FINALIZATION_ROLE_ID: &str = "manager.finalization";
pub const FINALIZATION_ROLE_PROMPT: &str = "Produce one final answer using only the supplied settled observations. Do not call tools or delegate.";
pub const FINALIZATION_OUTPUT_CONTRACT: &str = "Return one concise user-facing answer. State that the requested execution did not complete; do not claim that a failed action succeeded.";
pub const MANAGER_OUTPUT_CONTRACT: &str =
    "Return one user-facing answer or one registered delegation.";
pub const CONVERSATION_MODEL_CONSUMER: &str = "conversation.root";
/// Deterministic source-independent limitation for a blocked model dispatch.
///
/// A Manager cannot explain a denial by calling the very unapproved model,
/// so Conversation completes the original Run with this controlled copy plus
/// the durable interaction. Fixed text: no source names, no recipient
/// interpolation, no model output. The card discloses the exact review.
pub const MODEL_CONSENT_LIMITATION: &str = "The selected model needs your approval before it can process this request. No data has been sent. Review the pending request to continue.";

#[derive(Clone, Debug)]
pub struct ManagerConfig {
    pub role_spec: RoleSpec,
    pub purpose: String,
    pub max_iterations: u32,
    pub max_output_bytes: usize,
    pub max_run_duration: Duration,
    pub budget: BudgetConfig,
}

impl ManagerConfig {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.role_spec.validate()?;
        if self.role_spec.role_id != "manager"
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.max_iterations == 0
            || self.max_iterations > 64
            || self.max_output_bytes == 0
            || self.max_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
            || self.max_run_duration.is_zero()
            || self.max_run_duration > Duration::from_secs(300)
            || self.budget.max_tokens == 0
            || self.budget.max_cost_micros == 0
            || self.budget.finalization_tokens > self.budget.max_tokens
            || self.budget.finalization_cost_micros > self.budget.max_cost_micros
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub principal: String,
    pub prompt: String,
    pub mode: TurnMode,
    pub retry_of: Option<RunId>,
    pub profile: ProfileSelection,
    pub allowed_catalog: AllowedCatalog,
    pub replay: Vec<ReplayReceipt>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
    /// Runtime-only delegation host context, forwarded to the Engine without
    /// interpretation. Never part of the canonical turn intent or request
    /// identity, exactly like device/credential state.
    pub delegation_context: Option<DelegationExecutionContext>,
    /// The verified calling device, supplied by admitted App execution.
    /// Runtime-only (never part of intent): binds a published model
    /// blockage to this device so a foreign device can never decide it.
    pub device_id: String,
    /// Wall-clock milliseconds for durable interaction publication when a
    /// model dispatch blocks. Runtime-only (never part of intent).
    pub now_unix_ms: i64,
}

impl TurnRequest {
    pub(crate) fn validate(&self) -> Result<(), AgentFailure> {
        if !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.replay.len() > 128
            || self.retry_of.is_some_and(|run_id| !run_id.is_valid())
            || self.retry_of.is_some() && !matches!(&self.mode, TurnMode::New)
            || self.device_id.trim() != self.device_id
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.chars().any(char::is_control)
            || self.now_unix_ms < 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        crate::normalize_turn_text(&self.prompt)?;
        self.profile.validate()?;
        if let Some(context) = &self.delegation_context {
            context.validate()?;
        }
        match &self.mode {
            TurnMode::New => {}
            TurnMode::Continue(reference) => reference.validate()?,
            TurnMode::Resume(reference) => reference.validate()?,
        }
        self.allowed_catalog
            .cards
            .iter()
            .try_for_each(floe_agent_contract::AgentDefinition::validate)?;
        self.allowed_catalog
            .tools
            .iter()
            .try_for_each(floe_agent_contract::ToolDescriptor::validate)
    }

    pub(crate) fn canonical_intent(&self) -> Result<CanonicalTurnIntent, AgentFailure> {
        let mut turn = crate::StartTurn {
            command_id: self.command_id,
            session_id: self.session_id,
            expected_revision: self.expected_session_revision,
            text: self.prompt.clone(),
            mode: self.mode.clone(),
            retry_of: self.retry_of,
            profile: self.profile.clone(),
        };
        CanonicalTurnIntent::from_start_turn(&mut turn)
    }
}

#[derive(Clone, Copy)]
pub struct ConversationPorts<'a> {
    pub projection: &'a dyn ModelProjectionPort,
    pub coverage_resolver: &'a dyn floe_context::DependencyResolver,
    pub model: &'a dyn ModelPort,
    pub tools: &'a dyn ToolPort,
    pub delegation: &'a dyn DelegationPort,
    pub validator: &'a dyn FinalPayloadValidator,
}
