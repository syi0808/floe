//! Installing a set of Experts as pure package topology.
//!
//! The registry keeps the records; it never knows what any Expert means, and
//! it never records source permission. The crate that owns the Experts states
//! each one's identity and the packages that carry it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::{AGENT_VERSION, PersonId};
use floe_agent_contract::{AgentFailure, PackageKind};

use super::{
    AgentId, AgentPackage, AgentRegistry, BuiltinExpertAssignmentReceipt,
    BuiltinExpertSetupReceipt, ExpertPrivateState, PackageAssignment, PackageImplementation,
    PackageInstallation, RegistryOverview,
};

/// One Expert's installation, as its owning crate declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpertSetupSpec {
    pub expert: AgentId,
    /// The tool package and the Expert package installed together.
    pub packages: [AgentPackage; 2],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinExpertSetup {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
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
                tool_installation_id: Uuid::new_v4(),
                expert_installation_id: Uuid::new_v4(),
                tool_assignment_id: Uuid::new_v4(),
                expert_assignment_id: Uuid::new_v4(),
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
                    granted_view_handles: vec![],
                    private_state: ExpertPrivateState::default(),
                },
                PackageAssignment {
                    id: receipt.expert_assignment_id,
                    person_id,
                    installation_id: receipt.expert_installation_id,
                    enabled: false,
                    granted_tool_assignments: vec![receipt.tool_assignment_id],
                    granted_view_handles: vec![],
                    private_state: ExpertPrivateState::default(),
                },
            ]);
            receipts.push(receipt);
        }
        let receipt = BuiltinExpertSetupReceipt {
            setup_id: request.setup_id,
            person_id,
            expected_revision: request.expected_revision,
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
            .filter_map(|assignment| self.enabled_expert_card(person_id, assignment))
            .collect()
    }

    pub fn enabled_builtin_expert_cards(&self, person_id: PersonId) -> Vec<crate::AgentCard> {
        self.snapshot
            .builtin_setups
            .iter()
            .filter(|setup| setup.person_id == person_id)
            .flat_map(|setup| setup.assignments.iter())
            .filter_map(|receipt| {
                self.assignment(person_id, receipt.expert_assignment_id)
                    .ok()
            })
            .filter(|assignment| assignment.enabled)
            .filter_map(|assignment| self.enabled_expert_card(person_id, assignment))
            .collect()
    }

    fn enabled_expert_card(
        &self,
        person_id: PersonId,
        assignment: &PackageAssignment,
    ) -> Option<crate::AgentCard> {
        let installation = self.installation(assignment.installation_id).ok()?;
        if !installation.enabled {
            return None;
        }
        let package = self.package(&installation.package).ok()?;
        if package.reference.kind != PackageKind::Expert {
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
        // Tool linkage must still be intact; source state is irrelevant.
        if self.validate_tool_linkage(assignment).is_err() {
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
                || receipt
                    .assignments
                    .iter()
                    .enumerate()
                    .any(|(position, entry)| {
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
            for expert_receipt in &receipt.assignments {
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
                    || !matches!(
                        &expert_package.implementation,
                        PackageImplementation::Builtin { expert }
                            if expert == &expert_receipt.expert
                    )
                    || !expert_package
                        .required_tools
                        .contains(&tool_package.reference)
                    || tool_assignment.installation_id != expert_receipt.tool_installation_id
                    || expert_assignment.installation_id != expert_receipt.expert_installation_id
                    || !tool_assignment.granted_tool_assignments.is_empty()
                    || expert_assignment.granted_tool_assignments
                        != [expert_receipt.tool_assignment_id]
                    || !tool_assignment.granted_view_handles.is_empty()
                    || !expert_assignment.granted_view_handles.is_empty()
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
    Ok(())
}

/// The cards eligible for the execution classes observed by Inference.
pub fn eligible_cards_for_availability(
    cards: &[crate::AgentCard],
    availability: floe_inference::InferenceAvailability,
) -> Vec<crate::AgentCard> {
    use floe_agent_contract::ModelPlacement;
    use floe_inference::InferenceExecutionConstraint;

    let mut seen = std::collections::HashSet::new();
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

fn validate_specs(specs: &[ExpertSetupSpec]) -> Result<(), AgentFailure> {
    if specs.is_empty()
        || specs.len() > 32
        || specs.iter().enumerate().any(|(index, spec)| {
            spec.packages[0].reference.kind != PackageKind::Tool
                || spec.packages[1].reference.kind != PackageKind::Expert
                || spec.packages[1].reference.id != spec.expert.as_str()
                || !spec.packages[1]
                    .required_tools
                    .contains(&spec.packages[0].reference)
                || specs[..index]
                    .iter()
                    .any(|other| other.expert == spec.expert)
        })
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
