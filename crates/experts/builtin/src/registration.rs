use floe_agent_contract::{AGENT_VERSION, AgentCard, AgentDefinition, PackageKind, PackageRef};
use floe_experts::{
    ContractRef, EXPERT_MANIFEST_SCHEMA_VERSION, ExpertManifest, ExpertRegistration, ExpertRun,
    ExpertSourceRequirement,
};

use crate::{BuiltinExpertHost, BuiltinExpertKind, BuiltinExpertOutput, BuiltinExpertRequest};

pub fn registrations<Host: BuiltinExpertHost>()
-> Vec<ExpertRegistration<ExpertRun<Host, BuiltinExpertRequest, BuiltinExpertOutput>>> {
    BuiltinExpertKind::ALL
        .into_iter()
        .map(|kind| ExpertRegistration {
            manifest: manifest(kind),
            runner: runner(kind),
        })
        .collect()
}

fn runner<Host: BuiltinExpertHost>(
    kind: BuiltinExpertKind,
) -> ExpertRun<Host, BuiltinExpertRequest, BuiltinExpertOutput> {
    match kind {
        BuiltinExpertKind::Schedule => {
            |host, request| Box::pin(crate::schedule::dispatch::dispatch(host, request))
        }
        BuiltinExpertKind::Commitments => {
            |host, request| Box::pin(crate::commitments::dispatch(host, request))
        }
        BuiltinExpertKind::Communication => {
            |host, request| Box::pin(crate::communication::dispatch(host, request))
        }
        BuiltinExpertKind::Relationships => {
            |host, request| Box::pin(crate::relationships::dispatch(host, request))
        }
        BuiltinExpertKind::FocusAttention => {
            |host, request| Box::pin(crate::focus_attention::dispatch(host, request))
        }
        BuiltinExpertKind::Wellbeing => {
            |host, request| Box::pin(crate::wellbeing::dispatch(host, request))
        }
        BuiltinExpertKind::WorkContext => {
            |host, request| Box::pin(crate::work_context::dispatch(host, request))
        }
        BuiltinExpertKind::LifeLogistics => {
            |host, request| Box::pin(crate::life_logistics::dispatch(host, request))
        }
    }
}

pub fn manifests() -> Vec<ExpertManifest> {
    BuiltinExpertKind::ALL.into_iter().map(manifest).collect()
}

fn manifest(kind: BuiltinExpertKind) -> ExpertManifest {
    let declaration = kind.declaration();
    let package = PackageRef {
        kind: PackageKind::Expert,
        id: declaration.expert_id.into(),
        version: declaration.version.into(),
    };
    let definition = AgentDefinition {
        card: AgentCard {
            schema_version: AGENT_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: package.id.clone(),
            version: package.version.clone(),
            name: declaration.name.into(),
            description: declaration.description.into(),
            domain_tags: declaration.domain_tags,
            skills: declaration.skills,
            supported_placements: declaration.supported_placements,
        },
        definition_revision: 1,
    };
    ExpertManifest {
        schema_version: EXPERT_MANIFEST_SCHEMA_VERSION,
        package,
        publisher: declaration.publisher.into(),
        definition,
        data_class: declaration.data_class,
        prompt_contract: ContractRef {
            id: format!("{}.prompt", declaration.expert_id),
            revision: 1,
        },
        result_contracts: vec![ContractRef {
            id: format!("{}.result", declaration.expert_id),
            revision: 1,
        }],
        source_requirements: kind
            .required_sources()
            .iter()
            .map(|source| ExpertSourceRequirement {
                key: source.source_id().into(),
                capability: source.capability_id().into(),
                minimum_sources: u8::from(*source == kind.mandatory_source()),
                maximum_sources: 1,
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
