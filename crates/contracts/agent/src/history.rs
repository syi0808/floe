//! How far back a turn may carry its own history.
//!
//! A model call is bounded in bytes, and a turn is the unit that may not be cut
//! in half: the history it carries starts at a whole turn boundary or it starts
//! later. Every role that assembles a model call answers this the same way.

use std::collections::BTreeMap;

use floe_kernel::AgentFailure;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryMessageSize {
    pub turn_id: Uuid,
    pub encoded_bytes: usize,
}

pub fn bounded_history_start(
    messages: &[HistoryMessageSize],
    current_turn: Uuid,
    max_bytes: usize,
) -> Result<usize, AgentFailure> {
    if current_turn.is_nil()
        || messages
            .iter()
            .any(|message| message.turn_id.is_nil() || message.encoded_bytes == 0)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut first_positions = BTreeMap::new();
    for (position, message) in messages.iter().enumerate() {
        first_positions.entry(message.turn_id).or_insert(position);
    }
    let required_start = *first_positions
        .get(&current_turn)
        .ok_or(AgentFailure::InvalidInput)?;
    let mut encoded_bytes = 2usize;
    let mut earliest_turn_start = messages.len();
    let mut selected = None;
    for (position, message) in messages.iter().enumerate().rev() {
        encoded_bytes = encoded_bytes
            .checked_add(message.encoded_bytes)
            .and_then(|bytes| bytes.checked_add(usize::from(position + 1 < messages.len())))
            .ok_or(AgentFailure::BudgetExceeded)?;
        if encoded_bytes > max_bytes {
            break;
        }
        earliest_turn_start = earliest_turn_start.min(first_positions[&message.turn_id]);
        if earliest_turn_start == position && position <= required_start {
            selected = Some(position);
        }
    }
    selected.ok_or(AgentFailure::BudgetExceeded)
}
