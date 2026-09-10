use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentFailure, EpistemicStatus, LearningEvidenceRef, PersonalMemoryKind, SessionProtection,
};

pub const MAX_CONTEXT_MEMORIES: usize = 32;
pub const MAX_CONTEXT_MEMORY_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    Synthetic,
    Personal,
    TemporaryAiContext,
    HighlySensitive,
    DeviceOnlyRaw,
    Credential,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPlacement {
    DeviceLocal,
    Remote,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferConsent {
    NotGranted,
    Granted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePolicyDecision {
    pub purpose: String,
    pub data_classes: Vec<DataClass>,
    pub allowed_placements: Vec<ModelPlacement>,
    pub performance_class: String,
    pub projection_version: u32,
    pub external_transfer_consent: TransferConsent,
    pub bounded_sensitive_projection: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEvidence {
    pub source_handle: String,
    pub data_class: DataClass,
    pub untrusted_text: String,
    pub expires_at_unix_ms: u64,
}

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentContext {
    pub projection_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<crate::PersonaProfile>,
    #[serde(default)]
    pub memories: Vec<ContextMemory>,
    pub evidence: Vec<ContextEvidence>,
}

impl InferencePolicyDecision {
    pub fn authorize(
        &self,
        placement: ModelPlacement,
        protection: SessionProtection,
        context: &AgentContext,
        now_unix_ms: u64,
    ) -> Result<(), AgentFailure> {
        if self.purpose.trim().is_empty()
            || self.performance_class.trim().is_empty()
            || self.projection_version == 0
            || self.projection_version != context.projection_version
            || self.data_classes.is_empty()
            || !self.allowed_placements.contains(&placement)
            || context.evidence.iter().any(|evidence| {
                evidence.source_handle.trim().is_empty()
                    || !self.data_classes.contains(&evidence.data_class)
            })
            || ((context.persona.is_some() || !context.memories.is_empty())
                && !self.data_classes.contains(&DataClass::Personal))
            || self.data_classes.contains(&DataClass::Credential)
            || self.data_classes.contains(&DataClass::DeviceOnlyRaw)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if let Some(persona) = &context.persona {
            persona.validate()?;
        }
        validate_memories(&context.memories, now_unix_ms)?;
        match protection {
            SessionProtection::KeyUnavailable => return Err(AgentFailure::VaultUnavailable),
            SessionProtection::SyntheticOnly
                if self
                    .data_classes
                    .iter()
                    .any(|class| *class != DataClass::Synthetic) =>
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            _ => {}
        }
        if context
            .evidence
            .iter()
            .any(|evidence| evidence.expires_at_unix_ms <= now_unix_ms)
        {
            return Err(AgentFailure::StaleContext);
        }
        if placement == ModelPlacement::Remote {
            if self.external_transfer_consent != TransferConsent::Granted {
                return Err(AgentFailure::ConsentRequired);
            }
            if self.data_classes.contains(&DataClass::HighlySensitive)
                && !self.bounded_sensitive_projection
            {
                return Err(AgentFailure::PolicyDenied);
            }
        }
        Ok(())
    }
}

fn validate_memories(memories: &[ContextMemory], now_unix_ms: u64) -> Result<(), AgentFailure> {
    let now_unix_ms = i64::try_from(now_unix_ms).map_err(|_| AgentFailure::StaleContext)?;
    let mut targets = std::collections::HashSet::new();
    let total_bytes = memories.iter().try_fold(0usize, |total, memory| {
        total
            .checked_add(memory.statement.len())
            .ok_or(AgentFailure::BudgetExceeded)
    })?;
    if memories.len() > MAX_CONTEXT_MEMORIES || total_bytes > MAX_CONTEXT_MEMORY_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    if memories.iter().any(|memory| {
        memory.revision == 0
            || !targets.insert(memory.target_id)
            || memory.statement.trim().is_empty()
            || memory.confidence_millis > 1000
            || memory.source_refs.is_empty()
            || memory
                .source_refs
                .iter()
                .any(|source| source.session_id.is_nil() || source.turn_id.is_nil())
            || memory
                .valid_until_unix_ms
                .zip(memory.valid_from_unix_ms)
                .is_some_and(|(until, from)| until <= from)
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    if memories.iter().any(|memory| {
        memory
            .valid_from_unix_ms
            .is_some_and(|valid_from| valid_from > now_unix_ms)
            || memory
                .valid_until_unix_ms
                .is_some_and(|valid_until| valid_until <= now_unix_ms)
    }) {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}
