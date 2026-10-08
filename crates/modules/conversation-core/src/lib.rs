//! Pure role-neutral conversation state transitions.
//!
//! This crate owns no persistence, product policy, Agent Directory, Task
//! lifecycle, A2A binding, or learning hooks. Its admission API accepts
//! inbound work and schedules it for a Run; it does not record generated
//! assistant or Tool output. A production store must apply transitions
//! atomically through the Core-owned store port.

use std::collections::{HashMap, HashSet, VecDeque};

use floe_conversation_contract::{
    AdmissionDisposition, AdmissionReceipt, AdmissionResult, AdmissionTarget,
    ConversationCheckpoint, ConversationFailure, ConversationId, ConversationMessage,
    ConversationReference, MessageAdmissionRequest, MessageId, RunTaskLink, TranscriptReference,
};
use floe_execution::BoxFuture;
use floe_kernel::{CommandId, RunId};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriterClaim {
    pub link: RunTaskLink,
    pub message: TranscriptReference,
    pub writer_epoch: u64,
}

/// Core-owned persistence port. Implementations must commit an admission's
/// message, head revision, Task link, pending-schedule marker and
/// command/message receipt atomically. Dispatch is allowed only after this
/// method returns a durable receipt. `admit` represents inbound work scheduled
/// for a Run; generated assistant/Tool output uses a distinct future path.
pub trait ConversationStorePort: Send + Sync {
    fn admit<'a>(
        &'a self,
        request: MessageAdmissionRequest,
    ) -> BoxFuture<'a, Result<AdmissionResult, ConversationFailure>>;

    /// Atomically claim the next queued message for one Run writer. A second
    /// writer must fail while the current claim remains active.
    fn claim_writer<'a>(
        &'a self,
        target: ConversationReference,
        link: RunTaskLink,
    ) -> BoxFuture<'a, Result<WriterClaim, ConversationFailure>>;

    /// Release only the matching active Run claim after its owner has settled.
    fn complete_writer<'a>(
        &'a self,
        conversation_id: ConversationId,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<(), ConversationFailure>>;

    /// Apply a checkpoint only to an exact completed prefix. Active and queued
    /// input stays protected; later appended messages may remain outside the
    /// prefix. Implementations reject stale scope or checkpoint regression.
    fn apply_checkpoint<'a>(
        &'a self,
        checkpoint: ConversationCheckpoint,
    ) -> BoxFuture<'a, Result<(), ConversationFailure>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TranscriptEntry {
    reference: TranscriptReference,
    message: ConversationMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredAdmission {
    message: ConversationMessage,
    receipt: AdmissionReceipt,
}

/// One in-memory transition model, suitable for deterministic contract tests.
/// It is not a Vault row or a replacement for the existing Session aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationCore {
    identity: floe_conversation_contract::AgentIdentity,
    conversation_id: ConversationId,
    branch_id: floe_conversation_contract::ConversationBranchId,
    head_revision: u64,
    completed_prefix: u64,
    state_revision: u64,
    transcript: Vec<TranscriptEntry>,
    pending: VecDeque<TranscriptReference>,
    messages: HashMap<MessageId, StoredAdmission>,
    commands: HashMap<CommandId, MessageId>,
    active_writer: Option<WriterClaim>,
    used_runs: HashSet<RunId>,
    completed_runs: Vec<RunTaskLink>,
    checkpoint: Option<ConversationCheckpoint>,
}

impl ConversationCore {
    /// Admit inbound work to a new isolated conversation and queue it for a
    /// Run. Generated assistant/Tool output needs a separate recording path.
    pub fn open(
        request: MessageAdmissionRequest,
    ) -> Result<(Self, AdmissionResult), ConversationFailure> {
        request.validate()?;
        let (identity, conversation_id, branch_id) = match request.target {
            AdmissionTarget::New {
                identity,
                conversation_id,
                branch_id,
            } => (identity, conversation_id, branch_id),
            AdmissionTarget::Continue { .. } => {
                return Err(ConversationFailure::ConversationMismatch);
            }
        };
        let mut conversation = Self {
            identity,
            conversation_id,
            branch_id,
            head_revision: 0,
            completed_prefix: 0,
            state_revision: 0,
            transcript: Vec::new(),
            pending: VecDeque::new(),
            messages: HashMap::new(),
            commands: HashMap::new(),
            active_writer: None,
            used_runs: HashSet::new(),
            completed_runs: Vec::new(),
            checkpoint: None,
        };
        let result = conversation.append(request.message)?;
        Ok((conversation, result))
    }

    /// Admit inbound work against an explicit current head and queue it for a
    /// Run. Exact replay is classified before the mutable revision check.
    pub fn continue_with(
        &mut self,
        request: MessageAdmissionRequest,
    ) -> Result<AdmissionResult, ConversationFailure> {
        request.validate()?;
        let reference = match request.target {
            AdmissionTarget::Continue { reference } => reference,
            AdmissionTarget::New { .. } => return Err(ConversationFailure::ConversationMismatch),
        };
        self.check_target(&reference)?;

        if let Some(result) = self.replay(&request.message)? {
            return Ok(result);
        }
        if reference.head_revision != self.head_revision {
            return Err(ConversationFailure::RevisionConflict);
        }
        self.append(request.message)
    }

