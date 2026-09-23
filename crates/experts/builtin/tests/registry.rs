use floe_agent_contract::AgentFailure;
use floe_agent_contract::PersonId;
use floe_experts::{AgentId, AgentRegistry, BuiltinExpertSetup, ExpertPackaging, ExpertSetupSpec};
use floe_experts_builtin::BuiltinExpertKind;
use uuid::Uuid;

/// The builtin Experts, in the shape the registry installs them.
fn specs() -> Vec<ExpertSetupSpec> {
    floe_experts_builtin::builtin_setup_declarations()
        .into_iter()
        .map(|declaration| {
            let expert = agent_id(declaration.expert_id);
            let packaging = ExpertPackaging {
                expert: expert.clone(),
                tool_id: declaration.tool_id.clone(),
                version: declaration.version.to_owned(),
                publisher: declaration.publisher.to_owned(),
                metadata: floe_experts::ExpertMetadata {
                    name: declaration.name.to_owned(),
                    description: declaration.description.to_owned(),
                    domain_tags: declaration.domain_tags.clone(),
                    skills: declaration.skills.clone(),
                    supported_placements: declaration.supported_placements.clone(),
                },
                state_schema_version: floe_experts_builtin::BUILTIN_EXPERT_STATE_SCHEMA_VERSION,
            };
            ExpertSetupSpec {
                packages: packaging.packages(declaration.data_class),
                expert,
            }
        })
        .collect()
}

fn agent_id(value: &str) -> AgentId {
    AgentId::try_new(value).expect("builtin ids are valid")
}

fn expert_id(kind: BuiltinExpertKind) -> AgentId {
    agent_id(kind.package_id())
}

#[test]
fn builtins_install_without_source_input_and_publish_cards_on_enablement() {
    let person = PersonId::new();
    let other = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let request = BuiltinExpertSetup {
        instance_id: instance,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
    };
    let receipt = registry
        .install_builtin_experts(person, &request, &specs())
        .unwrap();
    assert_eq!(registry.revision(), 1);
    assert_eq!(registry.enabled_expert_cards(person), []);
    assert_eq!(registry.enabled_expert_cards(other), []);

    let communication = receipt
        .assignments
        .iter()
        .find(|entry| entry.expert == expert_id(BuiltinExpertKind::Communication))
        .unwrap();
    let mut revision = registry.revision();
    for installation in [
        communication.tool_installation_id,
        communication.expert_installation_id,
    ] {
        registry
            .set_installation_enabled(revision, installation, true)
            .unwrap();
        revision += 1;
    }
    registry
        .set_assignment_enabled(revision, person, communication.expert_assignment_id, true)
        .unwrap();
    revision += 1;
    assert!(registry.enabled_expert_cards(person).is_empty());
    registry
        .set_assignment_enabled(revision, person, communication.tool_assignment_id, true)
        .unwrap();
    revision += 1;
    let cards = registry.enabled_expert_cards(person);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].id, "floe.builtin.communication");
    assert_eq!(cards[0].version, "1.0.0");
    assert_eq!(
        registry.set_assignment_enabled(revision, other, communication.expert_assignment_id, false),
        Err(AgentFailure::NotFound)
    );

    let restored = AgentRegistry::restore(registry.snapshot(), instance).unwrap();
    assert_eq!(restored.enabled_expert_cards(person), cards);
}

#[test]
fn builtin_setup_is_idempotent_for_the_same_request() {
    let person = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let request = BuiltinExpertSetup {
        instance_id: instance,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
    };
    let first = registry
        .install_builtin_experts(person, &request, &specs())
        .unwrap();
    let revision = registry.revision();
    assert_eq!(
        registry
            .install_builtin_experts(person, &request, &specs())
            .unwrap(),
        first
    );
    assert_eq!(registry.revision(), revision);
}

#[test]
fn atomic_enabled_install_advertises_all_builtin_experts_without_sources() {
    let person = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let request = BuiltinExpertSetup {
        instance_id: instance,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
    };
    registry
        .install_builtin_experts_enabled(person, &request, &specs())
        .unwrap();

    assert_eq!(registry.revision(), 1);
    assert!(
        registry
            .snapshot()
            .installations
            .iter()
            .all(|installation| installation.enabled)
    );
    assert!(
        registry
            .snapshot()
            .assignments
            .iter()
            .all(|assignment| assignment.enabled)
    );
    let mut cards: Vec<_> = registry
        .enabled_expert_cards(person)
        .iter()
        .map(|card| card.id.clone())
        .collect();
    cards.sort();
    let mut expected: Vec<String> = BuiltinExpertKind::ALL
        .iter()
        .map(|kind| kind.package_id().to_owned())
        .collect();
    expected.sort();
    assert_eq!(cards, expected);

    let focus = registry.snapshot().builtin_setups[0]
        .assignments
        .iter()
        .find(|assignment| assignment.expert == expert_id(BuiltinExpertKind::FocusAttention))
        .unwrap()
        .expert_assignment_id;
    registry
        .set_assignment_enabled(registry.revision(), person, focus, false)
        .unwrap();
    let revision = registry.revision();
    registry
        .install_builtin_experts_enabled(person, &request, &specs())
        .unwrap();
    assert_eq!(registry.revision(), revision);
    assert!(
        !registry
            .snapshot()
            .assignments
            .iter()
            .find(|assignment| assignment.id == focus)
            .unwrap()
            .enabled
    );
    // Disabling one Expert removes only its card.
    assert_eq!(registry.enabled_expert_cards(person).len(), 7);
}
