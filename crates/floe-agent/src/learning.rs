use chrono::{DateTime, Utc};
use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AgentOutcome;

pub use floe_knowledge::{
    EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate,
    KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind, KnowledgeDecisionResult,
    KnowledgeKind, KnowledgeMutation, KnowledgeOperation, KnowledgePayload, KnowledgeRevision,
    KnowledgeRevisionState, LearningEvidenceRef, LearningObservationKind, PersonalMemoryKind,
    PersonalMemoryValue, StageMemoryCandidate,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningObservation {
    pub schema_version: u32,
    pub id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub evidence: Vec<LearningEvidenceRef>,
    pub outcome: AgentOutcome,
    pub kind: LearningObservationKind,
    pub digest: String,
    pub observed_at: DateTime<Utc>,
    pub content_hash: String,
}
