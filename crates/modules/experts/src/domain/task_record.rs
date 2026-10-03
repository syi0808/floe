//! Immutable Task admission and journal-derived terminal evidence.

use floe_agent_contract::{
    AgentFailure, Artifact, DependencyCoverage, InvocationKey, JournalEntry, JournalEvent,
    TaskBlockage, TaskExecutionKey, TaskExecutionReceipt, TaskExecutionReceiptRef,
    TaskSnapshot, TaskState,
};
use floe_agent_runtime::{
    JournalBlockage, JournalExecutionBinding, JournalProjection, JournalProjectionMode,
    project_execution_journal,
};
use floe_execution::budget::ModelReservationCeiling;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ExpertAdmissionIdentity, ExpertExecutionSelection};
use crate::task_repository::TaskExecutionCommit;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    pub snapshot: TaskSnapshot,
    pub admission: ExpertAdmissionIdentity,
    pub selection: ExpertExecutionSelection,
    pub invocation_key: InvocationKey,
    pub request_digest: [u8; 32],
    pub aggregate_revision: u64,
    /// The generation which admitted this execution, never rewritten on recovery.
    pub executor_generation: u64,
    pub execution_id: Uuid,
    pub device_id: String,
    pub catalog_revision: u64,
    pub model_allowance: ModelReservationCeiling,
    pub receipt: Option<TaskExecutionReceipt>,
}

impl TaskRecord {
    pub fn execution(&self) -> TaskExecutionKey {
        TaskExecutionKey {
            task_id: self.snapshot.task_id,
            execution_id: self.execution_id,
            executor_generation: self.executor_generation,
        }
    }