    /// Reserve the next FIFO inbox message for one execution writer.
    pub fn claim_next_writer(
        &mut self,
        link: RunTaskLink,
    ) -> Result<WriterClaim, ConversationFailure> {
        link.validate()?;
        if self.active_writer.is_some() {
            return Err(ConversationFailure::WriterAlreadyActive);
        }
        if self.used_runs.contains(&link.run_id) {
            return Err(ConversationFailure::RunAlreadyUsed);
        }
        let message = self
            .pending
            .front()
            .copied()
            .ok_or(ConversationFailure::NoPendingMessage)?;
        let message_index = usize::try_from(message.sequence)
            .ok()
            .and_then(|sequence| sequence.checked_sub(1))
            .ok_or(ConversationFailure::ConversationMismatch)?;
        let entry = self
            .transcript
            .get(message_index)
            .filter(|entry| entry.reference == message)
            .ok_or(ConversationFailure::ConversationMismatch)?;
        if entry.message.task_id != link.task_id {
            return Err(ConversationFailure::TaskMismatch);
        }
        let writer_epoch = self
            .state_revision
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        self.pending.pop_front();
        self.used_runs.insert(link.run_id);
        let claim = WriterClaim {
            link,
            message,
            writer_epoch,
        };
        self.active_writer = Some(claim);
        self.state_revision = writer_epoch;
        Ok(claim)
    }

    /// Release exactly the matching writer. A later Run may retain the same
    /// host Task ID without changing any Task state or legacy receipt.
    pub fn complete_writer(&mut self, run_id: RunId) -> Result<RunTaskLink, ConversationFailure> {
        let active = self.active_writer.ok_or(ConversationFailure::WrongWriter)?;
        if active.link.run_id != run_id {
            return Err(ConversationFailure::WrongWriter);
        }
        let expected_sequence = self
            .completed_prefix
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        if active.message.sequence != expected_sequence {
            return Err(ConversationFailure::ConversationMismatch);
        }
        let next_state_revision = self
            .state_revision
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        self.active_writer = None;
        self.completed_prefix = active.message.sequence;
        self.completed_runs.push(active.link);
        self.state_revision = next_state_revision;
        Ok(active.link)
    }

