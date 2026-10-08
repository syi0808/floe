use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor};
use floe_kernel::CommandFailure;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryStageOrigin {
    User,
    Learner {
        claim: crate::LearnerClaimRef,
        journal_revision: u64,
        journal_digest: [u8; 32],
    },
}

#[derive(Clone, Debug)]
pub struct MemoryStageRequest {
    pub actor: OwnerActor,
    /// Current owner clock for fresh admission; exact stored replay retains its original receipt.
    pub now: DateTime<Utc>,
    pub request: crate::StageMemoryCandidate,
    pub origin: MemoryStageOrigin,
}

#[derive(Clone, Debug)]
pub struct MemoryDecisionRequest {
    pub command_id: CommandId,
    pub candidate_id: Uuid,
    pub kind: crate::KnowledgeDecisionKind,
    pub decided_at: DateTime<Utc>,
}

/// One encrypted Knowledge store; pure decision/projection policy remains Knowledge-owned.
pub trait KnowledgeRepository: Send + Sync {
    fn read_context<'a>(
        &'a self,
        actor: &'a OwnerActor,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryContextSnapshot, AgentFailure>>;
    fn overview<'a>(
        &'a self,
        actor: &'a OwnerActor,
        limit: usize,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryOverviewSnapshot, AgentFailure>>;
    fn review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryReviewSnapshot, AgentFailure>>;
    fn decide<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: MemoryDecisionRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::KnowledgeDecisionResult, CommandFailure<AgentFailure>>>;
    fn stage<'a>(
        &'a self,
        request: MemoryStageRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::KnowledgeCandidate, AgentFailure>>;
}
