use floe_agent_contract::{
    ArchivePointer, ArchivedMessage,
    MAX_ARCHIVE_PROJECTION_BYTES as AGENT_MAX_ARCHIVE_PROJECTION_BYTES,
    MAX_ARCHIVE_PROJECTION_MESSAGES as AGENT_MAX_ARCHIVE_PROJECTION_MESSAGES,
};
use floe_context_contract::PersonId;
use uuid::Uuid;

pub const MAX_ARCHIVE_PROJECTION_BYTES: usize = AGENT_MAX_ARCHIVE_PROJECTION_BYTES;
pub const MAX_ARCHIVE_PROJECTION_MESSAGES: usize = AGENT_MAX_ARCHIVE_PROJECTION_MESSAGES;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveProjection {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub pointer: ArchivePointer,
    pub messages: Vec<ArchivedMessage>,
}

pub use floe_agent_contract::ArchiveReader;
