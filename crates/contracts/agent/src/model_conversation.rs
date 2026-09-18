//! The typed model conversation: what the model has seen and said, without loss.
//!
//! History is what earlier turns established; the current turn is what this
//! execution is doing. Tool and delegation exchanges keep the original call or
//! request next to its result, so recovery never has to guess which input a
//! result answers. Providers render these entries to wire JSON; the canonical
//! storage is this type, never ad-hoc JSON.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentFailure, DelegationRequest, TaskReceipt, ToolCall, ToolResult, MAX_AGENT_MESSAGES,
    MAX_OUTPUT_BYTES,
};

/// Hard cap on the encoded typed conversation. Individual entries are already
/// bounded by [`MAX_OUTPUT_BYTES`]; this bounds their sum.
pub const MAX_MODEL_CONVERSATION_BYTES: usize = 128 * 1024;

/// Maximum context references carried on one delegation exchange.
pub const MAX_CONTEXT_REFS: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConversation {
    pub history: Vec<ModelConversationEntry>,
    pub current_turn: Vec<ModelConversationEntry>,
}

impl ModelConversation {
    /// Total entries across history and the current turn.
    pub fn len(&self) -> usize {
        self.history.len() + self.current_turn.len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.is_empty() && self.current_turn.is_empty()
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.len() > MAX_AGENT_MESSAGES {
            return Err(AgentFailure::BudgetExceeded);
        }
        if !self
            .current_turn
            .iter()
            .any(|entry| matches!(entry, ModelConversationEntry::User { .. }))
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.history
            .iter()
            .chain(&self.current_turn)
            .try_for_each(ModelConversationEntry::validate)?;
        if serde_json::to_vec(self)
            .map(|encoded| encoded.len() > MAX_MODEL_CONVERSATION_BYTES)
            .unwrap_or(true)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelConversationEntry {
    User { message_id: Uuid, text: String },
    Preamble { message_id: Uuid, text: String },
    Assistant { message_id: Uuid, text: String },
    ToolExchange { call: ToolCall, result: ToolResult },
    DelegationExchange {
        request: DelegationRequest,
        receipt: TaskReceipt,
    },
}

impl ModelConversationEntry {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::User { message_id, text }
            | Self::Preamble { message_id, text }
            | Self::Assistant { message_id, text } => {
                if message_id.is_nil() || !crate::message::bounded(text, MAX_OUTPUT_BYTES) {
                    return Err(AgentFailure::InvalidInput);
                }
                Ok(())
            }
            Self::ToolExchange { call, result } => {
                if call.call_id.is_nil()
                    || call.invocation_key.as_uuid().is_nil()
                    || call.tool_id.trim().is_empty()
                    || call.definition_revision == 0
                    || call.call_id != result.call_id
                {
                    return Err(AgentFailure::InvalidInput);
                }
                crate::validate_tool_input(&call.input)?;
                result.validate(call.call_id, MAX_OUTPUT_BYTES)
            }
            Self::DelegationExchange { request, receipt } => {
                validate_delegation_exchange(request, receipt)
            }
        }
    }
}

fn validate_delegation_exchange(
    request: &DelegationRequest,
    receipt: &TaskReceipt,
) -> Result<(), AgentFailure> {
    if !request.task_id.is_valid()
        || request.principal.trim().is_empty()
        || request.invocation_key.as_uuid().is_nil()
        || request.selected_agent_id.trim().is_empty()
        || request.selected_definition_revision == 0
        || !crate::message::bounded(&request.message, MAX_OUTPUT_BYTES)
        || !crate::valid_context_refs(&request.context_refs)
        || receipt.task_id != request.task_id
    {
        return Err(AgentFailure::InvalidInput);
    }
    receipt.snapshot.validate(MAX_OUTPUT_BYTES)?;
    if receipt.snapshot.task_id != request.task_id
        || receipt.snapshot.parent_run_id != request.parent_run_id
        || receipt.snapshot.principal != request.principal
        || receipt.snapshot.agent_id != request.selected_agent_id
        || receipt.snapshot.definition_revision != request.selected_definition_revision
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
