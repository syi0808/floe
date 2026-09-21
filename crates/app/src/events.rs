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

#[cfg(test)]
mod tests {
    use floe_agent_contract::DependencyCoverage;
    use floe_conversation::RunState;
    use floe_kernel::{CommandId, RunId};
    use uuid::Uuid;

    use super::*;

    fn receipt(revision: u64) -> RunReceipt {
        RunReceipt {
            run_id: RunId::new(),
            command_id: CommandId::new(),
            session_id: Uuid::new_v4(),
            principal: Uuid::from_u128(1).to_string(),
            request_digest: [1; 32],
            state: RunState::Working,
            output: None,
            coverage: DependencyCoverage::Unknown,
            issue: None,
            session_revision: 1,
            aggregate_revision: revision,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
        }
    }

    #[test]
    fn initial_epoch_change_future_cursor_and_retention_gap_require_resync() {
        let buffer = AppEventBuffer::default();
        assert_eq!(
            buffer.read(&Uuid::from_u128(1).to_string(), 7, None, None, 16),
            EventRead::ResyncRequired { snapshot_cursor: 0 }
        );
        for revision in 1..=130 {
            buffer.publish_run(&receipt(revision));
        }
        assert!(matches!(
            buffer.read(&Uuid::from_u128(1).to_string(), 7, Some(6), Some(130), 16),
            EventRead::ResyncRequired { .. }
        ));
        assert!(matches!(
            buffer.read(&Uuid::from_u128(1).to_string(), 7, Some(7), Some(131), 16),
            EventRead::ResyncRequired { .. }
        ));
        assert_eq!(
            buffer.read(&Uuid::from_u128(1).to_string(), 7, Some(7), Some(0), 16),
            EventRead::ResyncRequired {
                snapshot_cursor: 130
            }
        );
    }

    #[test]
    fn bounded_reads_advance_only_to_the_last_delivered_cursor() {
        let buffer = AppEventBuffer::default();
        let receipt = receipt(1);
        buffer.publish_command(&receipt);
        buffer.publish_run(&receipt);
        let EventRead::Events {
            next_cursor,
            events,
        } = buffer.read(&Uuid::from_u128(1).to_string(), 9, Some(9), Some(0), 1)
        else {
            panic!("expected events");
        };
        assert_eq!(next_cursor, 1);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            buffer.read(&Uuid::from_u128(1).to_string(), 9, Some(9), Some(next_cursor), 16),
            EventRead::Events {
                next_cursor: 2,
                events
            } if events.len() == 1
        ));
    }

    #[test]
    fn read_never_releases_another_principals_events() {
        let buffer = AppEventBuffer::default();
        let owned = receipt(1);
        let mut foreign = receipt(2);
        foreign.principal = Uuid::new_v4().to_string();
        buffer.publish_command(&foreign);
        buffer.publish_run(&owned);
        let EventRead::Events {
            events,
            next_cursor,
        } = buffer.read(&owned.principal, 9, Some(9), Some(0), 1)
        else {
            panic!("expected owned event");
        };
        assert_eq!(next_cursor, 2);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0].payload, EventPayload::RunUpdated(run) if run.run_id == owned.run_id)
        );
        assert!(
            matches!(buffer.read(&Uuid::new_v4().to_string(), 9, Some(9), Some(0), 1), EventRead::Events { events, .. } if events.is_empty())
        );
    }
}
