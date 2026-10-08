//! Pure role-neutral conversation state transitions.
//!
//! This crate owns no persistence, product policy, Agent Directory, Task
//! lifecycle, A2A binding, or learning hooks. Its admission API accepts
//! inbound work and schedules it for a Run; it does not record generated
//! assistant or Tool output. A production store must apply transitions
//! atomically through the Core-owned store port.

use std::collections::{HashMap, HashSet, VecDeque};

use floe_conversation_contract::{
    AdmissionResult, AdmissionTarget, ConversationCheckpoint, ConversationFailure, ConversationId,
    ConversationMessage, ConversationReference, MessageAdmissionRequest, MessageId, RunTaskLink,
    TranscriptReference,
};
use floe_execution::BoxFuture;
use floe_kernel::{CommandId, RunId};

mod transitions;
pub use transitions::{
    AdmissionFacts, AdmissionTransition, CheckpointFacts, ClaimFacts, ClaimTransition,
    CompletionFacts, ConversationHead, EMPTY_PREFIX_DIGEST, PendingMessage, StoredAdmission,
    StoredCommand, TranscriptEntry, WriterClaim, admit as admit_transition, advance_prefix_digest,
    apply_checkpoint as apply_checkpoint_transition, claim_writer as claim_writer_transition,
    complete_writer as complete_writer_transition, validate_target as validate_reference_target,
};
use transitions::{
    admit as transition_admit, apply_checkpoint as transition_checkpoint, checkpoint_from_prefix,
    claim_writer as transition_claim_writer, complete_writer as transition_complete_writer,
};

/// Storage failures preserve the distinction between a confirmed rollback and
/// a commit whose acknowledgement or settlement is unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationStoreFailure {
    Transition(ConversationFailure),
    Busy,
    Unavailable,
    NotCommitted,
    OutcomeUnknown,
    PageItemExceedsBudget,
}

impl From<ConversationFailure> for ConversationStoreFailure {
    fn from(value: ConversationFailure) -> Self {
        Self::Transition(value)
    }
}

