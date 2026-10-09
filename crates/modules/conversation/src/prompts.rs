//! The Manager role prompt. Conversation owns the root Run's instructions.

use floe_agent_contract::{AgentFailure, RoleSpec};
use floe_conversation_contract::{AgentIdentity, AgentInstanceId, AssignmentId};
use floe_kernel::PersonId;
use floe_knowledge::prompts::{
    AGENT_VERSION, BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
    CAPABILITY_PROTOCOL_REVISION, PersonaProfile, PromptAssembly, PromptComponent,
    PromptComponentKind, PromptRole, product_component, validate_persona,
};
use uuid::Uuid;

use crate::MANAGER_OUTPUT_CONTRACT;

const MANAGER_ROLE: &str = include_str!("../prompts/manager_role.txt");
pub const MANAGER_ROLE_REVISION: u64 = 8;
pub const MANAGER_DEFINITION_ID: &str = "manager-role";

/// Stable default Manager identity for this Person. The runtime registry and
/// Run/Task IDs do not participate in the identity; the role policy revision
/// is pinned separately so a prompt change selects a fresh conversation.
pub fn default_manager_identity(person_id: PersonId) -> Result<AgentIdentity, AgentFailure> {
    if !person_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let instance = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("floe.manager.instance.v1:{person_id}").as_bytes(),
    );
    let assignment = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("floe.manager.assignment.v1:{person_id}").as_bytes(),
    );
    let identity = AgentIdentity {
        person_id,
        agent_instance_id: AgentInstanceId::from_uuid(instance)
            .ok_or(AgentFailure::InvalidInput)?,
        assignment_id: AssignmentId::from_uuid(assignment).ok_or(AgentFailure::InvalidInput)?,
        definition_id: MANAGER_DEFINITION_ID.to_owned(),
        definition_revision: MANAGER_ROLE_REVISION,
    };
    identity
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    Ok(identity)
}

/// The Manager role as role-only instructions: no behavior kernel, persona, or
/// capability protocol rendering. Canonical callers build their RoleSpec from
/// this instead of stuffing a rendered prompt into the role.
pub fn manager_role_spec() -> RoleSpec {
    RoleSpec {
        role_id: "manager".into(),
        instructions: MANAGER_ROLE.into(),
        output_contract: MANAGER_OUTPUT_CONTRACT.into(),
        output_format: floe_agent_contract::ModelOutputFormat::Text,
    }
}

pub fn manager_prompt(persona: Option<&PersonaProfile>) -> Result<PromptAssembly, AgentFailure> {
    let persona = persona.cloned().unwrap_or_default();
    validate_persona(&persona)?;
    let prompt = PromptAssembly {
        schema_version: AGENT_VERSION,
        role: PromptRole::Manager,
        components: vec![
            product_component(
                PromptComponentKind::BehaviorKernel,
                "behavior-kernel",
                BEHAVIOR_KERNEL_REVISION,
                BEHAVIOR_KERNEL,
            ),
            product_component(
                PromptComponentKind::Role,
                "manager-role",
                MANAGER_ROLE_REVISION,
                MANAGER_ROLE,
            ),
            PromptComponent {
                kind: PromptComponentKind::Persona,
                source: persona.source,
                revision: persona.revision,
                content: persona.instructions,
            },
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "capability-protocol",
                CAPABILITY_PROTOCOL_REVISION,
                CAPABILITY_PROTOCOL,
            ),
        ],
    };
    prompt.validate()?;
    Ok(prompt)
}
