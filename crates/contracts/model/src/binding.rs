//! Shared non-secret model routing values; none are dispatch authority.
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingBoundary {
    Device,
    Gateway,
}

/// Correlation only. Possession of a digest never authorizes a dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelBindingDigest(pub [u8; 32]);

/// Opaque provider-owned commitment to the selected target and its revision.
/// It is correlation evidence only; it cannot authorize a model dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelSelectionCommitment(pub [u8; 32]);

impl ModelSelectionCommitment {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        (self.0 != [0; 32])
            .then_some(())
            .ok_or(AgentFailure::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_neutral_binding_values_preserves_the_generation_wire() {
        assert_eq!(
            serde_json::to_string(&ProcessingBoundary::Gateway).unwrap(),
            "\"gateway\""
        );
        assert_eq!(
            serde_json::to_string(&ProcessingBoundary::Device).unwrap(),
            "\"device\""
        );
        let digest = ModelBindingDigest([7; 32]);
        assert_eq!(
            serde_json::to_value(digest).unwrap(),
            serde_json::to_value(vec![7; 32]).unwrap()
        );
        let commitment = ModelSelectionCommitment([8; 32]);
        commitment.validate().unwrap();
        assert_eq!(
            serde_json::from_value::<ModelSelectionCommitment>(
                serde_json::to_value(commitment).unwrap()
            )
            .unwrap(),
            commitment
        );
        assert_eq!(
            ModelSelectionCommitment([0; 32]).validate(),
            Err(AgentFailure::InvalidInput)
        );
    }
}