pub const MAX_TRANSCRIPT_PAGE_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptPageBudget {
    pub max_entries: usize,
    /// Sum of serialized UTF-8 `ConversationMessage` payload bytes.
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptPage {
    pub entries: Vec<TranscriptEntry>,
    /// Cursor for the last returned entry. Reuse it as `after` on the next page.
    pub next_cursor: Option<TranscriptReference>,
    pub has_more: bool,
    pub encoded_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterRecoveryState {
    /// No exact Run receipt exists in the requested conversation branch.
    Absent,
    /// The exact claim is still active and fenced by the current owner
    /// executor generation. This observation does not authorize dispatch.
    ActiveCurrentGeneration,
    /// The exact claim remains stored as active, but its executor generation
    /// is stale. It must not be released or re-executed by this API.
    Interrupted,
    /// The exact writer receipt is durably completed.
    Completed,
}

/// Bounded readback for one exact Run receipt. `active_writer` describes the
/// conversation head now; `writer` describes the requested Run receipt. A
/// missing observation is a successful read only when state is `Absent`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriterRecoveryObservation {
    pub state: WriterRecoveryState,
    pub head: ConversationHead,
    pub writer: Option<WriterClaim>,
    pub active_writer: Option<WriterClaim>,
    pub current_executor_generation: u64,
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
    ) -> BoxFuture<'a, Result<AdmissionResult, ConversationStoreFailure>>;

    /// Read a stable sequence page under independent item and serialized-byte
    /// budgets. The requested page is never materialized as a full history.
    fn read_transcript_page<'a>(
        &'a self,
        target: ConversationReference,
        after: Option<TranscriptReference>,
        budget: TranscriptPageBudget,
    ) -> BoxFuture<'a, Result<TranscriptPage, ConversationStoreFailure>>;

    /// Atomically claim the next queued message for one Run writer. A second
    /// writer must fail while the current claim remains active.
    fn claim_writer<'a>(
        &'a self,
        target: ConversationReference,
        link: RunTaskLink,
        executor_generation: u64,
    ) -> BoxFuture<'a, Result<WriterClaim, ConversationStoreFailure>>;

    /// Release only the matching active Run/Task/transcript/writer/executor
    /// fence after its owner has settled.
    fn complete_writer<'a>(
        &'a self,
        claim: WriterClaim,
    ) -> BoxFuture<'a, Result<(), ConversationStoreFailure>>;

    /// Read the exact stored receipt for one Run together with the current
    /// head and active claim. This is observability only: it does not grant
    /// dispatch authority, release an interrupted claim, or resume its Run.
    fn observe_writer<'a>(
        &'a self,
        target: ConversationReference,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<WriterRecoveryObservation, ConversationStoreFailure>>;

    /// Apply a checkpoint only to an exact completed prefix. Active and queued
    /// input stays protected; later appended messages may remain outside the
    /// prefix. Implementations reject stale scope or checkpoint regression.
    fn apply_checkpoint<'a>(
        &'a self,
        target: ConversationReference,
        checkpoint: ConversationCheckpoint,
    ) -> BoxFuture<'a, Result<(), ConversationStoreFailure>>;
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
    commands: HashMap<CommandId, StoredCommand>,
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
        let (identity, conversation_id, branch_id) = match &request.target {
            AdmissionTarget::New {
                identity,
                conversation_id,
                branch_id,
            } => (identity.clone(), *conversation_id, *branch_id),
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
        let transition = transition_admit(
            AdmissionFacts {
                head: None,
                active_writer: None,
                message: None,
                command: None,
                previous_prefix_digest: EMPTY_PREFIX_DIGEST,
            },
            request.clone(),
        )?;
        let result = transition.result.clone();
        conversation.apply_admission_transition(transition, request.message);
        Ok((conversation, result))
    }

    /// Admit inbound work against an explicit current head and queue it for a
    /// Run. Exact replay is classified before the mutable revision check.
    pub fn continue_with(
        &mut self,
        request: MessageAdmissionRequest,
    ) -> Result<AdmissionResult, ConversationFailure> {
        let facts = self.admission_facts(&request);
        let transition = transition_admit(facts, request.clone())?;
        let result = transition.result.clone();
        self.apply_admission_transition(transition, request.message);
        Ok(result)
    }

    /// Reserve the next FIFO inbox message for one execution writer.
    pub fn claim_next_writer(
        &mut self,
        link: RunTaskLink,
        executor_generation: u64,
    ) -> Result<WriterClaim, ConversationFailure> {
        let earliest_pending = self.pending.front().and_then(|reference| {
            usize::try_from(reference.sequence)
                .ok()
                .and_then(|sequence| sequence.checked_sub(1))
                .and_then(|index| self.transcript.get(index))
                .filter(|entry| entry.reference == *reference)
                .cloned()
                .map(|entry| PendingMessage { entry })
        });
        let transition = transition_claim_writer(
            &self.reference(),
            link,
            ClaimFacts {
                head: self.head(),
                active_writer: self.active_writer.clone(),
                earliest_pending,
                run_already_used: self.used_runs.contains(&link.run_id),
                requested_executor_generation: executor_generation,
                executor_generation,
            },
        )?;
        self.pending.pop_front();
        self.used_runs.insert(link.run_id);
        self.active_writer = Some(transition.claim.clone());
        self.state_revision = transition.head.state_revision;
        Ok(transition.claim)
    }

    /// Release exactly the matching writer. A later Run may retain the same
    /// host Task ID without changing any Task state or legacy receipt.
    pub fn complete_writer(
        &mut self,
        claim: WriterClaim,
        executor_generation: u64,
    ) -> Result<RunTaskLink, ConversationFailure> {
        let link = claim.link;
        let head = transition_complete_writer(
            claim.clone(),
            CompletionFacts {
                head: self.head(),
                active_writer: self.active_writer.clone(),
                executor_generation,
            },
        )?;
        self.active_writer = None;
        self.completed_prefix = head.completed_prefix;
        self.completed_runs.push(link);
        self.state_revision = head.state_revision;
        Ok(link)
    }

    /// Apply a checkpoint only when it names this conversation's exact prefix.
    pub fn apply_checkpoint(
        &mut self,
        checkpoint: ConversationCheckpoint,
    ) -> Result<(), ConversationFailure> {
        let stored_prefix_digest = self
            .entry(checkpoint.through)
            .map(|entry| entry.prefix_digest);
        let head = transition_checkpoint(
            &self.reference(),
            &checkpoint,
            CheckpointFacts {
                head: self.head(),
                current_checkpoint_sequence: self.checkpoint.as_ref().map(|v| v.through.sequence),
                stored_prefix_digest,
            },
        )?;
        self.state_revision = head.state_revision;
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
        if through.sequence > self.completed_prefix {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let entry = self
            .entry(through)
            .ok_or(ConversationFailure::CheckpointMismatch)?;
        checkpoint_from_prefix(through, entry.prefix_digest, summary)
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
        self.active_writer.clone()
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

    fn head(&self) -> ConversationHead {
        ConversationHead {
            identity: self.identity.clone(),
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            head_revision: self.head_revision,
            completed_prefix: self.completed_prefix,
            state_revision: self.state_revision,
        }
    }

    fn entry(&self, reference: TranscriptReference) -> Option<&TranscriptEntry> {
        let index = usize::try_from(reference.sequence).ok()?.checked_sub(1)?;
        self.transcript
            .get(index)
            .filter(|entry| entry.reference == reference)
    }

    fn admission_facts(&self, request: &MessageAdmissionRequest) -> AdmissionFacts {
        AdmissionFacts {
            head: Some(self.head()),
            active_writer: self.active_writer.clone(),
            message: self.messages.get(&request.message.message_id).cloned(),
            command: self.commands.get(&request.message.command_id).cloned(),
            previous_prefix_digest: self
                .transcript
                .last()
                .map_or(EMPTY_PREFIX_DIGEST, |entry| entry.prefix_digest),
        }
    }

    fn apply_admission_transition(
        &mut self,
        transition: AdmissionTransition,
        message: ConversationMessage,
    ) {
        if let Some(entry) = transition.appended {
            self.pending.push_back(entry.reference);
            self.transcript.push(entry);
            self.messages.insert(
                message.message_id,
                StoredAdmission {
                    message: message.clone(),
                    receipt: transition.result.receipt.clone(),
                },
            );
        }
        if transition.command_receipt.is_some() {
            let admission = self
                .messages
                .get(&message.message_id)
                .cloned()
                .expect("command receipts refer to an admitted message");
            self.commands.insert(
                message.command_id,
                StoredCommand {
                    identity: self.identity.clone(),
                    conversation_id: self.conversation_id,
                    branch_id: self.branch_id,
                    admission,
                },
            );
        }
        self.head_revision = transition.head.head_revision;
        self.completed_prefix = transition.head.completed_prefix;
        self.state_revision = transition.head.state_revision;
    }
}
