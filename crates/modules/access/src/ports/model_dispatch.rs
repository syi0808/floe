use super::gateway_admission::VerifiedGatewayBinding;
use floe_context_contract::{DataClass, DependencyCoverage};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelDispatchTarget {
    Device,
    Gateway { expected: VerifiedGatewayBinding },
}

impl ModelDispatchTarget {
    pub fn is_gateway(&self) -> bool {
        matches!(self, Self::Gateway { .. })
    }
}

/// Immutable prepared-plan and projection identity. Neither the digest nor the
/// expected Gateway is authority: admission reloads independent live evidence.
#[derive(Clone, Debug)]
pub struct ModelDispatchRequest {
    pub person_id: PersonId,
    pub device_id: String,
    pub plan_id: Uuid,
    pub binding_digest: [u8; 32],
    pub projection_ref: Uuid,
    pub projection_revision: u64,
    pub coverage: DependencyCoverage,
    pub input_data_classes: Vec<DataClass>,
    pub purpose: String,
    pub consumer: String,
    pub target: ModelDispatchTarget,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

impl ModelDispatchRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.person_id.is_valid()
            || self.plan_id.is_nil()
            || self.projection_ref.is_nil()
            || self.projection_revision == 0
            || self.binding_digest == [0; 32]
            || [&self.device_id, &self.purpose, &self.consumer]
                .iter()
                .any(|value| {
                    value.is_empty()
                        || value.len() > 256
                        || value.trim() != value.as_str()
                        || value.chars().any(char::is_control)
                })
            || self.input_data_classes.is_empty()
            || self.input_data_classes.len() > 32
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if let ModelDispatchTarget::Gateway { expected } = &self.target {
            expected.validate()?;
            if expected.binding_digest() != self.binding_digest {
                return Err(AgentFailure::PolicyDenied);
            }
            if expected.person_id != self.person_id.to_string()
                || expected.device_id != self.device_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
        }
        Ok(())
    }
}
