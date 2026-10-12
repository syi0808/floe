//! Typed conversation entries rendered to provider wire JSON.
//!
//! The canonical model conversation is typed; each provider translates it to
//! the message shapes its API expects. This module renders the shared OpenAI
//! role/content/tool-call shapes both transports use today. Preambles, including
//! host Resume markers, retain their text and chronological assistant position.

use floe_agent_contract::{
    ArtifactPart, ModelConversationEntry, TaskState, USER_INTERACTION_MEDIA_TYPE,
};
use serde_json::{Value, json};

pub struct ModelFrames {
    run_frame: String,
    history: Vec<Value>,
    attempt_context: String,
    current_turn: Vec<Value>,
}

impl ModelFrames {
    pub fn from_envelope(
        envelope: &floe_agent_contract::ContextEnvelope,
    ) -> Result<Self, floe_agent_contract::AgentFailure> {
        Ok(Self {
            run_frame: envelope.canonical_run_frame_json()?,
            history: wire_messages(&envelope.conversation.history),
            attempt_context: envelope.canonical_attempt_frame_json()?,
            current_turn: wire_messages(&envelope.conversation.current_turn),
        })
    }

    pub fn messages(self) -> Vec<Value> {
        let mut messages = vec![json!({"role": "user", "content": self.run_frame})];
        messages.extend(self.history);
        messages.push(json!({"role": "user", "content": self.attempt_context}));
        messages.extend(self.current_turn);
        messages
    }
}

pub fn embedded_json(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.into()))
}

/// One entry as zero or more wire messages. Exchanges expand to the
/// assistant tool-call message plus its tool result, mirroring the shapes the
/// old JSON conversation carried.
pub fn wire_messages(entries: &[ModelConversationEntry]) -> Vec<Value> {
    entries.iter().flat_map(wire_entry).collect()
}

fn wire_entry(entry: &ModelConversationEntry) -> Vec<Value> {
    match entry {
        ModelConversationEntry::User { text, .. } => vec![json!({
            "role": "user",
            "content": text,
        })],
        ModelConversationEntry::Assistant { text, .. }
        | ModelConversationEntry::Preamble { text, .. } => vec![json!({
            "role": "assistant",
            "content": text,
        })],
        ModelConversationEntry::ToolExchange { call, result } => {
            let call_message = json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": call.call_id,
                    "type": "function",
                    "function": {
                        "name": call.tool_id,
                        "arguments": embedded_json(&call.input),
                    }
                }]
            });
            let result_message = match &result.issue {
                None => json!({
                    "role": "tool",
                    "tool_call_id": call.call_id,
                    "capability_id": call.tool_id,
                    "status": "success",
                    "content": embedded_json(&result.text),
                }),
                Some(issue) => {
                    let user_action = result.artifacts.iter().any(|artifact| {
                        artifact.parts.iter().any(|part| matches!(part,
                            ArtifactPart::Data { media_type, .. } if media_type == USER_INTERACTION_MEDIA_TYPE
                        ))
                    });
                    if user_action {
                        json!({
                            "role": "tool",
                            "tool_call_id": call.call_id,
                            "capability_id": call.tool_id,
                            "status": "error",
                            "failure": issue.failure,
                            "content": result.text,
                        })
                    } else {
                        json!({
                            "role": "tool",
                            "tool_call_id": call.call_id,
                            "capability_id": call.tool_id,
                            "status": "error",
                            "failure": issue.failure,
                        })
                    }
                }
            };
            vec![call_message, result_message]
        }
        ModelConversationEntry::DelegationExchange { request, receipt } => {
            let call_message = json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": request.task_id.as_uuid(),
                    "type": "function",
                    "function": {
                        "name": "floe.a2a.delegate",
                        "arguments": {
                            "agent_id": request.selected_agent_id,
                            "message": request.message,
                            "context_refs": request.context_refs,
                        }
                    }
                }]
            });
            let mut content = json!({
                "agent_id": receipt.snapshot.agent_id,
                "state": receipt.snapshot.state,
                "result": receipt.snapshot.result,
                "artifacts": receipt.snapshot.artifacts,
            });
            if let Some(issue) = receipt.snapshot.issue {
                content["failure"] = json!(issue);
            }
            // The status/content split mirrors the old delegation rendering:
            // errors keep their content payload rather than a failure slot.
            let completed =
                receipt.snapshot.state == TaskState::Completed && receipt.snapshot.issue.is_none();
            let result_message = json!({
                "role": "tool",
                "tool_call_id": request.task_id.as_uuid(),
                "capability_id": "floe.a2a.delegate",
                "status": if completed { "success" } else { "error" },
                "content": content,
            });
            vec![call_message, result_message]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn preambles_survive_history_and_current_turn_wire_rendering_in_order() {
        let history = vec![
            ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "original request".into(),
            },
            ModelConversationEntry::Preamble {
                message_id: Uuid::new_v4(),
                text: "I will inspect the source.".into(),
            },
        ];
        let current = vec![
            ModelConversationEntry::Preamble {
                message_id: Uuid::new_v4(),
                text: "Resume the retained original request.".into(),
            },
            ModelConversationEntry::Assistant {
                message_id: Uuid::new_v4(),
                text: "Continuing.".into(),
            },
        ];
        let frames = ModelFrames {
            run_frame: "run context".into(),
            history: wire_messages(&history),
            attempt_context: "attempt context".into(),
            current_turn: wire_messages(&current),
        };
        assert_eq!(
            frames.messages(),
            vec![
                json!({"role":"user", "content":"run context"}),
                json!({"role":"user", "content":"original request"}),
                json!({"role":"assistant", "content":"I will inspect the source."}),
                json!({"role":"user", "content":"attempt context"}),
                json!({"role":"assistant", "content":"Resume the retained original request."}),
                json!({"role":"assistant", "content":"Continuing."}),
            ]
        );
    }
}
