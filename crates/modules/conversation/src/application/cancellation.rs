use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, CommandId, RunId};

const MAX_ACTIVE_RUNS: usize = 64;
const MAX_TRACKED_RUNS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelRunRequest {
    pub run_id: RunId,
    pub principal: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelRunCommand {
    pub command_id: CommandId,
    pub run_id: RunId,
    pub principal: String,
}

impl CancelRunCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_principal(&self.principal)?;
        if !self.command_id.is_valid() || !self.run_id.is_valid() {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelRunReceipt {
    pub command_id: CommandId,
    pub run_id: RunId,
    pub principal: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CancelRunAdmission {
    Created(CancelRunReceipt),
    Existing(CancelRunReceipt),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelCommandRequest {
    pub command_id: CommandId,
    pub principal: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelRunStatus {
    Cancelled,
    Inactive,
    Unknown,
}

struct TrackedRun {
    command_id: CommandId,
    principal: String,
    cancellation: Option<Cancellation>,
}

#[derive(Default)]
struct RegistryState {
    runs: HashMap<RunId, TrackedRun>,
    commands: HashMap<CommandId, RunId>,
    order: VecDeque<RunId>,
    active: usize,
}

#[derive(Clone, Default)]
pub struct RunCancellationRegistry {
    state: Arc<Mutex<RegistryState>>,
}

impl RunCancellationRegistry {
    pub fn cancel_run(&self, request: CancelRunRequest) -> Result<CancelRunStatus, AgentFailure> {
        validate_principal(&request.principal)?;
        if !request.run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        self.cancel(request.run_id, &request.principal)
    }

    pub fn cancel_command(
        &self,
        request: CancelCommandRequest,
    ) -> Result<CancelRunStatus, AgentFailure> {
        validate_principal(&request.principal)?;
        if !request.command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let run_id = {
            let state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
            state.commands.get(&request.command_id).copied()
        };
        match run_id {
            Some(run_id) => self.cancel(run_id, &request.principal),
            None => Ok(CancelRunStatus::Unknown),
        }
    }

    pub(crate) fn register(
        &self,
        run_id: RunId,
        command_id: CommandId,
        principal: &str,
        cancellation: Cancellation,
    ) -> Result<RunCancellationGuard, AgentFailure> {
        validate_principal(principal)?;
        if !run_id.is_valid() || !command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.active >= MAX_ACTIVE_RUNS
            || state.runs.contains_key(&run_id)
            || state.commands.contains_key(&command_id)
        {
            return Err(AgentFailure::Conflict);
        }
        while state.runs.len() >= MAX_TRACKED_RUNS {
            let Some(candidate) = state.order.pop_front() else {
                return Err(AgentFailure::StorageUnavailable);
            };
            if state
                .runs
                .get(&candidate)
                .is_some_and(|tracked| tracked.cancellation.is_some())
            {
                state.order.push_back(candidate);
                if state.active == state.runs.len() {
                    return Err(AgentFailure::Conflict);
                }
                continue;
            }
            if let Some(removed) = state.runs.remove(&candidate) {
                state.commands.remove(&removed.command_id);
            }
        }
        state.runs.insert(
            run_id,
            TrackedRun {
                command_id,
                principal: principal.into(),
                cancellation: Some(cancellation),
            },
        );
        state.commands.insert(command_id, run_id);
        state.order.push_back(run_id);
        state.active += 1;
        Ok(RunCancellationGuard {
            registry: self.clone(),
            run_id,
        })
    }

    fn cancel(&self, run_id: RunId, principal: &str) -> Result<CancelRunStatus, AgentFailure> {
        let cancellation = {
            let state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
            let Some(tracked) = state.runs.get(&run_id) else {
                return Ok(CancelRunStatus::Unknown);
            };
            if tracked.principal != principal {
                return Err(AgentFailure::CapabilityDenied);
            }
            tracked.cancellation.clone()
        };
        match cancellation {
            Some(cancellation) => {
                cancellation.cancel();
                Ok(CancelRunStatus::Cancelled)
            }
            None => Ok(CancelRunStatus::Inactive),
        }
    }

    fn unregister(&self, run_id: RunId) {
        if let Ok(mut state) = self.state.lock()
            && let Some(tracked) = state.runs.get_mut(&run_id)
            && tracked.cancellation.take().is_some()
        {
            state.active = state.active.saturating_sub(1);
        }
    }
}

pub(crate) struct RunCancellationGuard {
    registry: RunCancellationRegistry,
    run_id: RunId,
}

impl Drop for RunCancellationGuard {
    fn drop(&mut self) {
        self.registry.unregister(self.run_id);
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

/// Admit one caller cancel command and act on it.
///
/// The command is recorded first so a repeat is idempotent, then the Run's own
/// state decides the outcome: only a Working Run is signalled, a Run that has
/// already settled is inactive, and an unknown Run stays unknown. A Working Run
/// whose cancellation is not registered is an interrupted host, not a silent
/// success.
pub async fn cancel_run_command<Repository: crate::ConversationRepository>(
    repository: &Repository,
    run_cancellations: &RunCancellationRegistry,
    command: CancelRunCommand,
) -> Result<CancelRunStatus, AgentFailure> {
    command.validate()?;
    repository.admit_cancel(command.clone()).await?;
    let request = CancelRunRequest {
        run_id: command.run_id,
        principal: command.principal,
    };
    let receipt = super::query::get_run(
        repository,
        crate::RunQuery {
            principal: request.principal.clone(),
            run_id: request.run_id,
        },
    )
    .await?;
    match receipt {
        Some(receipt) if receipt.state == crate::RunState::Working => {
            match run_cancellations.cancel_run(request)? {
                CancelRunStatus::Unknown => Err(AgentFailure::Interrupted),
                status => Ok(status),
            }
        }
        Some(_) => Ok(CancelRunStatus::Inactive),
        None => Ok(CancelRunStatus::Unknown),
    }
}
