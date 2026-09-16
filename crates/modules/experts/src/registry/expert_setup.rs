//! Installing a set of Experts and the sources each of them was granted.
//!
//! The registry keeps the records; it never knows what any Expert means. The
//! crate that owns the Experts states each one's identity, the sources its
//! judgment reads and the packages that carry it, and the registry stores that
//! declaration so later eligibility checks need no Expert-specific knowledge.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::AgentFailure;
use floe_kernel::{AGENT_VERSION, PersonId};

use super::{
    AgentId, AgentPackage, AgentRegistry, BuiltinExpertAssignmentReceipt,
    BuiltinExpertSetupReceipt, BuiltinSourceBinding, BuiltinSourceState, ExpertPrivateState,
    PackageAssignment, PackageImplementation, PackageInstallation, PackageKind, RegistryOverview,
    SourceGrant,
};

/// One Expert's installation, as its owning crate declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpertSetupSpec {
    pub expert: AgentId,
    /// Every source this Expert reads, in its own declared order.
    pub required_sources: Vec<AgentId>,
    /// The one source it cannot answer without.
    pub mandatory_source: AgentId,
    /// The tool package and the Expert package installed together.
    pub packages: [AgentPackage; 2],
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
pub struct BuiltinExpertSetupResult {
    pub setup: BuiltinExpertSetupReceipt,
    pub registry: RegistryOverview,
}

