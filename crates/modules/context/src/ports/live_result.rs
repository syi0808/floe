//! Canonical published Manager output for live presentation. Reading this data
//! grants neither source processing nor permission to send it to a provider.
use floe_context_contract::{DependencyCoverage, LiveProjectionSource};
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, OwnerActor, PersonId};
use std::fmt;

/// One exact result, resolved with its publication and canonical Run in one
/// owner snapshot. No provider caption, UI cache or latest Run may substitute.
#[derive(Clone, Eq, PartialEq)]
pub struct LiveResultSnapshot {
    pub person_id: PersonId,
    pub source: LiveProjectionSource,
    pub text: String,
    pub partial: bool,
    pub coverage: DependencyCoverage,
}
impl LiveResultSnapshot {
    /// One canonical encoding used for projection and durable delivery binding.
    /// Internal Run identities, source proofs and credentials never enter it.
    pub fn presentable_payload(
        &self,
    ) -> Result<floe_model_contract::LiveContextPayload, AgentFailure> {
        floe_model_contract::LiveContextPayload::new(
            serde_json::json!({
                "kind": "manager_result", "partial": self.partial, "text": self.text,
            })
            .to_string(),
        )
        .map_err(|_| AgentFailure::ModelInputCapacityExceeded)
    }

    pub fn validate_for(
        &self,
        actor: &OwnerActor,
        source: &LiveProjectionSource,
    ) -> Result<(), AgentFailure> {
        actor.validate()?;
        source.validate()?;
        if !matches!(source, LiveProjectionSource::SurfaceResult { .. }) {
            return Err(AgentFailure::InvalidInput);
        }
        if self.person_id != actor.person_id || &self.source != source {
            return Err(AgentFailure::PolicyDenied);
        }
        if self.text.trim().is_empty() || self.text.len() > floe_agent_contract::MAX_OUTPUT_BYTES {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)
    }
}
impl fmt::Debug for LiveResultSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveResultSnapshot")
            .field("text_bytes", &self.text.len())
            .field("partial", &self.partial)
            .finish_non_exhaustive()
    }
}

pub trait LiveResultReader: Send + Sync {
    fn read_live_result<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a LiveProjectionSource,
    ) -> BoxFuture<'a, Result<LiveResultSnapshot, AgentFailure>>;
}