    /// Apply a checkpoint only when it names this conversation's exact prefix.
    pub fn apply_checkpoint(
        &mut self,
        checkpoint: ConversationCheckpoint,
    ) -> Result<(), ConversationFailure> {
        checkpoint.validate()?;
        if checkpoint.through.conversation_id != self.conversation_id
            || checkpoint.through.branch_id != self.branch_id
        {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let index = self.checkpoint_prefix_index(checkpoint.through)?;
        if self
            .checkpoint
            .as_ref()
            .is_some_and(|current| checkpoint.through.sequence < current.through.sequence)
        {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let entry = self
            .transcript
            .get(index)
            .ok_or(ConversationFailure::CheckpointMismatch)?;
        if entry.reference != checkpoint.through
            || self.prefix_digest(checkpoint.through.sequence)? != checkpoint.prefix_digest
        {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        self.state_revision = self
            .state_revision
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        self.checkpoint = Some(checkpoint);
        Ok(())
    }

    /// Build a checkpoint request from this exact transcript prefix. This
    /// helper does not grant permission to reuse the summary elsewhere.
    pub fn checkpoint_for(
        &self,
        through: TranscriptReference,
        summary: impl Into<String>,
    ) -> Result<ConversationCheckpoint, ConversationFailure> {
        if through.conversation_id != self.conversation_id || through.branch_id != self.branch_id {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let index = self.checkpoint_prefix_index(through)?;
        if self
            .transcript
            .get(index)
            .is_none_or(|entry| entry.reference != through)
        {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let checkpoint = ConversationCheckpoint {
            through,
            prefix_digest: self.prefix_digest(through.sequence)?,
            summary: summary.into(),
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    pub fn reference(&self) -> ConversationReference {
        ConversationReference {
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            identity: self.identity.clone(),
            head_revision: self.head_revision,
        }
    }

    pub fn head_revision(&self) -> u64 {
        self.head_revision
    }

    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub fn active_writer(&self) -> Option<WriterClaim> {
        self.active_writer
    }

    pub fn pending_messages(&self) -> impl Iterator<Item = TranscriptReference> + '_ {
        self.pending.iter().copied()
    }

    pub fn completed_runs(&self) -> &[RunTaskLink] {
        &self.completed_runs
    }

    pub fn transcript(&self) -> impl Iterator<Item = (&TranscriptReference, &ConversationMessage)> {
        self.transcript
            .iter()
            .map(|entry| (&entry.reference, &entry.message))
    }

    pub fn checkpoint(&self) -> Option<&ConversationCheckpoint> {
        self.checkpoint.as_ref()
    }

    fn check_target(&self, reference: &ConversationReference) -> Result<(), ConversationFailure> {
        reference.validate()?;
        if reference.conversation_id != self.conversation_id
            || reference.branch_id != self.branch_id
        {
            return Err(ConversationFailure::ConversationMismatch);
        }
        if reference.identity != self.identity {
            return Err(ConversationFailure::AgentMismatch);
        }
        Ok(())
    }

    fn checkpoint_prefix_index(
        &self,
        through: TranscriptReference,
    ) -> Result<usize, ConversationFailure> {
        through.validate()?;
        if through.sequence > self.completed_prefix {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let index = usize::try_from(through.sequence)
            .ok()
            .and_then(|sequence| sequence.checked_sub(1))
            .ok_or(ConversationFailure::CheckpointMismatch)?;
        if self
            .transcript
            .get(index)
            .is_none_or(|entry| entry.reference != through)
        {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        Ok(index)
    }

    fn replay(
        &mut self,
        message: &ConversationMessage,
    ) -> Result<Option<AdmissionResult>, ConversationFailure> {
        if let Some(stored) = self.messages.get(&message.message_id) {
            if !stored.message.same_delivery(message) {
                return Err(ConversationFailure::MessageIdConflict);
            }
            if self
                .commands
                .get(&message.command_id)
                .is_some_and(|message_id| *message_id != message.message_id)
            {
                return Err(ConversationFailure::CommandIdConflict);
            }
            let receipt = stored.receipt.clone();
            if !self.commands.contains_key(&message.command_id) {
                let next_revision = self
                    .state_revision
                    .checked_add(1)
                    .ok_or(ConversationFailure::InvalidInput)?;
                self.commands.insert(message.command_id, message.message_id);
                self.state_revision = next_revision;
            }
            return Ok(Some(AdmissionResult {
                disposition: AdmissionDisposition::Replayed,
                receipt,
            }));
        }
        if self.commands.contains_key(&message.command_id) {
            return Err(ConversationFailure::CommandIdConflict);
        }
        Ok(None)
    }

    fn append(
        &mut self,
        message: ConversationMessage,
    ) -> Result<AdmissionResult, ConversationFailure> {
        message.validate()?;
        if let Some(origin_person) = match &message.origin {
            floe_conversation_contract::MessageOrigin::Person { person_id } => Some(*person_id),
            _ => None,
        } {
            if origin_person != self.identity.person_id {
                return Err(ConversationFailure::AgentMismatch);
            }
        }
        if self.messages.contains_key(&message.message_id) {
            return Err(ConversationFailure::MessageIdConflict);
        }
        if self.commands.contains_key(&message.command_id) {
            return Err(ConversationFailure::CommandIdConflict);
        }
        let sequence = self
            .head_revision
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        let reference = TranscriptReference {
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            message_id: message.message_id,
            sequence,
        };
        let receipt = AdmissionReceipt {
            transcript: reference,
            head_revision: sequence,
            task_id: message.task_id,
        };
        let disposition = if self.active_writer.is_some() {
            AdmissionDisposition::Queued
        } else {
            AdmissionDisposition::Appended
        };
        let next_state_revision = self
            .state_revision
            .checked_add(1)
            .ok_or(ConversationFailure::InvalidInput)?;
        self.transcript.push(TranscriptEntry {
            reference,
            message: message.clone(),
        });
        self.pending.push_back(reference);
        self.messages.insert(
            message.message_id,
            StoredAdmission {
                message: message.clone(),
                receipt: receipt.clone(),
            },
        );
        self.commands.insert(message.command_id, message.message_id);
        self.head_revision = sequence;
        self.state_revision = next_state_revision;
        Ok(AdmissionResult {
            disposition,
            receipt,
        })
    }

    fn prefix_digest(&self, through_sequence: u64) -> Result<[u8; 32], ConversationFailure> {
        let count =
            usize::try_from(through_sequence).map_err(|_| ConversationFailure::InvalidInput)?;
        if count == 0 || count > self.transcript.len() {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let mut digest = Sha256::new();
        for entry in self.transcript.iter().take(count) {
            digest.update(entry.reference.message_id.as_uuid().as_bytes());
            digest.update(entry.message.body_digest());
            let origin = serde_json::to_vec(&entry.message.origin)
                .map_err(|_| ConversationFailure::InvalidInput)?;
            digest.update((origin.len() as u64).to_be_bytes());
            digest.update(origin);
            if let Some(task_id) = entry.message.task_id {
                digest.update([1]);
                digest.update(task_id.as_uuid().as_bytes());
            } else {
                digest.update([0]);
            }
        }
        Ok(digest.finalize().into())
    }
}
