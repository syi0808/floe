use floe_agent_contract::{
    AgentMessage, BatchCursor, DependencyCoverage, JournalEvent, ModelConversation, ReplayReceipt,
    ValidatedModelBatch,
};
use floe_kernel::{AgentFailure, CommandId, RunId};
use uuid::Uuid;

mod intent;
mod purpose;
pub use purpose::{CONVERSATION_CONSUMER, CONVERSATION_PURPOSE};
mod interaction;
mod run_record;
mod source_review;
pub use run_record::{
    PriorExhaustion, BlockedInteractionLink, RunBlockOrigin, RunBlockRecord, RunReceipt, RunRecord, RunState, RunTerminal,
    UnresolvedModelAttempt,
};
pub use source_review::{
    BlockedRunCommit, InteractionResolutionCommit, OwnerResolutionReceipt,
    BlockedReviewEvidence, SourceReviewLink, ReviewPublication, ReviewAuditRecord,
    ResumeChildAdmission, ResumeRequired,
};

pub use intent::{
    AdmittedExecution, CanonicalTurnIntent, ContinuationToken, MAX_TURN_TEXT_BYTES, StartTurn,
    normalize_turn_text,
};
pub use interaction::{
    ConversationInteraction, DecisionAdmission, ExpireInteraction,
    ExpireOutcome, INTERACTION_PENDING_LIFETIME_MS, InteractionDecision, InteractionDecisionKind,
    InteractionOrigin, InteractionRefresh, InteractionRequirement, InteractionRequirementKind,
    InteractionResolution, InteractionResolutionReceipt, InteractionResumeRef, InteractionState,
    MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_RESUME_LINEAGE, MAX_REVIEWED_IDENTIFIER_BYTES,
    MAX_REVIEWED_PURPOSE_BYTES, MAX_REVIEWED_SOURCE_BYTES, MAX_REVIEWED_TARGET_BYTES,
    MAX_STORED_INTERACTIONS_PER_RUN, MAX_TARGET_BUNDLE_MEMBERS, NavigationDestination,
    NavigationOnlyTarget, ReviewedTarget, SupersedeInteraction,
    canonical_requirement_digest, canonical_target_digest, decision_owner_command_id,
    interaction_publication_id, next_state_after_decision, resume_command_id,
    state_after_resolution,
};

pub const MAX_COMPACTION_SUMMARY_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartSessionRequest {
    pub principal: String,
    pub command_id: CommandId,
}
impl StartSessionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal)?;
        if !self.command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRequest {
    pub principal: String,
}

impl SessionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionReadRequest {
    pub principal: String,
    pub session_id: Uuid,
}

impl SessionReadRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal)?;
        if self.session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionReceipt {
    pub principal: String,
    pub session_id: Uuid,
    pub session_revision: u64,
}

