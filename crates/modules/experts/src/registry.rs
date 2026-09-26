use std::collections::HashSet;

use floe_agent_contract::{AgentFailure, DataClass, PackageKind, PackageRef, PersonId};
use floe_context_contract::SourceSelectionReference;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ExpertAdmissionIdentity, ExpertExecutionSelection, ExpertManifest, manifest_set_digest};

pub const EXPERT_REGISTRY_SCHEMA_VERSION: u32 = 3;
pub const EXPERT_BINDING_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertInstallOperation {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub operation_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledExpert {
    pub package: PackageRef,
    pub installation_id: Uuid,
    pub assignment_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertInstallReceipt {
    pub operation_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub manifest_digest: String,
    pub installed: Vec<InstalledExpert>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertInstallResult {
    pub receipt: ExpertInstallReceipt,
    pub registry: RegistryOverview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageInstallation {
    pub id: Uuid,
    pub package: PackageRef,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageAssignment {
    pub id: Uuid,
    pub person_id: PersonId,
    pub installation_id: Uuid,
    pub enabled: bool,
    pub private_state: ExpertPrivateState,
    pub binding: ExpertBindingState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertBindingState {
    pub schema_version: u32,
    pub revision: u64,
    pub entries: Vec<RequirementBinding>,
    pub last_operation: Option<BindingOperationReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementBinding {
    pub requirement_key: String,
    pub capability: String,
    pub contract_version: u32,
    pub selected: Vec<SourceSelectionReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingOperationReceipt {
    pub operation_id: Uuid,
    pub command_digest: [u8; 32],
    pub resulting_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertBindingCommand {
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub definition_revision: u64,
    pub requirement_key: String,
    pub expected_binding_revision: u64,
    pub selected: Vec<SourceSelectionReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertPrivateState {
    pub schema_version: u32,
    pub revision: u64,
    pub completed_invocations: u64,
    pub last_invocation_id: Option<Uuid>,
}

impl Default for ExpertPrivateState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            completed_invocations: 0,
            last_invocation_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySnapshot {
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub revision: u64,
    pub manifests: Vec<ExpertManifest>,
    pub installations: Vec<PackageInstallation>,
    pub assignments: Vec<PackageAssignment>,
    pub install_receipts: Vec<ExpertInstallReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryOverview {
    pub schema_version: u32,
    pub person_id: PersonId,
    pub instance_id: Uuid,
    pub revision: u64,
    pub installations: Vec<PackageInstallation>,
    pub assignments: Vec<AssignmentOverview>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentOverview {
    pub id: Uuid,
    pub installation_id: Uuid,
    pub enabled: bool,
    pub state_revision: u64,
    pub completed_invocations: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryConfiguration {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub target: RegistryConfigurationTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegistryConfigurationTarget {
    Installation { id: Uuid, enabled: bool },
    Assignment { id: Uuid, enabled: bool },
}

pub struct AgentRegistry {
    snapshot: RegistrySnapshot,
}

#[derive(Clone)]
pub struct ResolvedExpert {
    pub registry_revision: u64,
    pub manifest: ExpertManifest,
    pub assignment: PackageAssignment,
    pub data_class: DataClass,
}

impl AgentRegistry {
    pub fn new(instance_id: Uuid) -> Self {
        Self {
            snapshot: RegistrySnapshot {
                schema_version: EXPERT_REGISTRY_SCHEMA_VERSION,
                instance_id,
                revision: 0,
                manifests: vec![],
                installations: vec![],
                assignments: vec![],
                install_receipts: vec![],
            },
        }
    }

    pub fn restore(snapshot: RegistrySnapshot, instance_id: Uuid) -> Result<Self, AgentFailure> {
        if snapshot.schema_version != EXPERT_REGISTRY_SCHEMA_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if snapshot.instance_id != instance_id || instance_id.is_nil() {
            return Err(AgentFailure::NotFound);
        }
        if snapshot.manifests.len() > 64
            || snapshot.installations.len() > 128
            || snapshot.assignments.len() > 256
            || snapshot.install_receipts.len() > 64
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let registry = Self { snapshot };
        let mut packages = HashSet::new();
        for manifest in &registry.snapshot.manifests {
            manifest.validate()?;
            if !packages.insert((
                manifest.package.id.clone(),
                manifest.package.version.clone(),
            )) {
                return Err(AgentFailure::Conflict);
            }
        }
        let mut installations = HashSet::new();
        for installation in &registry.snapshot.installations {
            if installation.id.is_nil()
                || !installations.insert(installation.id)
                || registry.manifest(&installation.package).is_err()
            {
                return Err(AgentFailure::Conflict);
            }
        }
        let mut assignments = HashSet::new();
        for assignment in &registry.snapshot.assignments {
            if assignment.id.is_nil() || !assignments.insert(assignment.id) {
                return Err(AgentFailure::Conflict);
            }
            registry.installation(assignment.installation_id)?;
            let installation = registry.installation(assignment.installation_id)?;
            let manifest = registry.manifest(&installation.package)?;
            let binding = &assignment.binding;
            if binding.schema_version != EXPERT_BINDING_SCHEMA_VERSION
                || binding.revision == 0
                || binding.entries.len() != manifest.source_requirements.len()
            {
                return Err(AgentFailure::InvalidInput);
            }
            let mut requirements = manifest.source_requirements.iter().collect::<Vec<_>>();
            requirements.sort_by(|left, right| left.key.cmp(&right.key));
            for (entry, requirement) in binding.entries.iter().zip(requirements) {
                if entry.requirement_key != requirement.key
                    || entry.capability != requirement.capability
                    || entry.contract_version != requirement.contract_version
                    || entry.selected.len() > usize::from(requirement.maximum_sources)
                    || entry.selected.iter().any(|selected| {
                        selected.validate().is_err()
                            || selected.capability_id != entry.capability
                            || selected.contract_version != entry.contract_version
                    })
                    || entry.selected.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            if binding.last_operation.as_ref().is_some_and(|receipt| {
                receipt.operation_id.is_nil()
                    || receipt.resulting_revision != binding.revision
                    || binding.revision < 2
            }) {
                return Err(AgentFailure::InvalidInput);
            }
            let state = &assignment.private_state;
            if state.schema_version != 1
                || state.revision != state.completed_invocations
                || state.revision > registry.revision()
                || (state.revision == 0) != state.last_invocation_id.is_none()
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        let mut operations = HashSet::new();
        let mut received_installations = HashSet::new();
        let mut received_assignments = HashSet::new();
        let mut received_packages = HashSet::new();
        for receipt in &registry.snapshot.install_receipts {
            if receipt.operation_id.is_nil()
                || !operations.insert(receipt.operation_id)
                || receipt.expected_revision >= registry.revision()
                || receipt.installed.is_empty()
                || receipt.installed.len() > 64
            {
                return Err(AgentFailure::Conflict);
            }
            let mut manifests = Vec::with_capacity(receipt.installed.len());
            let mut receipt_packages = HashSet::new();
            for installed in &receipt.installed {
                if !receipt_packages.insert((
                    installed.package.id.clone(),
                    installed.package.version.clone(),
                )) || !received_installations.insert(installed.installation_id)
                    || !received_assignments.insert(installed.assignment_id)
                    || registry.installation(installed.installation_id)?.package
                        != installed.package
                    || registry
                        .assignment(receipt.person_id, installed.assignment_id)?
                        .installation_id
                        != installed.installation_id
                {
                    return Err(AgentFailure::Conflict);
                }
                received_packages.insert((
                    installed.package.id.clone(),
                    installed.package.version.clone(),
                ));
                manifests.push(registry.manifest(&installed.package)?.clone());
            }
            if manifest_set_digest(&manifests)? != receipt.manifest_digest {
                return Err(AgentFailure::Conflict);
            }
        }
        if received_installations.len() != registry.snapshot.installations.len()
            || received_assignments.len() != registry.snapshot.assignments.len()
            || received_packages.len() != registry.snapshot.manifests.len()
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(registry)
    }

    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }

    pub fn instance_id(&self) -> Uuid {
        self.snapshot.instance_id
    }

    pub fn snapshot(&self) -> RegistrySnapshot {
        self.snapshot.clone()
    }

    pub fn overview(&self, person_id: PersonId) -> RegistryOverview {
        let installation_ids: HashSet<_> = self
            .snapshot
            .assignments
            .iter()
            .filter(|assignment| assignment.person_id == person_id)
            .map(|assignment| assignment.installation_id)
            .collect();
        RegistryOverview {
            schema_version: EXPERT_REGISTRY_SCHEMA_VERSION,
            person_id,
            instance_id: self.instance_id(),
            revision: self.revision(),
            installations: self
                .snapshot
                .installations
                .iter()
                .filter(|installation| installation_ids.contains(&installation.id))
                .cloned()
                .collect(),
            assignments: self
                .snapshot
                .assignments
                .iter()
                .filter(|assignment| assignment.person_id == person_id)
                .map(|assignment| AssignmentOverview {
                    id: assignment.id,
                    installation_id: assignment.installation_id,
                    enabled: assignment.enabled,
                    state_revision: assignment.private_state.revision,
                    completed_invocations: assignment.private_state.completed_invocations,
                })
                .collect(),
        }
    }

    pub fn install_bundle(
        &mut self,
        person_id: PersonId,
        operation: &ExpertInstallOperation,
        manifests: &[ExpertManifest],
    ) -> Result<ExpertInstallReceipt, AgentFailure> {
        if operation.instance_id != self.instance_id() {
            return Err(AgentFailure::NotFound);
        }
        if operation.operation_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let digest = manifest_set_digest(manifests)?;
        if let Some(receipt) = self
            .snapshot
            .install_receipts
            .iter()
            .find(|receipt| receipt.operation_id == operation.operation_id)
        {
            return if receipt.person_id == person_id
                && receipt.expected_revision == operation.expected_revision
                && receipt.manifest_digest == digest
            {
                Ok(receipt.clone())
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        if let Some(receipt) = self
            .snapshot
            .install_receipts
            .iter()
            .find(|receipt| receipt.person_id == person_id && receipt.manifest_digest == digest)
        {
            return if receipt.manifest_digest == digest {
                Ok(receipt.clone())
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        self.check_revision(operation.expected_revision)?;
        let mut next = self.snapshot();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let mut installed = Vec::with_capacity(manifests.len());
        for manifest in manifests {
            if next
                .assignments
                .iter()
                .filter(|assignment| assignment.person_id == person_id)
                .any(|assignment| {
                    next.installations.iter().any(|installation| {
                        installation.id == assignment.installation_id
                            && installation.package == manifest.package
                    })
                })
            {
                return Err(AgentFailure::Conflict);
            }
            if let Some(existing) = next
                .manifests
                .iter()
                .find(|existing| existing.package == manifest.package)
            {
                if existing != manifest {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                next.manifests.push(manifest.clone());
            }
            let installation_id = Uuid::new_v4();
            let assignment_id = Uuid::new_v4();
            next.installations.push(PackageInstallation {
                id: installation_id,
                package: manifest.package.clone(),
                enabled: true,
            });
            next.assignments.push(PackageAssignment {
                id: assignment_id,
                person_id,
                installation_id,
                enabled: true,
                private_state: ExpertPrivateState::default(),
                binding: ExpertBindingState {
                    schema_version: EXPERT_BINDING_SCHEMA_VERSION,
                    revision: 1,
                    entries: {
                        let mut entries = manifest
                            .source_requirements
                            .iter()
                            .map(|requirement| RequirementBinding {
                                requirement_key: requirement.key.clone(),
                                capability: requirement.capability.clone(),
                                contract_version: requirement.contract_version,
                                selected: vec![],
                            })
                            .collect::<Vec<_>>();
                        entries.sort_by(|left, right| {
                            left.requirement_key.cmp(&right.requirement_key)
                        });
                        entries
                    },
                    last_operation: None,
                },
            });
            installed.push(InstalledExpert {
                package: manifest.package.clone(),
                installation_id,
                assignment_id,
            });
        }
        let receipt = ExpertInstallReceipt {
            operation_id: operation.operation_id,
            person_id,
            expected_revision: operation.expected_revision,
            manifest_digest: digest,
            installed,
        };
        next.install_receipts.push(receipt.clone());
        *self = Self::restore(next, self.instance_id())?;
        Ok(receipt)
    }

    pub fn enabled_expert_admissions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<(crate::AgentCard, ExpertAdmissionIdentity)>, AgentFailure> {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for assignment in self
            .snapshot
            .assignments
            .iter()
            .filter(|assignment| assignment.person_id == person_id && assignment.enabled)
        {
            let installation = self.installation(assignment.installation_id)?;
            if !installation.enabled {
                continue;
            }
            let manifest = self.manifest(&installation.package)?;
            if !seen.insert(&manifest.definition.card.id) {
                return Err(AgentFailure::Conflict);
            }
            entries.push((
                manifest.definition.card.clone(),
                ExpertAdmissionIdentity {
                    registry_instance_id: self.instance_id(),
                    assignment_id: assignment.id,
                    installation_id: installation.id,
                    package: manifest.package.clone(),
                    definition_revision: manifest.definition.definition_revision,
                },
            ));
        }
        Ok(entries)
    }

    pub fn enabled_expert_cards(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<crate::AgentCard>, AgentFailure> {
        Ok(self
            .enabled_expert_admissions(person_id)?
            .into_iter()
            .map(|(card, _)| card)
            .collect())
    }

    pub fn resolve_assignment(
        &self,
        instance_id: Uuid,
        person_id: PersonId,
        assignment_id: Uuid,
        package: &PackageRef,
        definition_revision: u64,
    ) -> Result<ResolvedExpert, AgentFailure> {
        if instance_id != self.instance_id() {
            return Err(AgentFailure::NotFound);
        }
        let assignment = self.assignment(person_id, assignment_id)?;
        let installation = self.installation(assignment.installation_id)?;
        if !assignment.enabled || !installation.enabled {
            return Err(AgentFailure::CapabilityDenied);
        }
        if &installation.package != package {
            return Err(AgentFailure::Conflict);
        }
        let manifest = self.manifest(package)?;
        if manifest.definition.definition_revision != definition_revision {
            return Err(AgentFailure::Conflict);
        }
        Ok(ResolvedExpert {
            registry_revision: self.revision(),
            manifest: manifest.clone(),
            assignment: assignment.clone(),
            data_class: manifest.data_class,
        })
    }

    pub fn resolve_admitted(
        &self,
        person_id: PersonId,
        admission: &ExpertAdmissionIdentity,
    ) -> Result<ResolvedExpert, AgentFailure> {
        if admission.registry_instance_id != self.instance_id() {
            return Err(AgentFailure::NotFound);
        }
        let assignment = self.assignment(person_id, admission.assignment_id)?;
        if assignment.installation_id != admission.installation_id {
            return Err(AgentFailure::Conflict);
        }
        let installation = self.installation(assignment.installation_id)?;
        if installation.package != admission.package {
            return Err(AgentFailure::Conflict);
        }
        let manifest = self.manifest(&installation.package)?;
        if manifest.definition.definition_revision != admission.definition_revision {
            return Err(AgentFailure::Conflict);
        }
        Ok(ResolvedExpert {
            registry_revision: self.revision(),
            manifest: manifest.clone(),
            assignment: assignment.clone(),
            data_class: manifest.data_class,
        })
    }

    pub fn validate_settled_invocation(
        &self,
        instance_id: Uuid,
        person_id: PersonId,
        assignment_id: Uuid,
        package: &PackageRef,
        state_revision: u64,
        data_class: DataClass,
    ) -> Result<(), AgentFailure> {
        if instance_id != self.instance_id() || state_revision == 0 {
            return Err(AgentFailure::NotFound);
        }
        let assignment = self.assignment(person_id, assignment_id)?;
        if state_revision > assignment.private_state.revision {
            return Err(AgentFailure::Conflict);
        }
        let installation = self.installation(assignment.installation_id)?;
        if &installation.package != package
            || package.kind != PackageKind::Expert
            || self.manifest(package)?.data_class != data_class
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    pub fn validate_active_assignment(
        &self,
        person_id: PersonId,
        assignment_id: Uuid,
    ) -> Result<(), AgentFailure> {
        let assignment = self.assignment(person_id, assignment_id)?;
        let installation = self.installation(assignment.installation_id)?;
        if !assignment.enabled || !installation.enabled {
            return Err(AgentFailure::CapabilityDenied);
        }
        self.manifest(&installation.package)?;
        Ok(())
    }

    pub fn set_installation_enabled(
        &mut self,
        expected_revision: u64,
        installation_id: Uuid,
        enabled: bool,
    ) -> Result<(), AgentFailure> {
        self.check_revision(expected_revision)?;
        self.installation(installation_id)?;
        self.advance()?;
        self.snapshot
            .installations
            .iter_mut()
            .find(|entry| entry.id == installation_id)
            .ok_or(AgentFailure::NotFound)?
            .enabled = enabled;
        Ok(())
    }

    pub fn set_assignment_enabled(
        &mut self,
        expected_revision: u64,
        person_id: PersonId,
        assignment_id: Uuid,
        enabled: bool,
    ) -> Result<(), AgentFailure> {
        self.check_revision(expected_revision)?;
        self.assignment(person_id, assignment_id)?;
        self.advance()?;
        self.snapshot
            .assignments
            .iter_mut()
            .find(|entry| entry.id == assignment_id)
            .ok_or(AgentFailure::NotFound)?
            .enabled = enabled;
        Ok(())
    }

    pub fn private_state(
        &self,
        person_id: PersonId,
        assignment_id: Uuid,
    ) -> Result<ExpertPrivateState, AgentFailure> {
        Ok(self
            .assignment(person_id, assignment_id)?
            .private_state
            .clone())
    }

    pub fn binding(
        &self,
        person_id: PersonId,
        assignment_id: Uuid,
    ) -> Result<ExpertBindingState, AgentFailure> {
        Ok(self.assignment(person_id, assignment_id)?.binding.clone())
    }

    pub fn execution_selection(
        &self,
        person_id: PersonId,
        admission: &ExpertAdmissionIdentity,
    ) -> Result<ExpertExecutionSelection, AgentFailure> {
        let resolved = self.resolve_admitted(person_id, admission)?;
        ExpertExecutionSelection::from_binding(&resolved.manifest, &resolved.assignment.binding)
    }

    pub fn validate_current_execution_selection(
        &self,
        person_id: PersonId,
        admission: &ExpertAdmissionIdentity,
        selection: &ExpertExecutionSelection,
        require_enabled: bool,
    ) -> Result<(), AgentFailure> {
        selection.validate()?;
        let resolved = self.resolve_admitted(person_id, admission)?;
        if require_enabled {
            self.validate_active_assignment(person_id, admission.assignment_id)?;
        }
        let current = ExpertExecutionSelection::from_binding(
            &resolved.manifest,
            &resolved.assignment.binding,
        )?;
        if &current != selection {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    pub fn replace_binding(
        &mut self,
        person_id: PersonId,
        operation_id: Uuid,
        command: ExpertBindingCommand,
    ) -> Result<ExpertBindingState, AgentFailure> {
        if operation_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let assignment = self.assignment(person_id, command.assignment_id)?;
        let installation = self.installation(assignment.installation_id)?;
        let manifest = self.manifest(&installation.package)?;
        if command.package != installation.package
            || command.definition_revision != manifest.definition.definition_revision
        {
            return Err(AgentFailure::Conflict);
        }
        let requirement = manifest
            .source_requirements
            .iter()
            .find(|requirement| requirement.key == command.requirement_key)
            .ok_or(AgentFailure::NotFound)?;
        if command.selected.len() > usize::from(requirement.maximum_sources)
            || command.selected.iter().any(|selected| {
                selected.validate().is_err()
                    || selected.capability_id != requirement.capability
                    || selected.contract_version != requirement.contract_version
            })
            || command.selected.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(AgentFailure::InvalidInput);
        }
        let digest_bytes = serde_json::to_vec(&(
            "floe.expert-binding-command.sha256.v1",
            self.instance_id(),
            person_id,
            command.assignment_id,
            assignment.installation_id,
            &command.package,
            command.definition_revision,
            &command.requirement_key,
            &requirement.capability,
            requirement.contract_version,
            command.expected_binding_revision,
            &command.selected,
        ))
        .map_err(|_| AgentFailure::InvalidInput)?;
        let digest: [u8; 32] = Sha256::digest(digest_bytes).into();
        if let Some(receipt) = &assignment.binding.last_operation {
            if receipt.operation_id == operation_id {
                return if receipt.command_digest == digest
                    && receipt.resulting_revision == assignment.binding.revision
                {
                    Ok(assignment.binding.clone())
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
        }
        if assignment.binding.revision != command.expected_binding_revision {
            return Err(AgentFailure::Conflict);
        }
        let next_revision = assignment
            .binding
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let mut next = self.snapshot();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let binding = &mut next
            .assignments
            .iter_mut()
            .find(|entry| entry.id == command.assignment_id && entry.person_id == person_id)
            .ok_or(AgentFailure::NotFound)?
            .binding;
        binding.revision = next_revision;
        binding
            .entries
            .iter_mut()
            .find(|entry| entry.requirement_key == command.requirement_key)
            .ok_or(AgentFailure::InvalidInput)?
            .selected = command.selected;
        binding.last_operation = Some(BindingOperationReceipt {
            operation_id,
            command_digest: digest,
            resulting_revision: next_revision,
        });
        *self = Self::restore(next, self.instance_id())?;
        self.binding(person_id, command.assignment_id)
    }

    pub fn complete(
        &mut self,
        resolved: &ResolvedExpert,
        invocation_id: Uuid,
    ) -> Result<u64, AgentFailure> {
        self.check_revision(resolved.registry_revision)?;
        let previous = &resolved.assignment.private_state;
        if previous.last_invocation_id == Some(invocation_id) {
            return Err(AgentFailure::Conflict);
        }
        let revision = previous
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        self.advance()?;
        self.snapshot
            .assignments
            .iter_mut()
            .find(|entry| entry.id == resolved.assignment.id)
            .ok_or(AgentFailure::NotFound)?
            .private_state = ExpertPrivateState {
            schema_version: 1,
            revision,
            completed_invocations: revision,
            last_invocation_id: Some(invocation_id),
        };
        Ok(revision)
    }

    pub fn settle_registered_expert_invocation(
        &self,
        owner: impl Into<String>,
        person_id: PersonId,
        admission: ExpertAdmissionIdentity,
        invocation_id: Uuid,
        dependencies: Vec<floe_agent_contract::ContextDependency>,
        task_result: String,
    ) -> Result<crate::ExpertSettlement, AgentFailure> {
        let resolved = self.resolve_admitted(person_id, &admission)?;
        if resolved.assignment.private_state.last_invocation_id != Some(invocation_id) {
            return Err(AgentFailure::Conflict);
        }
        let expected = resolved
            .assignment
            .private_state
            .revision
            .checked_sub(1)
            .ok_or(AgentFailure::Conflict)?;
        Ok(crate::ExpertSettlement::new(
            owner,
            admission,
            expected,
            resolved.assignment.private_state,
            invocation_id,
            dependencies,
            task_result,
        ))
    }

    fn manifest(&self, package: &PackageRef) -> Result<&ExpertManifest, AgentFailure> {
        self.snapshot
            .manifests
            .iter()
            .find(|manifest| &manifest.package == package)
            .ok_or(AgentFailure::NotFound)
    }

    fn installation(&self, id: Uuid) -> Result<&PackageInstallation, AgentFailure> {
        self.snapshot
            .installations
            .iter()
            .find(|installation| installation.id == id)
            .ok_or(AgentFailure::NotFound)
    }

    fn assignment(
        &self,
        person_id: PersonId,
        id: Uuid,
    ) -> Result<&PackageAssignment, AgentFailure> {
        self.snapshot
            .assignments
            .iter()
            .find(|assignment| assignment.id == id && assignment.person_id == person_id)
            .ok_or(AgentFailure::NotFound)
    }

    fn check_revision(&self, expected: u64) -> Result<(), AgentFailure> {
        if self.revision() == expected {
            Ok(())
        } else {
            Err(AgentFailure::Conflict)
        }
    }

    fn advance(&mut self) -> Result<(), AgentFailure> {
        self.snapshot.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        Ok(())
    }
}

pub fn eligible_cards_for_availability(
    cards: &[crate::AgentCard],
    availability: floe_inference::InferenceAvailability,
) -> Vec<crate::AgentCard> {
    use floe_agent_contract::ModelPlacement;
    use floe_inference::InferenceExecutionConstraint;
    let mut seen = HashSet::new();
    cards
        .iter()
        .filter(|card| {
            (availability.can_execute(InferenceExecutionConstraint::DeviceOnly)
                && card.runs_at(ModelPlacement::DeviceLocal))
                || (availability.can_execute(InferenceExecutionConstraint::RemoteOnly)
                    && card.runs_at(ModelPlacement::Remote))
        })
        .filter(|card| seen.insert(card.id.clone()))
        .cloned()
        .collect()
}
