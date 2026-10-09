//! Task-owned storage and journal capabilities. No parent journal is accepted.

use std::sync::Arc;

use floe_agent_contract::{
    AgentFailure, BoxFuture, EndpointSettlement, ExecutionJournal, JournalEntry, TaskExecutionKey,
    TaskExecutionReceipt, TaskExecutionReceiptRef, TaskId, TaskSnapshot,
};

use crate::task_record::TaskRecord;
use crate::{ExpertTaskAdmissionReference, ExpertTaskConversation, ExpertTaskConversationDraft};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskAdmission {
    Created {
        record: TaskRecord,
        expert_input: ExpertTaskAdmissionReference,
    },
    Existing {
        record: TaskRecord,
        expert_input: ExpertTaskAdmissionReference,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskActivation {
    pub executor_generation: u64,
    /// Interrupted records retain their original immutable execution keys.
    pub interrupted: Vec<TaskRecord>,
}

#[derive(Clone, Debug)]
pub struct TaskExecutionCommit {
    pub execution: TaskExecutionKey,
    pub expected_task_revision: u64,
    pub expected_journal_revision: u64,
    pub terminal: TaskSnapshot,
    pub settlement: Option<EndpointSettlement>,
}

pub trait TaskRepository: Send + Sync {
    /// Fence the previous executor before projecting and interrupting its orphaned journals.
    /// Activation must not race an unfenced, still-running previous executor.
    fn activate<'a>(&'a self) -> BoxFuture<'a, Result<TaskActivation, AgentFailure>>;

    /// Called only by a retained Task owner after its active reservation is
    /// registered. An error may follow commit; exact readback distinguishes
    /// that case without creating another execution or endpoint handoff.
    fn admit<'a>(
        &'a self,
        proposed: TaskRecord,
        conversation: ExpertTaskConversationDraft,
    ) -> BoxFuture<'a, Result<TaskAdmission, AgentFailure>>;

    /// Resolve only the immutable Task-pinned Expert prefix, with typed
    /// coverage attached to each model-history entry. The owner adapter must
    /// bound and validate the complete snapshot before hydrating payloads.
    fn load_conversation<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> BoxFuture<'a, Result<ExpertTaskConversation, AgentFailure>>;

    /// Resolve the durable owner-issued input reference after an uncertain
    /// admission acknowledgement. This never computes or advances the pin.
    fn expert_admission_reference<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> BoxFuture<'a, Result<ExpertTaskAdmissionReference, AgentFailure>>;

    /// Only Submitted -> Working. Terminal state requires journal-bound settlement.
    /// The owner retains this future past observer cancellation and may rejoin
    /// the exact Working row after an uncertain acknowledgement.
    fn compare_and_swap<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        expert_input: ExpertTaskAdmissionReference,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>>;

    /// Every append checks the exact Task/execution/generation and current active fence.
    /// A terminal Task rejects appends. Reserve a result slot for each admitted intent.
    fn journal(
        &self,
        execution: TaskExecutionKey,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure>;

    fn load_journal<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>>;

    fn read_execution_receipt<'a>(
        &'a self,
        reference: TaskExecutionReceiptRef,
    ) -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>>;

    fn validate_settlement(&self, settlement: &EndpointSettlement) -> Result<(), AgentFailure>;

    /// Recompute the journal projection under the short storage transaction and
    /// commit terminal Task, optional private state and immutable receipt together.
    /// An identical replay returns the stored receipt without reapplying private state.
    fn settle_execution<'a>(
        &'a self,
        commit: TaskExecutionCommit,
    ) -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>>;

    fn get<'a>(
        &'a self,
        task_id: TaskId,
    ) -> BoxFuture<'a, Result<Option<TaskRecord>, AgentFailure>>;
}
