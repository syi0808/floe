use floe_agent_contract::{AgentFailure};
use crate::{AgentMessage};
use uuid::Uuid;

pub fn bounded_model_history_start(
    messages: &[AgentMessage],
    current_turn: Uuid,
    max_bytes: usize,
) -> Result<usize, AgentFailure> {
    let sizes = messages
        .iter()
        .map(|message| {
            Ok(floe_context::HistoryMessageSize {
                turn_id: message.turn_id(),
                encoded_bytes: serde_json::to_vec(message)
                    .map_err(|_| AgentFailure::InvalidInput)?
                    .len(),
            })
        })
        .collect::<Result<Vec<_>, AgentFailure>>()?;
    let selected = floe_context::bounded_history_start(&sizes, current_turn, max_bytes)?;
    if floe_context::has_calendar_history(&messages[..selected]) {
        let current_start = messages
            .iter()
            .position(|message| message.turn_id() == current_turn)
            .ok_or(AgentFailure::InvalidInput)?;
        if messages[current_start..]
            .iter()
            .any(|message| message.turn_id() != current_turn)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        return Ok(current_start);
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_a_legacy_boundary_cannot_expose_later_derived_history() {
        let current = Uuid::new_v4();
        let messages = [
            AgentMessage::Capability {
                turn_id: Uuid::new_v4(),
                call_id: Uuid::new_v4(),
                capability_id: "calendar.read".into(),
                input: "{}".into(),
                result: Ok("private event".repeat(100)),
            },
            AgentMessage::Assistant {
                turn_id: Uuid::new_v4(),
                text: "Later derived answer".into(),
            },
            AgentMessage::User {
                turn_id: current,
                text: "Hello".into(),
            },
        ];
        let suffix_bytes = serde_json::to_vec(&messages[1..]).unwrap().len();
        assert_eq!(
            bounded_model_history_start(&messages, current, suffix_bytes),
            Ok(2)
        );
    }

    #[test]
    fn serialized_unicode_and_escapes_use_the_exact_array_budget() {
        let current = Uuid::new_v4();
        let messages = [
            AgentMessage::Assistant {
                turn_id: Uuid::new_v4(),
                text: "과거 답변 \"quoted\"\n".repeat(8),
            },
            AgentMessage::User {
                turn_id: current,
                text: "안녕\n".into(),
            },
        ];
        let all_bytes = serde_json::to_vec(&messages).unwrap().len();
        let current_bytes = serde_json::to_vec(&messages[1..]).unwrap().len();
        assert_eq!(
            bounded_model_history_start(&messages, current, all_bytes),
            Ok(0)
        );
        assert_eq!(
            bounded_model_history_start(&messages, current, current_bytes),
            Ok(1)
        );
        assert_eq!(
            bounded_model_history_start(&messages, current, current_bytes - 1),
            Err(AgentFailure::BudgetExceeded)
        );
    }
}
