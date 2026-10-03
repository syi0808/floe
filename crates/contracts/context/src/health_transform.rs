use crate::{CapacityState, RecoveryState, WellbeingView};
use chrono::{DateTime, Duration, Utc};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Receipt identity for a completed native Health privacy operation. Serialized
/// values are correlation only; the live host registry must independently attest
/// the identical operation before a dependency is authorized.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HealthTransformEvidence {
    pub operation_id: Uuid,
    pub host_epoch: String,
    pub device_id: String,
    pub output_sha256: [u8; 32],
    pub transformed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
impl HealthTransformEvidence {
    pub fn validate(&self, device_id: &str, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        if self.operation_id.is_nil()
            || self.host_epoch.is_empty()
            || self.host_epoch.len() > 256
            || self.device_id != device_id
            || device_id.is_empty()
            || self.output_sha256 == [0; 32]
            || self.transformed_at > now
            || self.expires_at <= now
            || self.expires_at <= self.transformed_at
            || self.expires_at - self.transformed_at > Duration::minutes(30)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub fn validate_view(
        &self,
        device_id: &str,
        view: &WellbeingView,
        now: DateTime<Utc>,
    ) -> Result<(), AgentFailure> {
        self.validate(device_id, now)?;
        crate::validate_wellbeing_view(view, now.timestamp_millis())?;
        if self.output_sha256 != health_output_digest(view.capacity, view.recovery)
            || self.transformed_at.timestamp_millis() != view.observed_at_unix_ms
            || self.expires_at.timestamp_millis() != view.expires_at_unix_ms
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}
pub fn health_output_digest(capacity: CapacityState, recovery: RecoveryState) -> [u8; 32] {
    let capacity = match capacity {
        CapacityState::Reduced => "reduced",
        CapacityState::Typical => "typical",
        CapacityState::Strong => "strong",
        CapacityState::Unknown => "unknown",
    };
    let recovery = match recovery {
        RecoveryState::NeedsRecovery => "needs_recovery",
        RecoveryState::Typical => "typical",
        RecoveryState::Recovered => "recovered",
        RecoveryState::Unknown => "unknown",
    };
    Sha256::digest(format!("floe.health.transform.v1\0{capacity}\0{recovery}").as_bytes()).into()
}
