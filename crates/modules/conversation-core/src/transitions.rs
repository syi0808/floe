//! Pure neutral transitions for ordered Conversation transcript custody.
//!
//! The Host Run owner admits and schedules work. These transitions only append
//! exact transcript entries, bind one immutable recorder per Run, record its
//! contributions, and close or retire that recorder from validated owner facts.

use std::collections::HashSet;

use floe_conversation_contract::{
    AdmissionDisposition, AdmissionReceipt, AdmissionResult, AdmissionTarget, AgentIdentity,
    ConversationBranchId, ConversationCheckpoint, ConversationFailure, ConversationId,
    ConversationMessage, ConversationReference, LogicalContributionId, MessageAdmissionRequest,
    MessageOrigin, TaskEvidenceReference, TranscriptReference,
};
use floe_kernel::{PersonId, RunId, TaskId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Initial value fed to the v3 chained transcript commitment.
pub const EMPTY_PREFIX_DIGEST: [u8; 32] = [0; 32];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationHead {
    pub identity: AgentIdentity,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub head_revision: u64,
    /// Highest contiguous prefix proven settled by the Host owner.
    pub settled_prefix: u64,
    /// Core-local state CAS revision, independent of transcript and owner revisions.
    pub state_revision: u64,
    /// Monotonic recorder epoch, independent of the state revision.
    pub recorder_epoch: u64,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorDomain {
    /// The existing Conversation Run owner and its durable executor fence.
    HostRun,
    /// A Task owner generation. Vault must reject this until its adapter is wired.
    TaskExecution,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerRunState {
    Working,
    Terminal,
    PendingTerminal,
}

/// A neutral fact assembled by the owner adapter from a durable owner record.
/// The digest identifies that exact record; it is not accepted directly by a
/// public Vault operation as a substitute for reading the owner store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerRunEvidence {
    pub domain: ExecutorDomain,
    pub run_id: RunId,
    pub person_id: PersonId,
    pub input: TranscriptReference,
    pub executor_generation: u64,
    pub aggregate_revision: u64,
    pub journal_revision: u64,
    pub state: OwnerRunState,
    pub record_digest: [u8; 32],
    /// Owner record references whose effects are still unresolved.
    pub unresolved_effects: Vec<Uuid>,
    /// Unresolved model attempts from the same owner accounting projection.
    /// This lets narrowly scoped recovery distinguish a late Task result from
    /// the separate model uncertainty that keeps its recorder open.
    pub unresolved_model_attempts: Vec<Uuid>,
}

impl OwnerRunEvidence {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        if !self.run_id.is_valid()
            || !self.person_id.is_valid()
            || !self.input.message_id.is_valid()
            || self.executor_generation == 0
            || self.aggregate_revision == 0
            || self.record_digest == [0; 32]
            || self.unresolved_effects.len() > 512
            || self.unresolved_effects.iter().any(Uuid::is_nil)
            || self.unresolved_model_attempts.len() > self.unresolved_effects.len()
            || self.unresolved_model_attempts.iter().any(Uuid::is_nil)
        {
            return Err(ConversationFailure::OwnerEvidenceMismatch);
        }
        self.input.validate()?;
        let mut seen_effects = HashSet::new();
        if self
            .unresolved_effects
            .iter()
            .any(|id| !seen_effects.insert(*id))
        {
            return Err(ConversationFailure::OwnerEvidenceMismatch);
        }
        let mut seen_attempts = HashSet::new();
        if self
            .unresolved_model_attempts
            .iter()
            .any(|id| !seen_attempts.insert(*id) || !seen_effects.contains(id))
        {
            return Err(ConversationFailure::OwnerEvidenceMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TranscriptEntry {
    pub reference: TranscriptReference,
    pub message: ConversationMessage,
    pub kind: TranscriptEntryKind,
    pub producer_run: Option<RunId>,
    pub contribution_id: Option<LogicalContributionId>,
    pub producing_task: Option<TaskEvidenceReference>,
    pub prefix_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptEntryKind {
    Inbound,
    GeneratedOutput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppendFacts {
    pub head: ConversationHead,
    pub stored_receipt: Option<AdmissionReceipt>,
    pub stored_message: Option<ConversationMessage>,
    pub message_id_in_transcript: bool,
    pub previous_prefix_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppendTransition {
    pub result: AdmissionResult,
    pub head: ConversationHead,
    pub appended: Option<TranscriptEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecorderStartRequest {
    pub identity: AgentIdentity,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub run_id: RunId,
    pub input: TranscriptReference,
    pub executor_domain: ExecutorDomain,
    pub executor_generation: u64,
    pub execution_task: Option<TaskId>,
}

impl RecorderStartRequest {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        self.identity.validate()?;
        self.input.validate()?;
        if !self.conversation_id.is_valid()
            || !self.branch_id.is_valid()
            || !self.run_id.is_valid()
            || self.executor_generation == 0
            || self.input.conversation_id != self.conversation_id
            || self.input.branch_id != self.branch_id
            || self.execution_task.is_some_and(|id| !id.is_valid())
            || (self.executor_domain == ExecutorDomain::TaskExecution
                && self.execution_task.is_none())
        {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecorderFence {
    pub identity: AgentIdentity,
    pub conversation_id: ConversationId,
    pub branch_id: ConversationBranchId,
    pub run_id: RunId,
    pub input: TranscriptReference,
    pub recorder_epoch: u64,
    pub executor_domain: ExecutorDomain,
    pub executor_generation: u64,
    pub execution_task: Option<TaskId>,
}

impl RecorderFence {
    pub fn validate(&self) -> Result<(), ConversationFailure> {
        RecorderStartRequest {
            identity: self.identity.clone(),
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            run_id: self.run_id,
            input: self.input,
            executor_domain: self.executor_domain,
            executor_generation: self.executor_generation,
            execution_task: self.execution_task,
        }
        .validate()?;
        if self.recorder_epoch == 0 {
            return Err(ConversationFailure::InvalidInput);
        }
        Ok(())
    }

    fn matches_start(&self, request: &RecorderStartRequest) -> bool {
        self.identity == request.identity
            && self.conversation_id == request.conversation_id
            && self.branch_id == request.branch_id
            && self.run_id == request.run_id
            && self.input == request.input
            && self.executor_domain == request.executor_domain
            && self.executor_generation == request.executor_generation
            && self.execution_task == request.execution_task
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecorderOpenReceipt {
    pub fence: RecorderFence,
    pub opened_head_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderOpenFacts {
    /// Optional on exact receipt replay so a stale owner generation is never
    /// consulted before the immutable stored receipt.
    pub owner: Option<OwnerRunEvidence>,
    pub head: ConversationHead,
    pub stored_receipt: Option<RecorderOpenReceipt>,
    pub active_recorder: Option<RecorderFence>,
    pub input_entry: Option<TranscriptEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderOpenTransition {
    pub receipt: RecorderOpenReceipt,
    pub head: Option<ConversationHead>,
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingRequest {
    pub recorder: RecorderFence,
    pub message: ConversationMessage,
    pub contribution_id: LogicalContributionId,
    pub producing_task: Option<TaskEvidenceReference>,
}

/// Neutral proof that an owner-authenticated Task result was first committed
/// after the originating Run entered terminal recovery. The owner adapter
/// derives `result_digest` from the exact Run journal receipt and supplies an
/// admitted Task reference only when the actual Task owner receipt exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredTaskContribution {
    pub task_id: TaskId,
    pub result_digest: [u8; 32],
    pub producing_task: Option<TaskEvidenceReference>,
    pub current_executor_generation: u64,
    pub generation_fence: Option<OwnerGenerationFence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingReceipt {
    pub recorder: RecorderFence,
    pub contribution_id: LogicalContributionId,
    pub transcript: TranscriptReference,
    pub producing_task: Option<TaskEvidenceReference>,
    pub content_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingFacts {
    pub head: ConversationHead,
    pub active_recorder: Option<RecorderFence>,
    pub owner: Option<OwnerRunEvidence>,
    /// Resolved from the actual Task owner store by Vault. Exact stored
    /// recording replay does not need to re-resolve mutable owner state.
    pub verified_task_reference: Option<TaskEvidenceReference>,
    pub stored_receipt: Option<RecordingReceipt>,
    pub message_id_in_transcript: bool,
    pub previous_prefix_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingTransition {
    pub receipt: RecordingReceipt,
    pub appended: Option<TranscriptEntry>,
    pub head: Option<ConversationHead>,
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSettlementEvidence {
    pub owner: OwnerRunEvidence,
    pub terminal_receipt_digest: [u8; 32],
    /// Exact contiguous prefix proven settled by the owner adapter.
    pub settled_through: Option<TranscriptReference>,
    /// Owner-validated entries that remain protected from settlement/retention.
    pub protection_roots: Vec<TranscriptReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecorderCloseReceipt {
    pub fence: RecorderFence,
    pub owner_aggregate_revision: u64,
    pub terminal_receipt_digest: [u8; 32],
    /// Owner-validated roots retained with the immutable close evidence.
    pub protection_roots: Vec<TranscriptReference>,
    pub settled_prefix: u64,
    pub close_state_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderCloseFacts {
    pub head: ConversationHead,
    pub active_recorder: Option<RecorderFence>,
    pub owner: Option<OwnerSettlementEvidence>,
    pub stored_receipt: Option<RecorderCloseReceipt>,
    /// True only when Vault verified `settled_through` against the exact Core
    /// entry and every intervening owner receipt in the same transaction.
    pub settlement_prefix_verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderCloseTransition {
    pub receipt: RecorderCloseReceipt,
    pub head: Option<ConversationHead>,
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerGenerationFence {
    pub domain: ExecutorDomain,
    pub old_generation: u64,
    pub current_generation: u64,
    pub fence_revision: u64,
    pub evidence_digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecorderRetirementReceipt {
    pub fence: RecorderFence,
    pub owner_evidence_digest: [u8; 32],
    pub generation_fence: OwnerGenerationFence,
    pub generation_fence_digest: [u8; 32],
    pub retirement_state_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderRetirementFacts {
    pub head: ConversationHead,
    pub active_recorder: Option<RecorderFence>,
    pub owner: Option<OwnerRunEvidence>,
    pub stored_receipt: Option<RecorderRetirementReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecorderRetirementTransition {
    pub receipt: RecorderRetirementReceipt,
    pub head: Option<ConversationHead>,
    pub replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointFacts {
    pub head: ConversationHead,
    pub current_checkpoint_sequence: Option<u64>,
    pub stored_prefix_digest: Option<[u8; 32]>,
}

pub fn append_input(
    request: MessageAdmissionRequest,
    facts: AppendFacts,
) -> Result<AppendTransition, ConversationFailure> {
    request.validate()?;
    let (target_identity, target_conversation, target_branch, expected_revision, is_new) =
        match &request.target {
            AdmissionTarget::New {
                identity,
                conversation_id,
                branch_id,
            } => (identity, *conversation_id, *branch_id, None, true),
            AdmissionTarget::AppendToExisting { reference } => (
                &reference.identity,
                reference.conversation_id,
                reference.branch_id,
                Some(reference.head_revision),
                false,
            ),
        };
    let mut head = facts.head;
    if head.identity != *target_identity
        || head.conversation_id != target_conversation
        || head.branch_id != target_branch
    {
        return Err(ConversationFailure::AgentMismatch);
    }

    // Exact MessageId replay is transcript custody. Command occupancy and
    // owner command replay remain outside Core.
    if let Some(receipt) = facts.stored_receipt {
        let Some(stored_message) = facts.stored_message else {
            return Err(ConversationFailure::MessageIdConflict);
        };
        if receipt.transcript.message_id != request.message.message_id
            || receipt.transcript.conversation_id != target_conversation
            || receipt.transcript.branch_id != target_branch
            || receipt.task_id != request.message.task_id
            || !stored_message.same_delivery(&request.message)
        {
            return Err(ConversationFailure::MessageIdConflict);
        }
        return Ok(AppendTransition {
            result: AdmissionResult {
                disposition: AdmissionDisposition::Replayed,
                receipt,
            },
            head,
            appended: None,
        });
    }
    if facts.message_id_in_transcript {
        return Err(ConversationFailure::MessageIdConflict);
    }
    if is_new && head.head_revision != 0 {
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
    let prefix_digest = advance_core_prefix_digest(
        facts.previous_prefix_digest,
        reference,
        &request.message,
        TranscriptEntryKind::Inbound,
        None,
        None,
        None,
    )?;
    let receipt = AdmissionReceipt {
        transcript: reference,
        head_revision: sequence,
        task_id: request.message.task_id,
    };
    let entry = TranscriptEntry {
        reference,
        message: request.message,
        kind: TranscriptEntryKind::Inbound,
        producer_run: None,
        contribution_id: None,
        producing_task: None,
        prefix_digest,
    };
    head.head_revision = sequence;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    Ok(AppendTransition {
        result: AdmissionResult {
            disposition: AdmissionDisposition::Appended,
            receipt,
        },
        head,
        appended: Some(entry),
    })
}

pub fn open_recording(
    request: RecorderStartRequest,
    facts: RecorderOpenFacts,
) -> Result<RecorderOpenTransition, ConversationFailure> {
    request.validate()?;
    if let Some(receipt) = facts.stored_receipt {
        return replay_open_recording(request, receipt);
    }
    let owner = facts
        .owner
        .as_ref()
        .ok_or(ConversationFailure::OwnerEvidenceMismatch)?;
    validate_owner_for_start(owner, &request)?;
    validate_head_scope(
        &facts.head,
        &request.identity,
        request.conversation_id,
        request.branch_id,
    )?;
    if request.identity.person_id != owner.person_id {
        return Err(ConversationFailure::AgentMismatch);
    }
    if request.input.sequence > facts.head.head_revision
        || facts.input_entry.as_ref().is_none_or(|entry| {
            entry.reference != request.input || entry.kind != TranscriptEntryKind::Inbound
        })
    {
        return Err(ConversationFailure::ConversationMismatch);
    }
    if facts.active_recorder.is_some() {
        return Err(ConversationFailure::WriterAlreadyActive);
    }
    let epoch = facts
        .head
        .recorder_epoch
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let mut head = facts.head;
    head.recorder_epoch = epoch;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let fence = RecorderFence {
        identity: request.identity,
        conversation_id: request.conversation_id,
        branch_id: request.branch_id,
        run_id: request.run_id,
        input: request.input,
        recorder_epoch: epoch,
        executor_domain: request.executor_domain,
        executor_generation: request.executor_generation,
        execution_task: request.execution_task,
    };
    Ok(RecorderOpenTransition {
        receipt: RecorderOpenReceipt {
            fence,
            opened_head_revision: head.head_revision,
        },
        head: Some(head),
        replayed: false,
    })
}

pub fn record_entry(
    request: RecordingRequest,
    facts: RecordingFacts,
) -> Result<RecordingTransition, ConversationFailure> {
    record_entry_with_owner_state(request, facts, Some(OwnerRunState::Working))
}

/// Append an exact Task result discovered by owner recovery after a terminal
/// request while the original recorder is still open. The caller must supply
/// the owner-verified Task receipt reference; this path never grants general
/// post-terminal output authority.
pub fn record_task_recovery_entry(
    request: RecordingRequest,
    recovery: RecoveredTaskContribution,
    facts: RecordingFacts,
) -> Result<RecordingTransition, ConversationFailure> {
    request.recorder.validate()?;
    if let Some(receipt) = facts.stored_receipt {
        return replay_recording_entry(request, receipt);
    }
    let owner = facts
        .owner
        .as_ref()
        .ok_or(ConversationFailure::OwnerEvidenceMismatch)?;
    if request.recorder.executor_domain != ExecutorDomain::HostRun
        || !recovery.task_id.is_valid()
        || recovery.result_digest == [0; 32]
        || !matches!(
            owner.state,
            OwnerRunState::Terminal | OwnerRunState::PendingTerminal
        )
        || request.message.origin != MessageOrigin::Host
        || request.message.evidence.is_none()
        || owner
            .unresolved_effects
            .contains(&recovery.task_id.as_uuid())
        || recovery.producing_task != request.producing_task
    {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    match (
        &recovery.generation_fence,
        recovery.current_executor_generation,
    ) {
        (None, current) if current == request.recorder.executor_generation => {}
        (Some(fence), current)
            if current > request.recorder.executor_generation
                && fence.domain == request.recorder.executor_domain
                && fence.old_generation == request.recorder.executor_generation
                && fence.current_generation == current
                && fence.fence_revision == current
                && fence.evidence_digest != [0; 32] => {}
        _ => return Err(ConversationFailure::OwnerEvidenceMismatch),
    }
    match (
        request.message.task_id,
        request.producing_task.as_ref(),
        facts.verified_task_reference.as_ref(),
    ) {
        (Some(message_task), Some(producing_task), Some(verified_task))
            if message_task == recovery.task_id
                && producing_task.task_id() == recovery.task_id
                && verified_task == producing_task => {}
        (None, None, None) if recovery.producing_task.is_none() => {}
        _ => return Err(ConversationFailure::OwnerEvidenceMismatch),
    }
    record_entry_with_owner_state(request, facts, None)
}

fn record_entry_with_owner_state(
    request: RecordingRequest,
    facts: RecordingFacts,
    required_state: Option<OwnerRunState>,
) -> Result<RecordingTransition, ConversationFailure> {
    request.recorder.validate()?;
    if let Some(receipt) = facts.stored_receipt {
        return replay_recording_entry(request, receipt);
    }
    let content_digest = recording_content_digest(
        &request.message,
        request.contribution_id,
        request.producing_task.as_ref(),
    )?;
    request.message.validate()?;
    validate_generated_origin(&request.recorder.identity, &request.message)?;
    validate_task_binding(&request.message, request.producing_task.as_ref())?;
    if facts.verified_task_reference != request.producing_task {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    let owner = facts
        .owner
        .as_ref()
        .ok_or(ConversationFailure::OwnerEvidenceMismatch)?;
    owner.validate()?;
    if owner.run_id != request.recorder.run_id
        || owner.person_id != request.recorder.identity.person_id
        || owner.input != request.recorder.input
        || owner.executor_generation != request.recorder.executor_generation
        || owner.domain != request.recorder.executor_domain
        || required_state.is_some_and(|required| owner.state != required)
        || (required_state.is_none()
            && !matches!(
                owner.state,
                OwnerRunState::Terminal | OwnerRunState::PendingTerminal
            ))
    {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    validate_head_scope(
        &facts.head,
        &request.recorder.identity,
        request.recorder.conversation_id,
        request.recorder.branch_id,
    )?;
    if facts.active_recorder.as_ref() != Some(&request.recorder) {
        return Err(ConversationFailure::WrongWriter);
    }
    if facts.message_id_in_transcript {
        return Err(ConversationFailure::MessageIdConflict);
    }
    let sequence = facts
        .head
        .head_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let reference = TranscriptReference {
        conversation_id: facts.head.conversation_id,
        branch_id: facts.head.branch_id,
        message_id: request.message.message_id,
        sequence,
    };
    let prefix_digest = advance_core_prefix_digest(
        facts.previous_prefix_digest,
        reference,
        &request.message,
        TranscriptEntryKind::GeneratedOutput,
        Some(request.recorder.run_id),
        Some(request.contribution_id),
        request.producing_task.as_ref(),
    )?;
    let entry = TranscriptEntry {
        reference,
        message: request.message,
        kind: TranscriptEntryKind::GeneratedOutput,
        producer_run: Some(request.recorder.run_id),
        contribution_id: Some(request.contribution_id),
        producing_task: request.producing_task.clone(),
        prefix_digest,
    };
    let mut head = facts.head;
    head.head_revision = sequence;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let receipt = RecordingReceipt {
        recorder: request.recorder,
        contribution_id: request.contribution_id,
        transcript: reference,
        producing_task: request.producing_task,
        content_digest,
    };
    Ok(RecordingTransition {
        receipt,
        appended: Some(entry),
        head: Some(head),
        replayed: false,
    })
}

pub fn close_recording(
    fence: RecorderFence,
    facts: RecorderCloseFacts,
) -> Result<RecorderCloseTransition, ConversationFailure> {
    fence.validate()?;
    if let Some(receipt) = facts.stored_receipt {
        return replay_close_recording(fence, receipt);
    }
    if facts.active_recorder.as_ref() != Some(&fence) {
        return Err(ConversationFailure::WrongWriter);
    }
    let owner = facts
        .owner
        .as_ref()
        .ok_or(ConversationFailure::OwnerEvidenceMismatch)?;
    validate_owner_for_recorder(&owner.owner, &fence, OwnerRunState::Terminal)?;
    if owner.terminal_receipt_digest == [0; 32] {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    if !owner.owner.unresolved_effects.is_empty() {
        return Err(ConversationFailure::UnresolvedEffect);
    }
    validate_head_scope(
        &facts.head,
        &fence.identity,
        fence.conversation_id,
        fence.branch_id,
    )?;
    let mut candidate = facts.head.settled_prefix;
    if let Some(settled_through) = owner.settled_through {
        settled_through.validate()?;
        if settled_through.conversation_id != fence.conversation_id
            || settled_through.branch_id != fence.branch_id
            || settled_through.sequence > facts.head.head_revision
            || !facts.settlement_prefix_verified
        {
            return Err(ConversationFailure::OwnerEvidenceMismatch);
        }
        candidate = settled_through.sequence.max(candidate);
    }
    let mut roots = Vec::new();
    let mut first_protected = None;
    for root in &owner.protection_roots {
        root.validate()?;
        if root.conversation_id != fence.conversation_id
            || root.branch_id != fence.branch_id
            || root.sequence > facts.head.head_revision
            || roots.contains(root)
        {
            return Err(ConversationFailure::OwnerEvidenceMismatch);
        }
        roots.push(*root);
        if root.sequence > facts.head.settled_prefix && root.sequence <= candidate {
            first_protected =
                Some(first_protected.map_or(root.sequence, |n: u64| n.min(root.sequence)));
        }
    }
    if let Some(root_sequence) = first_protected {
        candidate = candidate.min(root_sequence.saturating_sub(1));
    }
    candidate = candidate.max(facts.head.settled_prefix);
    let mut head = facts.head;
    head.settled_prefix = candidate;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let receipt = RecorderCloseReceipt {
        fence,
        owner_aggregate_revision: owner.owner.aggregate_revision,
        terminal_receipt_digest: owner.terminal_receipt_digest,
        protection_roots: owner.protection_roots.clone(),
        settled_prefix: candidate,
        close_state_revision: head.state_revision,
    };
    Ok(RecorderCloseTransition {
        receipt,
        head: Some(head),
        replayed: false,
    })
}

pub fn retire_stale_recording(
    fence: RecorderFence,
    generation_fence: OwnerGenerationFence,
    facts: RecorderRetirementFacts,
) -> Result<RecorderRetirementTransition, ConversationFailure> {
    fence.validate()?;
    if let Some(receipt) = facts.stored_receipt {
        return replay_retirement(fence, receipt);
    }
    if facts.active_recorder.as_ref() != Some(&fence) {
        return Err(ConversationFailure::WrongWriter);
    }
    let owner = facts
        .owner
        .as_ref()
        .ok_or(ConversationFailure::OwnerEvidenceMismatch)?;
    owner.validate()?;
    if owner.domain != fence.executor_domain
        || owner.run_id != fence.run_id
        || owner.person_id != fence.identity.person_id
        || owner.input != fence.input
        || owner.executor_generation != fence.executor_generation
        || !matches!(
            owner.state,
            OwnerRunState::Terminal | OwnerRunState::PendingTerminal
        )
    {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    if !owner.unresolved_effects.is_empty() {
        return Err(ConversationFailure::UnresolvedEffect);
    }
    if generation_fence.domain != fence.executor_domain
        || generation_fence.old_generation != fence.executor_generation
        || generation_fence.current_generation <= generation_fence.old_generation
        || generation_fence.fence_revision == 0
        || generation_fence.evidence_digest == [0; 32]
    {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    validate_head_scope(
        &facts.head,
        &fence.identity,
        fence.conversation_id,
        fence.branch_id,
    )?;
    let mut head = facts.head;
    head.state_revision = head
        .state_revision
        .checked_add(1)
        .ok_or(ConversationFailure::InvalidInput)?;
    let receipt = RecorderRetirementReceipt {
        fence,
        owner_evidence_digest: owner.record_digest,
        generation_fence: generation_fence.clone(),
        generation_fence_digest: generation_fence.evidence_digest,
        retirement_state_revision: head.state_revision,
    };
    Ok(RecorderRetirementTransition {
        receipt,
        head: Some(head),
        replayed: false,
    })
}

/// Exact immutable open-ack replay, independent of current owner or Core fences.
pub fn replay_open_recording(
    request: RecorderStartRequest,
    receipt: RecorderOpenReceipt,
) -> Result<RecorderOpenTransition, ConversationFailure> {
    request.validate()?;
    if !receipt.fence.matches_start(&request) {
        return Err(ConversationFailure::RunAlreadyUsed);
    }
    Ok(RecorderOpenTransition {
        receipt,
        head: None,
        replayed: true,
    })
}

/// Exact immutable output-ack replay, independent of current owner or Core fences.
pub fn replay_recording_entry(
    request: RecordingRequest,
    receipt: RecordingReceipt,
) -> Result<RecordingTransition, ConversationFailure> {
    request.recorder.validate()?;
    let content_digest = recording_content_digest(
        &request.message,
        request.contribution_id,
        request.producing_task.as_ref(),
    )?;
    if receipt.transcript.conversation_id != request.recorder.conversation_id
        || receipt.transcript.branch_id != request.recorder.branch_id
        || receipt.recorder != request.recorder
        || receipt.contribution_id != request.contribution_id
        || receipt.producing_task != request.producing_task
        || receipt.content_digest != content_digest
    {
        return Err(ConversationFailure::MessageIdConflict);
    }
    Ok(RecordingTransition {
        receipt,
        appended: None,
        head: None,
        replayed: true,
    })
}

/// Exact immutable close-ack replay, independent of current owner or Core fences.
pub fn replay_close_recording(
    fence: RecorderFence,
    receipt: RecorderCloseReceipt,
) -> Result<RecorderCloseTransition, ConversationFailure> {
    fence.validate()?;
    if receipt.fence != fence {
        return Err(ConversationFailure::RunAlreadyUsed);
    }
    Ok(RecorderCloseTransition {
        receipt,
        head: None,
        replayed: true,
    })
}

/// Exact immutable stale-retirement replay. The generation fence digest commits
/// to the full durable owner fence assembled by Vault.
pub fn replay_retirement(
    fence: RecorderFence,
    receipt: RecorderRetirementReceipt,
) -> Result<RecorderRetirementTransition, ConversationFailure> {
    fence.validate()?;
    if receipt.fence != fence {
        return Err(ConversationFailure::RunAlreadyUsed);
    }
    Ok(RecorderRetirementTransition {
        receipt,
        head: None,
        replayed: true,
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
        || checkpoint.through.sequence > facts.head.settled_prefix
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
    validate_head_scope(
        head,
        &reference.identity,
        reference.conversation_id,
        reference.branch_id,
    )
}

fn validate_head_scope(
    head: &ConversationHead,
    identity: &AgentIdentity,
    conversation_id: ConversationId,
    branch_id: ConversationBranchId,
) -> Result<(), ConversationFailure> {
    if head.conversation_id != conversation_id || head.branch_id != branch_id {
        return Err(ConversationFailure::ConversationMismatch);
    }
    if &head.identity != identity {
        return Err(ConversationFailure::AgentMismatch);
    }
    Ok(())
}

fn validate_owner_for_start(
    owner: &OwnerRunEvidence,
    request: &RecorderStartRequest,
) -> Result<(), ConversationFailure> {
    owner.validate()?;
    if owner.run_id != request.run_id
        || owner.person_id != request.identity.person_id
        || owner.input != request.input
        || owner.executor_generation != request.executor_generation
        || owner.domain != request.executor_domain
        || owner.state != OwnerRunState::Working
    {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    Ok(())
}

fn validate_owner_for_recorder(
    owner: &OwnerRunEvidence,
    fence: &RecorderFence,
    required_state: OwnerRunState,
) -> Result<(), ConversationFailure> {
    owner.validate()?;
    if owner.run_id != fence.run_id
        || owner.person_id != fence.identity.person_id
        || owner.input != fence.input
        || owner.executor_generation != fence.executor_generation
        || owner.domain != fence.executor_domain
        || owner.state != required_state
    {
        return Err(ConversationFailure::OwnerEvidenceMismatch);
    }
    Ok(())
}

fn validate_task_binding(
    message: &ConversationMessage,
    producing_task: Option<&TaskEvidenceReference>,
) -> Result<(), ConversationFailure> {
    match (message.task_id, producing_task) {
        (None, None) => Ok(()),
        (Some(message_task), Some(receipt)) => {
            receipt
                .validate()
                .map_err(|_| ConversationFailure::OwnerEvidenceMismatch)?;
            if receipt.task_id() == message_task {
                Ok(())
            } else {
                Err(ConversationFailure::TaskMismatch)
            }
        }
        _ => Err(ConversationFailure::OwnerEvidenceMismatch),
    }
}

pub fn recording_content_digest(
    message: &ConversationMessage,
    contribution_id: LogicalContributionId,
    producing_task: Option<&TaskEvidenceReference>,
) -> Result<[u8; 32], ConversationFailure> {
    let encoded = serde_json::to_vec(&(message, contribution_id, producing_task))
        .map_err(|_| ConversationFailure::InvalidInput)?;
    let mut digest = Sha256::new();
    digest.update(b"floe-conversation-recording-content-v2\0");
    digest.update((encoded.len() as u64).to_be_bytes());
    digest.update(encoded);
    Ok(digest.finalize().into())
}

pub fn advance_prefix_digest(
    previous: [u8; 32],
    reference: TranscriptReference,
    message: &ConversationMessage,
) -> Result<[u8; 32], ConversationFailure> {
    advance_core_prefix_digest(
        previous,
        reference,
        message,
        TranscriptEntryKind::Inbound,
        None,
        None,
        None,
    )
}

pub fn advance_core_prefix_digest(
    previous: [u8; 32],
    reference: TranscriptReference,
    message: &ConversationMessage,
    kind: TranscriptEntryKind,
    producer_run: Option<RunId>,
    contribution_id: Option<LogicalContributionId>,
    producing_task: Option<&TaskEvidenceReference>,
) -> Result<[u8; 32], ConversationFailure> {
    if !reference.message_id.is_valid()
        || (kind == TranscriptEntryKind::Inbound
            && (producer_run.is_some() || contribution_id.is_some() || producing_task.is_some()))
        || (kind == TranscriptEntryKind::GeneratedOutput
            && (producer_run.is_none() || contribution_id.is_none()))
    {
        return Err(ConversationFailure::InvalidInput);
    }
    let origin =
        serde_json::to_vec(&message.origin).map_err(|_| ConversationFailure::InvalidInput)?;
    let task = producing_task
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|_| ConversationFailure::InvalidInput)?;
    let mut digest = Sha256::new();
    digest.update(b"floe-conversation-transcript-prefix-chain-v3\0");
    digest.update([match kind {
        TranscriptEntryKind::Inbound => 0,
        TranscriptEntryKind::GeneratedOutput => 1,
    }]);
    digest.update(previous);
    digest.update(reference.conversation_id.as_uuid().as_bytes());
    digest.update(reference.branch_id.as_uuid().as_bytes());
    digest.update(reference.sequence.to_be_bytes());
    digest.update(reference.message_id.as_uuid().as_bytes());
    digest.update(message.body_digest());
    digest.update((origin.len() as u64).to_be_bytes());
    digest.update(origin);
    if let Some(run_id) = producer_run {
        digest.update([1]);
        digest.update(run_id.as_uuid().as_bytes());
    } else {
        digest.update([0]);
    }
    if let Some(contribution_id) = contribution_id {
        digest.update([1]);
        digest.update(contribution_id.as_uuid().as_bytes());
    } else {
        digest.update([0]);
    }
    if let Some(task) = task {
        digest.update([1]);
        digest.update((task.len() as u64).to_be_bytes());
        digest.update(task);
    } else {
        digest.update([0]);
    }
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
