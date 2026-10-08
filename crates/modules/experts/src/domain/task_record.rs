//! Immutable Task admission and journal-derived terminal evidence.

use floe_agent_contract::{
    AgentFailure, Artifact, DependencyCoverage, InvocationKey, JournalEntry, JournalEvent,
    TaskBlockage, TaskExecutionKey, TaskExecutionReceipt, TaskExecutionReceiptRef, TaskSnapshot,
    TaskState,
};
use floe_agent_runtime::{
    JournalBlockage, JournalExecutionBinding, JournalProjection, JournalProjectionMode,
    project_execution_journal,
};
use floe_execution::budget::ModelReservationCeiling;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::task_repository::TaskExecutionCommit;
use crate::{ExpertAdmissionIdentity, ExpertExecutionSelection};

pub const MAX_TASK_RECORD_BYTES: usize = 512 * 1024;
/// Replacing the initial snapshot and null receipt can grow a row by at most
/// these two bounded values plus scalar/framing growth. Admission and every
/// journal append preserve this space before any external handoff is allowed.
pub const MAX_TASK_TERMINAL_RESERVE_BYTES: usize = floe_agent_contract::MAX_OUTPUT_BYTES
    + floe_agent_contract::MAX_TASK_EXECUTION_RECEIPT_BYTES
    + 1024;

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
    pub maximum_output_bytes: usize,
    /// The actual acknowledged journal prefix, updated in the append transaction.
    pub journal_revision: u64,
    pub journal_digest: [u8; 32],
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
        let root_run_id = self
            .snapshot
            .parent_run_id
            .map(|id| {
                floe_agent_contract::RunId::from_uuid(id).ok_or(AgentFailure::StorageUnavailable)
            })
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
        if self.maximum_output_bytes == 0
            || self.maximum_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let maximum_bytes = maximum_bytes.min(self.maximum_output_bytes);
        self.snapshot.validate(maximum_bytes)?;
        self.execution().validate()?;
        self.model_allowance.validate()?;
        self.journal_binding()?;
        self.admission
            .validate_task(&self.snapshot.agent_id, self.snapshot.definition_revision)?;
        self.selection.validate()?;
        if self.aggregate_revision == 0
            || self.invocation_key.as_uuid().is_nil()
            || self.request_digest == [0; 32]
            || self.catalog_revision != self.snapshot.definition_revision
            || self.device_id.trim().is_empty()
            || self.device_id.trim() != self.device_id
            || self.device_id.len() > floe_agent_contract::MAX_DELEGATION_DEVICE_ID_BYTES
            || self.device_id.chars().any(char::is_control)
            || self.journal_revision > 512
            || self.journal_digest == [0; 32]
            || (self.journal_revision == 0
                && self.journal_digest != floe_agent_runtime::journal_digest(&[])?)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        validate_record_capacity(self)?;
        if !terminal(self.snapshot.state) {
            if self.receipt.is_some()
                || self.snapshot.result.is_some()
                || !self.snapshot.artifacts.is_empty()
                || self.snapshot.issue.is_some()
                || self.snapshot.blockage.is_some()
                || self.snapshot.coverage != DependencyCoverage::Unknown
                || (self.snapshot.state == TaskState::Submitted && self.aggregate_revision != 1)
                || (self.snapshot.state == TaskState::Submitted && self.journal_revision != 0)
                || (self.snapshot.state == TaskState::Working && self.aggregate_revision != 2)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            return Ok(());
        }
        let receipt = self
            .receipt
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        receipt.validate(maximum_bytes)?;
        if receipt.snapshot != self.snapshot
            || receipt.reference.execution != self.execution()
            || receipt.reference.task_revision != self.aggregate_revision
            || receipt.reference.journal_revision != self.journal_revision
            || receipt.journal_digest != self.journal_digest
            || receipt.reference.digest != receipt_digest(self, receipt)?
            || (!matches!(
                self.snapshot.state,
                TaskState::Completed | TaskState::Blocked
            ) && (self.snapshot.issue.is_none()
                || !self.snapshot.artifacts.is_empty()
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
            || self.snapshot.state != TaskState::Submitted
            || snapshot.state != TaskState::Working
            || !same_task_identity(&self.snapshot, &snapshot)
        {
            return Err(AgentFailure::Conflict);
        }
        let mut next = self.clone();
        next.snapshot = snapshot;
        next.aggregate_revision = self
            .aggregate_revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
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
    let maximum_bytes = maximum_bytes.min(record.maximum_output_bytes);
    record.validate(maximum_bytes)?;
    commit.execution.validate()?;
    commit.terminal.validate(maximum_bytes)?;
    if commit.execution != record.execution()
        || !same_task_identity(&record.snapshot, &commit.terminal)
        || !terminal(commit.terminal.state)
        || commit
            .settlement
            .as_ref()
            .is_some_and(|value| value.validate().is_err())
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
        let receipt = record
            .receipt
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
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
    next.aggregate_revision = record
        .aggregate_revision
        .checked_add(1)
        .ok_or(AgentFailure::Conflict)?;
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
    settle_task_execution(
        record,
        &TaskExecutionCommit {
            execution: record.execution(),
            expected_task_revision: record.aggregate_revision,
            expected_journal_revision: journal.last().map_or(0, |entry| entry.revision),
            terminal: snapshot,
            settlement: None,
        },
        journal,
        maximum_bytes,
    )
    .map(Some)
}

pub(crate) fn project_task_journal(
    record: &TaskRecord,
    journal: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    let projection = project_task_journal_contents(record, journal)?;
    if projection.journal_revision != record.journal_revision
        || projection.journal_digest != record.journal_digest
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(projection)
}

/// Authenticate a complete stored prefix against its Task-owned watermark.
pub fn validate_task_journal(
    record: &TaskRecord,
    journal: &[JournalEntry],
) -> Result<(), AgentFailure> {
    record.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    project_task_journal(record, journal).map(|_| ())
}

/// Derive the one next Task head. Storage appends the entry and this exact
/// owner-produced row in the same transaction before acknowledging either.
pub fn advance_task_journal(
    record: &TaskRecord,
    journal: &[JournalEntry],
) -> Result<TaskRecord, AgentFailure> {
    record.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    if record.snapshot.state != TaskState::Working
        || journal.len() as u64
            != record
                .journal_revision
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?
    {
        return Err(AgentFailure::Conflict);
    }
    let (last, prefix) = journal.split_last().ok_or(AgentFailure::InvalidInput)?;
    let prior_projection = project_task_journal(record, prefix)?;
    if let JournalEvent::ModelIntent { plan, .. } = &last.event {
        // Historical evidence remains readable, but a new append must keep
        // the execution pin and cannot recover from an Unproven prefix.
        floe_agent_contract::validate_model_intent_selection(
            &prior_projection.model_selection,
            plan,
        )?;
    }
    let projection = project_task_journal_contents(record, journal)?;
    // Reserve before dispatch; actual later charges remain recordable even
    // when the provider reports an overrun of that reservation.
    if matches!(&last.event, JournalEvent::ModelIntent { .. })
        && (projection.own_accounting.usage.tokens > record.model_allowance.tokens
            || projection.own_accounting.usage.cost_micros > record.model_allowance.cost_micros)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    let mut next = record.clone();
    next.journal_revision = projection.journal_revision;
    next.journal_digest = projection.journal_digest;
    next.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    // The acknowledgement can always be recovered into an accounting-bearing
    // Interrupted receipt, even if no more work can run after this append.
    let interrupted = TaskSnapshot {
        state: TaskState::Interrupted,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        issue: Some(AgentFailure::Interrupted),
        blockage: None,
        ..next.snapshot.clone()
    };
    settle_task_execution(
        &next,
        &TaskExecutionCommit {
            execution: next.execution(),
            expected_task_revision: next.aggregate_revision,
            expected_journal_revision: next.journal_revision,
            terminal: interrupted,
            settlement: None,
        },
        journal,
        floe_agent_contract::MAX_OUTPUT_BYTES,
    )?;
    // A final payload/review is acknowledged only when its actual Task terminal
    // snapshot also fits. Never acknowledge output which cannot become a receipt.
    let terminal = match &last.event {
        JournalEvent::Output {
            text, artifacts, ..
        } => Some(TaskSnapshot {
            state: TaskState::Completed,
            result: Some(text.clone()),
            artifacts: artifacts.clone(),
            coverage: journal_coverage(journal)?,
            issue: None,
            blockage: None,
            ..next.snapshot.clone()
        }),
        JournalEvent::ToolReviewRequired { call_id, blockers } => Some(TaskSnapshot {
            state: TaskState::Blocked,
            result: None,
            artifacts: vec![],
            coverage: journal_coverage(journal)?,
            issue: None,
            blockage: Some(TaskBlockage::SourceRead {
                tool_call_id: *call_id,
                blockers: blockers.clone(),
            }),
            ..next.snapshot.clone()
        }),
        _ => None,
    };
    if let Some(terminal) = terminal {
        settle_task_execution(
            &next,
            &TaskExecutionCommit {
                execution: next.execution(),
                expected_task_revision: next.aggregate_revision,
                expected_journal_revision: next.journal_revision,
                terminal,
                settlement: None,
            },
            journal,
            floe_agent_contract::MAX_OUTPUT_BYTES,
        )?;
    }
    Ok(next)
}

fn project_task_journal_contents(
    record: &TaskRecord,
    journal: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    // Current Expert packages have no subdelegation or finalization/continuation branch.
    if journal.iter().any(|entry| match &entry.event {
        JournalEvent::DelegationIntent { .. }
        | JournalEvent::DelegationResult { .. }
        | JournalEvent::FinalizationStarted { .. } => true,
        JournalEvent::ModelIntent {
            parent_task_id,
            reservation_ceiling,
            ..
        } => {
            *parent_task_id != Some(record.snapshot.task_id)
                || reservation_ceiling.tokens > record.model_allowance.tokens
                || reservation_ceiling.cost_micros > record.model_allowance.cost_micros
        }
        _ => false,
    }) {
        return Err(AgentFailure::StorageUnavailable);
    }
    let projection = project_execution_journal(
        &record.journal_binding()?,
        journal,
        JournalProjectionMode::DurablePrefix,
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
        let coverage = journal_coverage(journal)?;
        if terminal.coverage != coverage {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    match terminal.state {
        TaskState::Completed => {
            if projection.output.as_ref()
                != terminal
                    .result
                    .as_ref()
                    .map(|text| (text.clone(), terminal.artifacts.clone()))
                    .as_ref()
                || projection.blockage.is_some()
                || !projection.unresolved_attempts.is_empty()
            {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        TaskState::Blocked => {
            if projection.output.is_some() || !projection.unresolved_attempts.is_empty() {
                return Err(AgentFailure::StorageUnavailable);
            }
            match terminal
                .blockage
                .as_ref()
                .ok_or(AgentFailure::StorageUnavailable)?
            {
                TaskBlockage::Binding { requirement_keys } => {
                    if !journal.is_empty()
                        || terminal.coverage != DependencyCoverage::Independent
                        || *requirement_keys != missing_requirement_keys(&record.selection)
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
                TaskBlockage::SourceRead {
                    tool_call_id,
                    blockers,
                } => {
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
                    let digest: [u8; 32] = Sha256::digest(
                        serde_json::to_vec(&(
                            plan,
                            review.projection_operation_id,
                            &review.blockers,
                        ))
                        .map_err(|_| AgentFailure::StorageUnavailable)?,
                    )
                    .into();
                    if plan.principal != record.snapshot.principal
                        || plan.device_id != record.device_id
                        || plan.consumer != crate::DELEGATED_EXPERT_INFERENCE_CONSUMER
                        || review.target_digest != digest
                        || projection.blockage.is_some()
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

fn journal_coverage(journal: &[JournalEntry]) -> Result<DependencyCoverage, AgentFailure> {
    journal
        .iter()
        .try_fold(DependencyCoverage::Independent, |coverage, entry| {
            let observed = match &entry.event {
                JournalEvent::ToolResult { result } => Some(&result.coverage),
                JournalEvent::ValidatedBatch { batch } => Some(&batch.projection_coverage),
                _ => None,
            };
            match observed {
                Some(observed) => coverage
                    .merge(observed)
                    .map_err(|_| AgentFailure::StorageUnavailable),
                None => Ok(coverage),
            }
        })
}

#[cfg(test)]
mod model_selection_append_tests {
    use super::*;
    use floe_agent_contract::{
        ModelBindingDigest, ModelBudgetProfile, ModelCapabilities, ModelSelectionCommitment,
        PreparedModelPlan, ProcessingBoundary, ProjectionRef,
    };
    use floe_execution::budget::ModelReservationCeiling;

    fn task_record() -> TaskRecord {
        let task_id = floe_agent_contract::TaskId::new();
        let principal = "00000000-0000-4000-8000-000000000001".to_owned();
        let snapshot = TaskSnapshot {
            task_id,
            parent_run_id: Some(Uuid::new_v4()),
            principal,
            agent_id: "floe.test.expert".into(),
            definition_revision: 1,
            state: TaskState::Working,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: None,
            blockage: None,
        };
        TaskRecord {
            snapshot,
            admission: ExpertAdmissionIdentity {
                registry_instance_id: Uuid::new_v4(),
                assignment_id: Uuid::new_v4(),
                installation_id: Uuid::new_v4(),
                package: floe_agent_contract::PackageRef {
                    kind: floe_agent_contract::PackageKind::Expert,
                    id: "floe.test.expert".into(),
                    version: "1.0.0".into(),
                },
                definition_revision: 1,
            },
            selection: crate::ExpertExecutionSelection::without_requirements(1)
                .expect("valid empty expert binding"),
            invocation_key: floe_agent_contract::InvocationKey::new(),
            request_digest: [1; 32],
            aggregate_revision: 2,
            executor_generation: 1,
            execution_id: Uuid::new_v4(),
            device_id: "device-1".into(),
            catalog_revision: 1,
            model_allowance: ModelReservationCeiling {
                tokens: 100,
                cost_micros: 100,
            },
            maximum_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
            journal_revision: 0,
            journal_digest: floe_agent_runtime::journal_digest(&[])
                .expect("empty task journal digest"),
            receipt: None,
        }
    }

    fn plan(record: &TaskRecord, commitment: u8) -> PreparedModelPlan {
        PreparedModelPlan {
            operation_id: Uuid::new_v4(),
            principal: record.snapshot.principal.clone(),
            device_id: record.device_id.clone(),
            purpose: "everyday_assistance".into(),
            consumer: crate::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
            capabilities: ModelCapabilities::chat(),
            boundary: ProcessingBoundary::Device,
            binding_digest: ModelBindingDigest([2; 32]),
            selection_commitment: Some(ModelSelectionCommitment([commitment; 32])),
            budget_profile: Some(ModelBudgetProfile::unknown()),
        }
    }

    fn intent(record: &TaskRecord, attempt_id: Uuid, plan: PreparedModelPlan) -> JournalEntry {
        JournalEntry {
            revision: 1,
            event: JournalEvent::ModelIntent {
                attempt_id,
                parent_task_id: Some(record.snapshot.task_id),
                reservation_ceiling: ModelReservationCeiling {
                    tokens: 10,
                    cost_micros: 10,
                },
                projection_ref: ProjectionRef::new(),
                plan,
            },
        }
    }

    #[test]
    fn task_owner_requires_complete_stable_selection_on_each_new_intent() {
        let record = task_record();
        let mut incomplete = plan(&record, 1);
        incomplete.selection_commitment = None;
        incomplete.budget_profile = None;
        assert_eq!(
            advance_task_journal(&record, &[intent(&record, Uuid::new_v4(), incomplete)]),
            Err(AgentFailure::PolicyDenied)
        );

        let first = intent(&record, Uuid::new_v4(), plan(&record, 1));
        let updated = advance_task_journal(&record, std::slice::from_ref(&first))
            .expect("admit and durably project a complete Task model intent");
        assert_eq!(updated.journal_revision, 1);
        let mut conflicting = intent(&record, Uuid::new_v4(), plan(&record, 2));
        conflicting.revision = 2;
        assert_eq!(
            advance_task_journal(&updated, &[first, conflicting]),
            Err(AgentFailure::PolicyDenied),
            "the Task owner rejects model or profile drift before committing the next head"
        );
    }

    #[test]
    fn task_owner_does_not_dispatch_from_a_historical_unproven_prefix() {
        let mut record = task_record();
        let mut legacy = plan(&record, 1);
        legacy.selection_commitment = None;
        legacy.budget_profile = None;
        let entry = intent(&record, Uuid::new_v4(), legacy);
        record.journal_revision = 1;
        record.journal_digest = floe_agent_runtime::journal_digest(std::slice::from_ref(&entry))
            .expect("historical journal digest");
        let mut next = intent(&record, Uuid::new_v4(), plan(&record, 2));
        next.revision = 2;
        assert_eq!(
            advance_task_journal(&record, &[entry, next]),
            Err(AgentFailure::PolicyDenied),
            "permissive historical projection cannot authorize a new Task model handoff"
        );
    }
}

fn validate_record_capacity(record: &TaskRecord) -> Result<(), AgentFailure> {
    let bytes = serde_json::to_vec(record)
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .len();
    let maximum = if record.receipt.is_some() {
        MAX_TASK_RECORD_BYTES
    } else {
        MAX_TASK_RECORD_BYTES - MAX_TASK_TERMINAL_RESERVE_BYTES
    };
    if bytes > maximum {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(())
}

pub(crate) fn missing_requirement_keys(selection: &ExpertExecutionSelection) -> Vec<String> {
    selection
        .requirements
        .iter()
        .filter(|requirement| requirement.selected.len() < usize::from(requirement.minimum_sources))
        .map(|requirement| requirement.key.clone())
        .collect()
}

pub(crate) fn terminal(state: TaskState) -> bool {
    !matches!(state, TaskState::Submitted | TaskState::Working)
}

fn same_task_identity(left: &TaskSnapshot, right: &TaskSnapshot) -> bool {
    left.task_id == right.task_id
        && left.parent_run_id == right.parent_run_id
        && left.principal == right.principal
        && left.agent_id == right.agent_id
        && left.definition_revision == right.definition_revision
}

fn receipt_digest(
    record: &TaskRecord,
    receipt: &TaskExecutionReceipt,
) -> Result<[u8; 32], AgentFailure> {
    let bytes = serde_json::to_vec(&(
        "floe.task-execution-receipt.sha256.v1",
        record.execution(),
        &record.device_id,
        record.catalog_revision,
        record.model_allowance,
        &record.admission,
        &record.selection,
        record.maximum_output_bytes,
        record.invocation_key,
        record.request_digest,
        receipt.reference.task_revision,
        receipt.reference.journal_revision,
        receipt.journal_digest,
        &receipt.snapshot,
        &receipt.accounting,
    ))
    .map_err(|_| AgentFailure::StorageUnavailable)?;
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
    person_id: floe_kernel::PersonId,
    device_id: &str,
) -> Result<TaskArtifactEvidence, AgentFailure> {
    if !person_id.is_valid() || device_id.trim().is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    record.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    if person_id.to_string() != record.snapshot.principal || device_id != record.device_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let receipt = record.receipt.as_ref().ok_or(AgentFailure::Conflict)?;
    if record.snapshot.state != TaskState::Completed || receipt.reference != *reference {
        return Err(AgentFailure::Conflict);
    }
    let mut matches = record
        .snapshot
        .artifacts
        .iter()
        .filter(|artifact| artifact.artifact_id == artifact_id);
    let artifact = matches.next().ok_or(AgentFailure::NotFound)?;
    if matches.next().is_some() || !coverage_contains(&record.snapshot.coverage, &artifact.coverage)
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(TaskArtifactEvidence {
        receipt: receipt.clone(),
        artifact: artifact.clone(),
        admission: record.admission.clone(),
        selection: record.selection.clone(),
        invocation_key: record.invocation_key,
    })
}

fn coverage_contains(task: &DependencyCoverage, artifact: &DependencyCoverage) -> bool {
    match (task, artifact) {
        (_, DependencyCoverage::Unknown) => false,
        (_, DependencyCoverage::Independent) => true,
        (
            DependencyCoverage::Dependent { dependencies: task },
            DependencyCoverage::Dependent {
                dependencies: artifact,
            },
        ) => artifact.iter().all(|dependency| task.contains(dependency)),
        _ => false,
    }
}
