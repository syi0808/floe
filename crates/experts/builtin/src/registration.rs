use floe_agent_contract::{AGENT_VERSION, AgentCard, AgentDefinition, PackageKind, PackageRef};
use floe_experts::{
    ContractRef, EXPERT_MANIFEST_SCHEMA_VERSION, ExpertManifest, ExpertRegistration,
    ExpertSourceRequirement,
};

use crate::BuiltinExpertKind;
use floe_experts::ExpertProgram;
use std::sync::Arc;

pub fn registrations() -> Vec<ExpertRegistration<Arc<dyn ExpertProgram>>> {
    BuiltinExpertKind::ALL
        .into_iter()
        .map(|kind| {
            let program: Arc<dyn ExpertProgram> = match kind {
                BuiltinExpertKind::Schedule => Arc::new(crate::schedule::ScheduleProgram),
                BuiltinExpertKind::Commitments => Arc::new(crate::commitments::CommitmentsProgram),
                BuiltinExpertKind::Communication => {
                    Arc::new(crate::communication::CommunicationProgram)
                }
                BuiltinExpertKind::Relationships => {
                    Arc::new(crate::relationships::RelationshipsProgram)
                }
                BuiltinExpertKind::FocusAttention => {
                    Arc::new(crate::focus_attention::FocusAttentionProgram)
                }
                BuiltinExpertKind::Wellbeing => Arc::new(crate::wellbeing::WellbeingProgram),
                BuiltinExpertKind::WorkContext => Arc::new(crate::work_context::WorkContextProgram),
                BuiltinExpertKind::LifeLogistics => {
                    Arc::new(crate::life_logistics::LifeLogisticsProgram)
                }
            };
            ExpertRegistration {
                manifest: manifest(kind),
                runner: program,
            }
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
        },
        definition_revision: 2,
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
