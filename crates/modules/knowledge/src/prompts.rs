//! The Learner's own prompt.
//!
//! The kernel, the capability protocol, the persona and the assembly rules are
//! role-neutral and belong to the agent contract; what is Knowledge's own is the
//! Learner role it plays.

pub use floe_agent_contract::prompts::*;

pub const LEARNER_ROLE: &str = include_str!("../prompts/learner_role.txt");
pub const LEARNER_PROTOCOL: &str = include_str!("../prompts/learner_protocol.txt");
pub const LEARNER_ROLE_REVISION: u64 = 3;
pub const LEARNER_PROTOCOL_REVISION: u64 = 4;
pub const LEARNER_EXTRACTOR_VERSION: &str = "memory-extractor-v3";
pub const LEARNER_PROMPT_VERSION: &str = "memory-review-v4";

pub fn learner_prompt() -> PromptAssembly {
    PromptAssembly {
        schema_version: AGENT_VERSION,
        role: PromptRole::Learner,
        components: vec![
            product_component(
                PromptComponentKind::BehaviorKernel,
                "behavior-kernel",
                BEHAVIOR_KERNEL_REVISION,
                BEHAVIOR_KERNEL,
            ),
            product_component(
                PromptComponentKind::Role,
                "learner-role",
                LEARNER_ROLE_REVISION,
                LEARNER_ROLE,
            ),
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "learner-protocol",
                LEARNER_PROTOCOL_REVISION,
                LEARNER_PROTOCOL,
            ),
        ],
    }
}

/// Knowledge owns the exact candidate-only structured answer contract.
pub fn learner_output_schema() -> Result<floe_agent_contract::ModelSchema, floe_kernel::AgentFailure>
{
    use serde_json::json;
    floe_agent_contract::ModelSchema::new(json!({
        "type":"object","additionalProperties":false,
        "properties": {
            "schema_version":{"const":1},
            "proposals":{"type":"array","minItems":0,"maxItems":1,"items":{
                "type":"object","additionalProperties":false,
                "properties":{
                    "observation_kind":{"type":"string","enum":["explicit_remember","user_correction","outcome_conflict","reusable_procedure"]},
                    "value":{"type":"object","additionalProperties":false,
                        "properties":{
                            "kind":{"type":"string","enum":["fact","observation","inference","preference","commitment"]},
                            "statement":{"type":"string","minLength":1,"maxLength":2048,
                                "description":"Preserve the exact named subject and supported claim from evidence. Never substitute the user for a named person."},
                            "epistemic_status":{"type":"string","enum":["fact","inference"]},
                            "confidence_millis":{"type":"integer","minimum":0,"maximum":1000},
                            "valid_from":{"type":"string","minLength":1,"maxLength":64,
                                "description":"Only an explicitly stated start time, as RFC3339; omit when not stated."},
                            "valid_until":{"type":"string","minLength":1,"maxLength":64,
                                "description":"Preserve an explicitly stated expiry, as RFC3339; omit when not stated."},
                            "observed_at":{"const":"1970-01-01T00:00:00Z"}
                        },"required":["kind","statement","epistemic_status","confidence_millis","observed_at"]},
                    "target_id":{"type":"string","minLength":36,"maxLength":36,
                        "description":"For a revision only, the exact supplied current memory UUID; omit for a new memory."},
                    "base_revision":{"type":"integer","minimum":1,
                        "description":"For a revision only, the supplied target revision; omit for a new memory."}
                },"required":["observation_kind","value"]
            }}
        },"required":["schema_version","proposals"]
    })).map_err(|_| floe_kernel::AgentFailure::InvalidInput)
}
pub fn learner_output_format()
-> Result<floe_agent_contract::ModelOutputFormat, floe_kernel::AgentFailure> {
    Ok(floe_agent_contract::ModelOutputFormat::Json {
        schema: learner_output_schema()?,
    })
}
