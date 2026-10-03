use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum MemoryStageOrigin {
    User,
    Learner { claim: crate::LearnerClaimRef, journal_revision: u64, journal_digest: [u8; 32] },
}

#[derive(Clone, Debug)]
pub struct MemoryStageRequest {
    pub actor: OwnerActor,
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
    fn read_context<'a>(&'a self, actor: &'a OwnerActor, now: DateTime<Utc>, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryContextSnapshot, AgentFailure>>;
    fn overview<'a>(&'a self, actor: &'a OwnerActor, limit: usize, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryOverviewSnapshot, AgentFailure>>;
    fn review<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryReviewSnapshot, AgentFailure>>;
    fn decide<'a>(&'a self, actor: &'a OwnerActor, request: MemoryDecisionRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::KnowledgeDecisionResult, AgentFailure>>;
    fn stage<'a>(&'a self, request: MemoryStageRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::KnowledgeCandidate, AgentFailure>>;
}
