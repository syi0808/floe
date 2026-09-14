use chrono::{DateTime, Utc};
use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const KNOWLEDGE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningObservationKind {
    ExplicitRemember,
    UserCorrection,
    OutcomeConflict,
    ReusableProcedure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalMemoryKind {
    Fact,
    Observation,
    Inference,
    Preference,
    Commitment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicStatus {
    Fact,
    Inference,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningEvidenceSnapshot {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub revision: u64,
    pub completed: bool,
    pub personal: bool,
    pub active_turn: bool,
    pub pending_output: bool,
    pub turn_ids: Vec<Uuid>,
}
