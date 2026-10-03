//! Immutable capability descriptions carried by model projections.
use floe_context_contract::DataClass;
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub read_only: bool,
    pub output_data_class: DataClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != crate::AGENT_SCHEMA_VERSION
            || self.id.trim().is_empty()
            || self.version.trim().is_empty()
            || self
                .input_schema
                .as_ref()
                .is_some_and(|schema| !schema.is_object())
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
