use floe_domain::CalendarProvider;

use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertSetup {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertSetupReceipt {
    pub setup_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub view_handle: Uuid,
    pub tool_installation_id: Uuid,
    pub expert_installation_id: Uuid,
    pub tool_assignment_id: Uuid,
    pub expert_assignment_id: Uuid,
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
        replacement_setup_id: Uuid,
        provider: CalendarProvider,
        calendar_ids: Vec<String>,
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
            || original.calendar_ids != binding.calendar_ids
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(Some(receipt.clone()))
    }

    pub fn install_calendar_expert(
        &mut self,
        person_id: PersonId,
        request: &CalendarExpertSetup,
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
        let [tool, expert] = setup_packages(request.provider);
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
                replacement_setup_id,
                provider,
                calendar_ids,
            } => {
                if replacement_setup_id.is_nil()
                    || *replacement_setup_id == receipt.setup_id
                    || next
                        .calendar_setups
                        .iter()
                        .any(|entry| entry.setup_id == *replacement_setup_id)
                {
                    return Err(AgentFailure::InvalidInput);
                }
                let replacement = CalendarExpertSetup {
                    instance_id: configuration.instance_id,
                    expected_revision: configuration.expected_revision,
                    setup_id: *replacement_setup_id,
                    provider: *provider,
                    calendar_ids: calendar_ids.clone(),
                };
                let mut staged = Self::restore(self.snapshot(), self.instance_id())?;
                staged.install_calendar_expert(person_id, &replacement)?;
                next = staged.snapshot();
                disable_setup(&mut next, &receipt, person_id)?;
                next.revoked_calendar_setups.push(receipt.setup_id);
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
            let [tool, expert] = setup_packages(binding.provider);
            if binding.person_id != receipt.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            for (package, installation_id, assignment_id, tools) in [
                (
                    tool,
                    receipt.tool_installation_id,
                    receipt.tool_assignment_id,
                    vec![],
                ),
                (
                    expert,
                    receipt.expert_installation_id,
                    receipt.expert_assignment_id,
                    vec![receipt.tool_assignment_id],
                ),
            ] {
                let installation = self.installation(installation_id)?;
                let assignment = self.assignment(receipt.person_id, assignment_id)?;
                if installation.package != package.reference
                    || self.package(&package.reference)? != &package
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
        calendar_ids,
        enabled: false,
    };
    binding.validate()?;
    Ok(binding)
}

fn setup_packages(provider: CalendarProvider) -> [AgentPackage; 2] {
    let (source, data_class) = match provider {
        CalendarProvider::Fixture => ("fixture", DataClass::Synthetic),
        CalendarProvider::EventKit => ("eventkit", DataClass::Personal),
        CalendarProvider::Google => ("google", DataClass::Personal),
        CalendarProvider::Microsoft => ("microsoft", DataClass::Personal),
        CalendarProvider::Android => ("android", DataClass::Personal),
    };
    let tool = PackageRef {
        kind: PackageKind::Tool,
        id: format!("floe.calendar.{source}.timeline"),
        version: "1.0.0".into(),
    };
    [
        AgentPackage {
            schema_version: AGENT_VERSION,
            reference: tool.clone(),
            publisher: "floe".into(),
            implementation: PackageImplementation::TimelineRead { data_class },
            expert_metadata: None,
            required_tools: vec![],
            state_schema_version: 1,
        },
        AgentPackage {
            schema_version: AGENT_VERSION,
            reference: PackageRef {
                kind: PackageKind::Expert,
                id: format!("floe.calendar.{source}.schedule"),
                version: "1.0.0".into(),
            },
            publisher: "floe".into(),
            implementation: PackageImplementation::Schedule,
            expert_metadata: Some(ExpertMetadata {
                name: "Schedule Expert".into(),
                description: "Reviews calendars, availability, conflicts, and the realism of plans from a scheduling perspective.".into(),
                domain_tags: vec!["schedule".into(), "calendar".into()],
                skills: vec!["Provide independent scheduling judgment".into()],
            }),
            required_tools: vec![tool],
            state_schema_version: 1,
        },
    ]
}
