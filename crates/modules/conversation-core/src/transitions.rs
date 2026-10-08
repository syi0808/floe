//! Bounded, pure Conversation Core transitions shared by the in-memory model
//! and durable storage adapters. Each function receives only the facts needed
//! for one decision; it never scans or reconstructs transcript history.

use floe_conversation_contract::{
    AdmissionDisposition, AdmissionReceipt, AdmissionResult, AdmissionTarget, AgentIdentity,
    ConversationBranchId, ConversationCheckpoint, ConversationFailure, ConversationId,
    ConversationMessage, ConversationReference, MessageAdmissionRequest, MessageId, MessageOrigin,
    RunTaskLink, TranscriptReference,
};
use sha2::{Digest, Sha256};

/// The initial value fed to the chained transcript prefix commitment.
pub const EMPTY_PREFIX_DIGEST: [u8; 32] = [0; 32];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationHead {
    pub identity: AgentIdentity,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub head_revision: u64,
    pub completed_prefix: u64,
    pub state_revision: u64,
}

impl ConversationHead {
    pub fn reference(&self) -> ConversationReference {
        ConversationReference {
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            identity: self.identity.clone(),
            head_revision: self.head_revision,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptEntry {
    pub reference: TranscriptReference,
    pub message: ConversationMessage,
    /// Commitment to every transcript entry through this entry, inclusive.
    pub prefix_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAdmission {
    pub message: ConversationMessage,
    pub receipt: AdmissionReceipt,
}

/// Owner-wide command occupancy. Message IDs remain conversation-local, while
/// a Person's CommandId binds to one agent/conversation/branch and delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredCommand {
    pub identity: AgentIdentity,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub admission: StoredAdmission,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriterClaim {
    pub identity: AgentIdentity,
    pub link: RunTaskLink,
    pub message: TranscriptReference,
    /// Conversation-local state revision at which this claim was made.
    pub writer_epoch: u64,
    /// The existing Conversation executor generation that fenced this claim.
    pub executor_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionFacts {
    pub head: Option<ConversationHead>,
    pub active_writer: Option<WriterClaim>,
    pub message: Option<StoredAdmission>,
    pub command: Option<StoredCommand>,
    pub previous_prefix_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionTransition {
    pub result: AdmissionResult,
    pub head: ConversationHead,
    pub appended: Option<TranscriptEntry>,
    /// Insert a command receipt for this message when present. Exact retries
    /// of an already recorded command leave this absent.
    pub command_receipt: Option<MessageId>,
    pub head_changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMessage {
    pub entry: TranscriptEntry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimFacts {
    pub head: ConversationHead,
    pub active_writer: Option<WriterClaim>,
    pub earliest_pending: Option<PendingMessage>,
    pub run_already_used: bool,
    pub requested_executor_generation: u64,
    pub executor_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimTransition {
    pub head: ConversationHead,
    pub claim: WriterClaim,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionFacts {
    pub head: ConversationHead,
    pub active_writer: Option<WriterClaim>,
    pub executor_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointFacts {
    pub head: ConversationHead,
    pub current_checkpoint_sequence: Option<u64>,
    /// The already stored prefix commitment for the requested boundary.
    pub stored_prefix_digest: Option<[u8; 32]>,
}

pub fn admit(
    facts: AdmissionFacts,
    request: MessageAdmissionRequest,
) -> Result<AdmissionTransition, ConversationFailure> {
    request.validate()?;
    let existing_head = facts.head.is_some();

    let (target_identity, target_conversation, target_branch, expected_revision, is_new) =
        match &request.target {
            AdmissionTarget::New {
                identity,
                conversation_id,
                branch_id,
            } => (identity, *conversation_id, *branch_id, None, true),
            AdmissionTarget::Continue { reference } => (
                &reference.identity,
                reference.conversation_id,
                reference.branch_id,
                Some(reference.head_revision),
                false,
            ),
        };

    // A CommandId is scoped to its Person, not its destination conversation.
    // Classify its exact replay before consulting mutable head revisions or
    // allowing the request to route the same durable effect elsewhere.
    if let Some(command) = facts.command {
        if command.identity != *target_identity
            || command.conversation_id != target_conversation
            || command.branch_id != target_branch
            || !command.admission.message.same_delivery(&request.message)
        {
            return Err(ConversationFailure::CommandIdConflict);
        }
        let head = facts
            .head
            .ok_or(ConversationFailure::ConversationMismatch)?;
        if head.conversation_id != target_conversation || head.branch_id != target_branch {
            return Err(ConversationFailure::ConversationMismatch);
        }
        if head.identity != *target_identity || head.identity != command.identity {
            return Err(ConversationFailure::AgentMismatch);
        }
        return Ok(AdmissionTransition {
            result: AdmissionResult {
                disposition: AdmissionDisposition::Replayed,
                receipt: command.admission.receipt,
            },
            head,
            appended: None,
            command_receipt: None,
            head_changed: false,
        });
    }

    let mut head = match facts.head {
        Some(head) => {
            if head.conversation_id != target_conversation || head.branch_id != target_branch {
                return Err(ConversationFailure::ConversationMismatch);
            }
            if &head.identity != target_identity {
                return Err(ConversationFailure::AgentMismatch);
            }
            head
        }
        None if is_new => ConversationHead {
            identity: target_identity.clone(),
            conversation_id: target_conversation,
            branch_id: target_branch,
            head_revision: 0,
            completed_prefix: 0,
            state_revision: 0,
        },
        None => return Err(ConversationFailure::ConversationMismatch),
    };

    // Message idempotency remains local to this conversation and is also
    // classified before the mutable head revision check.
    if let Some(stored) = &facts.message {
        if !stored.message.same_delivery(&request.message) {
            return Err(ConversationFailure::MessageIdConflict);
        }
        head.state_revision = head
            .state_revision
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        return Ok(AdmissionTransition {
            result: AdmissionResult {
                disposition: AdmissionDisposition::Replayed,
                receipt: stored.receipt.clone(),
            },
            head,
            appended: None,
            command_receipt: Some(request.message.message_id),
            head_changed: true,
        });
    }
    if is_new && existing_head {
        return Err(ConversationFailure::ConversationConflict);
    }
    if let Some(expected_revision) = expected_revision {
        if expected_revision != head.head_revision {
            return Err(ConversationFailure::RevisionConflict);
        }
    }

    validate_message_owner(&head.identity, &request.message)?;
    let sequence = head
        .head_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let reference = TranscriptReference {
        conversation_id: head.conversation_id,
        branch_id: head.branch_id,
        message_id: request.message.message_id,
        sequence,
    };
    let receipt = AdmissionReceipt {
        transcript: reference,
        head_revision: sequence,
        task_id: request.message.task_id,
    };
    let prefix_digest =
        advance_prefix_digest(facts.previous_prefix_digest, reference, &request.message)?;
    let appended = TranscriptEntry {
        reference,
        message: request.message.clone(),
        prefix_digest,
    };
    let disposition = if facts.active_writer.is_some() {
        AdmissionDisposition::Queued
    } else {
        AdmissionDisposition::Appended
    };
    head.head_revision = sequence;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;

    Ok(AdmissionTransition {
        result: AdmissionResult {
            disposition,
            receipt,
        },
        head,
        appended: Some(appended),
        command_receipt: Some(request.message.message_id),
        head_changed: true,
    })
}

pub fn claim_writer(
    target: &ConversationReference,
    link: RunTaskLink,
    facts: ClaimFacts,
) -> Result<ClaimTransition, ConversationFailure> {
    validate_target(&facts.head, target)?;
    link.validate()?;
    if target.head_revision != facts.head.head_revision {
        return Err(ConversationFailure::RevisionConflict);
    }
    if facts.executor_generation == 0
        || facts.executor_generation != facts.requested_executor_generation
    {
        return Err(ConversationFailure::WrongWriter);
    }
    if facts.active_writer.is_some() {
        return Err(ConversationFailure::WriterAlreadyActive);
    }
    if facts.run_already_used {
        return Err(ConversationFailure::RunAlreadyUsed);
    }
    let entry = facts
        .earliest_pending
        .ok_or(ConversationFailure::NoPendingMessage)?
        .entry;
    if entry.reference.conversation_id != facts.head.conversation_id
        || entry.reference.branch_id != facts.head.branch_id
        || entry.reference.sequence
            != facts
                .head
                .completed_prefix
                .checked_add(1)
                .ok_or(ConversationFailure::InvalidInput)?
    {
        return Err(ConversationFailure::ConversationMismatch);
    }
    if entry.message.task_id != link.task_id {
        return Err(ConversationFailure::TaskMismatch);
    }
    let writer_epoch = facts
        .head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let claim = WriterClaim {
        identity: facts.head.identity.clone(),
        link,
        message: entry.reference,
        writer_epoch,
        executor_generation: facts.executor_generation,
    };
    let mut head = facts.head;
    head.state_revision = writer_epoch;
    Ok(ClaimTransition { head, claim })
}

pub fn complete_writer(
    claim: WriterClaim,
    facts: CompletionFacts,
) -> Result<ConversationHead, ConversationFailure> {
    if claim.identity != facts.head.identity
        || claim.message.conversation_id != facts.head.conversation_id
        || claim.message.branch_id != facts.head.branch_id
    {
        return Err(ConversationFailure::AgentMismatch);
    }
    if facts.active_writer.as_ref() != Some(&claim)
        || facts.executor_generation != claim.executor_generation
    {
        return Err(ConversationFailure::WrongWriter);
    }
    let expected_sequence = facts
        .head
        .completed_prefix
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    if claim.message.sequence != expected_sequence {
        return Err(ConversationFailure::ConversationMismatch);
    }
    let mut head = facts.head;
    head.completed_prefix = claim.message.sequence;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    Ok(head)
}

pub fn apply_checkpoint(
    target: &ConversationReference,
    checkpoint: &ConversationCheckpoint,
    facts: CheckpointFacts,
) -> Result<ConversationHead, ConversationFailure> {
    validate_target(&facts.head, target)?;
    checkpoint.validate()?;
    if checkpoint.through.conversation_id != facts.head.conversation_id
        || checkpoint.through.branch_id != facts.head.branch_id
        || checkpoint.through.sequence > facts.head.completed_prefix
        || facts
            .current_checkpoint_sequence
            .is_some_and(|sequence| checkpoint.through.sequence < sequence)
        || facts.stored_prefix_digest != Some(checkpoint.prefix_digest)
    {
        return Err(ConversationFailure::CheckpointMismatch);
    }
    let mut head = facts.head;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    Ok(head)
}

pub fn validate_target(
    head: &ConversationHead,
    reference: &ConversationReference,
) -> Result<(), ConversationFailure> {
    reference.validate()?;
    if reference.conversation_id != head.conversation_id || reference.branch_id != head.branch_id {
        return Err(ConversationFailure::ConversationMismatch);
    }
    if reference.identity != head.identity {
        return Err(ConversationFailure::AgentMismatch);
    }
    Ok(())
}

pub fn checkpoint_from_prefix(
    through: TranscriptReference,
    prefix_digest: [u8; 32],
    summary: impl Into<String>,
) -> Result<ConversationCheckpoint, ConversationFailure> {
    let checkpoint = ConversationCheckpoint {
        through,
        prefix_digest,
        summary: summary.into(),
    };
    checkpoint.validate()?;
    Ok(checkpoint)
}

pub fn advance_prefix_digest(
    previous: [u8; 32],
    reference: TranscriptReference,
    message: &ConversationMessage,
) -> Result<[u8; 32], ConversationFailure> {
    let origin =
        serde_json::to_vec(&message.origin).map_err(|_| ConversationFailure::InvalidInput)?;
    let mut digest = Sha256::new();
    digest.update(b"floe-conversation-transcript-prefix-chain-v1\0");
    digest.update(previous);
    digest.update(reference.conversation_id.as_uuid().as_bytes());
    digest.update(reference.branch_id.as_uuid().as_bytes());
    digest.update(reference.sequence.to_be_bytes());
    digest.update(reference.message_id.as_uuid().as_bytes());
    digest.update(message.body_digest());
    digest.update((origin.len() as u64).to_be_bytes());
    digest.update(origin);
    if let Some(task_id) = message.task_id {
        digest.update([1]);
        digest.update(task_id.as_uuid().as_bytes());
    } else {
        digest.update([0]);
    }
    Ok(digest.finalize().into())
}

fn validate_message_owner(
    identity: &AgentIdentity,
    message: &ConversationMessage,
) -> Result<(), ConversationFailure> {
    if let MessageOrigin::Person { person_id } = &message.origin {
        if *person_id != identity.person_id {
            return Err(ConversationFailure::AgentMismatch);
        }
    }
    Ok(())
}
