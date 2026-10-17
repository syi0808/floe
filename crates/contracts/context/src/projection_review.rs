//! Source-processing review values shared by generation and live projections.
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProjectionReview {
    pub projection_operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub blockers: crate::SourceAccessBlockers,
}

impl SourceProjectionReview {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.projection_operation_id.is_nil() || self.target_digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        self.blockers
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)
    }
}
