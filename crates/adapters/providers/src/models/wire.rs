//! Typed conversation entries rendered to provider wire JSON.
//!
//! The canonical model conversation is typed; each provider translates it to
//! the message shapes its API expects. This module renders the shared OpenAI
//! role/content/tool-call shapes both transports use today. Preambles never
//! cross: they were dropped from the wire on the old path too.

use floe_agent_contract::{ModelConversationEntry, TaskState};
use serde_json::{Value, json};

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
        ModelConversationEntry::Assistant { text, .. } => vec![json!({
            "role": "assistant",
            "content": text,
        })],
        ModelConversationEntry::Preamble { .. } => vec![],
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
                Some(issue) => json!({
                    "role": "tool",
                    "tool_call_id": call.call_id,
                    "capability_id": call.tool_id,
                    "status": "error",
                    "failure": issue.failure,
                }),
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
            let completed = receipt.snapshot.state == TaskState::Completed
                && receipt.snapshot.issue.is_none();
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
    use floe_agent_contract::{
        DelegationRequest, DependencyCoverage, InvocationKey, OutcomeIssue, TaskId, TaskReceipt,
        TaskSnapshot, ToolCall, ToolResult,
    };
    use floe_kernel::AgentFailure;
    use uuid::Uuid;

    use super::*;

    fn tool_exchange() -> ModelConversationEntry {
        let call_id = Uuid::new_v4();
        ModelConversationEntry::ToolExchange {
            call: ToolCall {
                call_id,
                invocation_key: InvocationKey::new(),
                tool_id: "fixture.read".into(),
                definition_revision: 1,
                input: r#"{"day":"today"}"#.into(),
            },
            result: ToolResult {
                call_id,
                text: r#"{"summary":"One meeting at 10:00"}"#.into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
            },
        }
    }

    #[test]
    fn tool_exchange_renders_call_pair_with_linked_ids() {
        let rendered = wire_messages(&[tool_exchange()]);
        assert_eq!(rendered.len(), 2);
        let call = &rendered[0];
        let result = &rendered[1];
        assert_eq!(call["role"], "assistant");
        assert_eq!(call["tool_calls"][0]["id"], result["tool_call_id"]);
        assert_eq!(call["tool_calls"][0]["function"]["name"], "fixture.read");
        assert_eq!(
            call["tool_calls"][0]["function"]["arguments"]["day"],
            "today"
        );
        assert_eq!(result["role"], "tool");
        assert_eq!(result["status"], "success");
        assert_eq!(result["content"]["summary"], "One meeting at 10:00");
    }

    #[test]
    fn failed_tool_renders_error_status_with_failure() {
        let ModelConversationEntry::ToolExchange { call, mut result } = tool_exchange() else {
            unreachable!()
        };
        result.issue = Some(OutcomeIssue {
            failure: AgentFailure::CapabilityDenied,
            retryable: false,
        });
        let rendered = wire_messages(&[ModelConversationEntry::ToolExchange { call, result }]);
        assert_eq!(rendered[1]["status"], "error");
        assert!(rendered[1].get("failure").is_some());
    }

    #[test]
    fn delegation_renders_delegate_call_pair() {
        let task_id = TaskId::new();
        let entry = ModelConversationEntry::DelegationExchange {
            request: DelegationRequest {
                task_id,
                parent_run_id: None,
                principal: "person:test".into(),
                invocation_key: InvocationKey::new(),
                selected_agent_id: "expert-a".into(),
                selected_definition_revision: 2,
                message: "summarize".into(),
                context_refs: vec![],
            },
            receipt: TaskReceipt {
                task_id,
                snapshot: TaskSnapshot {
                    task_id,
                    parent_run_id: None,
                    principal: "person:test".into(),
                    agent_id: "expert-a".into(),
                    definition_revision: 2,
                    state: TaskState::Completed,
                    result: Some("summary".into()),
                    artifacts: vec![],
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                },
                replay: None,
            },
        };
        let rendered = wire_messages(&[entry]);
        assert_eq!(rendered.len(), 2);
        assert_eq!(
            rendered[0]["tool_calls"][0]["function"]["name"],
            "floe.a2a.delegate"
        );
        assert_eq!(
            rendered[0]["tool_calls"][0]["id"],
            rendered[1]["tool_call_id"]
        );
        assert_eq!(rendered[1]["status"], "success");
        assert_eq!(rendered[1]["content"]["agent_id"], "expert-a");
    }

    fn delegation_entry(context_refs: Vec<String>) -> ModelConversationEntry {
        let task_id = TaskId::new();
        ModelConversationEntry::DelegationExchange {
            request: DelegationRequest {
                task_id,
                parent_run_id: None,
                principal: "person:test".into(),
                invocation_key: InvocationKey::new(),
                selected_agent_id: "expert-a".into(),
                selected_definition_revision: 2,
                message: "summarize".into(),
                context_refs,
            },
            receipt: TaskReceipt {
                task_id,
                snapshot: TaskSnapshot {
                    task_id,
                    parent_run_id: None,
                    principal: "person:test".into(),
                    agent_id: "expert-a".into(),
                    definition_revision: 2,
                    state: TaskState::Completed,
                    result: Some("summary".into()),
                    artifacts: vec![],
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                },
                replay: None,
            },
        }
    }

    #[test]
    fn delegation_wire_preserves_context_refs() {
        let rendered = wire_messages(&[delegation_entry(vec![
            "turn:1".into(),
            "evidence:9".into(),
        ])]);
        assert_eq!(rendered.len(), 2);
        assert_eq!(
            rendered[0]["tool_calls"][0]["function"]["arguments"]["context_refs"],
            json!(["turn:1", "evidence:9"])
        );
        assert_eq!(
            rendered[0]["tool_calls"][0]["id"],
            rendered[1]["tool_call_id"]
        );
    }

    #[test]
    fn empty_context_refs_round_trip_as_empty() {
        let rendered = wire_messages(&[delegation_entry(vec![])]);
        assert_eq!(rendered.len(), 2);
        assert_eq!(
            rendered[0]["tool_calls"][0]["function"]["arguments"]["context_refs"],
            json!([])
        );
    }

    #[test]
    fn preamble_never_crosses_to_the_wire() {
        let rendered = wire_messages(&[ModelConversationEntry::Preamble {
            message_id: Uuid::new_v4(),
            text: "thinking".into(),
        }]);
        assert!(rendered.is_empty());
    }
}
