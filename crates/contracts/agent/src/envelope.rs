//! The immutable model input one attempt is made from.
//!
//! An envelope is what a transport may see: the instructions, the bounded
//! context, the conversation already projected, and the manifest that says what
//! went into it. No Session, Task or ledger crosses this boundary.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentDefinition, AgentFailure, CapabilityDescriptor, ContextEvidence, ContextIssue,
    ContextMemory, DataClass, LearningEvidenceRef, ModelConversation, ModelCorrection,
    prompts::{PromptAssembly, PromptComponentKind},
};

pub const MAX_SCOPED_PURPOSE_BYTES: usize = 512;
pub const MAX_RESPONSE_CONTRACT_BYTES: usize = 8192;
pub const CONTEXT_ENVELOPE_SCHEMA_VERSION: u32 = 2;

pub fn content_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEnvelope {
    pub schema_version: u32,
    pub stable_instructions: PromptAssembly,
    pub run_instructions: RunInstructions,
    pub discovery: DiscoveryContext,
    pub contextual_data: ContextualData,
    pub conversation: ModelConversation,
    pub attempt: AttemptContext,
    pub manifest: ContextManifest,
}

impl ContextEnvelope {
    pub fn run_frame_sha256(&self) -> Result<String, AgentFailure> {
        Ok(content_sha256(self.canonical_run_frame_json()?.as_bytes()))
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != CONTEXT_ENVELOPE_SCHEMA_VERSION {
            return Err(AgentFailure::InvalidInput);
        }
        self.stable_instructions.validate()?;
        self.run_instructions.validate()?;
        self.discovery.validate()?;
        self.conversation.validate()?;
        if let Some(correction) = &self.attempt.correction {
            correction.validate()?;
        }
        if self.attempt.max_output_bytes == 0
            || self.attempt.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.manifest != self.derived_manifest(self.manifest.expert_environment.clone())? {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn canonical_run_frame_json(&self) -> Result<String, AgentFailure> {
        #[derive(Serialize)]
        struct RunFrame<'a> {
            run_instructions: &'a RunInstructions,
            discovery: &'a DiscoveryContext,
        }
        serde_json::to_string(&RunFrame {
            run_instructions: &self.run_instructions,
            discovery: &self.discovery,
        })
        .map_err(|_| AgentFailure::InvalidInput)
    }

    pub fn canonical_attempt_frame_json(&self) -> Result<String, AgentFailure> {
        #[derive(Serialize)]
        struct AttemptFrame<'a> {
            contextual_data: &'a ContextualData,
            attempt: &'a AttemptContext,
            manifest: &'a ContextManifest,
        }
        serde_json::to_string(&AttemptFrame {
            contextual_data: &self.contextual_data,
            attempt: &self.attempt,
            manifest: &self.manifest,
        })
        .map_err(|_| AgentFailure::InvalidInput)
    }

    pub fn derived_manifest(
        &self,
        expert_environment: Option<ExpertEnvironmentManifestEntry>,
    ) -> Result<ContextManifest, AgentFailure> {
        if let Some(environment) = &expert_environment {
            if environment.digest == [0; 32] || environment.revision != self.discovery.revision {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(ContextManifest {
            prompt_components: self
                .stable_instructions
                .components
                .iter()
                .map(|component| PromptManifestEntry {
                    kind: component.kind,
                    source: component.source.clone(),
                    revision: component.revision,
                    content_sha256: component.content_sha256(),
                })
                .collect(),
            stable_prompt_sha256: self.stable_instructions.stable_prompt_sha256(),
            run_frame_sha256: self.run_frame_sha256()?,
            expert_environment,
            agent_cards: self
                .discovery
                .active_experts
                .iter()
                .map(|definition| {
                    Ok(AgentCardManifestEntry {
                        id: definition.card.id.clone(),
                        version: definition.card.version.clone(),
                        definition_revision: definition.definition_revision,
                        card_sha256: definition.card.card_sha256()?,
                    })
                })
                .collect::<Result<_, AgentFailure>>()?,
            evidence: self
                .contextual_data
                .evidence
                .iter()
                .map(|evidence| EvidenceManifestEntry {
                    source_handle: evidence.source_handle.clone(),
                    data_class: evidence.data_class,
                    expires_at_unix_ms: evidence.expires_at_unix_ms,
                })
                .collect(),
            memories: self
                .contextual_data
                .memories
                .iter()
                .map(|memory| MemoryManifestEntry {
                    target_id: memory.target_id,
                    revision: memory.revision,
                    source_refs: memory.source_refs.clone(),
                })
                .collect(),
        })
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
pub struct RunInstructions {
    pub purpose: String,
    pub response_contract: String,
    pub output_format: crate::ModelOutputFormat,
}

impl RunInstructions {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.output_format.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if self.purpose.trim().is_empty()
            || self.purpose.len() > MAX_SCOPED_PURPOSE_BYTES
            || self.response_contract.len() > MAX_RESPONSE_CONTRACT_BYTES
            || self.response_contract.trim().is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptContext {
    pub correction: Option<ModelCorrection>,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryContext {
    pub revision: u64,
    pub available_capabilities: Vec<CapabilityDescriptor>,
    pub active_experts: Vec<AgentDefinition>,
}

impl DiscoveryContext {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.available_capabilities
            .iter()
            .try_for_each(CapabilityDescriptor::validate)?;
        self.active_experts
            .iter()
            .try_for_each(AgentDefinition::validate)?;
        if self
            .available_capabilities
            .windows(2)
            .any(|pair| pair[0].id >= pair[1].id)
            || self
                .active_experts
                .windows(2)
                .any(|pair| pair[0].card.id >= pair[1].card.id)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertEnvironmentManifestEntry {
    pub revision: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextManifest {
    pub stable_prompt_sha256: String,
    pub run_frame_sha256: String,
    pub expert_environment: Option<ExpertEnvironmentManifestEntry>,
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
    pub definition_revision: u64,
    pub card_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptManifestEntry {
    pub kind: PromptComponentKind,
    pub source: String,
    pub revision: u64,
    pub content_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceManifestEntry {
    pub source_handle: String,
    pub data_class: DataClass,
    pub expires_at_unix_ms: u64,
}
