use crate::{AgentFailure, PersonId};

/// Host-admitted caller facts. This value is not an authorization capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerActor {
    pub person_id: PersonId,
    pub device_id: String,
    pub runtime_epoch: u64,
}

impl OwnerActor {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.trim() != self.device_id
            || self.device_id.chars().any(char::is_control)
            || self.runtime_epoch == 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
