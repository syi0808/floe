use floe_agent_contract::{AgentFailure, AgentMessage, BoxFuture};
use floe_context_contract::PersonId;
use uuid::Uuid;

pub const MAX_ARCHIVE_PROJECTION_BYTES: usize = 128 * 1024;
pub const MAX_ARCHIVE_PROJECTION_MESSAGES: usize = floe_agent_contract::MAX_AGENT_MESSAGES;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveProjection {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub pointer: ArchivePointer,
    pub messages: Vec<ArchivedMessage>,
}

pub trait ArchiveReader: Send + Sync {
    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>>;
}
