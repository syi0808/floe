use std::sync::Arc;

use crate::{EncryptedAgentVault, VaultKeyProvider};
use floe_agent_contract::{
    AgentFailure, BoxFuture, EndpointSettlement, ExecutionJournal, JournalAck, JournalEvent,
    TaskExecutionKey, TaskExecutionReceipt, TaskExecutionReceiptRef, TaskId,
};
use floe_experts::{
    ExpertSettlement, ExpertTaskAdmissionReference, ExpertTaskConversation,
    ExpertTaskConversationDraft, TaskActivation, TaskAdmission, TaskExecutionCommit, TaskRecord,
    TaskRepository,
};

pub struct VaultTaskRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
}

impl<Keys> VaultTaskRepository<Keys> {
    pub fn new(vault: Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider + 'static> TaskRepository for VaultTaskRepository<Keys> {
    fn activate<'a>(&'a self) -> BoxFuture<'a, Result<TaskActivation, AgentFailure>> {
        Box::pin(async move { self.vault.activate_task_executor().await })
    }

    fn admit<'a>(
        &'a self,
        proposed: TaskRecord,
        conversation: ExpertTaskConversationDraft,
    ) -> BoxFuture<'a, Result<TaskAdmission, AgentFailure>> {
        Box::pin(async move { self.vault.admit_task(proposed, conversation).await })
    }

    fn load_conversation<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> BoxFuture<'a, Result<ExpertTaskConversation, AgentFailure>> {
        Box::pin(async move { self.vault.expert_task_conversation(execution).await })
    }

    fn expert_admission_reference<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> BoxFuture<'a, Result<ExpertTaskAdmissionReference, AgentFailure>> {
        Box::pin(async move { self.vault.expert_task_admission_reference(execution).await })
    }

    fn compare_and_swap<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: floe_agent_contract::TaskSnapshot,
        expert_input: ExpertTaskAdmissionReference,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .compare_and_swap_task(
                    task_id,
                    expected_aggregate_revision,
                    executor_generation,
                    snapshot,
                    expert_input,
                )
                .await
        })
    }

    fn journal(
        &self,
        execution: TaskExecutionKey,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        execution.validate()?;
        Ok(Arc::new(VaultTaskExecutionJournal {
            vault: self.vault.clone(),
            execution,
        }))
    }

    fn load_journal<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> BoxFuture<'a, Result<Vec<floe_agent_contract::JournalEntry>, AgentFailure>> {
        Box::pin(async move { self.vault.load_task_journal(execution).await })
    }

    fn read_execution_receipt<'a>(
        &'a self,
        reference: TaskExecutionReceiptRef,
    ) -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>> {
        Box::pin(async move { self.vault.read_task_execution_receipt(reference).await })
    }

    fn validate_settlement(&self, settlement: &EndpointSettlement) -> Result<(), AgentFailure> {
        ExpertSettlement::from_endpoint_settlement(settlement, settlement.owner()).map(|_| ())
    }

    fn settle_execution<'a>(
        &'a self,
        commit: TaskExecutionCommit,
    ) -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>> {
        Box::pin(async move { self.vault.settle_task_execution(commit).await })
    }

    fn get<'a>(
        &'a self,
        task_id: TaskId,
    ) -> BoxFuture<'a, Result<Option<TaskRecord>, AgentFailure>> {
        Box::pin(async move { self.vault.task(task_id).await })
    }
}

struct VaultTaskExecutionJournal<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    execution: TaskExecutionKey,
}

impl<Keys: VaultKeyProvider + 'static> VaultTaskExecutionJournal<Keys> {
    fn record<'a>(
        &'a self,
        phase: &'static str,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move {
            let revision = self
                .vault
                .append_task_journal(self.execution, phase, event)
                .await?;
            Ok(JournalAck::Accepted { revision })
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> ExecutionJournal for VaultTaskExecutionJournal<Keys> {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("intent", event)
    }

    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("result", event)
    }

    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("output", event)
    }

    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record("checkpoint", event)
    }
}
