use floe_agent_contract::{AgentFailure, PersonId};
use floe_experts::{AgentRegistry, ExpertInstallOperation};
use uuid::Uuid;

fn operation(instance_id: Uuid, expected_revision: u64) -> ExpertInstallOperation {
    ExpertInstallOperation {
        instance_id,
        expected_revision,
        operation_id: Uuid::new_v4(),
    }
}

#[test]
fn shipped_bundle_installs_without_source_authority_and_preserves_disablement() {
    let person = PersonId::new();
    let other = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let manifests = floe_experts_builtin::manifests();
    let request = operation(instance, 0);
    let receipt = registry.install_bundle(person, &request, &manifests).unwrap();
    assert_eq!(registry.revision(), 1);
    assert_eq!(receipt.installed.len(), manifests.len());
    assert_eq!(registry.enabled_expert_cards(person).unwrap().len(), manifests.len());
    assert!(registry.enabled_expert_cards(other).unwrap().is_empty());
    assert!(registry.snapshot().installations.iter().all(|entry| entry.package.kind == floe_experts::PackageKind::Expert));

    let assignment = receipt.installed[0].assignment_id;
    registry.set_assignment_enabled(registry.revision(), person, assignment, false).unwrap();
    let revision = registry.revision();
    assert_eq!(registry.install_bundle(person, &request, &manifests).unwrap(), receipt);
    assert_eq!(registry.revision(), revision);
    assert_eq!(registry.enabled_expert_cards(person).unwrap().len(), manifests.len() - 1);
    assert_eq!(registry.set_assignment_enabled(revision, other, assignment, true), Err(AgentFailure::NotFound));
    let restored = AgentRegistry::restore(registry.snapshot(), instance).unwrap();
    assert_eq!(restored.enabled_expert_cards(person).unwrap().len(), manifests.len() - 1);
}

#[test]
fn install_rejoin_binds_original_revision_person_and_exact_manifests() {
    let person = PersonId::new();
    let other = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let manifests = floe_experts_builtin::manifests();
    let request = operation(instance, 0);
    let receipt = registry.install_bundle(person, &request, &manifests).unwrap();
    assert_eq!(registry.install_bundle(person, &request, &manifests).unwrap(), receipt);
    let changed_revision = ExpertInstallOperation { expected_revision: 1, ..request.clone() };
    assert_eq!(registry.install_bundle(person, &changed_revision, &manifests), Err(AgentFailure::Conflict));
    assert_eq!(registry.install_bundle(other, &request, &manifests), Err(AgentFailure::Conflict));
    let mut changed = manifests.clone();
    changed[0].prompt_contract.revision += 1;
    assert_eq!(registry.install_bundle(person, &request, &changed), Err(AgentFailure::Conflict));
    let second_operation = operation(instance, registry.revision());
    assert_eq!(registry.install_bundle(person, &second_operation, &manifests).unwrap(), receipt);
    assert_eq!(registry.revision(), 1);
}

#[test]
fn independent_person_installs_are_exact_and_source_free() {
    let person = PersonId::new();
    let other = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let manifests = floe_experts_builtin::manifests();
    registry.install_bundle(person, &operation(instance, 0), &manifests).unwrap();
    registry.install_bundle(other, &operation(instance, 1), &manifests).unwrap();
    assert_eq!(registry.enabled_expert_cards(person).unwrap().len(), manifests.len());
    assert_eq!(registry.enabled_expert_cards(other).unwrap().len(), manifests.len());
    assert_eq!(registry.snapshot().manifests.len(), manifests.len());
}

#[test]
fn separate_bundles_can_install_for_one_person_without_reenabling_prior_assignments() {
    let person = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let manifests = floe_experts_builtin::manifests();
    let first = registry.install_bundle(person, &operation(instance, 0), &manifests[..1]).unwrap();
    registry.set_assignment_enabled(registry.revision(), person, first.installed[0].assignment_id, false).unwrap();
    let second = registry.install_bundle(person, &operation(instance, registry.revision()), &manifests[1..2]).unwrap();
    assert_ne!(first.operation_id, second.operation_id);
    assert_eq!(registry.overview(person).installations.len(), 2);
    assert_eq!(registry.enabled_expert_cards(person).unwrap().len(), 1);
    assert!(!registry.snapshot().assignments.iter().find(|assignment| assignment.id == first.installed[0].assignment_id).unwrap().enabled);
    AgentRegistry::restore(registry.snapshot(), instance).unwrap();
}

#[test]
fn two_callable_assignments_for_one_public_expert_are_ambiguous() {
    let person = PersonId::new();
    let other = PersonId::new();
    let instance = Uuid::new_v4();
    let mut registry = AgentRegistry::new(instance);
    let manifest = floe_experts_builtin::manifests().remove(0);
    registry.install_bundle(person, &operation(instance, 0), std::slice::from_ref(&manifest)).unwrap();
    registry.install_bundle(other, &operation(instance, 1), std::slice::from_ref(&manifest)).unwrap();
    let mut snapshot = registry.snapshot();
    snapshot.assignments[1].person_id = person;
    snapshot.install_receipts[1].person_id = person;
    let registry = AgentRegistry::restore(snapshot, instance).unwrap();
    assert_eq!(registry.enabled_expert_admissions(person), Err(AgentFailure::Conflict));
}
