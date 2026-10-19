//! Durable user intent precedes model selection. Replays observe custody only;
//! an admitted request is not native permission or provider dispatch authority.
use floe_kernel::{AgentFailure, OwnerActor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceStartCommand {
    /// Stable across observation retries; also the eventual Voice call identity.
    pub command_id: Uuid,
    pub surface_session_id: Uuid,
}
impl VoiceStartCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id.is_nil() || self.surface_session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn digest(&self, actor: &OwnerActor) -> Result<[u8; 32], AgentFailure> {
        self.validate()?;
        actor.validate()?;
        // Epoch belongs to the original admission, not retry identity. A new
        // epoch may observe/cancel a lost preparation, never acquire it again.
        let encoded = serde_json::to_vec(&(actor.person_id, &actor.device_id, self))
            .map_err(|_| AgentFailure::InvalidInput)?;
        let mut hash = Sha256::new();
        hash.update(b"floe.voice.start-command.v1\0");
        hash.update(encoded);
        Ok(hash.finalize().into())
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceStartCommandState {
    Preparing,
    Bound,
    Cancelled,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceStartCommandRecord {
    pub command: VoiceStartCommand,
    #[serde(with = "crate::session::actor_wire")]
    pub actor: OwnerActor,
    pub state: VoiceStartCommandState,
}
impl VoiceStartCommandRecord {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.command.validate()?;
        self.actor.validate()
    }
}
#[derive(Clone, Debug)]
pub struct VoiceStartCommandAdmission {
    pub record: VoiceStartCommandRecord,
    /// Only the original successful admission may proceed with preparation.
    /// Lost ACK recovery always returns true, including across runtime epochs.
    pub replayed: bool,
}
