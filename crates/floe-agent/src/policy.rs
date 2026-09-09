use serde::{Deserialize, Serialize};

use crate::{AgentFailure, SessionProtection};

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
pub struct AgentContext {
    pub projection_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<crate::PersonaProfile>,
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
            || (context.persona.is_some() && !self.data_classes.contains(&DataClass::Personal))
            || self.data_classes.contains(&DataClass::Credential)
            || self.data_classes.contains(&DataClass::DeviceOnlyRaw)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if let Some(persona) = &context.persona {
            persona.validate()?;
        }
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
