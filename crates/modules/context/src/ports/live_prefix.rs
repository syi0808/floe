//! A bounded, canonical Surface prefix with an existing semantic checkpoint.
use floe_agent_contract::{AgentMessage, MAX_AGENT_MESSAGES, MessageRole};
use floe_context_contract::LiveProjectionSource;
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, OwnerActor, PersonId};
use std::{collections::HashSet, fmt};
use uuid::Uuid;

#[derive(Clone, Eq, PartialEq)]
pub struct LivePrefixSnapshot {
    pub person_id: PersonId,
    pub source: LiveProjectionSource,
    pub messages: Vec<AgentMessage>,
    pub summary_message_id: Option<Uuid>,
    /// False is a compaction requirement, never permission to silently start
    /// with only this bounded suffix. Original product history is untouched.
    pub complete: bool,
}
impl LivePrefixSnapshot {
    pub fn validate_for(
        &self,
        actor: &OwnerActor,
        source: &LiveProjectionSource,
    ) -> Result<(), AgentFailure> {
        actor.validate()?;
        source.validate()?;
        if !matches!(source, LiveProjectionSource::SurfacePrefix { .. }) {
            return Err(AgentFailure::InvalidInput);
        }
        if self.person_id != actor.person_id || &self.source != source {
            return Err(AgentFailure::PolicyDenied);
        }
        let through = source.through();
        if self.messages.len() > MAX_AGENT_MESSAGES
            || (through.sequence == 0 && (!self.messages.is_empty() || !self.complete))
            || (through.sequence > 0 && self.messages.is_empty())
            || self.messages.len() as u64 > through.sequence
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut seen = HashSet::new();
        for message in &self.messages {
            message.validate()?;
            if message.message_id.is_nil()
                || !seen.insert(message.message_id)
                || message.call_id.is_some()
                || !matches!(
                    message.role,
                    MessageRole::User | MessageRole::Assistant | MessageRole::Preamble
                )
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if let Some(summary) = self.summary_message_id {
            if self
                .messages
                .first()
                .is_none_or(|m| m.message_id != summary || m.role != MessageRole::Assistant)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if serde_json::to_vec(&self.messages)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > floe_model_contract::MAX_LIVE_CONTEXT_BYTES
        {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        Ok(())
    }
}
impl fmt::Debug for LivePrefixSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LivePrefixSnapshot")
            .field("messages", &self.messages.len())
            .field("has_summary", &self.summary_message_id.is_some())
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}
pub trait LivePrefixReader: Send + Sync {
    fn read_live_prefix<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a LiveProjectionSource,
    ) -> BoxFuture<'a, Result<LivePrefixSnapshot, AgentFailure>>;
}
