use floe_agent::*;
use floe_domain::{CalendarProvider, PersonId};
use uuid::Uuid;

fn request(registry: &AgentRegistry, provider: CalendarProvider) -> CalendarExpertSetup {
    CalendarExpertSetup {
        instance_id: registry.instance_id(),
        expected_revision: registry.revision(),
        setup_id: Uuid::new_v4(),
        provider,
        calendar_ids: vec!["work".into(), "home".into()],
    }
}

#[test]
fn setup_is_one_revision_default_off_and_uses_provider_pinned_packages() {
    let person = PersonId::new();
    let mut registry = AgentRegistry::new(Uuid::new_v4());
    for (provider, data_class) in [
        (CalendarProvider::Fixture, DataClass::Synthetic),
        (CalendarProvider::EventKit, DataClass::Personal),
    ] {
        let request = request(&registry, provider);
        let setup = registry.install_calendar_expert(person, &request).unwrap();
        assert_eq!(registry.revision(), request.expected_revision + 1);
        let snapshot = registry.snapshot();
        let binding = snapshot.calendar_views.last().unwrap();
        assert_eq!(binding.calendar_ids, ["home", "work"]);
        assert!(!binding.enabled);
        assert_eq!(binding.data_class(), data_class);
        for assignment in &snapshot.assignments[snapshot.assignments.len() - 2..] {
            assert!(!assignment.enabled);
            assert_eq!(assignment.person_id, person);
            assert_eq!(assignment.private_state, ExpertPrivateState::default());
        }
        assert_eq!(
            registry.expert_descriptor(
                person,
                setup.expert_assignment_id,
                registry.revision(),
                setup.view_handle,
            ),
            Err(AgentFailure::CapabilityDenied)
        );
        for installation in [setup.tool_installation_id, setup.expert_installation_id] {
            registry
                .set_installation_enabled(registry.revision(), installation, true)
                .unwrap();
        }
        for assignment in [setup.tool_assignment_id, setup.expert_assignment_id] {
            registry
                .set_assignment_enabled(registry.revision(), person, assignment, true)
                .unwrap();
        }
        registry
            .set_calendar_view_enabled(registry.revision(), person, setup.view_handle, true)
            .unwrap();
        assert_eq!(
            registry
                .expert_descriptor(
                    person,
                    setup.expert_assignment_id,
                    registry.revision(),
                    setup.view_handle,
                )
                .unwrap()
                .output_data_class,
            data_class
        );
    }
    assert_eq!(registry.snapshot().packages.len(), 4);
    let next = request(&registry, CalendarProvider::EventKit);
    registry.install_calendar_expert(person, &next).unwrap();
    assert_eq!(registry.snapshot().packages.len(), 4);
    assert_eq!(registry.snapshot().installations.len(), 6);
}

#[test]
fn calendar_access_changes_scope_enablement_and_removal_atomically() {
    let person = PersonId::new();
    let mut registry = AgentRegistry::new(Uuid::new_v4());
    let request = request(&registry, CalendarProvider::EventKit);
    let setup = registry.install_calendar_expert(person, &request).unwrap();

    let configure = |registry: &AgentRegistry, setup_id, change| CalendarAccessConfiguration {
        instance_id: registry.instance_id(),
        expected_revision: registry.revision(),
        setup_id,
        change,
    };
    registry
        .configure_calendar_access(
            person,
            &configure(
                &registry,
                setup.setup_id,
                CalendarAccessChange::SetEnabled { enabled: true },
            ),
        )
        .unwrap();
    let enabled = registry.snapshot();
    assert!(enabled.calendar_views[0].enabled);
    assert!(enabled.installations.iter().all(|entry| entry.enabled));
    assert!(enabled.assignments.iter().all(|entry| entry.enabled));
    assert_eq!(enabled.revision, 2);

    let replacement_setup_id = Uuid::new_v4();
    registry
        .configure_calendar_access(
            person,
            &configure(
                &registry,
                setup.setup_id,
                CalendarAccessChange::SetScope {
                    replacement_setup_id,
                    provider: CalendarProvider::EventKit,
                    calendar_ids: vec!["shared".into(), "home".into()],
                },
            ),
        )
        .unwrap();
    assert_eq!(
        registry.calendar_expert_overview(person).views[0].calendar_ids,
        ["home", "shared"]
    );
    assert_eq!(
        registry.snapshot().calendar_views[0].calendar_ids,
        ["home", "work"]
    );
    assert_eq!(
        registry.snapshot().revoked_calendar_setups,
        [setup.setup_id]
    );

    let before = registry.snapshot();
    let mut invalid = configure(
        &registry,
        replacement_setup_id,
        CalendarAccessChange::SetScope {
            replacement_setup_id,
            provider: CalendarProvider::Fixture,
            calendar_ids: vec!["other".into()],
        },
    );
    assert_eq!(
        registry.configure_calendar_access(person, &invalid),
        Err(AgentFailure::InvalidInput)
    );
    assert_eq!(registry.snapshot(), before);
    invalid.instance_id = Uuid::new_v4();
    assert_eq!(
        registry.configure_calendar_access(person, &invalid),
        Err(AgentFailure::NotFound)
    );
    assert_eq!(registry.snapshot(), before);

    registry
        .configure_calendar_access(
            person,
            &configure(
                &registry,
                replacement_setup_id,
                CalendarAccessChange::Remove {},
            ),
        )
        .unwrap();
    let removed = registry.snapshot();
    assert_eq!(removed.calendar_setups.len(), 2);
    assert_eq!(removed.calendar_views.len(), 2);
    assert_eq!(removed.installations.len(), 4);
    assert_eq!(removed.assignments.len(), 4);
    assert_eq!(removed.revoked_calendar_setups.len(), 2);
    let overview = registry.calendar_expert_overview(person);
    assert!(overview.setups.is_empty());
    assert!(overview.views.is_empty());
    assert!(overview.registry.installations.is_empty());
    assert!(overview.registry.assignments.is_empty());
    assert_eq!(removed.revision, before.revision + 1);
}

