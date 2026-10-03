//! Role-neutral composition. Inference alone selects Primary or fallback.
use super::{
    credentials::GatewayCredentialStore,
    inference::{GatewayModelProvider, PreparedGatewayTransport},
};
use crate::models::foundation::{FoundationModelProvider, PreparedFoundationTransport};
use floe_agent_contract::{AgentFailure, BoxFuture, ModelPlanRequest};
use floe_execution::ExecutionScope;
use floe_inference::{
    AdmittedDispatchTarget, CanonicalModelRequest, CanonicalModelResponse, LocalObservation,
    ModelObservationError, ModelProvider, PreparedModelProfile, PreparedModelTransport,
    PrimaryObservation,
};

pub struct CompositeModelProvider {
    gateway: GatewayModelProvider,
    foundation: FoundationModelProvider,
}
impl CompositeModelProvider {
    pub fn new(store: GatewayCredentialStore) -> Self {
        Self {
            gateway: GatewayModelProvider::new(store),
            foundation: FoundationModelProvider::encrypted(),
        }
    }
}
pub enum PreparedCompositeTransport {
    Gateway(PreparedGatewayTransport),
    Device(PreparedFoundationTransport),
}
impl PreparedModelTransport for PreparedCompositeTransport {
    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget {
        match self {
            Self::Gateway(transport) => transport.dispatch_target(),
            Self::Device(transport) => transport.dispatch_target(),
        }
    }
    fn generate<'a>(
        &'a self,
        request: CanonicalModelRequest,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'a, Result<CanonicalModelResponse, AgentFailure>> {
        match self {
            Self::Gateway(transport) => transport.generate(request, target),
            Self::Device(transport) => transport.generate(request, target),
        }
    }
}
impl ModelProvider for CompositeModelProvider {
    type Prepared = PreparedCompositeTransport;
    fn observe_primary<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<PrimaryObservation<Self::Prepared>, ModelObservationError>> {
        Box::pin(async move {
            Ok(match self.gateway.observe_primary(request, scope).await? {
                PrimaryObservation::Available(profile) => {
                    PrimaryObservation::Available(PreparedModelProfile {
                        capability: profile.capability,
                        transport: PreparedCompositeTransport::Gateway(profile.transport),
                    })
                }
                PrimaryObservation::Absent(reason) => PrimaryObservation::Absent(reason),
            })
        })
    }
    fn observe_local_fallback<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<LocalObservation<Self::Prepared>, ModelObservationError>> {
        Box::pin(async move {
            Ok(
                match self
                    .foundation
                    .observe_local_fallback(request, scope)
                    .await?
                {
                    LocalObservation::Available(profile) => {
                        LocalObservation::Available(PreparedModelProfile {
                            capability: profile.capability,
                            transport: PreparedCompositeTransport::Device(profile.transport),
                        })
                    }
                    LocalObservation::Unavailable(reason) => LocalObservation::Unavailable(reason),
                },
            )
        })
    }
}
