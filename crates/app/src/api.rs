use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostError {
    InvalidIdentity,
    IdentityUnavailable,
    InvalidRequest,
    Closing,
    UnsupportedCaller,
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalIdentityClaim {
    pub person_id: Uuid,
    pub device_id: String,
}

pub trait LocalIdentityProvider {
    fn verified_local_identity(&self) -> Result<LocalIdentityClaim, HostError>;
}

pub trait HostServices {
    fn shutdown(&self) -> Result<(), HostError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallerContext {
    person_id: Uuid,
    device_id: String,
    runtime_epoch: Uuid,
}

impl CallerContext {
    pub fn person_id(&self) -> Uuid {
        self.person_id
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn runtime_epoch(&self) -> Uuid {
        self.runtime_epoch
    }

    pub(crate) fn verified(
        claim: LocalIdentityClaim,
        runtime_epoch: Uuid,
    ) -> Result<Self, HostError> {
        if claim.person_id.is_nil()
            || claim.device_id.trim() != claim.device_id
            || claim.device_id.is_empty()
            || claim.device_id.len() > 128
            || claim.device_id.chars().any(char::is_control)
            || runtime_epoch.is_nil()
        {
            return Err(HostError::InvalidIdentity);
        }
        Ok(Self {
            person_id: claim.person_id,
            device_id: claim.device_id,
            runtime_epoch,
        })
    }
}