#[test]
fn exact_replay_preserves_revocation_and_state_but_changed_intent_conflicts() {
    let person = PersonId::new();
    let mut registry = AgentRegistry::new(Uuid::new_v4());
    let request = request(&registry, CalendarProvider::EventKit);
    let setup = registry.install_calendar_expert(person, &request).unwrap();
    registry
        .set_calendar_view_enabled(registry.revision(), person, setup.view_handle, true)
        .unwrap();
    registry
        .set_calendar_view_enabled(registry.revision(), person, setup.view_handle, false)
        .unwrap();
    let mut recorded = registry.snapshot();
    recorded.assignments[1].private_state = ExpertPrivateState {
        schema_version: 1,
        revision: 1,
        completed_invocations: 1,
        last_invocation_id: Some(Uuid::new_v4()),
    };
    registry = AgentRegistry::restore(recorded.clone(), registry.instance_id()).unwrap();
    let mut reordered = request.clone();
    reordered.calendar_ids.reverse();
    assert_eq!(
        registry
            .install_calendar_expert(person, &reordered)
            .unwrap(),
        setup
    );
    assert_eq!(registry.snapshot(), recorded);
    for mode in 0..6 {
        let mut changed = request.clone();
        let mut owner = person;
        match mode {
            0 => changed.provider = CalendarProvider::Fixture,
            1 => changed.calendar_ids = vec!["another".into()],
            2 => changed.expected_revision = registry.revision(),
            3 => owner = PersonId::new(),
            4 => changed.instance_id = Uuid::new_v4(),
            _ => changed.setup_id = Uuid::new_v4(),
        }
        assert!(registry.install_calendar_expert(owner, &changed).is_err());
        assert_eq!(registry.snapshot(), recorded);
    }
}

#[test]
fn invalid_scope_capacity_and_package_collision_leave_no_partial_setup() {
    let person = PersonId::new();
    let mut registry = AgentRegistry::new(Uuid::new_v4());
    for identifiers in [
        vec![],
        vec!["same".into(), "same".into()],
        vec![" ".into()],
        vec!["x".repeat(513)],
        (0..5).map(|index| index.to_string()).collect(),
    ] {
        let mut request = request(&registry, CalendarProvider::Fixture);
        request.calendar_ids = identifiers;
        let before = registry.snapshot();
        assert_eq!(
            registry.install_calendar_expert(person, &request),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(registry.snapshot(), before);
    }
    let first = request(&registry, CalendarProvider::Fixture);
    registry.install_calendar_expert(person, &first).unwrap();
    let mut full = registry.snapshot();
    while full.installations.len() < 128 {
        let mut installation = full.installations[0].clone();
        installation.id = Uuid::new_v4();
        full.installations.push(installation);
    }
    registry = AgentRegistry::restore(full.clone(), registry.instance_id()).unwrap();
    let next = request(&registry, CalendarProvider::EventKit);
    assert_eq!(
        registry.install_calendar_expert(person, &next),
        Err(AgentFailure::BudgetExceeded)
    );
    assert_eq!(registry.snapshot(), full);

    let mut collision = AgentRegistry::new(Uuid::new_v4());
    let mut package = full.packages[0].clone();
    package.publisher = "different".into();
    collision.register(0, package).unwrap();
    let before = collision.snapshot();
    let next = request(&collision, CalendarProvider::Fixture);
    assert_eq!(
        collision.install_calendar_expert(person, &next),
        Err(AgentFailure::Conflict)
    );
    assert_eq!(collision.snapshot(), before);
}

#[test]
fn legacy_snapshots_remain_empty_and_corrupt_setup_receipts_fail_restore() {
    let person = PersonId::new();
    let mut registry = AgentRegistry::new(Uuid::new_v4());
    let encoded = serde_json::to_string(&registry.snapshot()).unwrap();
    assert!(!encoded.contains("calendar_setups"));
    assert!(
        serde_json::from_str::<RegistrySnapshot>(&encoded)
            .unwrap()
            .calendar_setups
            .is_empty()
    );
    let request = request(&registry, CalendarProvider::Fixture);
    registry.install_calendar_expert(person, &request).unwrap();
    let before = registry.snapshot();
    for mode in 0..7 {
        let mut invalid = before.clone();
        match mode {
            0 => invalid.calendar_setups[0].setup_id = Uuid::nil(),
            1 => invalid.calendar_setups[0].person_id = PersonId::new(),
            2 => invalid.calendar_setups[0].expected_revision = invalid.revision,
            3 => invalid.calendar_setups[0].view_handle = Uuid::new_v4(),
            4 => invalid
                .calendar_setups
                .push(invalid.calendar_setups[0].clone()),
            5 => invalid.assignments[1].granted_view_handles = vec![Uuid::new_v4()],
            _ => invalid.calendar_setups[0].expert_assignment_id = Uuid::new_v4(),
        }
        assert!(AgentRegistry::restore(invalid, registry.instance_id()).is_err());
    }
    let overview = serde_json::to_string(&registry.overview(person)).unwrap();
    assert!(!overview.contains("calendar_setups") && !overview.contains("home"));
}
