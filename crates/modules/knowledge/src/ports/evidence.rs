use std::future::Future;

use floe_kernel::AgentFailure;
use uuid::Uuid;

use crate::LearningEvidenceSnapshot;

pub trait EvidenceReader: Send + Sync {
    fn read_learning_evidence(
        &self,
        person_id: floe_kernel::PersonId,
        session_id: Uuid,
        turn_ids: &[Uuid],
    ) -> impl Future<Output = Result<LearningEvidenceSnapshot, AgentFailure>> + Send;
}

/// Narrow live-read port for already projected memory. It does not grant access
/// to a conversation or to mutation APIs and is checked at every model fence.
pub trait MemoryEvidenceReader: Send + Sync {
    fn validate_memories<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        references: &'a [floe_context_contract::MemoryRevisionReference],
        now: chrono::DateTime<chrono::Utc>,
        scope: &'a floe_execution::ExecutionScope,
    ) -> floe_execution::BoxFuture<'a, Result<(), AgentFailure>>;
}
