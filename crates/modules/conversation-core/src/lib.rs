//! Neutral transcript integrity and owner-driven recorder custody.
//!
//! Core never schedules work or owns command occupancy. Host owners admit and
//! schedule Runs, then compose these pure transitions with their durable owner
//! writes inside one Vault transaction.

use std::collections::HashMap;

use floe_conversation_contract::{
    AdmissionResult, ConversationBranchId, ConversationCheckpoint, ConversationFailure,
    ConversationId, ConversationReference, LogicalContributionId, MessageAdmissionRequest,
    MessageId, TaskEvidenceReference, TranscriptReference,
};
use floe_kernel::RunId;

mod transitions;
pub use transitions::{
    AppendFacts, AppendTransition, CheckpointFacts, ConversationHead, EMPTY_PREFIX_DIGEST,
    ExecutorDomain, OwnerGenerationFence, OwnerRunEvidence, OwnerRunState, OwnerSettlementEvidence,
    RecorderCloseFacts, RecorderCloseReceipt, RecorderCloseTransition, RecorderFence,
    RecorderOpenFacts, RecorderOpenReceipt, RecorderOpenTransition, RecorderRetirementFacts,
    RecorderRetirementReceipt, RecorderRetirementTransition, RecorderStartRequest, RecordingFacts,
    RecordingReceipt, RecordingRequest, RecordingTransition, TranscriptEntry, TranscriptEntryKind,
    advance_core_prefix_digest, advance_prefix_digest, append_input as append_input_transition,
    apply_checkpoint as apply_checkpoint_transition, close_recording as close_recording_transition,
    open_recording as open_recording_transition, record_entry as record_entry_transition,
    recording_content_digest, replay_close_recording, replay_open_recording,
    replay_recording_entry, replay_retirement,
    retire_stale_recording as retire_stale_recording_transition,
    validate_target as validate_reference_target,
};

/// Storage failures preserve confirmed rollback versus unknown commit outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationStoreFailure {
    Transition(ConversationFailure),
    Busy,
    Unavailable,
    NotCommitted,
    OutcomeUnknown,
    PageItemExceedsBudget,
    UnsupportedOwnerDomain,
    UnsupportedOwnerIntent,
    UnsupportedStoredMeaning,
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
    /// Sum of serialized UTF-8 ConversationMessage payload bytes.
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptPage {
    pub entries: Vec<TranscriptEntry>,
    pub next_cursor: Option<TranscriptReference>,
    pub has_more: bool,
    pub encoded_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecorderRecoveryState {
    Absent,
    ActiveCurrentGeneration,
    Interrupted,
    Closed,
    Retired,
}

/// A bounded read-only observation. It never closes or retires a recorder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderRecoveryObservation {
    pub state: RecorderRecoveryState,
    pub head: ConversationHead,
    pub recorder: Option<RecorderFence>,
    pub active_recorder: Option<RecorderFence>,
    pub current_generation: u64,
}

/// Deterministic pure-state harness that delegates every mutation to the same
/// transition functions used by Vault. It is not a second persistence API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationCore {
    head: ConversationHead,
    transcript: Vec<TranscriptEntry>,
    append_receipts: HashMap<MessageId, floe_conversation_contract::AdmissionReceipt>,
    open_receipts: HashMap<RunId, RecorderOpenReceipt>,
    active_recorder: Option<RecorderFence>,
    recordings: HashMap<LogicalContributionId, RecordingReceipt>,
    close_receipts: HashMap<RunId, RecorderCloseReceipt>,
    retirement_receipts: HashMap<RunId, RecorderRetirementReceipt>,
    checkpoint: Option<ConversationCheckpoint>,
}

