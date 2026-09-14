use floe_agent_contract::{
    AgentMessage, DependencyCoverage, EngineStep, JournalEvent, ReplayReceipt,
};
use floe_kernel::{AgentFailure, CommandId, RunId};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunState {
    Working,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
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
}

impl RunState {
    pub fn is_terminal(self) -> bool {
        self != Self::Working
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunReceipt {
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub principal: String,
    pub request_digest: [u8; 32],
    pub state: RunState,
    pub output: Option<String>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub session_revision: u64,
    pub aggregate_revision: u64,
    pub executor_generation: u64,
    pub continuation_of: Option<RunId>,
    pub continuation_level: u8,
    pub execution_profile: String,
}

impl RunReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.request_digest == [0; 32]
            || self.session_revision == 0
            || self.aggregate_revision == 0
            || self.executor_generation == 0
            || self.continuation_level > 3
            || self.execution_profile.trim() != self.execution_profile
            || self.execution_profile.is_empty()
            || self.execution_profile.len() > 64
            || self.execution_profile.chars().any(char::is_control)
            || self.coverage.validate().is_err()
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if self.continuation_of.is_some() != (self.continuation_level > 0) {
            return Err(AgentFailure::StorageUnavailable);
        }
        let valid = match self.state {
            RunState::Working => {
                self.output.is_none()
                    && self.issue.is_none()
                    && self.coverage == DependencyCoverage::Unknown
            }
            RunState::Completed => {
                self.output
                    .as_deref()
                    .is_some_and(|output| !output.trim().is_empty())
                    && self.issue.is_none()
            }
            RunState::Failed | RunState::Cancelled | RunState::TimedOut | RunState::Interrupted => {
                self.output.is_none()
                    && self.issue.is_some()
                    && self.coverage == DependencyCoverage::Unknown
            }
        };
        valid.then_some(()).ok_or(AgentFailure::StorageUnavailable)
    }

    pub fn continuation(&self) -> Option<ContinuationRef> {
        matches!(
            (self.state, self.issue),
            (RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
                | (RunState::Failed, Some(AgentFailure::BudgetExceeded))
        )
        .then(|| self.continuation_level.checked_add(1))
        .flatten()
        .filter(|level| *level <= 3)
        .map(|level| ContinuationRef {
            run_id: self.run_id,
            executor_generation: self.executor_generation,
            level,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TurnAdmissionRequest {
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub principal: String,
    pub request_digest: [u8; 32],
    pub mode: TurnMode,
    pub execution_profile: String,
    pub user_message: AgentMessage,
}

impl TurnAdmissionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.user_message.validate()?;
        if !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.request_digest == [0; 32]
            || self.execution_profile.trim() != self.execution_profile
            || self.execution_profile.is_empty()
            || self.execution_profile.len() > 64
            || self.execution_profile.chars().any(char::is_control)
            || self.user_message.message_id != self.command_id.as_uuid()
            || self.user_message.role != floe_agent_contract::MessageRole::User
            || self.user_message.call_id.is_some()
            || self.user_message.coverage != DependencyCoverage::Independent
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let TurnMode::Continue(reference) = &self.mode {
            reference.validate()?;
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
            || if self.receipt.continuation_of.is_some() {
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub principal: String,
}

impl RecoveryRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil()
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

#[derive(Clone, Debug)]
pub struct JournalEntry {
    pub revision: u64,
    pub event: JournalEvent,
}

#[derive(Clone, Debug)]
pub struct ContinuationSnapshot {
    pub reference: ContinuationRef,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub execution_profile: String,
    pub messages: Vec<AgentMessage>,
    pub replay: Vec<ReplayReceipt>,
    pub completed_iterations: u32,
    pub usage: floe_execution::budget::ModelUsage,
}

impl RecoveryReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil() {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RunTerminal {
    pub state: RunState,
    pub output: Option<String>,
    pub steps: Vec<EngineStep>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
}

impl RunTerminal {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.state.is_terminal()
            || self.coverage.validate().is_err()
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
            || self.steps.len() > floe_agent_contract::MAX_AGENT_MESSAGES
        {
            return Err(AgentFailure::InvalidInput);
        }
        let valid = match self.state {
            RunState::Completed => {
                self.output
                    .as_deref()
                    .is_some_and(|output| !output.trim().is_empty())
                    && self.issue.is_none()
                    && self.steps.last().is_some_and(|step| {
                        matches!(step, EngineStep::Answer { text, .. } if Some(text) == self.output.as_ref())
                    })
            }
            RunState::Failed | RunState::Cancelled | RunState::TimedOut | RunState::Interrupted => {
                self.output.is_none()
                    && self.steps.is_empty()
                    && self.issue.is_some()
                    && self.coverage == DependencyCoverage::Unknown
            }
            RunState::Working => false,
        };
        valid.then_some(()).ok_or(AgentFailure::InvalidInput)
    }

    pub(crate) fn from_failure(failure: AgentFailure) -> Self {
        let state = match failure {
            AgentFailure::Cancelled => RunState::Cancelled,
            AgentFailure::DeadlineExceeded => RunState::TimedOut,
            AgentFailure::Interrupted => RunState::Interrupted,
            _ => RunState::Failed,
        };
        Self {
            state,
            output: None,
            steps: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: Some(failure),
        }
    }
}
