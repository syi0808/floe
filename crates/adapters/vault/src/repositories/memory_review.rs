//! The Person's memory candidates, as their vault holds them.
//!
//! Knowledge decides which candidate may be decided and by whom; this reads the
//! pending candidates and commits one decision in a single transaction.

use chrono::{DateTime, Utc};
use floe_agent_contract::AgentFailure;
use floe_knowledge::{
    KnowledgeActor, KnowledgeDecisionKind, KnowledgeDecisionResult, MemoryReviewRepository,
    MemoryReviewSnapshot,
};

use crate::{EncryptedAgentVault, VaultKeyProvider};

impl<Keys: VaultKeyProvider> MemoryReviewRepository for EncryptedAgentVault<Keys> {
    async fn memory_review_snapshot(&self) -> Result<MemoryReviewSnapshot, AgentFailure> {
        EncryptedAgentVault::memory_review_snapshot(self).await
    }

    async fn decide_memory_candidate(
        &self,
        candidate_id: uuid::Uuid,
        kind: KnowledgeDecisionKind,
        actor: KnowledgeActor,
        decided_at: DateTime<Utc>,
    ) -> Result<KnowledgeDecisionResult, AgentFailure> {
        self.decide_knowledge_candidate(candidate_id, kind, actor, decided_at)
            .await
    }
}
