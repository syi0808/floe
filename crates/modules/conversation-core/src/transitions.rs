//! Bounded, pure Conversation Core transitions shared by the in-memory model
//! and durable storage adapters. Each function receives only the facts needed
//! for one decision; it never scans or reconstructs transcript history.

use floe_conversation_contract::{
    AdmissionDisposition, AdmissionReceipt, AdmissionResult, AdmissionTarget, AgentIdentity,
    ConversationBranchId, ConversationCheckpoint, ConversationFailure, ConversationId,
    ConversationMessage, ConversationReference, MessageAdmissionRequest, MessageId, MessageOrigin,
    RunTaskLink, TranscriptReference,
};
use floe_kernel::RunId;
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
    pub kind: TranscriptEntryKind,
    /// The producing Run is present only for generated output. Inbound
    /// messages remain independently schedulable work, regardless of origin.
    pub producer_run: Option<RunId>,
    pub commitment_version: PrefixCommitmentVersion,
    /// Commitment to every transcript entry through this entry, inclusive.
    pub prefix_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptEntryKind {
    Inbound,
    GeneratedOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrefixCommitmentVersion {
    /// Existing revision-2 rows keep their original commitment unchanged.
    V1,
    /// New entries bind their kind and producer Run into the prefix chain.
    V2,
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
    /// True when this stable message ID is already present as a generated
    /// entry without an inbound admission receipt.
    pub message_id_in_transcript: bool,
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
    /// First queued input after the active input has settled. Outputs are
    /// already settled, so this one bounded fact determines the new prefix.
    pub earliest_pending_sequence: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOutputRequest {
    pub target: ConversationReference,
    pub claim: WriterClaim,
    pub message: ConversationMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOutputReceipt {
    pub identity: AgentIdentity,
    pub producer: WriterClaim,
    pub transcript: TranscriptReference,
    pub content_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOutputFacts {
    /// Omitted on exact receipt readback, which must precede mutable checks.
    pub head: Option<ConversationHead>,
    pub active_writer: Option<WriterClaim>,
    pub executor_generation: u64,
    pub stored_receipt: Option<GeneratedOutputReceipt>,
    pub message_id_in_transcript: bool,
    pub previous_prefix_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOutputTransition {
    pub receipt: GeneratedOutputReceipt,
    /// Set only for a newly appended entry.
    pub appended: Option<TranscriptEntry>,
    /// Set only when a new entry changes the current head.
    pub next_head: Option<ConversationHead>,
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
    if facts.message_id_in_transcript {
        return Err(ConversationFailure::MessageIdConflict);
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
    let commitment_version = PrefixCommitmentVersion::V2;
    let prefix_digest = advance_entry_prefix_digest(
        facts.previous_prefix_digest,
        reference,
        &request.message,
        TranscriptEntryKind::Inbound,
        None,
        commitment_version,
    )?;
    let appended = TranscriptEntry {
        reference,
        message: request.message.clone(),
        kind: TranscriptEntryKind::Inbound,
        producer_run: None,
        commitment_version,
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
    head.completed_prefix = match facts.earliest_pending_sequence {
        Some(sequence) if sequence > claim.message.sequence && sequence <= head.head_revision => {
            sequence - 1
        }
        Some(_) => return Err(ConversationFailure::ConversationMismatch),
        None => head.head_revision,
    };
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    Ok(head)
}

/// Append one generated assistant/Tool/host entry to the current transcript.
/// A receipt is classified first so an exact retry can recover a lost commit
/// acknowledgement after the writer has completed or its generation changed.
pub fn record_generated_output(
    request: GeneratedOutputRequest,
    facts: GeneratedOutputFacts,
) -> Result<GeneratedOutputTransition, ConversationFailure> {
    request.target.validate()?;
    let content_digest = generated_output_content_digest(&request.message)?;

    if let Some(receipt) = facts.stored_receipt {
        if receipt.identity != request.target.identity
            || receipt.identity != request.claim.identity
            || receipt.transcript.conversation_id != request.target.conversation_id
            || receipt.transcript.branch_id != request.target.branch_id
            || receipt.transcript.message_id != request.message.message_id
            || receipt.producer != request.claim
            || receipt.content_digest != content_digest
        {
            return Err(ConversationFailure::MessageIdConflict);
        }
        return Ok(GeneratedOutputTransition {
            receipt,
            appended: None,
            next_head: None,
        });
    }

    request.message.validate()?;
    validate_generated_origin(&request.claim.identity, &request.message)?;
    if request.message.task_id != request.claim.link.task_id {
        return Err(ConversationFailure::TaskMismatch);
    }
    if facts.message_id_in_transcript {
        return Err(ConversationFailure::MessageIdConflict);
    }
    let head = facts
        .head
        .ok_or(ConversationFailure::ConversationMismatch)?;
    validate_target(&head, &request.target)?;
    if request.claim.identity != head.identity
        || request.claim.message.conversation_id != head.conversation_id
        || request.claim.message.branch_id != head.branch_id
    {
        return Err(ConversationFailure::WrongWriter);
    }
    if facts.active_writer.as_ref() != Some(&request.claim)
        || facts.executor_generation == 0
        || facts.executor_generation != request.claim.executor_generation
    {
        return Err(ConversationFailure::WrongWriter);
    }
    if request.claim.message.sequence
        != head
            .completed_prefix
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?
    {
        return Err(ConversationFailure::WrongWriter);
    }

    let sequence = head
        .head_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let transcript = TranscriptReference {
        conversation_id: head.conversation_id,
        branch_id: head.branch_id,
        message_id: request.message.message_id,
        sequence,
    };
    let commitment_version = PrefixCommitmentVersion::V2;
    let prefix_digest = advance_entry_prefix_digest(
        facts.previous_prefix_digest,
        transcript,
        &request.message,
        TranscriptEntryKind::GeneratedOutput,
        Some(request.claim.link.run_id),
        commitment_version,
    )?;
    let entry = TranscriptEntry {
        reference: transcript,
        message: request.message,
        kind: TranscriptEntryKind::GeneratedOutput,
        producer_run: Some(request.claim.link.run_id),
        commitment_version,
        prefix_digest,
    };
    let receipt = GeneratedOutputReceipt {
        identity: head.identity.clone(),
        producer: request.claim,
        transcript,
        content_digest,
    };
    let mut next_head = head;
    next_head.head_revision = sequence;
    next_head.state_revision = next_head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    Ok(GeneratedOutputTransition {
        receipt,
        appended: Some(entry),
        next_head: Some(next_head),
    })
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
    advance_entry_prefix_digest(
        previous,
        reference,
        message,
        TranscriptEntryKind::Inbound,
        None,
        PrefixCommitmentVersion::V1,
    )
}

pub fn advance_entry_prefix_digest(
    previous: [u8; 32],
    reference: TranscriptReference,
    message: &ConversationMessage,
    kind: TranscriptEntryKind,
    producer_run: Option<RunId>,
    version: PrefixCommitmentVersion,
) -> Result<[u8; 32], ConversationFailure> {
    if (kind == TranscriptEntryKind::Inbound && producer_run.is_some())
        || (kind == TranscriptEntryKind::GeneratedOutput && producer_run.is_none())
        || (version == PrefixCommitmentVersion::V1
            && (kind != TranscriptEntryKind::Inbound || producer_run.is_some()))
    {
        return Err(ConversationFailure::InvalidInput);
    }
    let origin =
        serde_json::to_vec(&message.origin).map_err(|_| ConversationFailure::InvalidInput)?;
    let mut digest = Sha256::new();
    match version {
        PrefixCommitmentVersion::V1 => {
            digest.update(b"floe-conversation-transcript-prefix-chain-v1\0");
        }
        PrefixCommitmentVersion::V2 => {
            digest.update(b"floe-conversation-transcript-prefix-chain-v2\0");
            digest.update([match kind {
                TranscriptEntryKind::Inbound => 0,
                TranscriptEntryKind::GeneratedOutput => 1,
            }]);
            if let Some(run_id) = producer_run {
                digest.update([1]);
                digest.update(run_id.as_uuid().as_bytes());
            } else {
                digest.update([0]);
            }
        }
    }
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

/// Hash the complete normalized output message. The evidence bytes themselves
/// remain outside the transcript; only their content-addressed reference is
/// included in this canonical serialization.
pub fn generated_output_content_digest(
    message: &ConversationMessage,
) -> Result<[u8; 32], ConversationFailure> {
    let encoded = serde_json::to_vec(message).map_err(|_| ConversationFailure::InvalidInput)?;
    let mut digest = Sha256::new();
    digest.update(b"floe-conversation-generated-output-content-v1\0");
    digest.update((encoded.len() as u64).to_be_bytes());
    digest.update(encoded);
    Ok(digest.finalize().into())
}

fn validate_generated_origin(
    identity: &AgentIdentity,
    message: &ConversationMessage,
) -> Result<(), ConversationFailure> {
    match &message.origin {
        MessageOrigin::Person { .. } => Err(ConversationFailure::AgentMismatch),
        MessageOrigin::Agent { agent_instance_id }
        | MessageOrigin::Tool {
            agent_instance_id, ..
        } if *agent_instance_id != identity.agent_instance_id => {
            Err(ConversationFailure::AgentMismatch)
        }
        MessageOrigin::Agent { .. } | MessageOrigin::Tool { .. } | MessageOrigin::Host => Ok(()),
    }
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
