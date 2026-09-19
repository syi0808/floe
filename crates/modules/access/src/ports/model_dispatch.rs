use floe_context_contract::{DataClass, DependencyCoverage};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

/// Where an already-authorized model input may be dispatched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelDispatchTarget {
    Device,
    External { recipient: String },
}

impl ModelDispatchTarget {
    pub fn is_external(&self) -> bool {
        matches!(self, Self::External { .. })
    }

    pub fn recipient(&self) -> Option<&str> {
        match self {
            Self::Device => None,
            Self::External { recipient } => Some(recipient),
        }
    }
}

/// What Inference asks Access to admit for one model handoff.
///
/// All values are owner values: person, projection identity, coverage,
/// input data classes, purpose, consumer and the exact dispatch target.
/// No endpoint, bearer or provider credential travels here.
#[derive(Clone, Debug)]
pub struct ModelDispatchRequest {
    pub person_id: PersonId,
    pub projection_ref: Uuid,
    pub projection_revision: u64,
    pub coverage: DependencyCoverage,
    pub input_data_classes: Vec<DataClass>,
    pub purpose: String,
    pub consumer: String,
    pub target: ModelDispatchTarget,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

impl ModelDispatchRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.person_id.is_valid()
            || self.projection_ref.is_nil()
            || self.projection_revision == 0
            || !valid_identifier(&self.purpose)
            || !valid_identifier(&self.consumer)
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if self.input_data_classes.len() > 32 {
            return Err(AgentFailure::InvalidInput);
        }
        match &self.target {
            ModelDispatchTarget::Device => Ok(()),
            ModelDispatchTarget::External { recipient } => {
                if !valid_recipient(recipient) {
                    return Err(AgentFailure::InvalidInput);
                }
                Ok(())
            }
        }
    }
}

/// Current exact-recipient authority for model dispatch, without credentials.
///
/// Reports whether one exact external recipient still holds processing
/// authority/consent right now. Device dispatch never consults it.
pub trait ModelDispatchRecipientAuthority: Send + Sync {
    fn check_recipient(&self, recipient: &str) -> Result<(), AgentFailure>;
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_recipient(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= 256
        && !value.chars().any(char::is_control)
}
