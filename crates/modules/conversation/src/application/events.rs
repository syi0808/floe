use std::collections::VecDeque;
use std::sync::Mutex;

use floe_kernel::{AgentFailure, CommandId, OwnerActor, RunId};
use uuid::Uuid;

use crate::{RunReceipt, RunState};

const MAX_RETAINED_EVENTS: usize = 128;

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
    pub pending_terminal: Option<crate::PendingRunTerminal>,
    pub generated_reply: bool,
    pub issue: Option<AgentFailure>,
    pub attempt_refs: Vec<Uuid>,
    pub task_refs: Vec<Uuid>,
    pub interaction_refs: Vec<Uuid>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadConversationEvents {
    pub runtime_epoch: Option<u64>,
    pub cursor: Option<u64>,
    pub limit: u16,
}

impl ReadConversationEvents {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.limit == 0
            || self.limit > 256
            || self.runtime_epoch.is_some() != self.cursor.is_some()
            || self.runtime_epoch == Some(0)
        {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct EventState {
    latest_cursor: u64,
    exhausted: bool,
    events: VecDeque<(String, ConversationEvent)>,
}

/// A bounded, process-local event snapshot buffer for one Conversation runtime.
pub struct ConversationEventBuffer {
    runtime_epoch: u64,
    state: Mutex<EventState>,
}

impl ConversationEventBuffer {
    pub fn new(runtime_epoch: u64) -> Result<Self, AgentFailure> {
        if runtime_epoch == 0 {
            return Err(AgentFailure::InvalidInput);
        }

        Ok(Self {
            runtime_epoch,
            state: Mutex::new(EventState::default()),
        })
    }

    pub fn publish_command(&self, receipt: &RunReceipt) -> Result<(), AgentFailure> {
        receipt.validate()?;
        self.publish(
            receipt.principal.clone(),
            receipt.aggregate_revision,
            EventPayload::CommandUpdated {
                command_id: receipt.command_id,
                run_id: receipt.run_id,
                session_revision: receipt.session_revision,
            },
        )
    }

    pub fn publish_run(&self, receipt: &RunReceipt) -> Result<(), AgentFailure> {
        receipt.validate()?;
        self.publish(
            receipt.principal.clone(),
            receipt.aggregate_revision,
            EventPayload::RunUpdated(RunEventRecord {
                run_id: receipt.run_id,
                session_id: receipt.session_id,
                aggregate_revision: receipt.aggregate_revision,
                executor_generation: receipt.executor_generation,
                state: receipt.state,
                pending_terminal: receipt.pending_terminal,
                generated_reply: receipt.output.is_some(),
                issue: receipt.issue.clone(),
                attempt_refs: receipt.attempt_refs.clone(),
                task_refs: receipt.task_refs.clone(),
                interaction_refs: receipt
                    .blocked
                    .as_ref()
                    .map_or_else(Vec::new, |block| block.interaction_refs()),
            }),
        )
    }

    pub fn read(
        &self,
        actor: &OwnerActor,
        request: &ReadConversationEvents,
    ) -> Result<EventRead, AgentFailure> {
        actor.validate()?;
        request.validate()?;
        if actor.runtime_epoch != self.runtime_epoch {
            return Err(AgentFailure::PolicyDenied);
        }

        let principal = actor.person_id.to_string();
        let state = self
            .state
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let resync_required = || EventRead::ResyncRequired {
            snapshot_cursor: state.latest_cursor,
        };

        let Some(cursor) = request.cursor else {
            return Ok(resync_required());
        };
        if state.exhausted
            || request.runtime_epoch != Some(self.runtime_epoch)
            || cursor > state.latest_cursor
        {
            return Ok(resync_required());
        }
        if state
            .events
            .front()
            .is_some_and(|oldest| cursor.saturating_add(1) < oldest.1.cursor)
        {
            return Ok(resync_required());
        }

        let events = state
            .events
            .iter()
            .filter(|(owner, event)| owner == &principal && event.cursor > cursor)
            .map(|(_, event)| event)
            .take(usize::from(request.limit))
            .cloned()
            .collect::<Vec<_>>();
        let next_cursor = events.last().map_or(cursor, |event| event.cursor);
        Ok(EventRead::Events {
            next_cursor,
            events,
        })
    }

    fn publish(
        &self,
        principal: String,
        aggregate_revision: u64,
        payload: EventPayload,
    ) -> Result<(), AgentFailure> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if state.exhausted {
            return Err(AgentFailure::StorageUnavailable);
        }
        let Some(cursor) = state.latest_cursor.checked_add(1) else {
            state.exhausted = true;
            return Err(AgentFailure::StorageUnavailable);
        };

        state.latest_cursor = cursor;
        state.events.push_back((
            principal,
            ConversationEvent {
                cursor,
                aggregate_revision,
                payload,
            },
        ));
        while state.events.len() > MAX_RETAINED_EVENTS {
            state.events.pop_front();
        }
        Ok(())
    }
}
