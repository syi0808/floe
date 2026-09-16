//! Confirmed memory as it appears inside an authorized context.
//!
//! Knowledge owns whether a memory exists, what it says and who may review it.
//! What crosses into a context is this bounded projection of it, so its shape
//! and its bounds belong here beside the source views.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ContextIssueReason;

pub const MAX_CONTEXT_MEMORIES: usize = 32;
pub const MAX_CONTEXT_MEMORY_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextMemory {
    pub target_id: Uuid,
    pub revision: u64,
    pub kind: PersonalMemoryKind,
    pub statement: String,
    pub epistemic_status: EpistemicStatus,
    pub confidence_millis: u16,
    pub observed_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until_unix_ms: Option<i64>,
    pub source_refs: Vec<LearningEvidenceRef>,
}

/// The memories one turn may see, and why any are missing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryContextSnapshot {
    pub memories: Vec<ContextMemory>,
    pub issue: Option<ContextIssueReason>,
}

/// The turn a memory was learned from.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningEvidenceRef {
    pub session_id: Uuid,
    pub turn_id: Uuid,
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
