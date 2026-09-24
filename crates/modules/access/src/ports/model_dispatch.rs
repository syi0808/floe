use std::future::Future;
use std::pin::Pin;

use floe_context_contract::{
    DataClass, DependencyCoverage, ProcessingRequirement, RecipientLineage,
};
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
/// input data classes, purpose, consumer, the reviewed route/profile
/// identity, the exact dispatch target, and the opaque intent lineage the
/// dispatch runs under (None only for callers with no Conversation
/// lineage, which never receive a reviewable requirement). No endpoint,
/// Bearer token, or provider credential travels here.
#[derive(Clone, Debug)]
pub struct ModelDispatchRequest {
    pub person_id: PersonId,
    pub projection_ref: Uuid,
    pub projection_revision: u64,
    pub coverage: DependencyCoverage,
    pub input_data_classes: Vec<DataClass>,
    pub purpose: String,
    pub consumer: String,
    pub profile_id: String,
    pub target: ModelDispatchTarget,
    pub lineage: Option<RecipientLineage>,
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
            || !valid_identifier(&self.profile_id)
            || self
                .lineage
                .is_some_and(|lineage| lineage.validate().is_err())
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

/// The authority's answer for one dispatch check.
///
/// `Missing` means no usable consent covers this dispatch under the live
/// pairing. Access then verifies the route is otherwise admissible and,
/// only then, derives the reviewable requirement from the actual request.
/// A missing lineage, a failed pairing admission, or a store failure is
/// never `Missing`: those fail closed as errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientCheckOutcome {
    Granted,
    Missing,
}

/// Why Access refused a dispatch: a hard prohibition, or a recoverable
/// missing consent.
///
/// Only an otherwise admissible route with a specific non-secret
/// recipient/profile and a reviewable lineage becomes `NeedsConsent`.
/// Policy prohibitions, unknown coverage, LocalOnly-to-external, recipient
/// mismatch, forged or stale dependencies, missing lineage, pairing
/// failures, and store failures stay `Hard` without a card.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelDispatchDenial {
    Hard(AgentFailure),
    NeedsConsent(ProcessingRequirement),
}

impl ModelDispatchDenial {
    /// The hard failure, mapping a recoverable denial to a fail-closed
    /// policy denial. Used where only hard outcomes can surface (response
    /// release: a revocation after handoff suppresses, never re-reviews).
    pub fn into_hard(self) -> AgentFailure {
        match self {
            Self::Hard(failure) => failure,
            Self::NeedsConsent(_) => AgentFailure::PolicyDenied,
        }
    }
}

/// Current exact-recipient authority for model dispatch, without credentials.
///
/// Checks whether a usable contextual consent covers this exact dispatch
/// under the live pairing right now. Device dispatch never consults it.
///
/// Implementations reload the current pairing and consent state on every
/// call, so revocation denies the very next fence. All denials must surface
/// as admission denials (PolicyDenied and friends): never return a
/// transport-style failure (StorageUnavailable, ModelUnavailable) that
/// would trigger a hidden fallback to another recipient.
pub trait ModelDispatchRecipientAuthority: Send + Sync {
    fn check_recipient<'a>(
        &'a self,
        request: &'a ModelDispatchRequest,
    ) -> Pin<Box<dyn Future<Output = Result<RecipientCheckOutcome, AgentFailure>> + Send + 'a>>;
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
