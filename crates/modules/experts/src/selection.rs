use floe_agent_contract::AgentFailure;
use floe_context_contract::SourceSelectionReference;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ExpertBindingState, ExpertManifest};

pub const EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertExecutionSelection {
    pub schema_version: u32,
    pub binding_revision: u64,
    pub requirements: Vec<AdmittedRequirementSelection>,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedRequirementSelection {
    pub key: String,
    pub capability: String,
    pub contract_version: u32,
    pub minimum_sources: u8,
    pub maximum_sources: u8,
    pub selected: Vec<SourceSelectionReference>,
}

impl ExpertExecutionSelection {
    pub fn without_requirements(binding_revision: u64) -> Result<Self, AgentFailure> {
        let selection = Self {
            schema_version: EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION,
            binding_revision,
            requirements: vec![],
            digest: Self::digest(binding_revision, &[])?,
        };
        selection.validate()?;
        Ok(selection)
    }

    pub fn from_binding(
        manifest: &ExpertManifest,
        binding: &ExpertBindingState,
    ) -> Result<Self, AgentFailure> {
        manifest.validate()?;
        if binding.revision == 0 || binding.entries.len() != manifest.source_requirements.len() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut requirements = manifest
            .source_requirements
            .iter()
            .map(|requirement| {
                let entry = binding
                    .entries
                    .iter()
                    .find(|entry| entry.requirement_key == requirement.key)
                    .ok_or(AgentFailure::InvalidInput)?;
                if entry.capability != requirement.capability
                    || entry.contract_version != requirement.contract_version
                {
                    return Err(AgentFailure::InvalidInput);
                }
                Ok(AdmittedRequirementSelection {
                    key: requirement.key.clone(),
                    capability: requirement.capability.clone(),
                    contract_version: requirement.contract_version,
                    minimum_sources: requirement.minimum_sources,
                    maximum_sources: requirement.maximum_sources,
                    selected: entry.selected.clone(),
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        requirements.sort_by(|left, right| left.key.cmp(&right.key));
        let digest = Self::digest(binding.revision, &requirements)?;
        let selection = Self {
            schema_version: EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION,
            binding_revision: binding.revision,
            requirements,
            digest,
        };
        selection.validate()?;
        Ok(selection)
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != EXPERT_EXECUTION_SELECTION_SCHEMA_VERSION
            || self.binding_revision == 0
            || self.requirements.len() > 32
            || self
                .requirements
                .windows(2)
                .any(|pair| pair[0].key >= pair[1].key)
            || self.requirements.iter().any(|requirement| {
                requirement.key.is_empty()
                    || requirement.capability.is_empty()
                    || requirement.contract_version == 0
                    || requirement.minimum_sources > requirement.maximum_sources
                    || requirement.selected.len() > usize::from(requirement.maximum_sources)
                    || requirement
                        .selected
                        .windows(2)
                        .any(|pair| pair[0] >= pair[1])
                    || requirement.selected.iter().any(|source| {
                        source.validate().is_err()
                            || source.capability_id != requirement.capability
                            || source.contract_version != requirement.contract_version
                    })
            })
            || self.digest != Self::digest(self.binding_revision, &self.requirements)?
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    fn digest(
        revision: u64,
        requirements: &[AdmittedRequirementSelection],
    ) -> Result<[u8; 32], AgentFailure> {
        let bytes = serde_json::to_vec(&(
            "floe.expert-execution-selection.sha256.v1",
            revision,
            requirements,
        ))
        .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(Sha256::digest(bytes).into())
    }
}
