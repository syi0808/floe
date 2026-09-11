use floe_agent::{
    AgentFailure, AgentRegistry, BuiltinContextSource, BuiltinExpertKind, BuiltinExpertSetup,
    BuiltinSourceBinding, BuiltinSourceState,
};
use floe_domain::PersonId;
use uuid::Uuid;

fn source(source: BuiltinContextSource, state: BuiltinSourceState) -> BuiltinSourceBinding {
    BuiltinSourceBinding {
        source,
        view_handle: Uuid::new_v4(),
        state,
    }
}

#[test]
fn builtins_only_grant_available_person_scoped_sources_and_publish_enabled_cards() {
    let person = PersonId::new();
    let other = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let request = BuiltinExpertSetup {
        instance_id: instance,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        sources: vec![
            source(BuiltinContextSource::Mail, BuiltinSourceState::Available),
            source(BuiltinContextSource::Calendar, BuiltinSourceState::Disabled),
            source(BuiltinContextSource::Tasks, BuiltinSourceState::Unavailable),
        ],
    };
    let receipt = registry.install_builtin_experts(person, &request).unwrap();
    assert_eq!(registry.revision(), 1);
    assert_eq!(registry.enabled_expert_cards(person), []);
    assert_eq!(registry.enabled_expert_cards(other), []);

    let commitments = receipt
        .assignments
        .iter()
        .find(|entry| entry.expert == BuiltinExpertKind::Commitments)
        .unwrap();
    assert_eq!(
        commitments.granted_view_handles,
        [request.sources[0].view_handle]
    );
    let communication = receipt
        .assignments
        .iter()
        .find(|entry| entry.expert == BuiltinExpertKind::Communication)
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
        sources: vec![],
    };
    let first = registry.install_builtin_experts(person, &request).unwrap();
    let revision = registry.revision();
    assert_eq!(
        registry.install_builtin_experts(person, &request).unwrap(),
        first
    );
    assert_eq!(registry.revision(), revision);
}

#[test]
fn source_refresh_updates_grants_without_changing_enablement() {
    let person = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let mail = source(BuiltinContextSource::Mail, BuiltinSourceState::Unavailable);
    let request = BuiltinExpertSetup {
        instance_id: instance,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        sources: vec![mail.clone()],
    };
    let receipt = registry.install_builtin_experts(person, &request).unwrap();
    let communication = receipt
        .assignments
        .iter()
        .find(|entry| entry.expert == BuiltinExpertKind::Communication)
        .unwrap();
    let revision = registry.revision();
    let refreshed = registry
        .refresh_builtin_expert_sources(
            person,
            revision,
            vec![BuiltinSourceBinding {
                state: BuiltinSourceState::Available,
                ..mail
            }],
        )
        .unwrap();

    assert_eq!(registry.revision(), revision + 1);
    assert_eq!(
        refreshed
            .assignments
            .iter()
            .find(|entry| entry.expert == BuiltinExpertKind::Communication)
            .unwrap()
            .granted_view_handles,
        [request.sources[0].view_handle]
    );
    assert!(
        !registry
            .snapshot()
            .assignments
            .iter()
            .find(|entry| entry.id == communication.expert_assignment_id)
            .unwrap()
            .enabled
    );
}

#[test]
fn atomic_enabled_install_only_advertises_executable_experts() {
    let person = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let request = BuiltinExpertSetup {
        instance_id: instance,
        expected_revision: 0,
        setup_id: Uuid::new_v4(),
        sources: vec![
            source(
                BuiltinContextSource::Calendar,
                BuiltinSourceState::Available,
            ),
            source(
                BuiltinContextSource::Attention,
                BuiltinSourceState::Available,
            ),
        ],
    };
    registry
        .install_builtin_experts_enabled(person, &request)
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
    assert_eq!(
        registry
            .enabled_expert_cards(person)
            .iter()
            .map(|card| card.id.as_str())
            .collect::<Vec<_>>(),
        ["floe.builtin.focus-attention"]
    );

    let focus = registry
        .snapshot()
        .builtin_setups[0]
        .assignments
        .iter()
        .find(|assignment| assignment.expert == BuiltinExpertKind::FocusAttention)
        .unwrap()
        .expert_assignment_id;
    registry
        .set_assignment_enabled(registry.revision(), person, focus, false)
        .unwrap();
    let revision = registry.revision();
    registry
        .install_builtin_experts_enabled(person, &request)
        .unwrap();
    assert_eq!(registry.revision(), revision);
    assert!(!registry
        .snapshot()
        .assignments
        .iter()
        .find(|assignment| assignment.id == focus)
        .unwrap()
        .enabled);
}
