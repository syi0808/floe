//! The immutable model input one attempt is made from.
//!
//! An envelope is what a transport may see: the instructions, the bounded
//! context, the conversation already projected, and the manifest that says what
//! went into it. No Session, Task or ledger crosses this boundary.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentCard, CapabilityDescriptor, ContextEvidence, ContextIssue, ContextMemory, DataClass,
    LearningEvidenceRef, prompts::{PromptAssembly, PromptComponentKind},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEnvelope {
    pub schema_version: u32,
    pub stable_instructions: PromptAssembly,
    pub scoped_instructions: ScopedInstructions,
    pub contextual_data: ContextualData,
    pub conversation: ConversationContext,
    pub runtime: RuntimeContext,
    pub manifest: ContextManifest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextualData {
    pub projection_version: u32,
    pub memories: Vec<ContextMemory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_context_issues: Vec<ContextIssue>,
    pub evidence: Vec<ContextEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedInstructions {
    pub purpose: String,
    pub available_capabilities: Vec<CapabilityDescriptor>,
    pub active_experts: Vec<AgentCard>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationContext {
    pub history: Vec<serde_json::Value>,
    pub current_turn: Vec<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeContext {
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextManifest {
    pub prompt_components: Vec<PromptManifestEntry>,
    pub evidence: Vec<EvidenceManifestEntry>,
    pub memories: Vec<MemoryManifestEntry>,
    pub agent_cards: Vec<AgentCardManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryManifestEntry {
    pub target_id: Uuid,
    pub revision: u64,
    pub source_refs: Vec<LearningEvidenceRef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCardManifestEntry {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptManifestEntry {
    pub kind: PromptComponentKind,
    pub source: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceManifestEntry {
    pub source_handle: String,
    pub data_class: DataClass,
    pub expires_at_unix_ms: u64,
}
