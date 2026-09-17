//! Installing the Schedule Expert against one bound calendar view.
//!
//! The registry keeps the binding, the installation and the assignment. Which
//! agent id and metadata carry the Expert is the owning crate's declaration,
//! supplied as [`ExpertPackaging`]; the data class follows from the provider
//! the registry already records.

use floe_agent_contract::{CalendarProvider, CalendarScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::{AGENT_VERSION, PersonId};
use floe_agent_contract::{AgentFailure, DataClass};

use super::{
    AgentPackage, AgentRegistry, CalendarExpertSetupReceipt, CalendarViewBinding, ExpertMetadata,
    ExpertPrivateState, PackageAssignment, PackageImplementation, PackageInstallation,
    RegistryOverview, RegistrySnapshot,
};
use floe_agent_contract::{PackageKind, PackageRef};

/// How the crate that owns an Expert wants it packaged in the registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpertPackaging {
    pub expert: super::AgentId,
    pub tool_id: String,
    pub version: String,
    pub publisher: String,
    pub metadata: ExpertMetadata,
    pub state_schema_version: u32,
}

impl ExpertPackaging {
    /// The tool and Expert packages this declaration installs for `data_class`.
    pub fn packages(&self, data_class: DataClass) -> [AgentPackage; 2] {
        let tool = PackageRef {
            kind: PackageKind::Tool,
            id: self.tool_id.clone(),
            version: self.version.clone(),
        };
        [
            AgentPackage {
                schema_version: AGENT_VERSION,
                reference: tool.clone(),
                publisher: self.publisher.clone(),
                implementation: PackageImplementation::TimelineRead { data_class },
                expert_metadata: None,
                required_tools: vec![],
                state_schema_version: self.state_schema_version,
            },
            AgentPackage {
                schema_version: AGENT_VERSION,
                reference: PackageRef {
                    kind: PackageKind::Expert,
                    id: self.expert.as_str().to_owned(),
                    version: self.version.clone(),
                },
                publisher: self.publisher.clone(),
                implementation: PackageImplementation::Builtin {
                    expert: self.expert.clone(),
                },
                expert_metadata: Some(self.metadata.clone()),
                required_tools: vec![tool],
                state_schema_version: self.state_schema_version,
            },
        ]
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertSetup {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub provider: CalendarProvider,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScope,
    pub connection_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_authority: Option<floe_agent_contract::SourceAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertSetupResult {
    pub setup: CalendarExpertSetupReceipt,
    pub registry: RegistryOverview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarAccessConfiguration {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub change: CalendarAccessChange,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarAccessChange {
    SetEnabled {
        enabled: bool,
    },
    SetScope {
        provider: CalendarProvider,
        device_id: String,
        calendar_ids: Vec<String>,
        connection_scope: CalendarScope,
        connection_revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_authority: Option<floe_agent_contract::SourceAuthority>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reviewed_native_subject_fingerprint: Option<String>,
    },
    Remove {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertOverview {
    pub registry: RegistryOverview,
    pub views: Vec<CalendarViewBinding>,
    pub setups: Vec<CalendarExpertSetupReceipt>,
}

impl AgentRegistry {
    pub fn calendar_expert_overview(&self, person_id: PersonId) -> CalendarExpertOverview {
        let active: Vec<_> = self
            .snapshot
            .calendar_setups
            .iter()
            .filter(|setup| {
                setup.person_id == person_id
                    && !self
                        .snapshot
                        .revoked_calendar_setups
                        .contains(&setup.setup_id)
            })
            .cloned()
            .collect();
        let active_views: Vec<_> = active.iter().map(|setup| setup.view_handle).collect();
        let revoked: Vec<_> = self
            .snapshot
            .calendar_setups
            .iter()
            .filter(|setup| {
                setup.person_id == person_id
                    && self
                        .snapshot
                        .revoked_calendar_setups
                        .contains(&setup.setup_id)
            })
            .collect();
        let revoked_installations: Vec<_> = revoked
            .iter()
            .flat_map(|setup| [setup.tool_installation_id, setup.expert_installation_id])
            .collect();
        let revoked_assignments: Vec<_> = revoked
            .iter()
            .flat_map(|setup| [setup.tool_assignment_id, setup.expert_assignment_id])
            .collect();
        let mut overview = self.overview(person_id);
        overview
            .installations
            .retain(|entry| !revoked_installations.contains(&entry.id));
        overview
            .assignments
            .retain(|entry| !revoked_assignments.contains(&entry.id));
        CalendarExpertOverview {
            registry: overview,
            views: self
                .snapshot
                .calendar_views
                .iter()
                .filter(|binding| {
                    binding.person_id == person_id && active_views.contains(&binding.handle)
                })
                .cloned()
                .collect(),
            setups: active,
        }
    }

    pub fn calendar_expert_setup(
        &self,
        person_id: PersonId,
        request: &CalendarExpertSetup,
    ) -> Result<Option<CalendarExpertSetupReceipt>, AgentFailure> {
        if request.instance_id != self.instance_id() {
            return Err(AgentFailure::NotFound);
        }
        let binding = setup_binding(person_id, request)?;
        let Some(receipt) = self
            .snapshot
            .calendar_setups
            .iter()
            .find(|receipt| receipt.setup_id == request.setup_id)
        else {
            return Ok(None);
        };
        let original = self
            .snapshot
            .calendar_views
            .iter()
            .find(|binding| binding.handle == receipt.view_handle)
            .ok_or(AgentFailure::Conflict)?;
        if receipt.person_id != person_id
            || receipt.expected_revision != request.expected_revision
            || original.provider != binding.provider
            || original.device_id != binding.device_id
            || original.calendar_ids != binding.calendar_ids
            || original.connection_scope != binding.connection_scope
            || original.connection_revision != binding.connection_revision
            || original.source_authority != binding.source_authority
            || receipt.connection_scope != binding.connection_scope
            || receipt.connection_revision != binding.connection_revision
            || receipt.source_authority != binding.source_authority
            || receipt.reviewed_native_subject_fingerprint
                != request.reviewed_native_subject_fingerprint
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(Some(receipt.clone()))
    }

    pub fn install_calendar_expert(
        &mut self,
        person_id: PersonId,
        request: &CalendarExpertSetup,
        packaging: &ExpertPackaging,
    ) -> Result<CalendarExpertSetupReceipt, AgentFailure> {
        if let Some(receipt) = self.calendar_expert_setup(person_id, request)? {
            return Ok(receipt);
        }
        self.check_revision(request.expected_revision)?;
        let mut binding = setup_binding(person_id, request)?;
        binding.handle = Uuid::new_v4();
        let receipt = CalendarExpertSetupReceipt {
            setup_id: request.setup_id,
            person_id,
            expected_revision: request.expected_revision,
            connection_scope: request.connection_scope,
            connection_revision: request.connection_revision,
            source_authority: request.source_authority,
            reviewed_native_subject_fingerprint: request
                .reviewed_native_subject_fingerprint
                .clone(),
            view_handle: binding.handle,
            tool_installation_id: Uuid::new_v4(),
            expert_installation_id: Uuid::new_v4(),
            tool_assignment_id: Uuid::new_v4(),
            expert_assignment_id: Uuid::new_v4(),
        };
        let mut next = self.snapshot();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let [tool, expert] = setup_packages(packaging, request.provider);
        for package in [&tool, &expert] {
            match next
                .packages
                .iter()
                .find(|entry| entry.reference == package.reference)
            {
                Some(entry) if entry == package => {}
                Some(_) => return Err(AgentFailure::Conflict),
                None => next.packages.push(package.clone()),
            }
        }
        next.installations.extend([
            PackageInstallation {
                id: receipt.tool_installation_id,
                package: tool.reference,
                enabled: false,
            },
            PackageInstallation {
                id: receipt.expert_installation_id,
                package: expert.reference,
                enabled: false,
            },
        ]);
        next.assignments.extend([
            PackageAssignment {
                id: receipt.tool_assignment_id,
                person_id,
                installation_id: receipt.tool_installation_id,
                enabled: false,
                granted_tool_assignments: vec![],
                granted_view_handles: vec![binding.handle],
                private_state: ExpertPrivateState::default(),
            },
            PackageAssignment {
                id: receipt.expert_assignment_id,
                person_id,
                installation_id: receipt.expert_installation_id,
                enabled: false,
                granted_tool_assignments: vec![receipt.tool_assignment_id],
                granted_view_handles: vec![binding.handle],
                private_state: ExpertPrivateState::default(),
            },
        ]);
        next.calendar_views.push(binding);
        next.calendar_setups.push(receipt.clone());
        *self = Self::restore(next, self.instance_id())?;
        Ok(receipt)
    }

    pub fn configure_calendar_access(
        &mut self,
        person_id: PersonId,
        configuration: &CalendarAccessConfiguration,
    ) -> Result<(), AgentFailure> {
        if configuration.instance_id != self.instance_id() {
            return Err(AgentFailure::NotFound);
        }
        self.check_revision(configuration.expected_revision)?;
        let receipt = self
            .snapshot
            .calendar_setups
            .iter()
            .find(|entry| entry.setup_id == configuration.setup_id)
            .filter(|entry| entry.person_id == person_id)
            .cloned()
            .ok_or(AgentFailure::NotFound)?;
        if self
            .snapshot
            .revoked_calendar_setups
            .contains(&receipt.setup_id)
        {
            return Err(AgentFailure::NotFound);
        }
        let mut next = self.snapshot();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        match &configuration.change {
            CalendarAccessChange::SetEnabled { enabled } => {
                next.calendar_views
                    .iter_mut()
                    .find(|entry| entry.handle == receipt.view_handle)
                    .ok_or(AgentFailure::NotFound)?
                    .enabled = *enabled;
                for identifier in [receipt.tool_installation_id, receipt.expert_installation_id] {
                    next.installations
                        .iter_mut()
                        .find(|entry| entry.id == identifier)
                        .ok_or(AgentFailure::NotFound)?
                        .enabled = *enabled;
                }
                for identifier in [receipt.tool_assignment_id, receipt.expert_assignment_id] {
                    let assignment = next
                        .assignments
                        .iter_mut()
                        .find(|entry| entry.id == identifier && entry.person_id == person_id)
                        .ok_or(AgentFailure::NotFound)?;
                    assignment.enabled = *enabled;
                }
            }
            CalendarAccessChange::SetScope {
                provider,
                device_id,
                calendar_ids,
                connection_scope,
                connection_revision,
                source_authority,
                reviewed_native_subject_fingerprint,
            } => {
                validate_native_subject_fingerprint(
                    *provider,
                    reviewed_native_subject_fingerprint.as_deref(),
                )?;
                let binding = next
                    .calendar_views
                    .iter_mut()
                    .find(|entry| entry.handle == receipt.view_handle)
                    .ok_or(AgentFailure::NotFound)?;
                if binding.provider != *provider {
                    return Err(AgentFailure::InvalidInput);
                }
                binding.device_id = device_id.clone();
                binding.calendar_ids = calendar_ids.clone();
                binding.calendar_ids.sort();
                binding.connection_scope = *connection_scope;
                binding.connection_revision = *connection_revision;
                binding.source_authority = *source_authority;
                binding.validate()?;
                let setup = next
                    .calendar_setups
                    .iter_mut()
                    .find(|entry| entry.setup_id == receipt.setup_id)
                    .ok_or(AgentFailure::NotFound)?;
                setup.connection_scope = *connection_scope;
                setup.connection_revision = *connection_revision;
                setup.source_authority = *source_authority;
                setup.reviewed_native_subject_fingerprint =
                    reviewed_native_subject_fingerprint.clone();
            }
            CalendarAccessChange::Remove {} => {
                disable_setup(&mut next, &receipt, person_id)?;
                next.revoked_calendar_setups.push(receipt.setup_id);
            }
        }
        *self = Self::restore(next, self.instance_id())?;
        Ok(())
    }

    pub(super) fn validate_calendar_setups(&self) -> Result<(), AgentFailure> {
        for (index, receipt) in self.snapshot.calendar_setups.iter().enumerate() {
            if receipt.setup_id.is_nil()
                || receipt.tool_installation_id.is_nil()
                || receipt.expert_installation_id.is_nil()
                || receipt.tool_assignment_id.is_nil()
                || receipt.expert_assignment_id.is_nil()
                || receipt.expected_revision >= self.revision()
                || receipt.connection_revision == 0
                || self.snapshot.calendar_setups[..index].iter().any(|other| {
                    other.setup_id == receipt.setup_id
                        || other.view_handle == receipt.view_handle
                        || other.tool_installation_id == receipt.tool_installation_id
                        || other.expert_installation_id == receipt.expert_installation_id
                        || other.tool_assignment_id == receipt.tool_assignment_id
                        || other.expert_assignment_id == receipt.expert_assignment_id
                })
            {
                return Err(AgentFailure::Conflict);
            }
            let binding = self
                .snapshot
                .calendar_views
                .iter()
                .find(|binding| binding.handle == receipt.view_handle)
                .ok_or(AgentFailure::NotFound)?;
            validate_native_subject_fingerprint(
                binding.provider,
                receipt.reviewed_native_subject_fingerprint.as_deref(),
            )?;
            if binding.person_id != receipt.person_id
                || binding.connection_scope != receipt.connection_scope
                || binding.connection_revision != receipt.connection_revision
                || binding.source_authority != receipt.source_authority
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            for (kind, installation_id, assignment_id, tools) in [
                (
                    PackageKind::Tool,
                    receipt.tool_installation_id,
                    receipt.tool_assignment_id,
                    vec![],
                ),
                (
                    PackageKind::Expert,
                    receipt.expert_installation_id,
                    receipt.expert_assignment_id,
                    vec![receipt.tool_assignment_id],
                ),
            ] {
                let installation = self.installation(installation_id)?;
                let assignment = self.assignment(receipt.person_id, assignment_id)?;
                let package = self.package(&installation.package)?;
                let data_class_matches = match &package.implementation {
                    PackageImplementation::TimelineRead { data_class } => {
                        *data_class == binding.data_class()
                    }
                    PackageImplementation::Builtin { .. } => true,
                    PackageImplementation::Declarative { .. } => false,
                };
                if package.reference.kind != kind
                    || !data_class_matches
                    || assignment.installation_id != installation_id
                    || assignment.granted_tool_assignments != tools
                    || assignment.granted_view_handles != [binding.handle]
                {
                    return Err(AgentFailure::Conflict);
                }
            }
        }
        Ok(())
    }
}

fn disable_setup(
    snapshot: &mut RegistrySnapshot,
    receipt: &CalendarExpertSetupReceipt,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    snapshot
        .calendar_views
        .iter_mut()
        .find(|entry| entry.handle == receipt.view_handle && entry.person_id == person_id)
        .ok_or(AgentFailure::NotFound)?
        .enabled = false;
    for identifier in [receipt.tool_installation_id, receipt.expert_installation_id] {
        snapshot
            .installations
            .iter_mut()
            .find(|entry| entry.id == identifier)
            .ok_or(AgentFailure::NotFound)?
            .enabled = false;
    }
    for identifier in [receipt.tool_assignment_id, receipt.expert_assignment_id] {
        snapshot
            .assignments
            .iter_mut()
            .find(|entry| entry.id == identifier && entry.person_id == person_id)
            .ok_or(AgentFailure::NotFound)?
            .enabled = false;
    }
    Ok(())
}

fn setup_binding(
    person_id: PersonId,
    request: &CalendarExpertSetup,
) -> Result<CalendarViewBinding, AgentFailure> {
    if request.calendar_ids.len() > 4 {
        return Err(AgentFailure::InvalidInput);
    }
    let mut calendar_ids = request.calendar_ids.clone();
    calendar_ids.sort();
    let binding = CalendarViewBinding {
        handle: request.setup_id,
        person_id,
        provider: request.provider,
        device_id: request.device_id.clone(),
        calendar_ids,
        connection_scope: request.connection_scope,
        connection_revision: request.connection_revision,
        source_authority: request.source_authority,
        enabled: false,
    };
    validate_native_subject_fingerprint(
        request.provider,
        request.reviewed_native_subject_fingerprint.as_deref(),
    )?;
    binding.validate()?;
    Ok(binding)
}

fn validate_native_subject_fingerprint(
    provider: CalendarProvider,
    fingerprint: Option<&str>,
) -> Result<(), AgentFailure> {
    if matches!(
        provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    ) {
        let Some(fingerprint) = fingerprint else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        if fingerprint.len() != 64
            || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
            || fingerprint.bytes().any(|byte| byte.is_ascii_uppercase())
        {
            return Err(AgentFailure::InvalidInput);
        }
    } else if fingerprint.is_some() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn setup_packages(packaging: &ExpertPackaging, provider: CalendarProvider) -> [AgentPackage; 2] {
    let data_class = match provider {
        CalendarProvider::Fixture => DataClass::Synthetic,
        CalendarProvider::EventKit
        | CalendarProvider::Google
        | CalendarProvider::Microsoft
        | CalendarProvider::Android => DataClass::Personal,
    };
    packaging.packages(data_class)
}
