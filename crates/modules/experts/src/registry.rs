use floe_agent_contract::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{AgentFailure, DataClass, PackageKind, PackageRef};

mod expert_setup;

pub use expert_setup::{
    BuiltinExpertSetup, BuiltinExpertSetupResult, ExpertPackaging, ExpertSetupSpec,
    eligible_cards_for_availability,
};

/// The identity of a builtin agent in the common delegation path.
///
/// The generic registry never interprets it; the crate that owns the builtin
/// Experts maps it to its own kind.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    pub fn try_new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
        .then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Durable setup records the registry stores for an agent. The registry keeps
// package topology only; source permission lives in Access.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertSetupReceipt {
    pub setup_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub assignments: Vec<BuiltinExpertAssignmentReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertAssignmentReceipt {
    pub expert: AgentId,
    pub tool_installation_id: Uuid,
    pub expert_installation_id: Uuid,
    pub tool_assignment_id: Uuid,
    pub expert_assignment_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageImplementation {
    TimelineRead { data_class: DataClass },
    Builtin { expert: AgentId },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPackage {
    pub schema_version: u32,
    pub reference: PackageRef,
    pub publisher: String,
    pub implementation: PackageImplementation,
    pub expert_metadata: Option<ExpertMetadata>,
    pub required_tools: Vec<PackageRef>,
    pub state_schema_version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertMetadata {
    pub name: String,
    pub description: String,
    pub domain_tags: Vec<String>,
    pub skills: Vec<String>,
    /// Where this Expert's judgment can run, as its owning crate declared it.
    #[serde(default = "every_placement")]
    pub supported_placements: Vec<floe_agent_contract::ModelPlacement>,
}

fn every_placement() -> Vec<floe_agent_contract::ModelPlacement> {
    vec![
        floe_agent_contract::ModelPlacement::DeviceLocal,
        floe_agent_contract::ModelPlacement::Remote,
    ]
}

impl AgentPackage {
    fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != AGENT_VERSION || self.state_schema_version != 1 {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if !valid_name(&self.reference.id)
            || !valid_name(&self.reference.version)
            || !valid_name(&self.publisher)
            || (self.reference.kind == PackageKind::Tool && self.expert_metadata.is_some())
            || (self.reference.kind == PackageKind::Expert && self.expert_metadata.is_none())
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(metadata) = &self.expert_metadata {
            crate::AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: crate::A2A_PROTOCOL_VERSION.into(),
                id: self.reference.id.clone(),
                version: self.reference.version.clone(),
                name: metadata.name.clone(),
                description: metadata.description.clone(),
                domain_tags: metadata.domain_tags.clone(),
                skills: metadata.skills.clone(),
                supported_placements: metadata.supported_placements.clone(),
            }
            .validate()?;
        }
        match &self.implementation {
            PackageImplementation::TimelineRead { data_class }
                if self.reference.kind == PackageKind::Tool
                    && self.required_tools.is_empty()
                    && !matches!(data_class, DataClass::Credential | DataClass::DeviceOnlyRaw) => {}
            PackageImplementation::Builtin { .. } if self.reference.kind == PackageKind::Expert => {
                self.validate_requirements()?;
            }
            _ => return Err(AgentFailure::CapabilityDenied),
        }
        Ok(())
    }

    fn validate_requirements(&self) -> Result<(), AgentFailure> {
        if self.required_tools.len() != 1
            || self.required_tools.iter().any(|tool| {
                tool.kind != PackageKind::Tool
                    || !valid_name(&tool.id)
                    || !valid_name(&tool.version)
            })
        {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }
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
    pub granted_tool_assignments: Vec<Uuid>,
    pub private_state: ExpertPrivateState,
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
    pub packages: Vec<AgentPackage>,
    pub installations: Vec<PackageInstallation>,
    pub assignments: Vec<PackageAssignment>,
    pub builtin_setups: Vec<BuiltinExpertSetupReceipt>,
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
    pub granted_tool_count: usize,
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

/// One Expert resolved against the registry: the package installed for it, the
/// assignment that grants it, and the data class it may read at.
#[derive(Clone)]
pub struct ResolvedExpert {
    pub registry_revision: u64,
    pub package: AgentPackage,
    pub assignment: PackageAssignment,
    pub data_class: DataClass,
}

impl AgentRegistry {
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
        let installed = self.package(&installation.package)?;
        if &installed.reference != package || package.kind != PackageKind::Expert {
            return Err(AgentFailure::Conflict);
        }
        let [required_tool] = installed.required_tools.as_slice() else {
            return Err(AgentFailure::Conflict);
        };
        let PackageImplementation::TimelineRead {
            data_class: installed_class,
        } = self.package(required_tool)?.implementation else {
            return Err(AgentFailure::Conflict);
        };
        if installed_class != data_class {
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
        self.validate_tool_linkage(assignment)
    }

    pub fn overview(&self, person_id: PersonId) -> RegistryOverview {
        RegistryOverview {
            schema_version: AGENT_VERSION,
            person_id,
            instance_id: self.instance_id(),
            revision: self.revision(),
            installations: self.snapshot.installations.clone(),
            assignments: self
                .snapshot
                .assignments
                .iter()
                .filter(|assignment| assignment.person_id == person_id)
                .map(|assignment| AssignmentOverview {
                    id: assignment.id,
                    installation_id: assignment.installation_id,
                    enabled: assignment.enabled,
                    granted_tool_count: assignment.granted_tool_assignments.len(),
                    state_revision: assignment.private_state.revision,
                    completed_invocations: assignment.private_state.completed_invocations,
                })
                .collect(),
        }
    }

    pub fn new(instance_id: Uuid) -> Self {
        Self {
            snapshot: RegistrySnapshot {
                schema_version: AGENT_VERSION,
                instance_id,
                revision: 0,
                packages: vec![],
                installations: vec![],
                assignments: vec![],
                builtin_setups: vec![],
            },
        }
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

    /// Restore a registry without validating any Expert-specific setup.
    pub fn restore(snapshot: RegistrySnapshot, instance_id: Uuid) -> Result<Self, AgentFailure> {
        Self::restore_with_setups(snapshot, instance_id, &NoSetupValidator)
    }

    /// Restore a registry, letting the supplied owner validate its own setup.
    pub fn restore_with_setups(
        snapshot: RegistrySnapshot,
        instance_id: Uuid,
        setups: &impl SetupValidator,
    ) -> Result<Self, AgentFailure> {
        if snapshot.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if snapshot.instance_id != instance_id {
            return Err(AgentFailure::NotFound);
        }
        if snapshot.packages.len() > 64
            || snapshot.installations.len() > 128
            || snapshot.assignments.len() > 256
            || snapshot.builtin_setups.len() > 64
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let registry = Self { snapshot };
        for (index, package) in registry.snapshot.packages.iter().enumerate() {
            package.validate()?;
            if registry.snapshot.packages[..index]
                .iter()
                .any(|other| other.reference == package.reference)
            {
                return Err(AgentFailure::Conflict);
            }
        }
        for (index, installation) in registry.snapshot.installations.iter().enumerate() {
            registry.package(&installation.package)?;
            if registry.snapshot.installations[..index]
                .iter()
                .any(|other| other.id == installation.id)
            {
                return Err(AgentFailure::Conflict);
            }
        }
        for (index, assignment) in registry.snapshot.assignments.iter().enumerate() {
            if registry.snapshot.assignments[..index]
                .iter()
                .any(|other| other.id == assignment.id)
            {
                return Err(AgentFailure::Conflict);
            }
            registry.validate_grants(assignment)?;
            let state = &assignment.private_state;
            if state.schema_version != 1
                || state.revision != state.completed_invocations
                || state.revision > registry.revision()
                || (state.revision == 0) != state.last_invocation_id.is_none()
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        // The registry keeps these records, so it validates them itself; the
        // port is for setup an owner outside this crate adds on top.
        registry.validate_builtin_setups()?;
        setups.validate_setups(&registry)?;
        Ok(registry)
    }

    pub fn register(
        &mut self,
        expected_revision: u64,
        package: AgentPackage,
    ) -> Result<(), AgentFailure> {
        self.check_revision(expected_revision)?;
        package.validate()?;
        if self.snapshot.packages.len() >= 64 {
            return Err(AgentFailure::BudgetExceeded);
        }
        if self
            .snapshot
            .packages
            .iter()
            .any(|entry| entry.reference == package.reference)
        {
            return Err(AgentFailure::Conflict);
        }
        self.advance()?;
        self.snapshot.packages.push(package);
        Ok(())
    }

    pub fn install(
        &mut self,
        expected_revision: u64,
        package: &PackageRef,
    ) -> Result<Uuid, AgentFailure> {
        self.check_revision(expected_revision)?;
        self.package(package)?;
        if self.snapshot.installations.len() >= 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let id = Uuid::new_v4();
        self.advance()?;
        self.snapshot.installations.push(PackageInstallation {
            id,
            package: package.clone(),
            enabled: false,
        });
        Ok(id)
    }

    pub fn assign(
        &mut self,
        expected_revision: u64,
        person_id: PersonId,
        installation_id: Uuid,
        granted_tool_assignments: Vec<Uuid>,
    ) -> Result<Uuid, AgentFailure> {
        self.check_revision(expected_revision)?;
        if self.snapshot.assignments.len() >= 256 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let assignment = PackageAssignment {
            id: Uuid::new_v4(),
            person_id,
            installation_id,
            enabled: false,
            granted_tool_assignments,
            private_state: ExpertPrivateState::default(),
        };
        self.validate_grants(&assignment)?;
        self.advance()?;
        let id = assignment.id;
        self.snapshot.assignments.push(assignment);
        Ok(id)
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


    /// Resolve one built-in assignment to the Expert it installs.
    ///
    /// This validates Registry/package/tool/private-state identity only.
    /// Source evidence is validated by Access/Context, never here.
    pub fn resolve_builtin(
        &self,
        instance_id: Uuid,
        person_id: PersonId,
        assignment_id: Uuid,
        expected_revision: u64,
        expected_expert: &AgentId,
    ) -> Result<ResolvedExpert, AgentFailure> {
        if instance_id != self.snapshot.instance_id {
            return Err(AgentFailure::NotFound);
        }
        self.check_revision(expected_revision)?;
        let assignment = self.assignment(person_id, assignment_id)?;
        let installation = self.installation(assignment.installation_id)?;
        if !assignment.enabled || !installation.enabled {
            return Err(AgentFailure::CapabilityDenied);
        }
        let package = self.package(&installation.package)?;
        if package.reference.kind != PackageKind::Expert
            || package.reference.id != expected_expert.as_str()
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        match &package.implementation {
            PackageImplementation::Builtin { expert } if expert == expected_expert => {}
            _ => return Err(AgentFailure::CapabilityDenied),
        }
        self.validate_tool_linkage(assignment)?;
        let tool = self.assignment(person_id, assignment.granted_tool_assignments[0])?;
        let tool_installation = self.installation(tool.installation_id)?;
        if !tool.enabled || !tool_installation.enabled {
            return Err(AgentFailure::CapabilityDenied);
        }
        let PackageImplementation::TimelineRead { data_class } =
            self.package(&tool_installation.package)?.implementation
        else {
            return Err(AgentFailure::CapabilityDenied);
        };
        Ok(ResolvedExpert {
            registry_revision: self.revision(),
            package: package.clone(),
            assignment: assignment.clone(),
            data_class,
        })
    }

    /// Record that this Expert completed `invocation_id` exactly once.
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
        let assignment = self
            .snapshot
            .assignments
            .iter_mut()
            .find(|entry| entry.id == resolved.assignment.id)
            .ok_or(AgentFailure::NotFound)?;
        assignment.private_state = ExpertPrivateState {
            schema_version: 1,
            revision,
            completed_invocations: revision,
            last_invocation_id: Some(invocation_id),
        };
        Ok(revision)
    }

    pub(crate) fn validate_grants(
        &self,
        assignment: &PackageAssignment,
    ) -> Result<(), AgentFailure> {
        self.validate_tool_linkage(assignment)
    }

    pub(crate) fn validate_tool_linkage(
        &self,
        assignment: &PackageAssignment,
    ) -> Result<(), AgentFailure> {
        let installation = self.installation(assignment.installation_id)?;
        let package = self.package(&installation.package)?;
        match package.reference.kind {
            PackageKind::Tool if assignment.granted_tool_assignments.is_empty() => Ok(()),
            PackageKind::Expert if assignment.granted_tool_assignments.len() == 1 => {
                let tool =
                    self.assignment(assignment.person_id, assignment.granted_tool_assignments[0])?;
                let tool_installation = self.installation(tool.installation_id)?;
                if package.required_tools != [tool_installation.package.clone()] {
                    return Err(AgentFailure::CapabilityDenied);
                }
                Ok(())
            }
            _ => Err(AgentFailure::CapabilityDenied),
        }
    }

    pub(crate) fn package(&self, reference: &PackageRef) -> Result<&AgentPackage, AgentFailure> {
        self.snapshot
            .packages
            .iter()
            .find(|package| &package.reference == reference)
            .ok_or(AgentFailure::NotFound)
    }

    pub(crate) fn installation(&self, id: Uuid) -> Result<&PackageInstallation, AgentFailure> {
        self.snapshot
            .installations
            .iter()
            .find(|installation| installation.id == id)
            .ok_or(AgentFailure::NotFound)
    }

    pub(crate) fn assignment(
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

    pub(crate) fn check_revision(&self, expected: u64) -> Result<(), AgentFailure> {
        if self.revision() == expected {
            Ok(())
        } else {
            Err(AgentFailure::Conflict)
        }
    }

    pub(crate) fn advance(&mut self) -> Result<(), AgentFailure> {
        self.snapshot.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        Ok(())
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
}


/// Validation of Expert-specific durable setup recorded in a registry snapshot.
///
/// The generic registry does not know what any builtin Expert's setup means. The
/// crate that owns those Experts implements this port and the caller supplies it.
pub trait SetupValidator {
    fn validate_setups(&self, registry: &AgentRegistry) -> Result<(), AgentFailure>;
}

/// A registry restored without any Expert-specific setup to validate.
pub struct NoSetupValidator;

impl SetupValidator for NoSetupValidator {
    fn validate_setups(&self, _registry: &AgentRegistry) -> Result<(), AgentFailure> {
        Ok(())
    }
}

impl AgentRegistry {
    /// Stage this registry for the settlement of one registered invocation.
    ///
    /// The Task owner commits the staged registry and the Expert's result
    /// together, or commits neither.
    pub fn settle_registered_expert_invocation(
        &self,
        owner: impl Into<String>,
        expected_revision: u64,
        assignment_id: Uuid,
        invocation_id: Uuid,
        dependencies: Vec<floe_agent_contract::ContextDependency>,
        task_result: String,
    ) -> crate::ExpertSettlement {
        crate::ExpertSettlement::new(
            owner,
            expected_revision,
            self.snapshot(),
            assignment_id,
            invocation_id,
            dependencies,
            task_result,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_names_the_invoked_assignment_and_stages_the_current_snapshot() {
        let registry = AgentRegistry::new(Uuid::new_v4());
        let assignment_id = Uuid::new_v4();
        let invocation_id = Uuid::new_v4();
        let settlement = registry.settle_registered_expert_invocation(
            "test-owner",
            registry.revision(),
            assignment_id,
            invocation_id,
            vec![],
            "task result".into(),
        );
        assert_eq!(settlement.assignment_id, assignment_id);
        assert_eq!(settlement.invocation_id, invocation_id);
        assert_eq!(settlement.expected_registry_revision, registry.revision());
        assert_eq!(settlement.staged_registry, registry.snapshot());
        assert_eq!(settlement.task_result, "task result");
        assert!(settlement.dependencies.is_empty());
    }
}
