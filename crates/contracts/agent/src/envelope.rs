//! The immutable model input one attempt is made from.
//!
//! An envelope is what a transport may see: the instructions, the bounded
//! context, the conversation already projected, and the manifest that says what
//! went into it. No Session, Task or ledger crosses this boundary.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentCard, AgentFailure, CapabilityDescriptor, ContextEvidence, ContextIssue, ContextMemory,
    DataClass, LearningEvidenceRef, ModelConversation, ModelCorrection,
    prompts::{PromptAssembly, PromptComponentKind},
};

pub const MAX_SCOPED_PURPOSE_BYTES: usize = 512;
pub const MAX_RESPONSE_CONTRACT_BYTES: usize = 8192;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEnvelope {
    pub schema_version: u32,
    pub stable_instructions: PromptAssembly,
    pub scoped_instructions: ScopedInstructions,
    pub contextual_data: ContextualData,
    pub conversation: ModelConversation,
    pub runtime: RuntimeContext,
    pub manifest: ContextManifest,
}

impl ContextEnvelope {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != crate::AGENT_SCHEMA_VERSION {
            return Err(AgentFailure::InvalidInput);
        }
        self.stable_instructions.validate()?;
        self.scoped_instructions.validate()?;
        self.conversation.validate()?;
        if self.runtime.max_output_bytes == 0
            || self.runtime.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
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
    pub response_contract: String,
    pub available_capabilities: Vec<CapabilityDescriptor>,
    pub active_experts: Vec<AgentCard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<ModelCorrection>,
}

impl ScopedInstructions {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.purpose.trim().is_empty()
            || self.purpose.len() > MAX_SCOPED_PURPOSE_BYTES
            || self.response_contract.len() > MAX_RESPONSE_CONTRACT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        // An empty response contract means the legacy envelope path, which has no
        // separate role output contract; canonical projections always set it from
        // the role spec.
        self.active_experts
            .iter()
            .try_for_each(AgentCard::validate)?;
        if let Some(correction) = &self.correction {
            correction.validate()?;
        }
        Ok(())
    }
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
