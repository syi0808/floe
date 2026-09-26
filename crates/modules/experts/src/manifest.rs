use std::collections::HashSet;

use floe_agent_contract::{AgentDefinition, AgentFailure, DataClass, PackageKind, PackageRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EXPERT_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MAX_REQUIREMENT_SOURCES: u8 = 16;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractRef {
    pub id: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertSourceRequirement {
    pub key: String,
    pub capability: String,
    pub minimum_sources: u8,
    pub maximum_sources: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertManifest {
    pub schema_version: u32,
    pub package: PackageRef,
    pub publisher: String,
    pub definition: AgentDefinition,
    pub data_class: DataClass,
    pub prompt_contract: ContractRef,
    pub result_contracts: Vec<ContractRef>,
    pub source_requirements: Vec<ExpertSourceRequirement>,
    pub capability_requirements: Vec<String>,
    pub state_schema_version: u32,
}

impl ExpertManifest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != EXPERT_MANIFEST_SCHEMA_VERSION
            || self.package.kind != PackageKind::Expert
            || self.package.id != self.definition.card.id
            || self.package.version != self.definition.card.version
            || !bounded_identifier(&self.publisher)
            || matches!(
                self.data_class,
                DataClass::Credential | DataClass::DeviceOnlyRaw
            )
            || self.state_schema_version == 0
            || self.result_contracts.len() > 16
            || self.source_requirements.len() > 32
            || self.capability_requirements.len() > 32
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.definition.validate()?;
        let valid_contract =
            |contract: &ContractRef| bounded_identifier(&contract.id) && contract.revision > 0;
        if !valid_contract(&self.prompt_contract)
            || self
                .result_contracts
                .iter()
                .any(|contract| !valid_contract(contract))
            || self
                .result_contracts
                .iter()
                .enumerate()
                .any(|(index, contract)| {
                    self.result_contracts[..index]
                        .iter()
                        .any(|other| other.id == contract.id)
                })
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut keys = HashSet::new();
        for requirement in &self.source_requirements {
            if !bounded_identifier(&requirement.key)
                || !bounded_identifier(&requirement.capability)
                || requirement.minimum_sources > requirement.maximum_sources
                || requirement.maximum_sources > MAX_REQUIREMENT_SOURCES
                || !keys.insert(&requirement.key)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        let mut capabilities = HashSet::new();
        if self
            .capability_requirements
            .iter()
            .any(|capability| !bounded_identifier(capability) || !capabilities.insert(capability))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

pub fn manifest_set_digest(manifests: &[ExpertManifest]) -> Result<String, AgentFailure> {
    if manifests.is_empty() || manifests.len() > 64 {
        return Err(AgentFailure::InvalidInput);
    }
    let mut canonical = manifests.to_vec();
    canonical.sort_by(|left, right| {
        left.package
            .id
            .cmp(&right.package.id)
            .then(left.package.version.cmp(&right.package.version))
    });
    for (index, manifest) in canonical.iter().enumerate() {
        manifest.validate()?;
        if index > 0 && canonical[index - 1].package == manifest.package {
            return Err(AgentFailure::Conflict);
        }
    }
    let bytes = serde_json::to_vec(&("floe.expert-manifest-set.sha256.v1", canonical))
        .map_err(|_| AgentFailure::InvalidInput)?;
    if bytes.len() > 65_536 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub struct ExpertRegistration<Runner> {
    pub manifest: ExpertManifest,
    pub runner: Runner,
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::{A2A_PROTOCOL_VERSION, AGENT_VERSION, AgentCard, ModelPlacement};

    fn manifest() -> ExpertManifest {
        ExpertManifest {
            schema_version: EXPERT_MANIFEST_SCHEMA_VERSION,
            package: PackageRef {
                kind: PackageKind::Expert,
                id: "example.test.expert".into(),
                version: "1.0.0".into(),
            },
            publisher: "example.test".into(),
            definition: AgentDefinition {
                card: AgentCard {
                    schema_version: AGENT_VERSION,
                    protocol_version: A2A_PROTOCOL_VERSION.into(),
                    id: "example.test.expert".into(),
                    version: "1.0.0".into(),
                    name: "Test Expert".into(),
                    description: "A test Expert".into(),
                    domain_tags: vec![],
                    skills: vec![],
                    supported_placements: vec![ModelPlacement::DeviceLocal],
                },
                definition_revision: 1,
            },
            data_class: DataClass::Personal,
            prompt_contract: ContractRef {
                id: "example.test.prompt".into(),
                revision: 1,
            },
            result_contracts: vec![],
            source_requirements: vec![],
            capability_requirements: vec![],
            state_schema_version: 1,
        }
    }

    #[test]
    fn zero_source_manifest_is_valid() {
        manifest().validate().unwrap();
    }

    #[test]
    fn source_cardinality_and_keys_are_bounded() {
        let mut manifest = manifest();
        manifest.source_requirements.push(ExpertSourceRequirement {
            key: "calendar".into(),
            capability: "calendar.timeline".into(),
            minimum_sources: 0,
            maximum_sources: 2,
        });
        manifest.validate().unwrap();
        manifest
            .source_requirements
            .push(manifest.source_requirements[0].clone());
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
        manifest.source_requirements.pop();
        manifest.source_requirements[0].minimum_sources = 3;
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
        manifest.source_requirements[0].minimum_sources = 0;
        manifest.source_requirements[0].maximum_sources = MAX_REQUIREMENT_SOURCES + 1;
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
    }

    #[test]
    fn package_and_definition_identity_must_match() {
        let mut manifest = manifest();
        manifest.definition.card.version = "2.0.0".into();
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
        manifest.definition.card.version = "1.0.0".into();
        manifest.package.kind = PackageKind::Tool;
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
    }

    #[test]
    fn manifest_rejects_raw_authority_and_duplicate_result_identity() {
        let mut manifest = manifest();
        manifest.data_class = DataClass::Credential;
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
        manifest.data_class = DataClass::Personal;
        manifest.result_contracts = vec![
            ContractRef {
                id: "result".into(),
                revision: 1,
            },
            ContractRef {
                id: "result".into(),
                revision: 2,
            },
        ];
        assert_eq!(manifest.validate(), Err(AgentFailure::InvalidInput));
    }

    #[test]
    fn canonical_manifest_digest_binds_contents_not_input_order() {
        let first = manifest();
        let mut second = manifest();
        second.package.id = "example.test.other".into();
        second.definition.card.id = second.package.id.clone();
        let digest = manifest_set_digest(&[first.clone(), second.clone()]).unwrap();
        assert_eq!(
            digest,
            manifest_set_digest(&[second.clone(), first.clone()]).unwrap()
        );
        assert_eq!(
            manifest_set_digest(&[first.clone(), first]),
            Err(AgentFailure::Conflict)
        );
        second.prompt_contract.revision += 1;
        assert_ne!(digest, manifest_set_digest(&[second]).unwrap());
    }
}
