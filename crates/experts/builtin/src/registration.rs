use floe_agent_contract::{AGENT_VERSION, AgentCard, AgentDefinition, PackageKind, PackageRef};
use floe_experts::{
    ContractRef, EXPERT_MANIFEST_SCHEMA_VERSION, ExpertManifest, ExpertRegistration,
    ExpertSourceRequirement,
};

use crate::{BuiltinExpertHost, BuiltinExpertKind, BuiltinExpertOutput, BuiltinExpertRequest};

#[derive(Clone, Copy)]
pub struct BuiltinExpertRunner(BuiltinExpertKind);

impl BuiltinExpertRunner {
    pub fn manifest(self) -> ExpertManifest {
        manifest(self.0)
    }

    pub async fn run<Host: BuiltinExpertHost>(
        self,
        host: &Host,
        request: &BuiltinExpertRequest,
    ) -> Result<BuiltinExpertOutput, floe_agent_contract::AgentFailure> {
        match self.0 {
            BuiltinExpertKind::Schedule => crate::schedule::dispatch::dispatch(host, request).await,
            BuiltinExpertKind::Commitments => crate::commitments::dispatch(host, request).await,
            BuiltinExpertKind::Communication => crate::communication::dispatch(host, request).await,
            BuiltinExpertKind::Relationships => crate::relationships::dispatch(host, request).await,
            BuiltinExpertKind::FocusAttention => {
                crate::focus_attention::dispatch(host, request).await
            }
            BuiltinExpertKind::Wellbeing => crate::wellbeing::dispatch(host, request).await,
            BuiltinExpertKind::WorkContext => crate::work_context::dispatch(host, request).await,
            BuiltinExpertKind::LifeLogistics => {
                crate::life_logistics::dispatch(host, request).await
            }
        }
    }
}

pub fn registrations() -> Vec<ExpertRegistration<BuiltinExpertRunner>> {
    BuiltinExpertKind::ALL
        .into_iter()
        .map(|kind| ExpertRegistration {
            manifest: manifest(kind),
            runner: BuiltinExpertRunner(kind),
        })
        .collect()
}

pub fn manifests() -> Vec<ExpertManifest> {
    BuiltinExpertKind::ALL.into_iter().map(manifest).collect()
}

fn manifest(kind: BuiltinExpertKind) -> ExpertManifest {
    let (name, description, domain_tags, skill) = kind.metadata();
    let package = PackageRef {
        kind: PackageKind::Expert,
        id: kind.package_id().into(),
        version: crate::BUILTIN_EXPERT_PACKAGE_VERSION.into(),
    };
    let definition = AgentDefinition {
        card: AgentCard {
            schema_version: AGENT_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: package.id.clone(),
            version: package.version.clone(),
            name: name.into(),
            description: description.into(),
            domain_tags: domain_tags.into_iter().map(str::to_owned).collect(),
            skills: vec![skill.into()],
            supported_placements: if kind.supports_device_model() {
                vec![
                    floe_agent_contract::ModelPlacement::DeviceLocal,
                    floe_agent_contract::ModelPlacement::Remote,
                ]
            } else {
                vec![floe_agent_contract::ModelPlacement::Remote]
            },
        },
        definition_revision: 1,
    };
    ExpertManifest {
        schema_version: EXPERT_MANIFEST_SCHEMA_VERSION,
        package,
        publisher: crate::BUILTIN_EXPERT_PUBLISHER.into(),
        definition,
        data_class: kind.context_data_class(),
        prompt_contract: ContractRef {
            id: format!("{}.prompt", kind.package_id()),
            revision: 1,
        },
        result_contracts: vec![ContractRef {
            id: format!("{}.result", kind.package_id()),
            revision: 1,
        }],
        source_requirements: kind
            .required_sources()
            .iter()
            .map(|source| ExpertSourceRequirement {
                key: source.source_id().into(),
                capability: source.capability_id().into(),
                contract_version: 1,
                minimum_sources: u8::from(*source == kind.mandatory_source()),
                maximum_sources: match source.capability_id() {
                    "calendar.timeline" | "mail.communication" | "work.context"
                    | "life.logistics" => floe_experts::MAX_REQUIREMENT_SOURCES,
                    _ => 1,
                },
            })
            .collect(),
        capability_requirements: vec![],
        state_schema_version: crate::BUILTIN_EXPERT_STATE_SCHEMA_VERSION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_manifests_are_valid_and_unique() {
        let manifests = manifests();
        assert_eq!(manifests.len(), 8);
        assert_eq!(
            floe_experts::manifest_set_digest(&manifests).unwrap().len(),
            64
        );
        for (index, manifest) in manifests.iter().enumerate() {
            manifest.validate().unwrap();
            assert!(
                manifests[..index]
                    .iter()
                    .all(|other| other.package != manifest.package)
            );
        }
    }

    #[test]
    fn communication_requirement_is_declared_by_shipped_packages_only() {
        let consumers: Vec<_> = manifests()
            .into_iter()
            .filter(|manifest| {
                manifest
                    .source_requirements
                    .iter()
                    .any(|requirement| requirement.capability == "mail.communication")
            })
            .map(|manifest| manifest.package.id)
            .collect();
        assert_eq!(
            consumers,
            ["floe.builtin.commitments", "floe.builtin.communication"]
        );
    }
}
