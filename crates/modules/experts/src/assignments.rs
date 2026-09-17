//! Admitting one Expert invocation against the Person's registry, and recording
//! that it ran.
//!
//! Which installation and assignment an invocation resolves to, whether the
//! data class it asks for is one it was granted, what a package's own rules
//! oblige it to, and what the assignment's state becomes afterwards are all the
//! registry's. An Expert states its judgment; it never resolves itself.

use std::sync::Mutex;

use floe_agent_contract::{
    AdmittedExpert, AgentFailure, DataClass, ExpertAssignments, ExpertInvocation, ExpertResult,
    check_running,
};

use crate::{AgentRegistry, ExpertRule, PackageImplementation};

/// The registry an Expert invocation is admitted against.
pub struct RegistryAssignments<'registry> {
    pub registry: &'registry Mutex<AgentRegistry>,
}

impl<'registry> RegistryAssignments<'registry> {
    pub fn new(registry: &'registry Mutex<AgentRegistry>) -> Self {
        Self { registry }
    }
}

impl ExpertAssignments for RegistryAssignments<'_> {
    fn admit(
        &self,
        invocation: &ExpertInvocation,
        focus_minutes: Option<u16>,
    ) -> Result<AdmittedExpert, AgentFailure> {
        let resolved = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .resolve(
                invocation.instance_id,
                invocation.person_id,
                invocation.assignment_id,
                invocation.expected_registry_revision,
                &invocation.granted_view_handles,
            )?;
        if !invocation
            .allowed_data_classes
            .contains(&resolved.data_class)
            || invocation
                .allowed_data_classes
                .iter()
                .any(|class| matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        // The same invocation cannot be admitted twice against one assignment.
        if resolved.assignment.private_state.last_invocation_id == Some(invocation.invocation_id) {
            return Err(AgentFailure::Conflict);
        }
        let (builtin_expert, focus_minimum_minutes) = match &resolved.package.implementation {
            PackageImplementation::Builtin { expert } => (Some(expert.as_str().to_owned()), focus_minutes),
            // A declarative package's own rule sets the floor; a request may ask
            // for a longer window but never a shorter one.
            PackageImplementation::Declarative { rules } => match rules.as_slice() {
                [ExpertRule::FindFocusWindow { minimum_minutes }] => (
                    None,
                    Some(
                        focus_minutes
                            .unwrap_or(*minimum_minutes)
                            .max(*minimum_minutes),
                    ),
                ),
                _ => return Err(AgentFailure::CapabilityDenied),
            },
            _ => return Err(AgentFailure::CapabilityDenied),
        };
        Ok(AdmittedExpert {
            package: resolved.package.reference.clone(),
            data_class: resolved.data_class,
            builtin_expert,
            focus_minimum_minutes,
            registry_revision: resolved.registry_revision,
        })
    }

    fn settle(
        &self,
        invocation: &ExpertInvocation,
        admitted: &AdmittedExpert,
        result: &ExpertResult,
    ) -> Result<u64, AgentFailure> {
        if serde_json::to_vec(result)
            .map_err(|_| AgentFailure::InvalidModelOutput)?
            .len()
            > invocation.budget.max_output_bytes.min(16384)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        check_running(invocation)?;
        if result.expires_at_unix_ms <= now_unix_ms()? {
            return Err(AgentFailure::StaleContext);
        }
        // The assignment has to still be the one that was admitted, at the very
        // revision it was admitted against.
        let resolved = registry.resolve(
            invocation.instance_id,
            invocation.person_id,
            invocation.assignment_id,
            admitted.registry_revision,
            &invocation.granted_view_handles,
        )?;
        registry.complete(&resolved, invocation.invocation_id)
    }
}

fn now_unix_ms() -> Result<u64, AgentFailure> {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?
            .as_millis(),
    )
    .map_err(|_| AgentFailure::StaleContext)
}
