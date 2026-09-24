use uuid::Uuid;

use crate::{CallerContext, ServiceError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingTarget {
    pub base_url: String,
}

impl PairingTarget {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.base_url.is_empty()
            || self.base_url.len() > 2048
            || self.base_url.trim() != self.base_url
            || self.base_url.chars().any(char::is_control)
        {
            Err(ServiceError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum RemotePairingCommand {
    Prepare,
    Confirm {
        target: PairingTarget,
        challenge: Box<crate::RemotePairingChallenge>,
        issuer_fingerprint: String,
        polling_proof: String,
    },
    Status {
        target: PairingTarget,
        pairing_id: String,
        polling_proof: String,
    },
    Finalize {
        target: PairingTarget,
        pairing_id: String,
        polling_proof: String,
        challenge: Box<crate::RemotePairingChallenge>,
        issuer_fingerprint: String,
    },
}

impl RemotePairingCommand {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Prepare => "remote_pairing_prepare",
            Self::Confirm { .. } => "remote_pairing_confirm",
            Self::Status { .. } => "remote_pairing_status",
            Self::Finalize { .. } => "remote_pairing_finalize",
        }
    }

    pub fn validate(&self) -> Result<(), ServiceError> {
        match self {
            Self::Prepare => Ok(()),
            Self::Confirm {
                target,
                challenge,
                polling_proof,
                ..
            } => {
                target.validate()?;
                validate_pairing_text(&challenge.pairing_id, polling_proof)
            }
            Self::Status {
                target,
                pairing_id,
                polling_proof,
            }
            | Self::Finalize {
                target,
                pairing_id,
                polling_proof,
                ..
            } => {
                target.validate()?;
                validate_pairing_text(pairing_id, polling_proof)
            }
        }
    }
}

impl std::fmt::Debug for RemotePairingCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

fn validate_pairing_text(pairing_id: &str, proof: &str) -> Result<(), ServiceError> {
    if !Uuid::parse_str(pairing_id).is_ok_and(|identifier| !identifier.is_nil())
        || proof.is_empty()
        || proof.len() > 256
        || !proof
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Err(ServiceError::InvalidInput)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RemotePairingResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub owner: Option<crate::RemoteOwnerPublicKey>,
    pub pairing: Option<crate::PairingStatus>,
    pub failure: Option<crate::AgentFailure>,
}

pub trait RemotePairingCommands {
    fn remote_pairing(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
        command: RemotePairingCommand,
    ) -> Result<RemotePairingResult, ServiceError>;

    fn read_pairing_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<RemotePairingResult, ServiceError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoteAccessCommand {
    InspectProducer,
    ReviewAndEnroll {
        producer: Box<crate::RemoteProducerIdentity>,
    },
    EnrollmentStatus {
        enrollment_id: String,
    },
    ConnectionObserve {
        connector_id: String,
        connection_id: String,
        resource: Option<String>,
        enabled: Option<bool>,
        disconnecting: bool,
    },
}

impl RemoteAccessCommand {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::InspectProducer => "remote_authority_inspect_producer",
            Self::ReviewAndEnroll { .. } => "remote_authority_review_and_enroll",
            Self::EnrollmentStatus { .. } => "remote_authority_enrollment_status",
            Self::ConnectionObserve { enabled, .. } => match enabled {
                None => "remote_connection_observe_inspect",
                Some(true) => "remote_connection_observe_enable",
                Some(false) => "remote_connection_observe_disable",
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoteAccessResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub person_id: crate::PersonId,
    pub producer: Option<crate::RemoteProducerIdentity>,
    pub owner: Option<crate::RemoteOwnerPublicKey>,
    pub enrollment: Option<crate::RemoteEnrollmentStatus>,
    pub connection_observe_status: Option<String>,
    pub failure: Option<crate::AgentFailure>,
}

pub trait RemoteAccessCommands {
    fn remote_access(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
        command: RemoteAccessCommand,
    ) -> Result<RemoteAccessResult, ServiceError>;

    fn read_access_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<RemoteAccessResult, ServiceError>;
}
