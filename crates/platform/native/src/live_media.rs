//! Private native WebRTC signaling/handle boundary. No Dart JSON, provider
//! credential, automatic permission prompt or unmute is part of this port.
use floe_execution::{BoxFuture, Cancellation};
use floe_kernel::{AgentFailure, OwnerActor};
use std::sync::Arc;
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveMediaBinding {
    pub actor: OwnerActor,
    pub voice_session_id: Uuid,
    pub media_lease_id: Uuid,
    pub plan_id: Uuid,
}
impl LiveMediaBinding {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        if [self.voice_session_id, self.media_lease_id, self.plan_id]
            .iter()
            .any(Uuid::is_nil)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Signaling is private adapter/native data, never a product or persisted value.
/// It is deliberately neither Serialize nor Clone and diagnostic text is redacted.
pub struct LiveSessionDescription(String);
impl LiveSessionDescription {
    pub fn new(value: String) -> Result<Self, AgentFailure> {
        if value.len() > 65_536 || !value.starts_with("v=0\r\n") || value.contains('\0') {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for LiveSessionDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveSessionDescription[redacted]")
    }
}

/// Lookup only an already-prepared, muted, exact native lease. This cannot
/// manufacture permission or a substitute lease if the host/generation changed.
pub trait LiveMediaRegistry: Send + Sync {
    fn lease(&self, binding: &LiveMediaBinding) -> Result<Arc<dyn LiveMediaLease>, AgentFailure>;
}
pub trait LiveMediaLease: Send + Sync {
    fn binding(&self) -> &LiveMediaBinding;
    fn validate_current(&self) -> Result<(), AgentFailure>;
    /// Offer creation stays muted and must never ask for microphone permission.
    fn offer<'a>(
        &'a self,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<LiveSessionDescription, AgentFailure>>;
    /// Applying an answer does not unmute. The exact Voice media command owns it.
    fn apply_answer<'a>(
        &'a self,
        answer: LiveSessionDescription,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
    /// Immediate local fence. A late answer/unmute/write must stay rejected.
    /// A fence is not the host's completed microphone/speaker Stop receipt.
    fn fence(&self);
    fn lifetime_cancellation(&self) -> Cancellation;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signaling_is_bounded_and_redacted() {
        let value = LiveSessionDescription::new("v=0\r\ns=private-address\r\n".into()).unwrap();
        assert!(!format!("{value:?}").contains("private-address"));
        assert!(LiveSessionDescription::new("not SDP".into()).is_err());
        assert!(LiveSessionDescription::new(format!("v=0\r\n{}", "x".repeat(65_536))).is_err());
        assert!(LiveSessionDescription::new("v=0\r\n\0".into()).is_err());
    }
}
