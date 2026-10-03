use crate::gateway::json::strict_json_bytes;
use floe_agent_contract::{AgentFailure, MAX_CONTEXT_REFS, valid_context_refs};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn encode_agent_input(
    request: &floe_inference::CanonicalModelRequest,
) -> Result<serde_json::Value, AgentFailure> {
    let mut aliases: std::collections::HashSet<_> = request
        .catalog
        .tools
        .iter()
        .map(|tool| tool_name(&tool.id))
        .collect();
    if aliases.len() != request.catalog.tools.len() {
        return Err(AgentFailure::InvalidInput);
    }
    if !request.catalog.cards.is_empty() && !aliases.insert(tool_name(DELEGATION_CAPABILITY_ID)) {
        return Err(AgentFailure::InvalidInput);
    }
    let envelope = &request.envelope;
    let mut messages = vec![];
    for mut message in crate::models::wire::ModelFrames::from_envelope(envelope)?.messages() {
        rewrite_tool_calls(&mut message)?;
        if message["role"] == "tool" {
            let content = if message["status"] == "error" {
                if message.get("content").is_some() {
                    json!({"status":"error", "failure": message["failure"], "content": message["content"]})
                } else {
                    json!({"status":"error", "failure": message["failure"]})
                }
            } else {
                json!({"status":"success", "content": message["content"]})
            };
            message = json!({"role":"tool", "tool_call_id":message["tool_call_id"],"content":content.to_string()});
        }
        messages.push(message);
    }
    let mut ordered_tools: Vec<_> = request.catalog.tools.iter().collect();
    ordered_tools.sort_by(|left, right| left.id.cmp(&right.id));
    let mut agent_ids: Vec<_> = request
        .catalog
        .cards
        .iter()
        .map(|definition| definition.card.id.clone())
        .collect();
    agent_ids.sort();
    let mut tools: Vec<_> = ordered_tools
        .into_iter()
        .map(|tool| {
            let parameters = strict_object(&tool.input_schema)?;
            if parameters["type"] != "object" {
                return Err(AgentFailure::InvalidInput);
            }
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool_name(&tool.id),
                    "description": tool.description,
                    "parameters": parameters,
                    "strict": false
                }
            }))
        })
        .collect::<Result<Vec<_>, AgentFailure>>()?;
    if !request.catalog.cards.is_empty() {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": tool_name(DELEGATION_CAPABILITY_ID),
                "description": "Delegate a natural-language assignment to one active Expert. Select its exact agent_id from the current catalog and include the relevant context, constraints, and desired outcome in message.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "enum": agent_ids
                        },
                        "message": {"type": "string", "minLength": 1, "maxLength": 4096},
                        "context_refs": {
                            "type": "array",
                            "items": {"type": "string"},
                            "maxItems": MAX_CONTEXT_REFS
                        }
                    },
                    "required": ["agent_id", "message"],
                    "additionalProperties": false
                },
                "strict": false
            }
        }));
    }
    let input = json!({"messages": messages, "tools": tools});
    validate_input(&input)?;
    Ok(input)
}

