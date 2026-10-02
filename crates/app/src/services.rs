use floe_conversation::RunState;
use floe_kernel::{AgentFailure, CommandId, RunId};
use uuid::Uuid;

use crate::CallerContext;

/// The largest request body this host will even look at. The canonical text
/// rule itself belongs to Conversation.
const MAX_TURN_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartTurn {
    pub command_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub mode: TurnMode,
    pub retry_of: Option<Uuid>,
    pub profile: ProfileSelection,
}

impl StartTurn {
    pub fn normalize_text(&mut self) -> Result<(), ServiceError> {
        self.text = normalize_turn_text(&self.text)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.command_id.is_nil()
            || self.session_id.is_nil()
            || self.text.len() > MAX_TURN_PAYLOAD_BYTES
            || normalize_turn_text(&self.text).is_err()
            || self.retry_of.is_some_and(|id| id.is_nil())
            || self.retry_of.is_some() && !matches!(&self.mode, TurnMode::New)
        {
            return Err(ServiceError::InvalidInput);
        }
        self.profile
            .validate()
            .map_err(|_| ServiceError::InvalidInput)?;
        match &self.mode {
            TurnMode::New => Ok(()),
            TurnMode::Continue(reference) => reference.validate(),
        }
    }
}

fn normalize_turn_text(text: &str) -> Result<String, ServiceError> {
    if text.len() > MAX_TURN_PAYLOAD_BYTES {
        return Err(ServiceError::InvalidInput);
    }
    floe_conversation::normalize_turn_text(text).map_err(|_| ServiceError::InvalidInput)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TurnMode {
    New,
    Continue(ContinuationRef),
}

/// Which model profile a turn runs on is Conversation's own choice of words.
pub use floe_conversation::ProfileSelection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuationRef {
    pub run_id: Uuid,
    pub executor_generation: u64,
    pub level: u8,
}

impl ContinuationRef {
    fn validate(&self) -> Result<(), ServiceError> {
        if self.run_id.is_nil() || self.executor_generation == 0 || !(1..=3).contains(&self.level) {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandReceipt {
    pub command_id: Uuid,
    pub run_id: Uuid,
    pub session_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelRun {
    pub command_id: Uuid,
    pub run_id: Uuid,
}

impl CancelRun {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.command_id.is_nil() || self.run_id.is_nil() {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelRunOutcome {
    Accepted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelRunReceipt {
    pub command_id: Uuid,
    pub run_id: Uuid,
    pub outcome: CancelRunOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionDecision {
    Approve,
    Deny,
    Dismiss,
}

/// Decide one interaction: the stable command id names the decision, the
/// reviewed digest binds what the person saw. No authority value crosses:
/// source, recipient, grant, profile and text all load from the stored
/// reviewed descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveInteraction {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub decision: InteractionDecision,
    pub target_digest: [u8; 32],
}

impl ResolveInteraction {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.command_id.is_nil()
            || self.interaction_id.is_nil()
            || self.session_id.is_nil()
            || self.expected_revision == 0
            || self.target_digest == [0; 32]
        {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

/// Reconcile one interaction without deciding it: satisfy, supersede or
/// rejoin, never approve.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshInteraction {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
}

impl RefreshInteraction {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.command_id.is_nil()
            || self.interaction_id.is_nil()
            || self.session_id.is_nil()
            || self.expected_revision == 0
        {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

/// Continue one origin's request explicitly: the backend derives intent
/// from the origin's durable admission, never from caller text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeInteraction {
    pub command_id: Uuid,
    pub session_id: Uuid,
    pub origin_run_id: Uuid,
    pub expected_revision: u64,
}

impl ResumeInteraction {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.command_id.is_nil() || self.session_id.is_nil() || self.origin_run_id.is_nil() {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolveInteractionOutcome {
    Resolved,
    Resolving,
    Denied,
    Cancelled,
    Superseded,
    Expired,
    Stale,
    Terminal,
    WrongDevice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshInteractionOutcome {
    Resolved,
    StillPending,
    Superseded,
    Terminal,
    Expired,
    Stale,
    WrongDevice,
}

/// One decision response: the current snapshot plus, when a child was
/// admitted, the standard linked Run/command receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveInteractionResult {
    pub command_id: Uuid,
    pub outcome: ResolveInteractionOutcome,
    pub interaction: floe_conversation::ConversationInteraction,
    pub replacement_id: Option<Uuid>,
    pub linked_run: Option<CommandReceipt>,
}

/// One refresh response: the reconciled snapshot plus, when refresh
/// completed the group, the standard linked Run/command receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshInteractionResult {
    pub command_id: Uuid,
    pub outcome: RefreshInteractionOutcome,
    pub interaction: floe_conversation::ConversationInteraction,
    pub replacement_id: Option<Uuid>,
    pub linked_run: Option<CommandReceipt>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    InvalidInput,
    NotFound,
    Conflict,
    AccessDenied,
    Unavailable,
    Internal,
}

pub trait ConversationCommands {
    fn start_turn(
        &self,
        caller: &CallerContext,
        request: StartTurn,
    ) -> Result<CommandReceipt, ServiceError>;

    fn cancel_run(
        &self,
        caller: &CallerContext,
        request: CancelRun,
    ) -> Result<CancelRunReceipt, ServiceError>;

    fn resolve_interaction(
        &self,
        caller: &CallerContext,
        request: ResolveInteraction,
    ) -> Result<ResolveInteractionResult, ServiceError>;

    fn refresh_interaction(
        &self,
        caller: &CallerContext,
        request: RefreshInteraction,
    ) -> Result<RefreshInteractionResult, ServiceError>;

    fn resume_interaction(
        &self,
        caller: &CallerContext,
        request: ResumeInteraction,
    ) -> Result<CommandReceipt, ServiceError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadConversation {
    Command { command_id: Uuid },
    Run { run_id: Uuid },
    Message { message_id: Uuid },
}

impl ReadConversation {
    pub fn validate(&self) -> Result<(), ServiceError> {
        let identifier = match self {
            Self::Command { command_id } => command_id,
            Self::Run { run_id } => run_id,
            Self::Message { message_id } => message_id,
        };
        if identifier.is_nil() {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

/// Maximum interaction snapshots returned by one Session list read.
pub const MAX_SESSION_INTERACTIONS: usize = 64;

pub trait ConversationQueries {
    fn read_conversation(
        &self,
        caller: &CallerContext,
        request: ReadConversation,
    ) -> Result<Option<floe_conversation::RunReceipt>, ServiceError>;

    /// Read one interaction snapshot. Read-only: never reconciles, admits
    /// or mutates.
    fn read_interaction(
        &self,
        caller: &CallerContext,
        interaction_id: Uuid,
    ) -> Result<Option<floe_conversation::ConversationInteraction>, ServiceError>;

    /// List one Session's interaction snapshots, oldest first, bounded.
    /// Read-only: never reconciles, admits or mutates.
    fn list_interactions(
        &self,
        caller: &CallerContext,
        session_id: Uuid,
    ) -> Result<Vec<floe_conversation::ConversationInteraction>, ServiceError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadConversationEvents {
    pub runtime_epoch: Option<u64>,
    pub cursor: Option<u64>,
    pub limit: u16,
}

impl ReadConversationEvents {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.limit == 0
            || self.limit > 256
            || self.runtime_epoch.is_some() != self.cursor.is_some()
            || self.runtime_epoch == Some(0)
        {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

pub trait ConversationEvents {
    fn read_conversation_events(
        &self,
        caller: &CallerContext,
        request: ReadConversationEvents,
    ) -> Result<EventRead, ServiceError>;
}

/// The calendar actions one caller may see, with the authority they stand
/// under when the caller is allowed to know it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CalendarActionsResult {
    pub actions: Vec<floe_actions::CalendarAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<floe_actions::ActionAuthority>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventPayload {
    CommandUpdated {
        command_id: CommandId,
        run_id: RunId,
        session_revision: u64,
    },
    RunUpdated(RunEventRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunEventRecord {
    pub run_id: RunId,
    pub session_id: Uuid,
    pub aggregate_revision: u64,
    pub executor_generation: u64,
    pub state: RunState,
    pub generated_reply: bool,
    pub issue: Option<AgentFailure>,
    pub attempt_refs: Vec<Uuid>,
    pub task_refs: Vec<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationEvent {
    pub cursor: u64,
    pub aggregate_revision: u64,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventRead {
    Events {
        next_cursor: u64,
        events: Vec<ConversationEvent>,
    },
    ResyncRequired {
        snapshot_cursor: u64,
    },
}
