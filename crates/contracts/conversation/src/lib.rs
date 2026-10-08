//! Role-neutral identity, transcript, admission, and checkpoint contracts.
//!
//! These values describe an agent conversation; they do not authenticate a
//! sender, grant source access, own a persistence port, or define a persisted
//! Vault schema.

mod identity;
mod message;

pub use identity::{
    AgentIdentity, AgentInstanceId, AssignmentId, ConversationBranchId, ConversationId,
    ConversationReference, MessageId, RunTaskLink, TranscriptReference,
};
pub use message::{
    AdmissionDisposition, AdmissionReceipt, AdmissionResult, AdmissionTarget,
    ConversationCheckpoint, ConversationMessage, MessageAdmissionRequest, MessageEvidenceReference,
    MessageOrigin,
};

/// A semantic failure for the role-neutral conversation contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationFailure {
    InvalidInput,
    ConversationConflict,
    ConversationMismatch,
    AgentMismatch,
    TaskMismatch,
    RevisionConflict,
    MessageIdConflict,
    CommandIdConflict,
    WriterAlreadyActive,
    NoPendingMessage,
    RunAlreadyUsed,
    WrongWriter,
    CheckpointMismatch,
    StorageUnavailable,
}
