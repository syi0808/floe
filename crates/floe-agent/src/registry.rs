use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AGENT_VERSION, AgentFailure, DataClass};

mod calendar_setup;
pub use calendar_setup::*;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Tool,
    Expert,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub kind: PackageKind,
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertRule {
    FindFocusWindow { minimum_minutes: u16 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageImplementation {
    TimelineRead { data_class: DataClass },
    Schedule,
    Declarative { rules: Vec<ExpertRule> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPackage {
    pub schema_version: u32,
    pub reference: PackageRef,
    pub publisher: String,
    pub implementation: PackageImplementation,
    pub required_tools: Vec<PackageRef>,
    pub state_schema_version: u32,
}

impl AgentPackage {
    fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != AGENT_VERSION || self.state_schema_version != 1 {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if !valid_name(&self.reference.id)
            || !valid_name(&self.reference.version)
            || !valid_name(&self.publisher)
        {
            return Err(AgentFailure::InvalidInput);
        }
        match &self.implementation {
            PackageImplementation::TimelineRead { data_class }
                if self.reference.kind == PackageKind::Tool
                    && self.required_tools.is_empty()
                    && !matches!(data_class, DataClass::Credential | DataClass::DeviceOnlyRaw) => {}
            PackageImplementation::Schedule if self.reference.kind == PackageKind::Expert => {
                self.validate_requirements()?;
            }
            PackageImplementation::Declarative { rules }
                if self.reference.kind == PackageKind::Expert && rules.len() == 1 =>
            {
                self.validate_requirements()?;
                for rule in rules {
                    match rule {
                        ExpertRule::FindFocusWindow { minimum_minutes }
                            if (1..=240).contains(minimum_minutes) => {}
                        _ => return Err(AgentFailure::InvalidInput),
                    }
                }
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
    pub granted_view_handles: Vec<Uuid>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calendar_views: Vec<CalendarViewBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calendar_setups: Vec<CalendarExpertSetupReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revoked_calendar_setups: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarViewBinding {
    pub handle: Uuid,
    pub person_id: PersonId,
    pub provider: floe_domain::CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub enabled: bool,
}

impl CalendarViewBinding {
    pub fn data_class(&self) -> DataClass {
        match self.provider {
            floe_domain::CalendarProvider::Fixture => DataClass::Synthetic,
            floe_domain::CalendarProvider::EventKit => DataClass::Personal,
        }
    }

    fn validate(&self) -> Result<(), AgentFailure> {
        if self.handle.is_nil()
            || self.calendar_ids.is_empty()
            || self.calendar_ids.len() > 4
            || self
                .calendar_ids
                .iter()
                .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
            || self.calendar_ids.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
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
    pub granted_view_count: usize,
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
    CalendarView { id: Uuid, enabled: bool },
}

pub struct AgentRegistry {
    snapshot: RegistrySnapshot,
}

#[derive(Clone)]
pub(crate) struct ResolvedExpert {
    pub registry_revision: u64,
    pub package: AgentPackage,
    pub assignment: PackageAssignment,
    pub data_class: DataClass,
}

impl AgentRegistry {
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
                    granted_view_count: assignment.granted_view_handles.len(),
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
                calendar_views: vec![],
                calendar_setups: vec![],
                revoked_calendar_setups: vec![],
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

    pub fn restore(snapshot: RegistrySnapshot, instance_id: Uuid) -> Result<Self, AgentFailure> {
        if snapshot.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if snapshot.instance_id != instance_id {
            return Err(AgentFailure::NotFound);
        }
        if snapshot.packages.len() > 64
            || snapshot.installations.len() > 128
            || snapshot.assignments.len() > 256
            || snapshot.calendar_views.len() > 256
            || snapshot.calendar_setups.len() > 64
            || snapshot.revoked_calendar_setups.len() > 64
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let registry = Self { snapshot };
        for (index, binding) in registry.snapshot.calendar_views.iter().enumerate() {
            binding.validate()?;
            if registry.snapshot.calendar_views[..index]
                .iter()
                .any(|other| other.handle == binding.handle)
            {
                return Err(AgentFailure::Conflict);
            }
        }
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
        registry.validate_calendar_setups()?;
        for (index, setup_id) in registry.snapshot.revoked_calendar_setups.iter().enumerate() {
            if setup_id.is_nil()
                || registry.snapshot.revoked_calendar_setups[..index].contains(setup_id)
                || !registry
                    .snapshot
                    .calendar_setups
                    .iter()
                    .any(|setup| setup.setup_id == *setup_id)
            {
                return Err(AgentFailure::Conflict);
            }
        }
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
        granted_view_handles: Vec<Uuid>,
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
            granted_view_handles,
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

    pub fn record_result(
        &mut self,
        expected_revision: u64,
        result: &crate::ExpertResult,
    ) -> Result<(), AgentFailure> {
        let resolved = self.resolve_result(expected_revision, result)?;
        if result.state_revision
            != resolved
                .assignment
                .private_state
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.complete(&resolved, result.invocation_id)?;
        Ok(())
    }

    pub fn validate_recorded_result(
        &self,
        result: &crate::ExpertResult,
    ) -> Result<(), AgentFailure> {
        let resolved = self.resolve_result(self.revision(), result)?;
        if result.state_revision == 0
            || result.state_revision > resolved.assignment.private_state.revision
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn validate_historical_result(
        &self,
        result: &crate::ExpertResult,
    ) -> Result<(), AgentFailure> {
        if result.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if result.instance_id != self.instance_id() {
            return Err(AgentFailure::NotFound);
        }
        let assignment = self.assignment(result.person_id, result.assignment_id)?;
        let installation = self.installation(assignment.installation_id)?;
        let package = self.package(&installation.package)?;
        if package.reference.kind != PackageKind::Expert || package.required_tools.len() != 1 {
            return Err(AgentFailure::CapabilityDenied);
        }
        let PackageImplementation::TimelineRead { data_class } =
            self.package(&package.required_tools[0])?.implementation
        else {
            return Err(AgentFailure::CapabilityDenied);
        };
        let resolved = ResolvedExpert {
            registry_revision: self.revision(),
            package: package.clone(),
            assignment: assignment.clone(),
            data_class,
        };
        Self::validate_result_content(result, &resolved)?;
        if result.state_revision == 0 || result.state_revision > assignment.private_state.revision {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    fn resolve_result(
        &self,
        expected_revision: u64,
        result: &crate::ExpertResult,
    ) -> Result<ResolvedExpert, AgentFailure> {
        if result.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        let resolved = self.resolve(
            result.instance_id,
            result.person_id,
            result.assignment_id,
            expected_revision,
            &[result.view_handle],
        )?;
        Self::validate_result_content(result, &resolved)?;
        Ok(resolved)
    }

    fn validate_result_content(
        result: &crate::ExpertResult,
        resolved: &ResolvedExpert,
    ) -> Result<(), AgentFailure> {
        if result.package != resolved.package.reference
            || result.data_class != resolved.data_class
            || result.view_calls != 1
            || result.insights.is_empty()
            || result.insights.len() > 8
            || result.action_proposals.len() > 1
            || result.model_calls == 1
            || result.model_calls > 10
            || result
                .summary
                .as_ref()
                .is_some_and(|summary| summary.trim().is_empty() || summary.len() > 2048)
            || (result.summary.is_some() != (2..=10).contains(&result.model_calls))
            || (matches!(
                resolved.package.implementation,
                PackageImplementation::Declarative { .. }
            ) && result.model_calls != 0)
            || result.source_handle.trim().is_empty()
            || result.source_handle.len() > 128
            || result.insights.iter().any(|insight| match insight {
                crate::ExpertInsight::Commitment {
                    untrusted_title,
                    starts_at_unix_ms,
                    ends_at_unix_ms,
                    ..
                } => {
                    untrusted_title.len() > 256
                        || !valid_interval(*starts_at_unix_ms, *ends_at_unix_ms)
                }
                crate::ExpertInsight::FocusWindow {
                    starts_at_unix_ms,
                    ends_at_unix_ms,
                } => !valid_interval(*starts_at_unix_ms, *ends_at_unix_ms),
                crate::ExpertInsight::NoFocusWindow => false,
            })
            || result.action_proposals.iter().any(|proposal| {
                proposal.view_handle != result.view_handle
                    || !result
                        .insights
                        .contains(&crate::ExpertInsight::FocusWindow {
                            starts_at_unix_ms: proposal.starts_at_unix_ms,
                            ends_at_unix_ms: proposal.ends_at_unix_ms,
                        })
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn register_calendar_view(
        &mut self,
        expected_revision: u64,
        person_id: PersonId,
        provider: floe_domain::CalendarProvider,
        mut calendar_ids: Vec<String>,
    ) -> Result<Uuid, AgentFailure> {
        self.check_revision(expected_revision)?;
        if self.snapshot.calendar_views.len() >= 256 {
            return Err(AgentFailure::BudgetExceeded);
        }
        calendar_ids.sort();
        let binding = CalendarViewBinding {
            handle: Uuid::new_v4(),
            person_id,
            provider,
            calendar_ids,
            enabled: false,
        };
        binding.validate()?;
        let handle = binding.handle;
        self.advance()?;
        self.snapshot.calendar_views.push(binding);
        Ok(handle)
    }

    pub fn set_calendar_view_enabled(
        &mut self,
        expected_revision: u64,
        person_id: PersonId,
        handle: Uuid,
        enabled: bool,
    ) -> Result<(), AgentFailure> {
        self.check_revision(expected_revision)?;
        let index = self
            .snapshot
            .calendar_views
            .iter()
            .position(|binding| binding.person_id == person_id && binding.handle == handle)
            .ok_or(AgentFailure::NotFound)?;
        self.advance()?;
        self.snapshot.calendar_views[index].enabled = enabled;
        Ok(())
    }

    pub fn calendar_view(
        &self,
        person_id: PersonId,
        handle: Uuid,
    ) -> Result<&CalendarViewBinding, AgentFailure> {
        self.snapshot
            .calendar_views
            .iter()
            .find(|binding| {
                binding.person_id == person_id && binding.handle == handle && binding.enabled
            })
            .ok_or(AgentFailure::CapabilityDenied)
    }

    pub fn expert_descriptor(
        &self,
        person_id: PersonId,
        assignment_id: Uuid,
        expected_revision: u64,
        view_handle: Uuid,
    ) -> Result<crate::CapabilityDescriptor, AgentFailure> {
        let resolved = self.resolve(
            self.instance_id(),
            person_id,
            assignment_id,
            expected_revision,
            &[view_handle],
        )?;
        Ok(crate::CapabilityDescriptor {
            schema_version: AGENT_VERSION,
            id: format!("expert.{}", resolved.package.reference.id),
            version: resolved.package.reference.version,
            read_only: true,
            output_data_class: resolved.data_class,
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["briefing", "propose_focus", "analyze"]},
                    "focus_minutes": {"type": "integer", "minimum": 1, "maximum": 240},
                    "request": {"type": "string", "minLength": 1, "maxLength": 2048}
                },
                "required": ["kind"],
                "oneOf": [
                    {
                        "properties": {
                            "kind": {"enum": ["briefing", "propose_focus"]}
                        },
                        "required": ["focus_minutes"]
                    },
                    {
                        "properties": {
                            "kind": {"const": "analyze"}
                        },
                        "required": ["request"]
                    }
                ],
                "additionalProperties": false
            })),
        })
    }

    pub(crate) fn resolve(
        &self,
        instance_id: Uuid,
        person_id: PersonId,
        assignment_id: Uuid,
        expected_revision: u64,
        views: &[Uuid],
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
            || views.len() != 1
            || views
                .iter()
                .any(|view| !assignment.granted_view_handles.contains(view))
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        self.validate_grants(assignment)?;
        let tool = self.assignment(person_id, assignment.granted_tool_assignments[0])?;
        let tool_installation = self.installation(tool.installation_id)?;
        if !tool.enabled
            || !tool_installation.enabled
            || !tool.granted_view_handles.contains(&views[0])
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let PackageImplementation::TimelineRead { data_class } =
            self.package(&tool_installation.package)?.implementation
        else {
            return Err(AgentFailure::CapabilityDenied);
        };
        if self
            .snapshot
            .calendar_views
            .iter()
            .any(|binding| binding.handle == views[0])
            && self.calendar_view(person_id, views[0])?.data_class() != data_class
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(ResolvedExpert {
            registry_revision: self.revision(),
            package: package.clone(),
            assignment: assignment.clone(),
            data_class,
        })
    }

    pub(crate) fn complete(
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

    fn validate_grants(&self, assignment: &PackageAssignment) -> Result<(), AgentFailure> {
        let installation = self.installation(assignment.installation_id)?;
        let package = self.package(&installation.package)?;
        for handle in &assignment.granted_view_handles {
            if let Some(binding) = self
                .snapshot
                .calendar_views
                .iter()
                .find(|binding| binding.handle == *handle)
            {
                if binding.person_id != assignment.person_id {
                    return Err(AgentFailure::CapabilityDenied);
                }
                if let PackageImplementation::TimelineRead { data_class } = package.implementation
                    && data_class != binding.data_class()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
        }
        if assignment.granted_view_handles.len() > 4
            || assignment
                .granted_view_handles
                .iter()
                .enumerate()
                .any(|(index, view)| assignment.granted_view_handles[..index].contains(view))
        {
            return Err(AgentFailure::InvalidInput);
        }
        match package.reference.kind {
            PackageKind::Tool if assignment.granted_tool_assignments.is_empty() => Ok(()),
            PackageKind::Expert if assignment.granted_tool_assignments.len() == 1 => {
                let tool =
                    self.assignment(assignment.person_id, assignment.granted_tool_assignments[0])?;
                let tool_installation = self.installation(tool.installation_id)?;
                if package.required_tools != [tool_installation.package.clone()]
                    || assignment
                        .granted_view_handles
                        .iter()
                        .any(|view| !tool.granted_view_handles.contains(view))
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
                Ok(())
            }
            _ => Err(AgentFailure::CapabilityDenied),
        }
    }

    fn package(&self, reference: &PackageRef) -> Result<&AgentPackage, AgentFailure> {
        self.snapshot
            .packages
            .iter()
            .find(|package| &package.reference == reference)
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

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
}

fn valid_interval(start: u64, end: u64) -> bool {
    end > start && end - start <= 86_400_000
}
