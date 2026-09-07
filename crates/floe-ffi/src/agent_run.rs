use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
    time::Duration,
};

use floe_agent::{AgentEvent, AgentFailure, AgentSession, Cancellation};
use floe_core::{AgentFixturePrompt, AgentFixtureTurn};
use floe_domain::PersonId;
use floe_protocol::*;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{BridgeResult, FloeHandle, agent_failure, check_version, parse_id, parse_person};

#[derive(Default)]
pub(crate) struct AgentRuns(RefCell<Option<AgentRun>>);

struct AgentRun {
    person_id: PersonId,
    session_id: Uuid,
    expected_revision: u64,
    prompt: AgentFixturePromptDto,
    cancellation: Cancellation,
    events: Arc<Mutex<Vec<AgentEvent>>>,
    task: JoinHandle<Result<AgentSession, AgentFailure>>,
    result: Option<Result<AgentSession, AgentFailure>>,
}

impl AgentRuns {
    pub(crate) fn ensure_idle(&self, person_id: PersonId, session_id: Uuid) -> BridgeResult<()> {
        if self.0.borrow().as_ref().is_some_and(|run| {
            run.person_id == person_id && run.session_id == session_id && run.result.is_none()
        }) {
            return Err(agent_failure(AgentFailure::Conflict));
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

pub(crate) fn run(
    handle: &FloeHandle,
    request: AgentFixtureRunRequestDto,
) -> BridgeResult<AgentFixtureRunDto> {
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    let session_id = parse_id(&request.session_id, "session_id", |value| value)?;
    let mut slot = handle.agent_runs.0.borrow_mut();
    if let AgentFixtureRunOperationDto::Begin { prompt } = request.operation {
        if let Some(run) = slot.as_ref() {
            if run.person_id != person_id
                || run.session_id != session_id
                || run.expected_revision != request.expected_revision
                || run.prompt != prompt
            {
                return Err(agent_failure(AgentFailure::Conflict));
            }
        } else {
            let session = handle
                .runtime
                .block_on(handle.core.agent_fixture_session(person_id, session_id))
                .map_err(agent_failure)?;
            if session.revision != request.expected_revision || session.active_turn.is_some() {
                return Err(agent_failure(AgentFailure::Conflict));
            }
            let cancellation = Cancellation::default();
            let worker_cancellation = cancellation.clone();
            let events = Arc::new(Mutex::new(vec![]));
            let worker_events = events.clone();
            let core = handle.core.clone();
            let turn = AgentFixtureTurn {
                person_id,
                session_id,
                expected_revision: request.expected_revision,
                prompt: fixture_prompt(prompt),
            };
            let task = handle.runtime.spawn(async move {
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
            });
            *slot = Some(AgentRun {
                person_id,
                session_id,
                expected_revision: request.expected_revision,
                prompt,
                cancellation,
                events,
                task,
                result: None,
            });
        }
    }
    let run = slot
        .as_mut()
        .ok_or_else(|| agent_failure(AgentFailure::NotFound))?;
    if run.person_id != person_id
        || run.session_id != session_id
        || run.expected_revision != request.expected_revision
    {
        return Err(agent_failure(AgentFailure::NotFound));
    }
    if matches!(request.operation, AgentFixtureRunOperationDto::Stop {}) {
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
    let after_sequence = match request.operation {
        AgentFixtureRunOperationDto::Poll { after_sequence } => after_sequence,
        _ => 0,
    };
    let result = snapshot(run, after_sequence)?;
    if matches!(request.operation, AgentFixtureRunOperationDto::Release {}) {
        if !result.done {
            return Err(agent_failure(AgentFailure::Conflict));
        }
        *slot = None;
    }
    Ok(result)
}

fn snapshot(run: &AgentRun, after_sequence: usize) -> BridgeResult<AgentFixtureRunDto> {
    let events = run
        .events
        .lock()
        .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
    if after_sequence > events.len() {
        return Err(agent_failure(AgentFailure::InvalidInput));
    }
    Ok(AgentFixtureRunDto {
        session_id: run.session_id.to_string(),
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

pub(crate) fn fixture_prompt(prompt: AgentFixturePromptDto) -> AgentFixturePrompt {
    match prompt {
        AgentFixturePromptDto::Today => AgentFixturePrompt::Today,
        AgentFixturePromptDto::FollowUp => AgentFixturePrompt::FollowUp,
        AgentFixturePromptDto::RepeatedCall => AgentFixturePrompt::RepeatedCall,
        AgentFixturePromptDto::Unavailable => AgentFixturePrompt::Unavailable,
    }
}
