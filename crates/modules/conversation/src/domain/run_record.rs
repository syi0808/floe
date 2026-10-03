use super::{
    ContinuationRef, InteractionOrigin, InteractionResumeRef, MAX_ACTIVE_INTERACTIONS_PER_RUN,
    MAX_RESUME_LINEAGE,
};
use floe_agent_contract::{DependencyCoverage, EngineStep};
use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorExhaustion {
    BudgetExceeded,
    Stalled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunBlockOrigin {
    pub session_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub run_id: RunId,
    pub executor_generation: u64,
    pub origin: InteractionOrigin,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedInteractionLink {
    pub interaction_id: Uuid,
    pub origin: RunBlockOrigin,
    pub target: super::ReviewedTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunBlockRecord {
    pub review_group_id: Uuid,
    pub interactions: Vec<BlockedInteractionLink>,
    pub prior_exhaustion: Option<PriorExhaustion>,
}

impl RunBlockRecord {
    pub fn interaction_refs(&self) -> Vec<Uuid> {
        self.interactions.iter().map(|link| link.interaction_id).collect()
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.review_group_id.is_nil()
            || self.interactions.is_empty()
            || self.interactions.len() > MAX_ACTIVE_INTERACTIONS_PER_RUN
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut seen = std::collections::HashSet::new();
        for link in &self.interactions {
            let origin = &link.origin;
            origin.origin.validate()?;
            link.target.validate()?;
            if link.interaction_id.is_nil()
                || !seen.insert(link.interaction_id)
                || origin.session_id.is_nil()
                || origin.person_id.0.is_nil()
                || !origin.run_id.is_valid()
                || origin.executor_generation == 0
                || origin.device_id.is_empty()
                || origin.device_id.len() > 256
                || origin.device_id.trim() != origin.device_id
                || origin.device_id.chars().any(char::is_control)
                || matches!(&origin.origin, InteractionOrigin::Projection { run_id, .. } if *run_id != origin.run_id)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Working,
    Blocked,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

impl RunState {
    pub fn is_terminal(self) -> bool {
        self != Self::Working
    }
}

/// An immutable request to stop a Run whose dispatched Task evidence has not
/// settled yet. The Session remains active until the exact receipt is attached.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRunTerminal {
    pub failure: AgentFailure,
    pub requested_from_revision: u64,
}

impl PendingRunTerminal {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.requested_from_revision != 1 { return Err(AgentFailure::StorageUnavailable); }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunReceipt {
    pub expert_environment: floe_experts::RunExpertEnvironmentIdentity,
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub principal: String,
    pub device_id: String,
    pub user_message_id: Uuid,
    pub request_digest: [u8; 32],
    pub state: RunState,
    pub pending_terminal: Option<PendingRunTerminal>,
    pub output: Option<String>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub blocked: Option<RunBlockRecord>,
    pub session_revision: u64,
    pub aggregate_revision: u64,
    pub executor_generation: u64,
    pub continuation_of: Option<RunId>,
    pub continuation_executor_generation: Option<u64>,
    pub continuation_level: u8,
    pub retry_of: Option<RunId>,
    pub resume_of: Option<RunId>,
    pub resume_lineage: u8,
    pub attempt_refs: Vec<Uuid>,
    pub unresolved_attempts: Vec<UnresolvedModelAttempt>,
    pub unresolved_delegations: Vec<floe_agent_contract::TaskId>,
    pub task_refs: Vec<Uuid>,
}

impl RunReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.expert_environment
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.user_message_id.is_nil()
            || self.request_digest == [0; 32]
            || self.session_revision == 0
            || self.aggregate_revision == 0
            || self.executor_generation == 0
            || self.continuation_level > 3
            || self.retry_of == Some(self.run_id)
            || self.resume_of == Some(self.run_id)
            || self.resume_lineage > MAX_RESUME_LINEAGE
            || self.attempt_refs.len() > floe_execution::budget::MAX_MODEL_ATTEMPTS_PER_SCOPE
            || (!self.unresolved_attempts.is_empty()
                && matches!(self.state, RunState::Completed | RunState::Blocked))
            || self.unresolved_attempts.len() > floe_execution::budget::MAX_MODEL_ATTEMPTS_PER_SCOPE
            || self.unresolved_attempts.iter().any(|attempt| {
                !self.attempt_refs.contains(&attempt.attempt_id)
                    || attempt.reservation_ceiling.validate().is_err()
                    || attempt.accounting.observed_tokens.is_some()
                    || attempt.accounting.observed_cost_micros.is_some()
                    || !attempt.accounting.unknown_tokens
                    || !attempt.accounting.unknown_cost
            })
            || self.task_refs.len() > 64
            || self.unresolved_delegations.len() > 64
            || (!self.unresolved_delegations.is_empty() && matches!(self.state, RunState::Completed | RunState::Blocked))
            || self.unresolved_delegations.iter().any(|id| !id.is_valid()
                || !self.task_refs.contains(&id.as_uuid()))
            || self.unresolved_delegations.iter().collect::<std::collections::HashSet<_>>().len()
                != self.unresolved_delegations.len()
            || self.pending_terminal.is_some_and(|pending| pending.validate().is_err()
                || self.state != RunState::Working || self.aggregate_revision < 2)
            || self.attempt_refs.iter().any(Uuid::is_nil)
            || self.task_refs.iter().any(Uuid::is_nil)
            || self
                .attempt_refs
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.attempt_refs.len()
            || self
                .task_refs
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.task_refs.len()
            || self.coverage.validate().is_err()
            || self
                .blocked
                .as_ref()
                .is_some_and(|record| record.validate().is_err())
            || self.blocked.is_some() != (self.state == RunState::Blocked)
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if self.continuation_of.is_some() != (self.continuation_level > 0)
            || self.continuation_executor_generation.is_some() != (self.continuation_level > 0)
            || self.continuation_executor_generation == Some(0)
            || self.resume_of.is_some() != (self.resume_lineage > 0)
            || self.resume_of.is_some()
                && (self.continuation_of.is_some()
                    || self.continuation_level > 0
                    || self.retry_of.is_some())
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let valid = match self.state {
            RunState::Working => {
                self.output.is_none()
                    && self.issue.is_none()
                    && self.coverage == DependencyCoverage::Unknown
            }
            RunState::Blocked => {
                self.output.is_none() && self.issue.is_none() && self.blocked.is_some()
            }
            RunState::Completed => {
                self.output
                    .as_deref()
                    .is_some_and(|output| !output.trim().is_empty())
                    && self.issue.is_none()
            }
            RunState::Failed => match self.output.as_deref() {
                Some(output) => {
                    !output.trim().is_empty()
                        && self.coverage != DependencyCoverage::Unknown
                        && matches!(
                            self.issue,
                            Some(AgentFailure::BudgetExceeded | AgentFailure::Stalled)
                        )
                }
                None => self.issue.is_some() && self.coverage == DependencyCoverage::Unknown,
            },
            RunState::Cancelled | RunState::TimedOut | RunState::Interrupted => {
                self.output.is_none()
                    && self.issue.is_some()
                    && self.coverage == DependencyCoverage::Unknown
            }
        };
        valid.then_some(()).ok_or(AgentFailure::StorageUnavailable)
    }

    pub fn continuation(&self) -> Option<ContinuationRef> {
        (self.output.is_none()
            && self.pending_terminal.is_none()
            && self.unresolved_attempts.is_empty()
            && self.unresolved_delegations.is_empty()
            && matches!(
                (self.state, self.issue),
                (RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
                    | (RunState::Failed, Some(AgentFailure::BudgetExceeded))
            ))
        .then(|| self.continuation_level.checked_add(1))
        .flatten()
        .filter(|level| *level <= 3)
        .map(|level| ContinuationRef {
            run_id: self.run_id,
            executor_generation: self.executor_generation,
            level,
        })
    }

    /// The linked-resume reference this Run's child would carry, if this Run
    /// may be an origin: Completed, with chain depth left. Whether the
    /// interaction group actually admits that child is decided atomically at
    /// admission, never from this receipt alone.
    pub fn resume(&self) -> Option<InteractionResumeRef> {
        matches!(self.state, RunState::Completed | RunState::Blocked)
            .then(|| self.resume_lineage.checked_add(1))
            .flatten()
            .filter(|lineage| *lineage <= MAX_RESUME_LINEAGE)
            .map(|lineage| InteractionResumeRef {
                origin_run_id: self.run_id,
                lineage,
            })
    }
}

#[derive(Clone, Debug)]
pub struct RunTerminal {
    pub state: RunState,
    pub output: Option<String>,
    pub steps: Vec<EngineStep>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub blocked: Option<RunBlockRecord>,
    /// Interactions this completion references beyond settled step artifacts.
    ///
    /// Set explicitly by Conversation for completions the model did not
    /// author (deterministic no-model limitation): the repository projects
    /// one immutable Interaction message per ref. Model-authored Answers
    /// never project refs, so model output cannot inject interaction
    /// messages; only this owner-set linkage and trusted-port step
    /// artifacts do.
    pub interactions: Vec<floe_agent_contract::UserInteractionRef>,
}

impl RunTerminal {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.state.is_terminal()
            || self.coverage.validate().is_err()
            || self
                .blocked
                .as_ref()
                .is_some_and(|record| record.validate().is_err())
            || self.blocked.is_some() != (self.state == RunState::Blocked)
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
            || self.steps.len() > floe_agent_contract::MAX_AGENT_MESSAGES
            || self.interactions.len() > MAX_ACTIVE_INTERACTIONS_PER_RUN
            || self
                .interactions
                .iter()
                .any(|reference| reference.validate().is_err())
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut seen = std::collections::HashSet::new();
        if self
            .interactions
            .iter()
            .any(|reference| !seen.insert(reference.interaction_id))
        {
            return Err(AgentFailure::InvalidInput);
        }
        let valid = match self.state {
            RunState::Blocked => {
                self.output.is_none() && self.issue.is_none() && self.blocked.is_some()
                    && !self.steps.iter().any(|step| matches!(step, EngineStep::Answer { .. }))
                    && !self.interactions.is_empty()
            }
            RunState::Completed => {
                self.output
                    .as_deref()
                    .is_some_and(|output| !output.trim().is_empty())
                    && self.issue.is_none()
                    && self.steps.last().is_some_and(|step| {
                        matches!(step, EngineStep::Answer { text, .. } if Some(text) == self.output.as_ref())
                    })
            }
            RunState::Failed => match self.output.as_deref() {
                Some(output) => {
                    !output.trim().is_empty()
                        && self.coverage != DependencyCoverage::Unknown
                        && matches!(
                            self.issue,
                            Some(AgentFailure::BudgetExceeded | AgentFailure::Stalled)
                        )
                        && self.steps.last().is_some_and(|step| {
                            matches!(step, EngineStep::Answer { text, .. } if text == output)
                        })
                }
                None => {
                    self.steps.is_empty()
                        && self.issue.is_some()
                        && self.coverage == DependencyCoverage::Unknown
                }
            },
            RunState::Cancelled | RunState::TimedOut | RunState::Interrupted => {
                self.output.is_none()
                    && self.steps.is_empty()
                    && self.issue.is_some()
                    && self.coverage == DependencyCoverage::Unknown
            }
            RunState::Working => false,
        };
        valid.then_some(()).ok_or(AgentFailure::InvalidInput)
    }

    pub fn from_failure(failure: AgentFailure) -> Self {
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
            blocked: None,
            interactions: vec![],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub expert_environment: floe_experts::RunExpertEnvironmentIdentity,
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub user_message_id: Uuid,
    pub initial_session_revision: u64,
    pub request_digest: [u8; 32],
    pub state: RunState,
    pub pending_terminal: Option<PendingRunTerminal>,
    pub output: Option<String>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub blocked: Option<RunBlockRecord>,
    pub session_revision: u64,
    pub aggregate_revision: u64,
    pub journal_revision: u64,
    pub executor_generation: u64,
    pub continuation_of: Option<RunId>,
    pub continuation_executor_generation: Option<u64>,
    pub continuation_level: u8,
    pub retry_of: Option<RunId>,
    pub resume_of: Option<RunId>,
    pub resume_lineage: u8,
}

impl RunRecord {
    pub fn validate(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        self.expert_environment
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let admitted_revision = self
            .initial_session_revision
            .checked_add(1)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let terminal_revision = self
            .initial_session_revision
            .checked_add(2)
            .ok_or(AgentFailure::StorageUnavailable)?;
        if self.person_id != person_id
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.trim() != self.device_id
            || self.device_id.chars().any(char::is_control)
            || self.user_message_id.is_nil()
            || !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.request_digest == [0; 32]
            || self.session_revision <= self.initial_session_revision
            || self.aggregate_revision == 0
            || self.executor_generation == 0
            || self.continuation_level > 3
            || self.retry_of == Some(self.run_id)
            || self.resume_of == Some(self.run_id)
            || self.resume_lineage > MAX_RESUME_LINEAGE
            || self.journal_revision > 512
            || self.pending_terminal.is_some_and(|pending| pending.validate().is_err()
                || self.state != RunState::Working)
            || self
                .blocked
                .as_ref()
                .is_some_and(|record| record.validate().is_err())
            || self.blocked.is_some() != (self.state == RunState::Blocked)
            || self.coverage.validate().is_err()
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if self.continuation_of.is_some() != (self.continuation_level > 0)
            || self.continuation_executor_generation.is_some() != (self.continuation_level > 0)
            || self.continuation_executor_generation == Some(0)
            || self.resume_of.is_some() != (self.resume_lineage > 0)
            || self.resume_of.is_some()
                && (self.continuation_of.is_some()
                    || self.continuation_level > 0
                    || self.retry_of.is_some())
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let valid = match self.state {
            RunState::Working => {
                self.session_revision == admitted_revision
                    && (if self.pending_terminal.is_some() { self.aggregate_revision >= 2 }
                        else { self.aggregate_revision == 1 })
                    && self.output.is_none()
                    && self.coverage == DependencyCoverage::Unknown
                    && self.issue.is_none()
            }
            RunState::Blocked => {
                self.session_revision == terminal_revision
                    && self.aggregate_revision == 2
                    && self.output.is_none()
                    && self.issue.is_none()
                    && self.blocked.as_ref().is_some_and(|record| {
                        record.interactions.iter().all(|link| {
                            let origin = &link.origin;
                            origin.session_id == self.session_id
                                && origin.person_id == self.person_id
                                && origin.device_id == self.device_id
                                && origin.run_id == self.run_id
                                && origin.executor_generation == self.executor_generation
                        })
                    })
            }
            RunState::Completed => {
                self.session_revision == terminal_revision
                    && self.aggregate_revision == 2
                    && self
                        .output
                        .as_deref()
                        .is_some_and(|output| !output.trim().is_empty())
                    && self.issue.is_none()
            }
            RunState::Failed => match self.output.as_deref() {
                Some(output) => {
                    self.session_revision == terminal_revision
                        && self.aggregate_revision == 2
                        && !output.trim().is_empty()
                        && self.coverage != DependencyCoverage::Unknown
                        && matches!(
                            self.issue,
                            Some(AgentFailure::BudgetExceeded | AgentFailure::Stalled)
                        )
                }
                None => {
                    self.session_revision >= terminal_revision
                        && self.aggregate_revision >= 2
                        && self.coverage == DependencyCoverage::Unknown
                        && self.issue.is_some()
                }
            },
            RunState::Cancelled | RunState::TimedOut | RunState::Interrupted => {
                self.session_revision >= terminal_revision
                    && self.aggregate_revision >= 2
                    && self.output.is_none()
                    && self.coverage == DependencyCoverage::Unknown
                    && self.issue.is_some()
            }
        };
        valid.then_some(()).ok_or(AgentFailure::StorageUnavailable)
    }

    pub fn matches_admission(&self, request: &crate::TurnAdmissionRequest) -> bool {
        let (continuation, resume) = match &request.mode {
            crate::TurnMode::New => (None, None),
            crate::TurnMode::Continue(value) => (Some(value), None),
            crate::TurnMode::Resume(value) => (None, Some(value)),
        };
        let message_id = match &request.input {
            crate::TurnInput::NewMessage(message) => message.message_id,
            crate::TurnInput::ExistingMessage { message_id } => *message_id,
        };
        self.command_id == request.command_id
            && self.session_id == request.session_id
            && self.person_id.to_string() == request.principal
            && self.device_id == request.device_id
            && self.user_message_id == message_id
            && self.initial_session_revision == request.expected_session_revision
            && self.request_digest == request.request_digest
            && self.retry_of == request.retry_of
            && self.continuation_of == continuation.map(|value| value.run_id)
            && self.continuation_executor_generation
                == continuation.map(|value| value.executor_generation)
            && self.continuation_level == continuation.map_or(0, |value| value.level)
            && self.resume_of == resume.map(|value| value.origin_run_id)
            && self.resume_lineage == resume.map_or(0, |value| value.lineage)
    }
}

pub use floe_agent_contract::UnresolvedModelAttempt;