    pub fn journal_binding(&self) -> Result<JournalExecutionBinding, AgentFailure> {
        let root_run_id = self.snapshot.parent_run_id
            .map(|id| floe_agent_contract::RunId::from_uuid(id).ok_or(AgentFailure::StorageUnavailable))
            .transpose()?;
        let binding = JournalExecutionBinding {
            principal: self.snapshot.principal.clone(),
            device_id: self.device_id.clone(),
            execution_id: self.execution_id,
            catalog_revision: self.catalog_revision,
            root_run_id,
            owning_task_id: Some(self.snapshot.task_id),
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.snapshot.validate(maximum_bytes)?;
        self.execution().validate()?;
        self.model_allowance.validate()?;
        self.journal_binding()?;
        self.admission.validate_task(&self.snapshot.agent_id, self.snapshot.definition_revision)?;
        self.selection.validate()?;
        if self.aggregate_revision == 0 || self.invocation_key.as_uuid().is_nil()
            || self.request_digest == [0; 32]
            || self.catalog_revision != self.snapshot.definition_revision
            || self.device_id.trim().is_empty()
            || self.device_id.trim() != self.device_id
            || self.device_id.len() > floe_agent_contract::MAX_DELEGATION_DEVICE_ID_BYTES
            || self.device_id.chars().any(char::is_control)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if !terminal(self.snapshot.state) {
            if self.receipt.is_some() || self.snapshot.result.is_some()
                || !self.snapshot.artifacts.is_empty() || self.snapshot.issue.is_some()
                || self.snapshot.blockage.is_some()
                || self.snapshot.coverage != DependencyCoverage::Unknown
                || (self.snapshot.state == TaskState::Submitted && self.aggregate_revision != 1)
                || (self.snapshot.state == TaskState::Working && self.aggregate_revision != 2)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            return Ok(());
        }
        let receipt = self.receipt.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
        receipt.validate(maximum_bytes)?;
        if receipt.snapshot != self.snapshot || receipt.reference.execution != self.execution()
            || receipt.reference.task_revision != self.aggregate_revision
            || receipt.reference.digest != receipt_digest(self, receipt)?
            || (!matches!(self.snapshot.state, TaskState::Completed | TaskState::Blocked)
                && (self.snapshot.issue.is_none() || !self.snapshot.artifacts.is_empty()
                    || self.snapshot.coverage != DependencyCoverage::Unknown))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    pub fn validate_initial(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.validate(maximum_bytes)?;
        if self.aggregate_revision != 1 || self.snapshot.state != TaskState::Submitted {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    /// The only ordinary CAS transition. A terminal snapshot cannot bypass its journal.
    pub fn transition(
        &self,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        maximum_bytes: usize,
    ) -> Result<Self, AgentFailure> {
        self.validate(maximum_bytes)?;
        if self.aggregate_revision != expected_aggregate_revision
            || self.executor_generation != executor_generation
            || self.snapshot.state != TaskState::Submitted || snapshot.state != TaskState::Working
            || !same_task_identity(&self.snapshot, &snapshot)
        {
            return Err(AgentFailure::Conflict);
        }
        let mut next = self.clone();
        next.snapshot = snapshot;
        next.aggregate_revision = self.aggregate_revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
        next.validate(maximum_bytes)?;
        Ok(next)
    }
}

pub fn settle_task_execution(
    record: &TaskRecord,
    commit: &TaskExecutionCommit,
    journal: &[JournalEntry],
    maximum_bytes: usize,
) -> Result<TaskRecord, AgentFailure> {
    record.validate(maximum_bytes)?;
    commit.execution.validate()?;
    commit.terminal.validate(maximum_bytes)?;
    if commit.execution != record.execution()
        || !same_task_identity(&record.snapshot, &commit.terminal)
        || !terminal(commit.terminal.state)
        || commit.settlement.as_ref().is_some_and(|value| value.validate().is_err())
        || (commit.settlement.is_some() && commit.terminal.state != TaskState::Completed)
    {
        return Err(AgentFailure::Conflict);
    }
    let projection = project_task_journal(record, journal)?;
    if projection.journal_revision != commit.expected_journal_revision {
        return Err(AgentFailure::Conflict);
    }
    validate_terminal_evidence(record, &commit.terminal, journal, &projection)?;
    if terminal(record.snapshot.state) {
        let receipt = record.receipt.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
        if record.snapshot != commit.terminal
            || commit.expected_task_revision.checked_add(1) != Some(record.aggregate_revision)
            || receipt.reference.journal_revision != projection.journal_revision
            || receipt.journal_digest != projection.journal_digest
            || receipt.accounting != projection.own_accounting
        {
            return Err(AgentFailure::Conflict);
        }
        return Ok(record.clone());
    }
    if record.aggregate_revision != commit.expected_task_revision {
        return Err(AgentFailure::Conflict);
    }
    // An unstarted Task may only be interrupted after its executor was fenced.
    if record.snapshot.state == TaskState::Submitted
        && (commit.terminal.state != TaskState::Interrupted || !journal.is_empty())
    {
        return Err(AgentFailure::Conflict);
    }
    let mut next = record.clone();
    next.snapshot = commit.terminal.clone();
    next.aggregate_revision = record.aggregate_revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
    let mut receipt = TaskExecutionReceipt {
        reference: TaskExecutionReceiptRef {
            execution: record.execution(),
            task_revision: next.aggregate_revision,
            journal_revision: projection.journal_revision,
            digest: [0; 32],
        },
        journal_digest: projection.journal_digest,
        snapshot: commit.terminal.clone(),
        accounting: projection.own_accounting,
    };
    receipt.reference.digest = receipt_digest(&next, &receipt)?;
    next.receipt = Some(receipt);
    next.validate(maximum_bytes)?;
    Ok(next)
}

/// Called only after the repository advances its global executor fence. It never
/// rewrites the original execution key or reissues work from an unfinished journal.
pub fn interrupt_task_execution(
    record: &TaskRecord,
    replacement_generation: u64,
    journal: &[JournalEntry],
    maximum_bytes: usize,
) -> Result<Option<TaskRecord>, AgentFailure> {
    record.validate(maximum_bytes)?;
    if replacement_generation == 0 || replacement_generation < record.executor_generation {
        return Err(AgentFailure::Conflict);
    }
    if terminal(record.snapshot.state) || replacement_generation == record.executor_generation {
        return Ok(None);
    }
    let snapshot = TaskSnapshot {
        state: TaskState::Interrupted,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        issue: Some(AgentFailure::Interrupted),
        blockage: None,
        ..record.snapshot.clone()
    };
    settle_task_execution(record, &TaskExecutionCommit {
        execution: record.execution(),
        expected_task_revision: record.aggregate_revision,
        expected_journal_revision: journal.last().map_or(0, |entry| entry.revision),
        terminal: snapshot,
        settlement: None,
    }, journal, maximum_bytes).map(Some)
}

pub(crate) fn project_task_journal(record: &TaskRecord, journal: &[JournalEntry])
    -> Result<JournalProjection, AgentFailure>
{
    // Current Expert packages have no subdelegation or finalization/continuation branch.
    if journal.iter().any(|entry| match &entry.event {
        JournalEvent::DelegationIntent { .. } | JournalEvent::DelegationResult { .. }
            | JournalEvent::FinalizationStarted { .. } => true,
        JournalEvent::ModelIntent { parent_task_id, reservation_ceiling, .. } =>
            *parent_task_id != Some(record.snapshot.task_id)
                || reservation_ceiling.tokens > record.model_allowance.tokens
                || reservation_ceiling.cost_micros > record.model_allowance.cost_micros,
        _ => false,
    }) {
        return Err(AgentFailure::StorageUnavailable);
    }
    let projection = project_execution_journal(
        &record.journal_binding()?, journal, JournalProjectionMode::DurablePrefix,
    )?;
    if !projection.delegated_receipts.is_empty() || !projection.task_refs.is_empty() {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(projection)
}

fn validate_terminal_evidence(
    record: &TaskRecord,
    terminal: &TaskSnapshot,
    journal: &[JournalEntry],
    projection: &JournalProjection,
) -> Result<(), AgentFailure> {
    if matches!(terminal.state, TaskState::Completed | TaskState::Blocked) {
        let coverage = journal.iter().try_fold(DependencyCoverage::Independent, |coverage, entry| {
            let observed = match &entry.event {
                JournalEvent::ToolResult { result } => Some(&result.coverage),
                JournalEvent::ValidatedBatch { batch } => Some(&batch.projection_coverage),
                _ => None,
            };
            match observed {
                Some(observed) => coverage.merge(observed).map_err(|_| AgentFailure::StorageUnavailable),
                None => Ok(coverage),
            }
        })?;
        if terminal.coverage != coverage {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    match terminal.state {
        TaskState::Completed => {
            if projection.output.as_ref() != terminal.result.as_ref().map(|text| (text.clone(), terminal.artifacts.clone())).as_ref()
                || projection.blockage.is_some() || !projection.unresolved_attempts.is_empty()
            {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        TaskState::Blocked => {
            if projection.output.is_some() || !projection.unresolved_attempts.is_empty() {
                return Err(AgentFailure::StorageUnavailable);
            }
            match terminal.blockage.as_ref().ok_or(AgentFailure::StorageUnavailable)? {
                TaskBlockage::Binding { requirement_keys } => {
                    if !journal.is_empty() || terminal.coverage != DependencyCoverage::Independent
                        || *requirement_keys != missing_requirement_keys(&record.selection)
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
                TaskBlockage::SourceRead { tool_call_id, blockers } => {
                    if !matches!(&projection.blockage,
                        Some(JournalBlockage::SourceRead { call_id, blockers: actual })
                            if call_id == tool_call_id && actual == blockers)
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
                TaskBlockage::ModelProjection { plan, review } => {
                    plan.validate()?;
                    review.validate()?;
                    let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(&(
                        plan, review.projection_operation_id, &review.blockers,
                    )).map_err(|_| AgentFailure::StorageUnavailable)?).into();
                    if plan.principal != record.snapshot.principal || plan.device_id != record.device_id
                        || plan.consumer != crate::DELEGATED_EXPERT_INFERENCE_CONSUMER
                        || review.target_digest != digest || projection.blockage.is_some()
                        || projection.pending_batch.is_some()
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
            }
        }
        TaskState::Submitted | TaskState::Working => return Err(AgentFailure::Conflict),
        _ => {}
    }
    Ok(())
}

pub(crate) fn missing_requirement_keys(selection: &ExpertExecutionSelection) -> Vec<String> {
    selection.requirements.iter()
        .filter(|requirement| requirement.selected.len() < usize::from(requirement.minimum_sources))
        .map(|requirement| requirement.key.clone()).collect()
}

pub(crate) fn terminal(state: TaskState) -> bool {
    !matches!(state, TaskState::Submitted | TaskState::Working)
}

fn same_task_identity(left: &TaskSnapshot, right: &TaskSnapshot) -> bool {
    left.task_id == right.task_id && left.parent_run_id == right.parent_run_id
        && left.principal == right.principal && left.agent_id == right.agent_id
        && left.definition_revision == right.definition_revision
}

fn receipt_digest(record: &TaskRecord, receipt: &TaskExecutionReceipt) -> Result<[u8; 32], AgentFailure> {
    let bytes = serde_json::to_vec(&(
        "floe.task-execution-receipt.sha256.v1", record.execution(), &record.device_id,
        record.catalog_revision, record.model_allowance, &record.admission, &record.selection,
        record.invocation_key, record.request_digest, receipt.reference.task_revision,
        receipt.reference.journal_revision, receipt.journal_digest, &receipt.snapshot,
        &receipt.accounting,
    )).map_err(|_| AgentFailure::StorageUnavailable)?;
    Ok(Sha256::digest(bytes).into())
}

#[derive(Clone, Debug)]
pub struct TaskArtifactEvidence {
    pub receipt: TaskExecutionReceipt,
    pub artifact: Artifact,
    pub admission: ExpertAdmissionIdentity,
    pub selection: ExpertExecutionSelection,
    pub invocation_key: InvocationKey,
}

pub fn validate_task_artifact(
    record: &TaskRecord,
    reference: &TaskExecutionReceiptRef,
    artifact_id: Uuid,
    actor: &floe_kernel::OwnerActor,
) -> Result<TaskArtifactEvidence, AgentFailure> {
    actor.validate()?;
    record.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    if actor.person_id.to_string() != record.snapshot.principal || actor.device_id != record.device_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let receipt = record.receipt.as_ref().ok_or(AgentFailure::Conflict)?;
    if record.snapshot.state != TaskState::Completed || receipt.reference != *reference {
        return Err(AgentFailure::Conflict);
    }
    let mut matches = record.snapshot.artifacts.iter().filter(|artifact| artifact.artifact_id == artifact_id);
    let artifact = matches.next().ok_or(AgentFailure::NotFound)?;
    if matches.next().is_some() || !coverage_contains(&record.snapshot.coverage, &artifact.coverage) {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(TaskArtifactEvidence {
        receipt: receipt.clone(), artifact: artifact.clone(), admission: record.admission.clone(),
        selection: record.selection.clone(), invocation_key: record.invocation_key,
    })
}

fn coverage_contains(task: &DependencyCoverage, artifact: &DependencyCoverage) -> bool {
    match (task, artifact) {
        (_, DependencyCoverage::Unknown) => false,
        (_, DependencyCoverage::Independent) => true,
        (DependencyCoverage::Dependent { dependencies: task }, DependencyCoverage::Dependent { dependencies: artifact }) =>
            artifact.iter().all(|dependency| task.contains(dependency)),
        _ => false,
    }
}