impl ConversationCore {
    pub fn new(
        identity: floe_conversation_contract::AgentIdentity,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
    ) -> Result<Self, ConversationFailure> {
        identity.validate()?;
        if !conversation_id.is_valid() || !branch_id.is_valid() {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(Self {
            head: ConversationHead {
                identity,
                conversation_id,
                branch_id,
                head_revision: 0,
                settled_prefix: 0,
                state_revision: 0,
                recorder_epoch: 0,
            },
            transcript: Vec::new(),
            append_receipts: HashMap::new(),
            open_receipts: HashMap::new(),
            active_recorder: None,
            recordings: HashMap::new(),
            close_receipts: HashMap::new(),
            retirement_receipts: HashMap::new(),
            checkpoint: None,
        })
    }

    /// Append an inbound transcript entry. This records custody only; it never
    /// enqueues or selects work and never claims a CommandId receipt.
    pub fn append_input(
        &mut self,
        request: MessageAdmissionRequest,
    ) -> Result<AdmissionResult, ConversationFailure> {
        let message_id = request.message.message_id;
        let stored_receipt = self.append_receipts.get(&message_id).cloned();
        let message_id_in_transcript = self
            .transcript
            .iter()
            .any(|entry| entry.reference.message_id == message_id);
        let stored_message = self
            .transcript
            .iter()
            .find(|entry| entry.reference.message_id == message_id)
            .map(|entry| entry.message.clone());
        let previous_prefix_digest = self
            .transcript
            .last()
            .map_or(EMPTY_PREFIX_DIGEST, |entry| entry.prefix_digest);
        let transition = append_input_transition(
            request,
            AppendFacts {
                head: self.head.clone(),
                stored_receipt,
                stored_message,
                message_id_in_transcript,
                previous_prefix_digest,
            },
        )?;
        if let Some(entry) = transition.appended {
            self.append_receipts.insert(
                entry.reference.message_id,
                transition.result.receipt.clone(),
            );
            self.transcript.push(entry);
        }
        self.head = transition.head;
        Ok(transition.result)
    }

    pub fn open_recording(
        &mut self,
        request: RecorderStartRequest,
        owner: Option<OwnerRunEvidence>,
    ) -> Result<RecorderOpenReceipt, ConversationFailure> {
        let run_id = request.run_id;
        let input_entry = self
            .transcript
            .iter()
            .find(|entry| entry.reference == request.input)
            .cloned();
        let transition = open_recording_transition(
            request,
            RecorderOpenFacts {
                owner,
                head: self.head.clone(),
                stored_receipt: self.open_receipts.get(&run_id).cloned(),
                active_recorder: self.active_recorder.clone(),
                input_entry,
            },
        )?;
        if let Some(head) = transition.head {
            self.head = head;
            self.active_recorder = Some(transition.receipt.fence.clone());
            self.open_receipts
                .insert(transition.receipt.fence.run_id, transition.receipt.clone());
        }
        Ok(transition.receipt)
    }

    pub fn record_entry(
        &mut self,
        request: RecordingRequest,
        verified_task_reference: Option<TaskEvidenceReference>,
        owner: Option<OwnerRunEvidence>,
    ) -> Result<RecordingReceipt, ConversationFailure> {
        let contribution_id = request.contribution_id;
        let message_id = request.message.message_id;
        let stored_receipt = self.recordings.get(&contribution_id).cloned();
        let message_id_in_transcript = self
            .transcript
            .iter()
            .any(|entry| entry.reference.message_id == message_id);
        let previous_prefix_digest = self
            .transcript
            .last()
            .map_or(EMPTY_PREFIX_DIGEST, |entry| entry.prefix_digest);
        let transition = record_entry_transition(
            request,
            RecordingFacts {
                head: self.head.clone(),
                active_recorder: self.active_recorder.clone(),
                owner,
                verified_task_reference,
                stored_receipt,
                message_id_in_transcript,
                previous_prefix_digest,
            },
        )?;
        if let Some(entry) = transition.appended {
            self.recordings
                .insert(contribution_id, transition.receipt.clone());
            self.transcript.push(entry);
        }
        if let Some(head) = transition.head {
            self.head = head;
        }
        Ok(transition.receipt)
    }

    pub fn close_recording(
        &mut self,
        fence: RecorderFence,
        owner: Option<OwnerSettlementEvidence>,
        settlement_prefix_verified: bool,
    ) -> Result<RecorderCloseReceipt, ConversationFailure> {
        let stored_receipt = self.close_receipts.get(&fence.run_id).cloned();
        let transition = close_recording_transition(
            fence,
            RecorderCloseFacts {
                head: self.head.clone(),
                active_recorder: self.active_recorder.clone(),
                owner,
                stored_receipt,
                settlement_prefix_verified,
            },
        )?;
        if let Some(head) = transition.head {
            self.head = head;
            self.active_recorder = None;
            self.close_receipts
                .insert(transition.receipt.fence.run_id, transition.receipt.clone());
        }
        Ok(transition.receipt)
    }

    pub fn retire_stale_recording(
        &mut self,
        fence: RecorderFence,
        generation_fence: OwnerGenerationFence,
        owner: Option<OwnerRunEvidence>,
    ) -> Result<RecorderRetirementReceipt, ConversationFailure> {
        let stored_receipt = self.retirement_receipts.get(&fence.run_id).cloned();
        let transition = retire_stale_recording_transition(
            fence,
            generation_fence.clone(),
            RecorderRetirementFacts {
                head: self.head.clone(),
                active_recorder: self.active_recorder.clone(),
                owner,
                stored_receipt,
            },
        )?;
        if let Some(head) = transition.head {
            self.head = head;
            self.active_recorder = None;
            self.retirement_receipts
                .insert(transition.receipt.fence.run_id, transition.receipt.clone());
        }
        Ok(transition.receipt)
    }

    pub fn apply_checkpoint(
        &mut self,
        checkpoint: ConversationCheckpoint,
    ) -> Result<(), ConversationFailure> {
        let stored_prefix_digest = self
            .entry(checkpoint.through)
            .map(|entry| entry.prefix_digest);
        self.head = apply_checkpoint_transition(
            &self.head.reference(),
            &checkpoint,
            CheckpointFacts {
                head: self.head.clone(),
                current_checkpoint_sequence: self
                    .checkpoint
                    .as_ref()
                    .map(|value| value.through.sequence),
                stored_prefix_digest,
            },
        )?;
        self.checkpoint = Some(checkpoint);
        Ok(())
    }

    pub fn checkpoint_for(
        &self,
        through: TranscriptReference,
        summary: impl Into<String>,
    ) -> Result<ConversationCheckpoint, ConversationFailure> {
        if through.conversation_id != self.head.conversation_id
            || through.branch_id != self.head.branch_id
            || through.sequence > self.head.settled_prefix
        {
            return Err(ConversationFailure::CheckpointMismatch);
        }
        let entry = self
            .entry(through)
            .ok_or(ConversationFailure::CheckpointMismatch)?;
        Ok(ConversationCheckpoint {
            through,
            prefix_digest: entry.prefix_digest,
            summary: summary.into(),
        })
    }

    pub fn observe_recorder(
        &self,
        run_id: RunId,
        current_generation: u64,
    ) -> RecorderRecoveryObservation {
        let recorder = self
            .open_receipts
            .get(&run_id)
            .map(|receipt| receipt.fence.clone());
        let state = if self.retirement_receipts.contains_key(&run_id) {
            RecorderRecoveryState::Retired
        } else if self.close_receipts.contains_key(&run_id) {
            RecorderRecoveryState::Closed
        } else if let Some(recorder) = &recorder {
            if self.active_recorder.as_ref() != Some(recorder) {
                RecorderRecoveryState::Interrupted
            } else if recorder.executor_generation == current_generation {
                RecorderRecoveryState::ActiveCurrentGeneration
            } else {
                RecorderRecoveryState::Interrupted
            }
        } else {
            RecorderRecoveryState::Absent
        };
        RecorderRecoveryObservation {
            state,
            head: self.head.clone(),
            recorder,
            active_recorder: self.active_recorder.clone(),
            current_generation,
        }
    }

    pub fn read_page(
        &self,
        target: &ConversationReference,
        after: Option<TranscriptReference>,
        budget: TranscriptPageBudget,
    ) -> Result<TranscriptPage, ConversationFailure> {
        validate_reference_target(&self.head, target)?;
        if budget.max_entries == 0 || budget.max_bytes == 0 {
            return Err(ConversationFailure::InvalidInput);
        }
        let max_entries = budget.max_entries.min(MAX_TRANSCRIPT_PAGE_ENTRIES);
        let mut cursor_sequence = 0;
        if let Some(cursor) = after {
            cursor.validate()?;
            if cursor.conversation_id != self.head.conversation_id
                || cursor.branch_id != self.head.branch_id
                || self.entry(cursor).is_none()
            {
                return Err(ConversationFailure::ConversationMismatch);
            }
            cursor_sequence = cursor.sequence;
        }
        let mut entries = Vec::with_capacity(max_entries.min(32));
        let mut encoded_bytes = 0usize;
        for entry in self
            .transcript
            .iter()
            .filter(|entry| entry.reference.sequence > cursor_sequence)
        {
            if entries.len() >= max_entries {
                break;
            }
            let entry_bytes = serde_json::to_vec(&entry.message)
                .map_err(|_| ConversationFailure::InvalidInput)?
                .len();
            if entry_bytes > budget.max_bytes.saturating_sub(encoded_bytes) {
                if entries.is_empty() {
                    return Err(ConversationFailure::StorageUnavailable);
                }
                break;
            }
            encoded_bytes = encoded_bytes
                .checked_add(entry_bytes)
                .ok_or(ConversationFailure::InvalidInput)?;
            entries.push(entry.clone());
        }
        let next_cursor = entries.last().map(|entry| entry.reference);
        let through = next_cursor.map_or(cursor_sequence, |cursor| cursor.sequence);
        Ok(TranscriptPage {
            entries,
            next_cursor,
            has_more: self.head.head_revision > through,
            encoded_bytes,
        })
    }

    pub fn reference(&self) -> ConversationReference {
        self.head.reference()
    }

    pub fn head(&self) -> &ConversationHead {
        &self.head
    }

    pub fn active_recorder(&self) -> Option<&RecorderFence> {
        self.active_recorder.as_ref()
    }

    pub fn transcript_entries(&self) -> impl Iterator<Item = &TranscriptEntry> {
        self.transcript.iter()
    }

    pub fn checkpoint(&self) -> Option<&ConversationCheckpoint> {
        self.checkpoint.as_ref()
    }

    fn entry(&self, reference: TranscriptReference) -> Option<&TranscriptEntry> {
        let index = usize::try_from(reference.sequence).ok()?.checked_sub(1)?;
        self.transcript
            .get(index)
            .filter(|entry| entry.reference == reference)
    }
}
