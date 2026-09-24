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
            TurnMode::Resume(reference) => reference.validate(),
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
    Resume(ResumeRef),
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
pub struct ResumeRef {
    pub origin_run_id: Uuid,
    pub lineage: u8,
}

impl ResumeRef {
    fn validate(&self) -> Result<(), ServiceError> {
        if self.origin_run_id.is_nil()
            || self.lineage == 0
            || self.lineage > floe_conversation::MAX_RESUME_LINEAGE
        {
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

pub trait ConversationQueries {
    fn read_conversation(
        &self,
        caller: &CallerContext,
        request: ReadConversation,
    ) -> Result<Option<floe_conversation::RunReceipt>, ServiceError>;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StartTurn {
        StartTurn {
            command_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            expected_revision: 0,
            text: "hello".into(),
            mode: TurnMode::New,
            retry_of: None,
            profile: ProfileSelection::Auto,
        }
    }

    #[test]
    fn start_turn_rejects_invalid_ids_text_and_continuation() {
        assert_eq!(request().validate(), Ok(()));
        let mut invalid = request();
        invalid.command_id = Uuid::nil();
        assert_eq!(invalid.validate(), Err(ServiceError::InvalidInput));
        let mut invalid = request();
        invalid.text = " ".into();
        assert_eq!(invalid.validate(), Err(ServiceError::InvalidInput));
        let mut invalid = request();
        invalid.mode = TurnMode::Continue(ContinuationRef {
            run_id: Uuid::new_v4(),
            executor_generation: 0,
            level: 1,
        });
        assert_eq!(invalid.validate(), Err(ServiceError::InvalidInput));
        let mut invalid = request();
        invalid.retry_of = Some(Uuid::new_v4());
        invalid.mode = TurnMode::Continue(ContinuationRef {
            run_id: Uuid::new_v4(),
            executor_generation: 1,
            level: 1,
        });
        assert_eq!(invalid.validate(), Err(ServiceError::InvalidInput));
    }

    #[test]
    fn start_turn_normalizes_canonical_text_before_forwarding() {
        let mut request = request();
        request.text = "\thello\t".into();
        assert_eq!(request.validate(), Ok(()));
        request.normalize_text().unwrap();
        assert_eq!(request.text, "hello");

        request.text = "hello\tworld".into();
        assert_eq!(request.validate(), Err(ServiceError::InvalidInput));

        request.text = "한".repeat(2_730) + "ab";
        assert_eq!(request.validate(), Ok(()));
        request.text.push('c');
        assert_eq!(request.validate(), Err(ServiceError::InvalidInput));
    }

    #[test]
    fn cancel_run_rejects_invalid_ids() {
        let request = CancelRun {
            command_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
        };
        assert_eq!(request.validate(), Ok(()));
        assert_eq!(
            CancelRun {
                command_id: Uuid::nil(),
                ..request
            }
            .validate(),
            Err(ServiceError::InvalidInput)
        );
    }

    #[test]
    fn read_services_validate_ids_and_cursor_bounds_without_credentials() {
        for request in [
            ReadConversation::Command {
                command_id: Uuid::nil(),
            },
            ReadConversation::Run {
                run_id: Uuid::nil(),
            },
            ReadConversation::Message {
                message_id: Uuid::nil(),
            },
        ] {
            assert_eq!(request.validate(), Err(ServiceError::InvalidInput));
        }
        let valid = ReadConversationEvents {
            runtime_epoch: Some(7),
            cursor: Some(0),
            limit: 256,
        };
        assert_eq!(valid.validate(), Ok(()));
        for invalid in [
            ReadConversationEvents {
                limit: 0,
                ..valid.clone()
            },
            ReadConversationEvents {
                limit: 257,
                ..valid.clone()
            },
            ReadConversationEvents {
                runtime_epoch: Some(0),
                ..valid.clone()
            },
            ReadConversationEvents {
                runtime_epoch: None,
                ..valid.clone()
            },
            ReadConversationEvents {
                cursor: None,
                ..valid.clone()
            },
        ] {
            assert_eq!(invalid.validate(), Err(ServiceError::InvalidInput));
        }
        let debug = format!(
            "{:?} {:?} {:?}",
            request(),
            valid,
            ReadConversation::Run {
                run_id: Uuid::new_v4()
            }
        );
        for secret in ["token", "bearer", "base_url", "credential"] {
            assert!(!debug.contains(secret));
        }
    }
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
