//! Typed observations and one selected, private provider capability.

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, BoxFuture, ContextEnvelope, ModelBindingDigest,
    ModelCapabilities, ModelPlanRequest, ModelStep, ProcessingBoundary,
};
use floe_context_contract::DataClass;
use floe_execution::{Cancellation, ExecutionScope};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{ModelConsumer, ModelPurpose};

/// Proof of consumed Access admission. Product-supplied digests cannot create it.
#[derive(Clone, Debug)]
pub struct AdmittedDispatchTarget {
    binding_digest: ModelBindingDigest,
    target: floe_access::ModelDispatchTarget,
}

impl AdmittedDispatchTarget {
    pub fn from_consumed<Resolver, Authority>(
        fence: &floe_access::ModelDispatchFence<'_, '_, '_, Resolver, Authority>,
    ) -> Self {
        let (binding_digest, target) = fence.target();
        Self {
            binding_digest: ModelBindingDigest(*binding_digest),
            target: target.clone(),
        }
    }

    pub fn matches(
        &self,
        binding_digest: &ModelBindingDigest,
        boundary: ProcessingBoundary,
    ) -> bool {
        self.binding_digest == *binding_digest
            && matches!(
                (&self.target, boundary),
                (
                    floe_access::ModelDispatchTarget::Device,
                    ProcessingBoundary::Device
                ) | (
                    floe_access::ModelDispatchTarget::Gateway { .. },
                    ProcessingBoundary::Gateway
                )
            )
    }
}

#[derive(Clone, Debug)]
pub struct CanonicalModelRequest {
    pub attempt_id: Uuid,
    pub envelope: ContextEnvelope,
    pub catalog: AllowedCatalog,
    pub input_data_classes: Vec<DataClass>,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

impl CanonicalModelRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.attempt_id.is_nil()
            || self.remaining_tokens == 0
            || self.max_output_bytes == 0
            || self.max_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
            || self.input_data_classes.is_empty()
            || self.input_data_classes.iter().any(|class| {
                !matches!(
                    class,
                    DataClass::Synthetic | DataClass::Personal | DataClass::HighlySensitive
                )
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.envelope.validate()?;
        ModelCapabilities::for_request(&self.envelope.run_instructions.output_format, &self.catalog)?;
        self.catalog
            .tools
            .iter()
            .try_for_each(floe_agent_contract::ToolDescriptor::validate)?;
        self.catalog
            .cards
            .iter()
            .try_for_each(floe_agent_contract::AgentDefinition::validate)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProviderUsageObservation {
    pub tokens: Option<u64>,
    pub cost_micros: Option<u64>,
}

/// A trustworthy envelope retains usage even when its output is invalid.
#[derive(Clone, Debug)]
pub struct CanonicalModelResponse {
    pub output: Result<Vec<ModelStep>, AgentFailure>,
    pub usage: ProviderUsageObservation,
}

pub trait PreparedModelTransport: Send + Sync {
    /// Pure codec/limits preflight, before the irreversible dispatch fact.
    fn validate_request(&self, request: &CanonicalModelRequest) -> Result<(), AgentFailure>;

    /// The adapter-retained non-secret expected binding; Access verifies it live.
    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget;

    fn generate<'a>(
        &'a self,
        request: CanonicalModelRequest,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'a, Result<CanonicalModelResponse, AgentFailure>>;
}

pub struct PreparedModelProfile<P> {
    pub capability: ObservedModelCapability,
    pub transport: P,
}

#[derive(Clone, Debug)]
pub struct ObservedModelCapability {
    pub purpose: ModelPurpose,
    pub consumer: ModelConsumer,
    pub capabilities: ModelCapabilities,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
}

pub enum PrimaryObservation<P> {
    Available(PreparedModelProfile<P>),
    Absent(PrimaryAbsence),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimaryAbsence {
    NoGatewayConfigured,
    PurposeNotConfigured,
    PurposeDisabled,
}

pub enum LocalObservation<P> {
    Available(PreparedModelProfile<P>),
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
            ModelObservationError::InvalidIdentity | ModelObservationError::PermissionDenied => {
                Self::PolicyDenied
            }
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

pub trait ModelProvider: Send + Sync {
    type Prepared: PreparedModelTransport + Send + Sync + 'static;

    fn observe_primary<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<PrimaryObservation<Self::Prepared>, ModelObservationError>>;

    fn observe_local_fallback<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<LocalObservation<Self::Prepared>, ModelObservationError>>;
}