impl AgentRegistry {
    pub fn install_builtin_experts_enabled(
        &mut self,
        person_id: PersonId,
        request: &BuiltinExpertSetup,
        specs: &[ExpertSetupSpec],
    ) -> Result<BuiltinExpertSetupReceipt, AgentFailure> {
        let already_installed = self
            .snapshot
            .builtin_setups
            .iter()
            .any(|setup| setup.setup_id == request.setup_id);
        let receipt = self.install_builtin_experts(person_id, request, specs)?;
        if already_installed {
            return Ok(receipt);
        }
        for expert in &receipt.assignments {
            for installation_id in [expert.tool_installation_id, expert.expert_installation_id] {
                self.snapshot
                    .installations
                    .iter_mut()
                    .find(|installation| installation.id == installation_id)
                    .ok_or(AgentFailure::NotFound)?
                    .enabled = true;
            }
            for assignment_id in [expert.tool_assignment_id, expert.expert_assignment_id] {
                self.snapshot
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.id == assignment_id)
                    .ok_or(AgentFailure::NotFound)?
                    .enabled = true;
            }
        }
        Ok(receipt)
    }

    pub fn install_builtin_experts(
        &mut self,
        person_id: PersonId,
        request: &BuiltinExpertSetup,
        specs: &[ExpertSetupSpec],
    ) -> Result<BuiltinExpertSetupReceipt, AgentFailure> {
        validate_request(request, self.instance_id())?;
        validate_specs(specs)?;
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
        let mut receipts = Vec::with_capacity(specs.len());
        for spec in specs {
            let views = granted_views(&spec.required_sources, &request.sources);
            let [tool, package] = spec.packages.clone();
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
                expert: spec.expert.clone(),
                required_sources: spec.required_sources.clone(),
                mandatory_source: spec.mandatory_source.clone(),
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

    pub fn refresh_builtin_expert_sources(
        &mut self,
        person_id: PersonId,
        expected_revision: u64,
        sources: Vec<BuiltinSourceBinding>,
    ) -> Result<BuiltinExpertSetupReceipt, AgentFailure> {
        self.check_revision(expected_revision)?;
        validate_sources(&sources)?;
        let index = self
            .snapshot
            .builtin_setups
            .iter()
            .position(|setup| setup.person_id == person_id)
            .ok_or(AgentFailure::NotFound)?;
        if self.snapshot.builtin_setups[index].sources == sources {
            return Ok(self.snapshot.builtin_setups[index].clone());
        }
        let assignments = self.snapshot.builtin_setups[index].assignments.clone();
        let grants = assignments
            .iter()
            .map(|receipt| granted_views(&receipt.required_sources, &sources))
            .collect::<Vec<_>>();
        self.advance()?;
        for (receipt, granted_view_handles) in assignments.iter().zip(grants) {
            for assignment_id in [receipt.tool_assignment_id, receipt.expert_assignment_id] {
                self.snapshot
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.id == assignment_id)
                    .ok_or(AgentFailure::NotFound)?
                    .granted_view_handles = granted_view_handles.clone();
            }
            self.snapshot.builtin_setups[index]
                .assignments
                .iter_mut()
                .find(|assignment| assignment.expert == receipt.expert)
                .ok_or(AgentFailure::NotFound)?
                .granted_view_handles = granted_view_handles;
        }
        self.snapshot.builtin_setups[index].sources = sources;
        Ok(self.snapshot.builtin_setups[index].clone())
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
                if matches!(package.implementation, PackageImplementation::Builtin { .. })
                    && !self.assignment_has_mandatory_source(assignment.id)
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
                    supported_placements: metadata.supported_placements.clone(),
                };
                card.validate().ok().map(|_| card)
            })
            .collect()
    }

    /// Whether this assignment still holds the source its Expert declared it
    /// cannot answer without.
    ///
    /// A builtin installed outside a recorded setup has stated no mandatory
    /// source, so there is nothing here to confirm and the card stands on its
    /// own grants.
    fn assignment_has_mandatory_source(&self, assignment_id: Uuid) -> bool {
        let Some((setup, receipt)) = self.snapshot.builtin_setups.iter().find_map(|setup| {
            setup
                .assignments
                .iter()
                .find(|receipt| receipt.expert_assignment_id == assignment_id)
                .map(|receipt| (setup, receipt))
        }) else {
            return true;
        };
        setup.sources.iter().any(|binding| {
            binding.source == receipt.mandatory_source
                && binding.state == BuiltinSourceState::Available
                && receipt.granted_view_handles.contains(&binding.view_handle)
        })
    }

    /// Whether the Expert behind `expert_assignment_id` was granted `source`
    /// at setup time, and what stands in the way when it was not.
    pub fn assignment_source_grant(
        &self,
        assignment_id: Uuid,
        source: &AgentId,
    ) -> SourceGrant {
        let Some((setup, receipt)) = self.snapshot.builtin_setups.iter().find_map(|setup| {
            setup
                .assignments
                .iter()
                .find(|receipt| receipt.expert_assignment_id == assignment_id)
                .map(|receipt| (setup, receipt))
        }) else {
            return SourceGrant::NotConfigured;
        };
        let Some(binding) = setup
            .sources
            .iter()
            .find(|binding| binding.source == *source)
        else {
            return SourceGrant::NotConfigured;
        };
        match binding.state {
            _ if !receipt.granted_view_handles.contains(&binding.view_handle) => {
                SourceGrant::Denied
            }
            BuiltinSourceState::Available => SourceGrant::Granted,
            BuiltinSourceState::Disabled | BuiltinSourceState::Unavailable => {
                SourceGrant::Unavailable
            }
        }
    }

    /// The Expert assignment recorded for `expert` in this Person's setup.
    pub fn expert_assignment_id(&self, person_id: PersonId, expert: &AgentId) -> Option<Uuid> {
        self.snapshot
            .builtin_setups
            .iter()
            .filter(|setup| setup.person_id == person_id)
            .find_map(|setup| {
                setup
                    .assignments
                    .iter()
                    .find(|receipt| receipt.expert == *expert)
                    .map(|receipt| receipt.expert_assignment_id)
            })
    }

    pub(super) fn validate_builtin_setups(&self) -> Result<(), AgentFailure> {
        for (index, receipt) in self.snapshot.builtin_setups.iter().enumerate() {
            if receipt.setup_id.is_nil()
                || receipt.expected_revision >= self.revision()
                || receipt.assignments.is_empty()
                || receipt.assignments.iter().enumerate().any(|(position, entry)| {
                    receipt.assignments[..position]
                        .iter()
                        .any(|other| other.expert == entry.expert)
                })
                || self.snapshot.builtin_setups[..index].iter().any(|other| {
                    other.setup_id == receipt.setup_id || other.person_id == receipt.person_id
                })
            {
                return Err(AgentFailure::Conflict);
            }
            validate_sources(&receipt.sources)?;
            for expert_receipt in &receipt.assignments {
                let expected_views =
                    granted_views(&expert_receipt.required_sources, &receipt.sources);
                if expert_receipt.granted_view_handles != expected_views
                    || !expert_receipt
                        .required_sources
                        .contains(&expert_receipt.mandatory_source)
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
                let tool_installation = self.installation(expert_receipt.tool_installation_id)?;
                let expert_installation =
                    self.installation(expert_receipt.expert_installation_id)?;
                let tool_assignment =
                    self.assignment(receipt.person_id, expert_receipt.tool_assignment_id)?;
                let expert_assignment =
                    self.assignment(receipt.person_id, expert_receipt.expert_assignment_id)?;
                let tool_package = self.package(&tool_installation.package)?;
                let expert_package = self.package(&expert_installation.package)?;
                if tool_package.reference.kind != PackageKind::Tool
                    || expert_package.reference.kind != PackageKind::Expert
                    || expert_package.reference.id != expert_receipt.expert.as_str()
                    || !expert_package.required_tools.contains(&tool_package.reference)
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

/// What each Expert in one Person's setup may read right now.
///
/// The decision is the registry's; a caller holds the answer and forwards it,
/// rather than walking assignments and bindings itself.
#[derive(Clone, Debug, Default)]
pub struct SourceGrants {
    setup: Option<BuiltinExpertSetupReceipt>,
}

impl SourceGrants {
    pub fn new(setup: Option<BuiltinExpertSetupReceipt>) -> Self {
        Self { setup }
    }

    /// Whether `expert` may read `source`, and what stands in the way if not.
    pub fn grant(&self, expert: &str, source: &str) -> SourceGrant {
        let Some(setup) = &self.setup else {
            return SourceGrant::NotConfigured;
        };
        let Some(receipt) = setup
            .assignments
            .iter()
            .find(|receipt| receipt.expert.as_str() == expert)
        else {
            return SourceGrant::NotConfigured;
        };
        let Some(binding) = setup
            .sources
            .iter()
            .find(|binding| binding.source.as_str() == source)
        else {
            return SourceGrant::NotConfigured;
        };
        if !receipt.granted_view_handles.contains(&binding.view_handle) {
            return SourceGrant::Denied;
        }
        match binding.state {
            BuiltinSourceState::Available => SourceGrant::Granted,
            BuiltinSourceState::Disabled | BuiltinSourceState::Unavailable => {
                SourceGrant::Unavailable
            }
        }
    }
}

/// The cards a caller running at `placement` may offer.
///
/// A card states where its Expert runs; the caller never decides eligibility
/// from what an agent id looks like.
pub fn eligible_cards(
    cards: &[crate::AgentCard],
    placement: floe_context_contract::ModelPlacement,
) -> Vec<crate::AgentCard> {
    cards
        .iter()
        .filter(|card| card.runs_at(placement))
        .cloned()
        .collect()
}

/// The view handles a declared source list actually resolves to right now.
fn granted_views(required: &[AgentId], sources: &[BuiltinSourceBinding]) -> Vec<Uuid> {
    required
        .iter()
        .filter_map(|source| {
            sources.iter().find(|binding| {
                binding.source == *source && binding.state == BuiltinSourceState::Available
            })
        })
        .map(|binding| binding.view_handle)
        .collect()
}

fn validate_specs(specs: &[ExpertSetupSpec]) -> Result<(), AgentFailure> {
    if specs.is_empty()
        || specs.len() > 32
        || specs.iter().enumerate().any(|(index, spec)| {
            !spec.required_sources.contains(&spec.mandatory_source)
                || spec.required_sources.len() > 16
                || spec.packages[0].reference.kind != PackageKind::Tool
                || spec.packages[1].reference.kind != PackageKind::Expert
                || spec.packages[1].reference.id != spec.expert.as_str()
                || !spec.packages[1]
                    .required_tools
                    .contains(&spec.packages[0].reference)
                || specs[..index].iter().any(|other| other.expert == spec.expert)
        })
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
