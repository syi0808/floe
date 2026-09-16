use chrono::{DateTime, Utc};
use floe_context_contract::DependencyCoverage;
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const KNOWLEDGE_VERSION: u32 = 1;

/// How a confirmed memory reaches an authorized context. The projection and its
/// bounds belong to the context contract; this module owns the record behind it.
pub use floe_context_contract::{
    ContextMemory, EpistemicStatus, LearningEvidenceRef, MAX_CONTEXT_MEMORIES,
    MAX_CONTEXT_MEMORY_BYTES, MemoryContextSnapshot, PersonalMemoryKind,
};
pub const MAX_MEMORY_OVERVIEW_ITEMS: usize = 100;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LearningOutcome {
    Completed,
    Halted { reason: AgentFailure },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningObservationKind {
    ExplicitRemember,
    UserCorrection,
    OutcomeConflict,
    ReusableProcedure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalMemoryValue {
    pub kind: PersonalMemoryKind,
    pub statement: String,
    pub epistemic_status: EpistemicStatus,
    pub confidence_millis: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeActor {
    User,
    Learner { run_id: Uuid },
    Curator,
    System,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageMemoryCandidate {
    pub session_id: Uuid,
    pub expected_session_revision: u64,
    pub turn_ids: Vec<Uuid>,
    pub observation_kind: LearningObservationKind,
    pub digest: String,
    pub value: PersonalMemoryValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    pub extractor_version: String,
    pub prompt_version: String,
    pub actor: KnowledgeActor,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Memory,
    Playbook,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeOperation {
    Create,
    Revise,
    Retire,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCandidateState {
    Pending,
    Approved,
    Rejected,
    Superseded,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRevisionState {
    Active,
    Superseded,
    Stale,
    Archived,
    Tombstoned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgePayload {
    Memory { value: PersonalMemoryValue },
    Playbook { value: crate::Playbook },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeCandidate {
    pub schema_version: u32,
    pub id: Uuid,
    pub person_id: PersonId,
    pub observation_id: Uuid,
    pub idempotency_key: String,
    pub kind: KnowledgeKind,
    pub operation: KnowledgeOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    pub payload: KnowledgePayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    pub after_hash: String,
    pub source_refs: Vec<LearningEvidenceRef>,
    pub extractor_version: String,
    pub prompt_version: String,
    pub actor: KnowledgeActor,
    pub state: KnowledgeCandidateState,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeDecisionKind {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeDecision {
    pub schema_version: u32,
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub decision: KnowledgeDecisionKind,
    pub actor: KnowledgeActor,
    pub decided_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeRevision {
    pub schema_version: u32,
    pub target_id: Uuid,
    pub revision: u64,
    pub person_id: PersonId,
    pub kind: KnowledgeKind,
    pub payload: KnowledgePayload,
    pub state: KnowledgeRevisionState,
    pub source_refs: Vec<LearningEvidenceRef>,
    pub created_by: KnowledgeActor,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOrigin {
    UserProvided,
    Learned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySummary {
    pub target_id: Uuid,
    pub revision: u64,
    pub statement: String,
    pub memory_kind: PersonalMemoryKind,
    pub epistemic_status: EpistemicStatus,
    pub confidence_millis: u16,
    pub source_count: usize,
    pub origin: MemoryOrigin,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryOverviewSnapshot {
    pub person_id: PersonId,
    pub saved_count: usize,
    pub pending_count: usize,
    pub memories: Vec<MemorySummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReviewSnapshot {
    pub person_id: PersonId,
    pub candidates: Vec<KnowledgeCandidate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeMutation {
    pub schema_version: u32,
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub target_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_revision: Option<u64>,
    pub to_revision: u64,
    pub actor: KnowledgeActor,
    pub operation: KnowledgeOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    pub after_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_revision: Option<u64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeDecisionResult {
    pub candidate: KnowledgeCandidate,
    pub decision: KnowledgeDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<KnowledgeRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation: Option<KnowledgeMutation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningEvidenceSnapshot {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub revision: u64,
    pub outcome: Option<LearningOutcome>,
    pub personal: bool,
    pub active_turn: bool,
    pub pending_output: bool,
    pub turn_ids: Vec<Uuid>,
    pub coverage: DependencyCoverage,
    pub purpose: EvidenceProjectionPurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceProjectionPurpose {
    Learning,
    Context,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningObservation {
    pub schema_version: u32,
    pub id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub evidence: Vec<LearningEvidenceRef>,
    pub outcome: LearningOutcome,
    pub kind: LearningObservationKind,
    pub digest: String,
    pub observed_at: DateTime<Utc>,
    pub content_hash: String,
}
