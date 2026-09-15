use uuid::Uuid;

use crate::CallerContext;

const MAX_TURN_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartTurn {
    pub command_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub mode: TurnMode,
    pub retry_of: Option<Uuid>,
}

impl StartTurn {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.command_id.is_nil()
            || self.session_id.is_nil()
            || self.text.trim().is_empty()
            || self.text.len() > MAX_TURN_TEXT_BYTES
            || self.retry_of.is_some_and(|id| id.is_nil())
            || self.retry_of.is_some() && !matches!(&self.mode, TurnMode::New)
        {
            return Err(ServiceError::InvalidInput);
        }
        match &self.mode {
            TurnMode::New => Ok(()),
            TurnMode::Continue(reference) => reference.validate(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TurnMode {
    New,
    Continue(ContinuationRef),
}

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
}