impl SessionReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal).map_err(|_| AgentFailure::StorageUnavailable)?;
        if self.session_id.is_nil() {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

fn validate_principal(principal: &str) -> Result<(), AgentFailure> {
    if principal.trim() != principal
        || principal.is_empty()
        || principal.len() > 256
        || principal.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandQuery {
    pub principal: String,
    pub command_id: CommandId,
}

impl CommandQuery {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal)?;
        if !self.command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunQuery {
    pub principal: String,
    pub run_id: RunId,
}

impl RunQuery {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal)?;
        if !self.run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuationRef {
    pub run_id: RunId,
    pub executor_generation: u64,
    pub level: u8,
}

impl ContinuationRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.run_id.is_valid()
            || self.executor_generation == 0
            || self.level == 0
            || self.level > 3
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TurnMode {
    #[default]
    New,
    Continue(ContinuationRef),
    Resume(InteractionResumeRef),
}

#[derive(Clone, Debug)]
pub enum TurnInput {
    NewMessage(AgentMessage),
    ExistingMessage { message_id: Uuid },
}

#[derive(Clone, Debug)]
pub struct TurnAdmissionRequest {
    pub expert_environment: floe_experts::RunExpertEnvironmentIdentity,
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub principal: String,
    pub device_id: String,
    pub request_digest: [u8; 32],
    pub mode: TurnMode,
    pub retry_of: Option<RunId>,
    pub input: TurnInput,
}

impl TurnAdmissionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.expert_environment.validate()?;
        match (&self.mode, &self.input) {
            (TurnMode::New, TurnInput::NewMessage(message)) => {
                message.validate()?;
                if message.message_id != self.command_id.as_uuid()
                    || message.role != floe_agent_contract::MessageRole::User
                    || message.call_id.is_some()
                    || message.coverage != DependencyCoverage::Independent
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            (
                TurnMode::Continue(_) | TurnMode::Resume(_),
                TurnInput::ExistingMessage { message_id },
            ) if !message_id.is_nil() => {}
            _ => return Err(AgentFailure::InvalidInput),
        }
        if !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.trim() != self.device_id
            || self.device_id.chars().any(char::is_control)
            || self.request_digest == [0; 32]
            || self.retry_of.is_some_and(|run_id| !run_id.is_valid())
            || self.retry_of.is_some() && !matches!(&self.mode, TurnMode::New)
        {
            return Err(AgentFailure::InvalidInput);
        }
        match &self.mode {
            TurnMode::New => {}
            TurnMode::Continue(reference) => reference.validate()?,
            TurnMode::Resume(reference) => reference.validate()?,
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AdmittedTurn {
    pub receipt: RunReceipt,
    pub transcript: Vec<AgentMessage>,
}

impl AdmittedTurn {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.receipt.validate()?;
        if self.transcript.is_empty()
            || self.transcript.len() > floe_agent_contract::MAX_AGENT_MESSAGES
            || self
                .transcript
                .iter()
                .any(|message| message.validate().is_err())
            || if self.receipt.continuation_of.is_some() || self.receipt.resume_of.is_some() {
                self.transcript
                    .iter()
                    .any(|message| message.message_id == self.receipt.command_id.as_uuid())
                    || !self
                        .transcript
                        .iter()
                        .any(|message| message.role == floe_agent_contract::MessageRole::User)
            } else {
                !self.transcript.iter().any(|message| {
                    message.message_id == self.receipt.command_id.as_uuid()
                        && message.role == floe_agent_contract::MessageRole::User
                })
            }
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum TurnAdmission {
    Created(AdmittedTurn),
    Existing(RunReceipt),
    /// The origin slot already admitted this resume under another command.
    /// The receipt is the canonical child; its digest is the winner's and
    /// is never compared against the loser's request.
    Resumed(RunReceipt),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub principal: String,
}

impl RecoveryRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReceipt {
    pub session_id: Uuid,
    pub session_revision: u64,
}

pub use floe_agent_contract::JournalEntry;

#[derive(Clone, Debug)]
pub struct ContinuationSnapshot {
    pub user_message_id: Uuid,
    pub expert_environment: floe_experts::RunExpertEnvironmentIdentity,
    pub reference: ContinuationRef,
    pub session_id: Uuid,
    pub session_revision: u64,
    /// History from the durable transcript plus the settled exchanges of the
    /// continued execution. The continuing turn prepends its own user message;
    /// until then the current turn carries exchanges only.
    pub model_conversation: ModelConversation,
    pub replay: Vec<ReplayReceipt>,
    /// A validated batch that never completed, with its cursor. The batch keeps
    /// its original execution id; step identity is never re-rooted at the
    /// resuming run.
    pub pending_batch: Option<ValidatedModelBatch>,
    pub batch_cursor: Option<BatchCursor>,
    pub completed_iterations: u32,
    pub usage: floe_execution::budget::ModelUsage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionRequest {
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub principal: String,
    pub through_turn_id: Uuid,
    pub summary: String,
}

impl CompactionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil()
            || self.expected_session_revision == 0
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.through_turn_id.is_nil()
            || self.summary.trim() != self.summary
            || self.summary.is_empty()
            || self.summary.len() > MAX_COMPACTION_SUMMARY_BYTES
            || self
                .summary
                .chars()
                .any(|character| character.is_control() && character != '\n')
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionReceipt {
    pub session_id: Uuid,
    pub session_revision: u64,
    pub pointer: floe_agent_contract::ArchivePointer,
    pub summary: AgentMessage,
}

impl CompactionReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.pointer
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        self.summary
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if self.session_id.is_nil()
            || self.session_revision == 0
            || self.summary.message_id != self.pointer.through_turn_id
            || self.summary.role != floe_agent_contract::MessageRole::Assistant
            || self.summary.call_id.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

impl RecoveryReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil() {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}
