use floe_context_contract::PersonId;
use uuid::Uuid;

use crate::{AgentFailure, AgentMessage, BoxFuture};

pub const MAX_ARCHIVE_PROJECTION_BYTES: usize = 128 * 1024;
pub const MAX_ARCHIVE_PROJECTION_MESSAGES: usize = crate::MAX_AGENT_MESSAGES;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivePointer {
    pub archive_id: Uuid,
    pub source_revision: u64,
    pub through_turn_id: Uuid,
    pub archived_message_count: usize,
}

impl ArchivePointer {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.archive_id.is_nil()
            || self.source_revision == 0
            || self.through_turn_id.is_nil()
            || self.archived_message_count == 0
            || self.archived_message_count > MAX_ARCHIVE_PROJECTION_MESSAGES
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveReadRequest {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub pointer: ArchivePointer,
    pub max_messages: usize,
    pub max_bytes: usize,
}

impl ArchiveReadRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.pointer.validate()?;
        if self.person_id.0.is_nil()
            || self.session_id.is_nil()
            || self.max_messages == 0
            || self.max_messages > MAX_ARCHIVE_PROJECTION_MESSAGES
            || self.max_bytes < 2
            || self.max_bytes > MAX_ARCHIVE_PROJECTION_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivedMessage {
    pub turn_id: Uuid,
    pub message: AgentMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveSnapshot {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub pointer: ArchivePointer,
    pub messages: Vec<ArchivedMessage>,
}

impl ArchiveSnapshot {
    pub fn validate(&self, request: &ArchiveReadRequest) -> Result<(), AgentFailure> {
        if self.person_id != request.person_id
            || self.session_id != request.session_id
            || self.pointer != request.pointer
            || self.messages.len() != request.pointer.archived_message_count
            || self.messages.is_empty()
            || self
                .messages
                .iter()
                .any(|message| message.turn_id.is_nil() || message.message.validate().is_err())
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut completed = std::collections::HashSet::new();
        let mut previous = None;
        for message in &self.messages {
            if previous != Some(message.turn_id) {
                if !completed.insert(message.turn_id) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                previous = Some(message.turn_id);
            }
        }
        if self.messages.last().map(|message| message.turn_id)
            != Some(request.pointer.through_turn_id)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

pub trait ArchiveReader: Send + Sync {
    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>>;
}
