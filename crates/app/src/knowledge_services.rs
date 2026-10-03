//! Mechanical forwarding to the admitted Knowledge owner.
use crate::{AppComposition, CallerContext};
use floe_kernel::{AgentFailure, CommandId};
use uuid::Uuid;

pub enum KnowledgeQuery { Overview { limit: usize }, Review }
pub enum KnowledgeQueryResult {
    Overview(floe_knowledge::MemoryOverviewSnapshot),
    Review(floe_knowledge::MemoryReviewSnapshot),
}
pub trait KnowledgeCommands {
    fn knowledge_decide(&self, caller: &CallerContext, command_id: Uuid, candidate_id: Uuid,
        decision: floe_knowledge::KnowledgeDecisionKind) -> Result<floe_knowledge::KnowledgeDecisionResult, AgentFailure>;
}
pub trait KnowledgeQueries {
    fn knowledge_query(&self, caller: &CallerContext, request_id: Uuid, query: KnowledgeQuery)
        -> Result<KnowledgeQueryResult, AgentFailure>;
}
impl KnowledgeCommands for AppComposition {
    fn knowledge_decide(&self, caller: &CallerContext, command_id: Uuid, candidate_id: Uuid,
        decision: floe_knowledge::KnowledgeDecisionKind) -> Result<floe_knowledge::KnowledgeDecisionResult, AgentFailure> {
        let command_id = CommandId::from_uuid(command_id).ok_or(AgentFailure::InvalidInput)?;
        let owners = self.ready_owners(caller)?;
        let actor = caller.owner_actor();
        let scope = crate::host_scope(command_id.as_uuid(), floe_execution::Cancellation::new(), std::time::Duration::from_secs(35));
        self.execute_owner(owners.knowledge.decide(&actor, command_id, candidate_id, decision, &scope))
    }
}
impl KnowledgeQueries for AppComposition {
    fn knowledge_query(&self, caller: &CallerContext, request_id: Uuid, query: KnowledgeQuery)
        -> Result<KnowledgeQueryResult, AgentFailure> {
        if request_id.is_nil() { return Err(AgentFailure::InvalidInput); }
        let owners = self.ready_owners(caller)?;
        let actor = caller.owner_actor();
        let scope = crate::host_scope(request_id, floe_execution::Cancellation::new(), std::time::Duration::from_secs(35));
        self.execute_owner(async {
            match query {
                KnowledgeQuery::Overview { limit } => owners.knowledge.overview(&actor, limit, &scope).await.map(KnowledgeQueryResult::Overview),
                KnowledgeQuery::Review => owners.knowledge.review(&actor, &scope).await.map(KnowledgeQueryResult::Review),
            }
        })
    }
}
