//! Route observations shared by generation and long-lived model planning.
use floe_agent_contract::AgentFailure;

pub enum PrimaryObservation<P> {
    Available(P),
    Absent(PrimaryAbsence),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimaryAbsence {
    NoGatewayConfigured,
    PurposeNotConfigured,
    PurposeDisabled,
}

pub enum LocalObservation<P> {
    Available(P),
    Unavailable(LocalAvailabilityReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalAvailabilityReason {
    Unsupported,
    Disabled,
    NotReady,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelObservationError {
    InvalidIdentity,
    InvalidInventory,
    CapabilityUnavailable,
    CredentialRejected,
    PermissionDenied,
    Timeout,
    TransportUnavailable,
    Cancelled,
    StorageUnavailable,
    QuotaExceeded,
}

impl From<ModelObservationError> for AgentFailure {
    fn from(error: ModelObservationError) -> Self {
        match error {
            ModelObservationError::InvalidIdentity
            | ModelObservationError::PermissionDenied
            | ModelObservationError::CapabilityUnavailable => Self::PolicyDenied,
            ModelObservationError::InvalidInventory => Self::ServerModelInvalidOutput,
            ModelObservationError::CredentialRejected => Self::CredentialExpired,
            ModelObservationError::Timeout => Self::DeadlineExceeded,
            ModelObservationError::TransportUnavailable => Self::ServerModelUnavailable,
            ModelObservationError::Cancelled => Self::Cancelled,
            ModelObservationError::StorageUnavailable => Self::StorageUnavailable,
            ModelObservationError::QuotaExceeded => Self::QuotaExceeded,
        }
    }
}