/// Resolve one decoded wire step to its canonical agent step. Tool and Expert
/// identity resolve against the bounded catalog; the definition revision is
/// copied from the catalog entry the wire alias matched.
pub(crate) fn decode_agent_step(
    step: WireStep,
    catalog: &floe_agent_contract::AllowedCatalog,
) -> Result<floe_agent_contract::ModelStep, AgentFailure> {
    match step {
        WireStep::Answer { text } => Ok(floe_agent_contract::ModelStep::Answer {
            text,
            artifacts: vec![],
        }),
        WireStep::Preamble { text } => Ok(floe_agent_contract::ModelStep::Preamble { text }),
        WireStep::Call {
            capability_id,
            input,
        } if capability_id == tool_name(DELEGATION_CAPABILITY_ID) => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct DelegationInput {
                agent_id: String,
                message: String,
                #[serde(default)]
                context_refs: Vec<String>,
            }
            let delegation: DelegationInput = serde_json::from_value(strict_object(&input)?)
                .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
            if !valid_context_refs(&delegation.context_refs) {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            let definition = catalog
                .cards
                .iter()
                .find(|definition| definition.card.id == delegation.agent_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            if delegation.message.trim().is_empty() {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            Ok(floe_agent_contract::ModelStep::Delegate {
                agent_id: definition.card.id.clone(),
                definition_revision: definition.definition_revision,
                message: delegation.message,
                context_refs: delegation.context_refs,
            })
        }
        WireStep::Call {
            capability_id,
            input,
        } => {
            let value =
                strict_object(&input).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
            let input = serde_json::to_string(&value)
                .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
            let descriptor = catalog
                .tools
                .iter()
                .find(|tool| tool_name(&tool.id) == capability_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            Ok(floe_agent_contract::ModelStep::CallTool {
                tool_id: descriptor.id.clone(),
                definition_revision: descriptor.definition_revision,
                input,
            })
        }
    }
}

fn tool_name(identifier: &str) -> String {
    let hash = identifier
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, value| {
            (hash ^ u64::from(value)).wrapping_mul(0x100000001b3)
        });
    format!("floe_{hash:016x}")
}

const DELEGATION_CAPABILITY_ID: &str = "floe.a2a.delegate";

fn rewrite_tool_calls(message: &mut serde_json::Value) -> Result<(), AgentFailure> {
    let Some(calls) = message
        .get_mut("tool_calls")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for call in calls {
        let function = &mut call["function"];
        let identifier = function["name"]
            .as_str()
            .ok_or(AgentFailure::InvalidInput)?;
        function["name"] = json!(tool_name(identifier));
        function["arguments"] = json!(function["arguments"].to_string());
    }
    Ok(())
}

fn strict_object(raw: &str) -> Result<serde_json::Value, AgentFailure> {
    let value = floe_model_contract::strict_json(raw.as_bytes(), 32768)
        .map_err(|_| AgentFailure::InvalidInput)?;
    if !value.is_object() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(value)
}

fn validate_input(input: &serde_json::Value) -> Result<(), AgentFailure> {
    let bytes = serde_json::to_vec(input).map_err(|_| AgentFailure::InvalidInput)?;
    strict_json_bytes(&bytes, 32768)?;
    let messages = input["messages"]
        .as_array()
        .ok_or(AgentFailure::InvalidInput)?;
    let tools = input["tools"]
        .as_array()
        .ok_or(AgentFailure::InvalidInput)?;
    if messages.is_empty() || messages.len() > 256 || tools.len() > 64 {
        return Err(AgentFailure::InvalidInput);
    }
    let mut used = std::collections::BTreeSet::new();
    let mut pending = std::collections::BTreeSet::new();
    for message in messages {
        match message["role"].as_str() {
            Some("tool") => {
                let id = message["tool_call_id"]
                    .as_str()
                    .ok_or(AgentFailure::InvalidInput)?;
                if !pending.remove(id) || !message["content"].is_string() {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            Some("user" | "assistant") => {
                if !pending.is_empty() {
                    return Err(AgentFailure::InvalidInput);
                }
                if let Some(calls) = message.get("tool_calls") {
                    let calls = calls.as_array().ok_or(AgentFailure::InvalidInput)?;
                    if calls.is_empty() || calls.len() > 8 {
                        return Err(AgentFailure::InvalidInput);
                    }
                    for call in calls {
                        let id = call["id"].as_str().ok_or(AgentFailure::InvalidInput)?;
                        if !valid_call_id(id) || !used.insert(id.to_owned()) {
                            return Err(AgentFailure::InvalidInput);
                        }
                        pending.insert(id.to_owned());
                        let name = call["function"]["name"]
                            .as_str()
                            .ok_or(AgentFailure::InvalidInput)?;
                        if !valid_alias(name) {
                            return Err(AgentFailure::InvalidInput);
                        }
                        strict_object(
                            call["function"]["arguments"]
                                .as_str()
                                .ok_or(AgentFailure::InvalidInput)?,
                        )?;
                    }
                } else if !message["content"].is_string() {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            _ => return Err(AgentFailure::InvalidInput),
        }
    }
    if !pending.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum WireStep {
    Preamble {
        text: String,
    },
    Answer {
        text: String,
    },
    Call {
        capability_id: String,
        input: String,
    },
}

pub(crate) fn valid_call_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
pub(crate) fn valid_alias(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
