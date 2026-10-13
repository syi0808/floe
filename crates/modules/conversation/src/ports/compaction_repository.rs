use crate::{CompactionClaim, CompactionExecution, RecoveryPage};
use floe_agent_contract::{BoxFuture, ExecutionJournal};
use floe_kernel::{AgentFailure, OwnerActor};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct CompactionAdmission {
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub source_byte_limit: usize,
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub max_output_bytes: usize,
}
impl CompactionAdmission {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil()
            || self.expected_session_revision == 0
            || !(2..=crate::COMPACTION_TRIGGER_BYTES).contains(&self.source_byte_limit)
            || self.max_tokens == 0
            || self.max_output_bytes == 0
            || self.max_output_bytes > crate::MAX_COMPACTION_SUMMARY_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Sole custody of a Conversation-owned summarization execution. All writes
/// fence the active executor; these operations never call a model or source.
pub trait CompactionRepository: Send + Sync {
    fn admit_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: CompactionAdmission,
    ) -> BoxFuture<'a, Result<Option<CompactionExecution>, AgentFailure>>;
    fn load_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: CompactionClaim,
    ) -> BoxFuture<'a, Result<CompactionExecution, AgentFailure>>;
    fn pending_compactions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<RecoveryPage<Uuid, Uuid>, AgentFailure>>;
    fn recover_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        execution_id: Uuid,
    ) -> BoxFuture<'a, Result<CompactionExecution, AgentFailure>>;
    fn retire_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: CompactionClaim,
        failure: AgentFailure,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
    /// Install only the exact recorded answer. Implementations atomically verify
    /// the source pin, advance the checkpoint, and retain the terminal receipt.
    fn complete_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: CompactionClaim,
    ) -> BoxFuture<'a, Result<crate::CompactionReceipt, AgentFailure>>;
    fn compaction_journal(
        &self,
        actor: OwnerActor,
        claim: CompactionClaim,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure>;
}
