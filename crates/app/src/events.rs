use std::{collections::VecDeque, sync::Mutex};

use crate::services::{ConversationEvent, EventPayload, EventRead, RunEventRecord};
use floe_conversation::RunReceipt;

const MAX_RETAINED_EVENTS: usize = 128;

#[derive(Default)]
struct EventState {
    latest_cursor: u64,
    events: VecDeque<(String, ConversationEvent)>,
}

#[derive(Default)]
pub(crate) struct AppEventBuffer {
    state: Mutex<EventState>,
}

impl AppEventBuffer {
    pub(crate) fn publish_command(&self, receipt: &RunReceipt) {
        self.publish(
            &receipt.principal,
            receipt.aggregate_revision,
            EventPayload::CommandUpdated {
                command_id: receipt.command_id,
                run_id: receipt.run_id,
                session_revision: receipt.session_revision,
            },
        );
    }

    pub(crate) fn publish_run(&self, receipt: &RunReceipt) {
        self.publish(
            &receipt.principal,
            receipt.aggregate_revision,
            EventPayload::RunUpdated(RunEventRecord {
                run_id: receipt.run_id,
                session_id: receipt.session_id,
                aggregate_revision: receipt.aggregate_revision,
                executor_generation: receipt.executor_generation,
                state: receipt.state,
                generated_reply: receipt.output.is_some(),
                issue: receipt.issue,
                attempt_refs: receipt.attempt_refs.clone(),
                task_refs: receipt.task_refs.clone(),
            }),
        );
    }

    pub(crate) fn read(
        &self,
        principal: &str,
        runtime_epoch: u64,
        requested_epoch: Option<u64>,
        cursor: Option<u64>,
        limit: u16,
    ) -> EventRead {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(cursor) = cursor else {
            return EventRead::ResyncRequired {
                snapshot_cursor: state.latest_cursor,
            };
        };
        if requested_epoch != Some(runtime_epoch) || cursor > state.latest_cursor {
            return EventRead::ResyncRequired {
                snapshot_cursor: state.latest_cursor,
            };
        }
        if state
            .events
            .front()
            .is_some_and(|oldest| cursor.saturating_add(1) < oldest.1.cursor)
        {
            return EventRead::ResyncRequired {
                snapshot_cursor: state.latest_cursor,
            };
        }
        let events = state
            .events
            .iter()
            .filter(|(owner, event)| owner == principal && event.cursor > cursor)
            .map(|(_, event)| event)
            .take(usize::from(limit))
            .cloned()
            .collect::<Vec<_>>();
        let next_cursor = events.last().map_or(cursor, |event| event.cursor);
        EventRead::Events {
            next_cursor,
            events,
        }
    }

    fn publish(&self, principal: &str, aggregate_revision: u64, payload: EventPayload) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(cursor) = state.latest_cursor.checked_add(1) else {
            state.events.clear();
            state.latest_cursor = 0;
            return;
        };
        state.latest_cursor = cursor;
        state.events.push_back((
            principal.to_owned(),
            ConversationEvent {
                cursor,
                aggregate_revision,
                payload,
            },
        ));
        while state.events.len() > MAX_RETAINED_EVENTS {
            state.events.pop_front();
        }
    }
}
