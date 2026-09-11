use super::*;

pub const BUILTIN_EXPERT_PACKAGE_VERSION: &str = "1.0.0";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinExpertKind {
    Commitments,
    Communication,
    Relationships,
    FocusAttention,
    Wellbeing,
    WorkContext,
    LifeLogistics,
}

impl BuiltinExpertKind {
    pub const ALL: [Self; 7] = [
        Self::Commitments,
        Self::Communication,
        Self::Relationships,
        Self::FocusAttention,
        Self::Wellbeing,
        Self::WorkContext,
        Self::LifeLogistics,
    ];

    pub fn package_id(self) -> &'static str {
        match self {
            Self::Commitments => "floe.builtin.commitments",
            Self::Communication => "floe.builtin.communication",
            Self::Relationships => "floe.builtin.relationships",
            Self::FocusAttention => "floe.builtin.focus-attention",
            Self::Wellbeing => "floe.builtin.wellbeing",
            Self::WorkContext => "floe.builtin.work-context",
            Self::LifeLogistics => "floe.builtin.life-logistics",
        }
    }

    fn tool_id(self) -> String {
        format!("{}.context", self.package_id())
    }

    pub(crate) fn metadata(self) -> ExpertMetadata {
        let (name, description, domain_tags, skills) = match self {
            Self::Commitments => (
                "Commitments Expert",
                "Finds obligations and follow-ups across the bounded personal context granted to it.",
                vec!["commitments", "planning"],
                "Review commitments and follow-ups",
            ),
            Self::Communication => (
                "Communication Expert",
                "Assesses whether communication needs a response and prepares reviewable drafts.",
                vec!["communication"],
                "Recommend bounded communication actions",
            ),
            Self::Relationships => (
                "Relationships Expert",
                "Reviews explicitly granted people and confirmed-interaction context for follow-ups.",
                vec!["relationships"],
                "Identify relationship follow-ups",
            ),
            Self::FocusAttention => (
                "Focus & Attention Expert",
                "Combines bounded attention, schedule, and active-work context into focus guidance.",
                vec!["focus", "attention"],
                "Recommend focus protection",
            ),
            Self::Wellbeing => (
                "Wellbeing Expert",
                "Uses coarse derived wellbeing and schedule context to recommend sustainable load.",
                vec!["wellbeing"],
                "Recommend sustainable schedule load",
            ),
            Self::WorkContext => (
                "Work Context Expert",
                "Synthesizes bounded work context into blockers and next actions.",
                vec!["work"],
                "Identify work blockers and next actions",
            ),
            Self::LifeLogistics => (
                "Life Logistics Expert",
                "Synthesizes bounded logistics context into preparation recommendations.",
                vec!["life", "logistics"],
                "Recommend logistics preparation",
            ),
        };
        ExpertMetadata {
            name: name.into(),
            description: description.into(),
            domain_tags: domain_tags.into_iter().map(str::to_owned).collect(),
            skills: vec![skills.into()],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinContextSource {
    Calendar,
    Mail,
    Tasks,
    ConfirmedMemory,
    Contacts,
    ConfirmedInteractions,
    Attention,
    WorkContext,
    Wellbeing,
    Logistics,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinSourceState {
    Available,
    Disabled,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinSourceBinding {
    pub source: BuiltinContextSource,
    pub view_handle: Uuid,
    pub state: BuiltinSourceState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertSetup {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub sources: Vec<BuiltinSourceBinding>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertAssignmentReceipt {
    pub expert: BuiltinExpertKind,
    pub tool_installation_id: Uuid,
    pub expert_installation_id: Uuid,
    pub tool_assignment_id: Uuid,
    pub expert_assignment_id: Uuid,
    pub granted_view_handles: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertSetupReceipt {
    pub setup_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub sources: Vec<BuiltinSourceBinding>,
    pub assignments: Vec<BuiltinExpertAssignmentReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertSetupResult {
    pub setup: BuiltinExpertSetupReceipt,
    pub registry: RegistryOverview,
}

impl AgentRegistry {
    pub fn install_builtin_experts(
        &mut self,
        person_id: PersonId,
        request: &BuiltinExpertSetup,
    ) -> Result<BuiltinExpertSetupReceipt, AgentFailure> {
        validate_request(request, self.instance_id())?;
        if let Some(receipt) = self
            .snapshot
            .builtin_setups
            .iter()
            .find(|receipt| receipt.setup_id == request.setup_id)
        {
            return if receipt.person_id == person_id
                && receipt.expected_revision == request.expected_revision
                && receipt.sources == request.sources
            {
                Ok(receipt.clone())
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        if self
            .snapshot
            .builtin_setups
            .iter()
            .any(|receipt| receipt.person_id == person_id)
        {
            return Err(AgentFailure::Conflict);
        }
        self.check_revision(request.expected_revision)?;
        let mut next = self.snapshot();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let mut receipts = Vec::with_capacity(BuiltinExpertKind::ALL.len());
        for expert in BuiltinExpertKind::ALL {
            let views = required_sources(expert)
                .iter()
                .filter_map(|source| {
                    request.sources.iter().find(|binding| {
                        binding.source == *source && binding.state == BuiltinSourceState::Available
                    })
                })
                .map(|binding| binding.view_handle)
                .collect::<Vec<_>>();
            let [tool, package] = builtin_packages(expert);
            for candidate in [&tool, &package] {
                match next
                    .packages
                    .iter()
                    .find(|entry| entry.reference == candidate.reference)
                {
                    Some(existing) if existing == candidate => {}
                    Some(_) => return Err(AgentFailure::Conflict),
                    None => next.packages.push(candidate.clone()),
                }
            }
            let receipt = BuiltinExpertAssignmentReceipt {
                expert,
                tool_installation_id: Uuid::new_v4(),
                expert_installation_id: Uuid::new_v4(),
                tool_assignment_id: Uuid::new_v4(),
                expert_assignment_id: Uuid::new_v4(),
                granted_view_handles: views.clone(),
            };
            next.installations.extend([
                PackageInstallation {
                    id: receipt.tool_installation_id,
                    package: tool.reference,
                    enabled: false,
                },
                PackageInstallation {
                    id: receipt.expert_installation_id,
                    package: package.reference,
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
                    granted_view_handles: views.clone(),
                    private_state: ExpertPrivateState::default(),
                },
                PackageAssignment {
                    id: receipt.expert_assignment_id,
                    person_id,
                    installation_id: receipt.expert_installation_id,
                    enabled: false,
                    granted_tool_assignments: vec![receipt.tool_assignment_id],
                    granted_view_handles: views,
                    private_state: ExpertPrivateState::default(),
                },
            ]);
            receipts.push(receipt);
        }
        let receipt = BuiltinExpertSetupReceipt {
            setup_id: request.setup_id,
            person_id,
            expected_revision: request.expected_revision,
            sources: request.sources.clone(),
            assignments: receipts,
        };
        next.builtin_setups.push(receipt.clone());
        *self = Self::restore(next, self.instance_id())?;
        Ok(receipt)
    }

    pub fn enabled_expert_cards(&self, person_id: PersonId) -> Vec<crate::AgentCard> {
        self.snapshot
            .assignments
            .iter()
            .filter(|assignment| assignment.person_id == person_id && assignment.enabled)
            .filter_map(|assignment| {
                let installation = self.installation(assignment.installation_id).ok()?;
                if !installation.enabled {
                    return None;
                }
                let package = self.package(&installation.package).ok()?;
                if package.reference.kind != PackageKind::Expert
                    || assignment.granted_view_handles.is_empty()
                    || self.validate_grants(assignment).is_err()
                {
                    return None;
                }
                let tool_assignment = self
                    .assignment(person_id, *assignment.granted_tool_assignments.first()?)
                    .ok()?;
                if !tool_assignment.enabled
                    || !self
                        .installation(tool_assignment.installation_id)
                        .ok()?
                        .enabled
                {
                    return None;
                }
                let metadata = package.expert_metadata.as_ref()?;
                let card = crate::AgentCard {
                    schema_version: AGENT_VERSION,
                    protocol_version: crate::A2A_PROTOCOL_VERSION.into(),
                    id: package.reference.id.clone(),
                    version: package.reference.version.clone(),
                    name: metadata.name.clone(),
                    description: metadata.description.clone(),
                    domain_tags: metadata.domain_tags.clone(),
                    skills: metadata.skills.clone(),
                };
                card.validate().ok().map(|_| card)
            })
            .collect()
    }

    pub(super) fn validate_builtin_setups(&self) -> Result<(), AgentFailure> {
        for (index, receipt) in self.snapshot.builtin_setups.iter().enumerate() {
            if receipt.setup_id.is_nil()
                || receipt.expected_revision >= self.revision()
                || receipt.assignments.len() != BuiltinExpertKind::ALL.len()
                || !receipt
                    .assignments
                    .iter()
                    .map(|entry| entry.expert)
                    .eq(BuiltinExpertKind::ALL)
                || self.snapshot.builtin_setups[..index].iter().any(|other| {
                    other.setup_id == receipt.setup_id || other.person_id == receipt.person_id
                })
            {
                return Err(AgentFailure::Conflict);
            }
            validate_sources(&receipt.sources)?;
            for expert_receipt in &receipt.assignments {
                let expected_views = required_sources(expert_receipt.expert)
                    .iter()
                    .filter_map(|source| {
                        receipt.sources.iter().find(|binding| {
                            binding.source == *source
                                && binding.state == BuiltinSourceState::Available
                        })
                    })
                    .map(|binding| binding.view_handle)
                    .collect::<Vec<_>>();
                if expert_receipt.granted_view_handles != expected_views {
                    return Err(AgentFailure::CapabilityDenied);
                }
                let [tool, expert] = builtin_packages(expert_receipt.expert);
                let tool_installation = self.installation(expert_receipt.tool_installation_id)?;
                let expert_installation =
                    self.installation(expert_receipt.expert_installation_id)?;
                let tool_assignment =
                    self.assignment(receipt.person_id, expert_receipt.tool_assignment_id)?;
                let expert_assignment =
                    self.assignment(receipt.person_id, expert_receipt.expert_assignment_id)?;
                if tool_installation.package != tool.reference
                    || expert_installation.package != expert.reference
                    || self.package(&tool.reference)? != &tool
                    || self.package(&expert.reference)? != &expert
                    || tool_assignment.installation_id != expert_receipt.tool_installation_id
                    || expert_assignment.installation_id != expert_receipt.expert_installation_id
                    || tool_assignment.granted_view_handles != expected_views
                    || expert_assignment.granted_view_handles != expected_views
                    || !tool_assignment.granted_tool_assignments.is_empty()
                    || expert_assignment.granted_tool_assignments
                        != [expert_receipt.tool_assignment_id]
                {
                    return Err(AgentFailure::Conflict);
                }
            }
        }
        Ok(())
    }
}

fn validate_request(request: &BuiltinExpertSetup, instance_id: Uuid) -> Result<(), AgentFailure> {
    if request.instance_id != instance_id {
        return Err(AgentFailure::NotFound);
    }
    if request.setup_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    validate_sources(&request.sources)
}

fn validate_sources(sources: &[BuiltinSourceBinding]) -> Result<(), AgentFailure> {
    if sources.len() > 16
        || sources.iter().enumerate().any(|(index, binding)| {
            binding.view_handle.is_nil()
                || sources[..index].iter().any(|other| {
                    other.source == binding.source || other.view_handle == binding.view_handle
                })
        })
    {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn required_sources(expert: BuiltinExpertKind) -> &'static [BuiltinContextSource] {
    use BuiltinContextSource::*;
    match expert {
        BuiltinExpertKind::Commitments => &[Mail, Calendar, Tasks, ConfirmedMemory],
        BuiltinExpertKind::Communication => &[Mail],
        BuiltinExpertKind::Relationships => &[Contacts, ConfirmedInteractions],
        BuiltinExpertKind::FocusAttention => &[Attention, Calendar, WorkContext],
        BuiltinExpertKind::Wellbeing => &[Wellbeing, Calendar],
        BuiltinExpertKind::WorkContext => &[WorkContext],
        BuiltinExpertKind::LifeLogistics => &[Logistics],
    }
}

fn builtin_packages(kind: BuiltinExpertKind) -> [AgentPackage; 2] {
    let tool = PackageRef {
        kind: PackageKind::Tool,
        id: kind.tool_id(),
        version: BUILTIN_EXPERT_PACKAGE_VERSION.into(),
    };
    [
        AgentPackage {
            schema_version: AGENT_VERSION,
            reference: tool.clone(),
            publisher: "floe".into(),
            implementation: PackageImplementation::TimelineRead {
                data_class: if kind == BuiltinExpertKind::Wellbeing {
                    DataClass::HighlySensitive
                } else {
                    DataClass::Personal
                },
            },
            expert_metadata: None,
            required_tools: vec![],
            state_schema_version: 1,
        },
        AgentPackage {
            schema_version: AGENT_VERSION,
            reference: PackageRef {
                kind: PackageKind::Expert,
                id: kind.package_id().into(),
                version: BUILTIN_EXPERT_PACKAGE_VERSION.into(),
            },
            publisher: "floe".into(),
            implementation: PackageImplementation::Builtin { expert: kind },
            expert_metadata: Some(kind.metadata()),
            required_tools: vec![tool],
            state_schema_version: 1,
        },
    ]
}
