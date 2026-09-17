use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
    time::Duration,
};

use floe_agent_contract::{AgentFailure};
use floe_conversation::{AgentEvent, AgentSession};
use floe_execution::{Cancellation};
use crate::{AgentFixturePrompt, AgentFixtureTurn};
use floe_diagnostics::{TraceContext, current_context};
use floe_kernel::PersonId;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// What a caller asks of the scripted agent run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentFixtureRunCommand {
    /// Start the run, or rejoin the one already in flight.
    Begin { prompt: AgentFixturePrompt },
    /// Read what has happened since a sequence the caller already saw.
    Poll { after_sequence: usize },
    /// Cancel the run without releasing it.
    Stop,
    /// Release a finished run.
    Release,
}

/// One request against the scripted agent run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentFixtureRunRequest {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub command: AgentFixtureRunCommand,
}

/// Where the run has got to.
pub struct AgentFixtureRunSnapshot {
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub events: Vec<AgentEvent>,
    pub next_sequence: usize,
    pub done: bool,
    pub session: Option<AgentSession>,
    pub failure: Option<AgentFailure>,
}

#[derive(Default)]
pub struct AgentRuns(RefCell<Option<AgentRun>>);

struct AgentRun {
    person_id: PersonId,
    session_id: Uuid,
    expected_revision: u64,
    prompt: AgentFixturePrompt,
    cancellation: Cancellation,
    events: Arc<Mutex<Vec<AgentEvent>>>,
    task: JoinHandle<Result<AgentSession, AgentFailure>>,
    result: Option<Result<AgentSession, AgentFailure>>,
}

impl AgentRuns {
    pub fn ensure_idle(&self, person_id: PersonId, session_id: Uuid) -> Result<(), AgentFailure> {
        if self.0.borrow().as_ref().is_some_and(|run| {
            run.person_id == person_id && run.session_id == session_id && run.result.is_none()
        }) {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    pub(crate) fn close(&self, runtime: &tokio::runtime::Runtime) {
        if let Some(mut run) = self.0.borrow_mut().take() {
            run.cancellation.cancel();
            if run.result.is_none() {
                let finished = runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(2), &mut run.task).await
                });
                if finished.is_err() {
                    run.task.abort();
                }
            }
        }
    }
}

pub fn run(
    handle: &crate::AppComposition,
    request: AgentFixtureRunRequest,
) -> Result<AgentFixtureRunSnapshot, AgentFailure> {
    let AgentFixtureRunRequest {
        person_id,
        session_id,
        expected_revision,
        command,
    } = request;
    let mut slot = handle.agent_runs.0.borrow_mut();
    if let AgentFixtureRunCommand::Begin { prompt } = command {
        if let Some(run) = slot.as_ref() {
            if run.person_id != person_id
                || run.session_id != session_id
                || run.expected_revision != expected_revision
                || run.prompt != prompt
            {
                return Err(AgentFailure::Conflict);
            }
        } else {
            let session = handle
                .runtime
                .block_on(handle.core.agent_fixture_session(person_id, session_id))?;
            if session.revision != expected_revision || session.active_turn.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let cancellation = Cancellation::default();
            let worker_cancellation = cancellation.clone();
            let events = Arc::new(Mutex::new(vec![]));
            let worker_events = events.clone();
            let trace_context =
                current_context().unwrap_or_else(|| TraceContext::new(Uuid::new_v4()));
            let worker_trace_context = trace_context;
            let core = handle.core.clone();
            let turn = AgentFixtureTurn {
                person_id,
                session_id,
                expected_revision,
                prompt,
            };
            let task = handle.runtime.spawn(crate::diagnostics::instrument(
                async move {
                    core.stream_agent_fixture(turn, worker_cancellation.clone(), |event| {
                        if let Ok(mut events) = worker_events.lock() {
                            if events.len() < 64 {
                                events.push(event);
                            } else {
                                worker_cancellation.cancel();
                            }
                        } else {
                            worker_cancellation.cancel();
                        }
                    })
                    .await
                },
                worker_trace_context,
                "agent_fixture_run",
            ));
            *slot = Some(AgentRun {
                person_id,
                session_id,
                expected_revision,
                prompt,
                cancellation,
                events,
                task,
                result: None,
            });
        }
    }
    let run = slot.as_mut().ok_or(AgentFailure::NotFound)?;
    if run.person_id != person_id
        || run.session_id != session_id
        || run.expected_revision != expected_revision
    {
        return Err(AgentFailure::NotFound);
    }
    if matches!(command, AgentFixtureRunCommand::Stop) {
        run.cancellation.cancel();
    }
    if run.result.is_none() {
        let result = handle.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(2), &mut run.task).await
        });
        if let Ok(result) = result {
            run.result = Some(result.unwrap_or(Err(AgentFailure::Interrupted)));
        }
    }
    let after_sequence = match command {
        AgentFixtureRunCommand::Poll { after_sequence } => after_sequence,
        _ => 0,
    };
    let result = snapshot(run, after_sequence)?;
    if matches!(command, AgentFixtureRunCommand::Release) {
        if !result.done {
            return Err(AgentFailure::Conflict);
        }
        *slot = None;
    }
    Ok(result)
}

fn snapshot(run: &AgentRun, after_sequence: usize) -> Result<AgentFixtureRunSnapshot, AgentFailure> {
    let events = run.events.lock().map_err(|_| AgentFailure::Interrupted)?;
    if after_sequence > events.len() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(AgentFixtureRunSnapshot {
        session_id: run.session_id,
        expected_revision: run.expected_revision,
        events: events[after_sequence..].to_vec(),
        next_sequence: events.len(),
        done: run.result.is_some(),
        session: run
            .result
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned(),
        failure: run
            .result
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .copied(),
    })
}
