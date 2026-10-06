//! Role-neutral composition. Inference alone selects Primary or fallback.
use super::{credentials::GatewayCredentialStore, inference::GatewayModelProvider};
use crate::models::device::DeviceModelProvider;
use floe_agent_contract::{BoxFuture, ModelPlanRequest};
use floe_execution::ExecutionScope;
use floe_inference::{
    LocalObservation, ModelObservationError, ModelProvider, PreparedModelProfile,
    PreparedModelTransport, PrimaryObservation,
};

pub struct CompositeModelProvider {
    gateway: GatewayModelProvider,
    device: DeviceModelProvider,
}
impl CompositeModelProvider {
    pub fn new(store: GatewayCredentialStore) -> Self {
        Self {
            gateway: GatewayModelProvider::new(store),
            device: DeviceModelProvider::encrypted(),
        }
    }
}
impl ModelProvider for CompositeModelProvider {
    fn observe_primary<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<PrimaryObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        Box::pin(async move {
            Ok(match self.gateway.observe_primary(request, scope).await? {
                PrimaryObservation::Available(profile) => {
                    let transport: Box<dyn PreparedModelTransport> = Box::new(profile.transport);
                    PrimaryObservation::Available(PreparedModelProfile {
                        capability: profile.capability,
                        transport,
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
    ) -> BoxFuture<
        'a,
        Result<LocalObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        Box::pin(async move {
            Ok(
                match self.device.observe_local_fallback(request, scope).await? {
                    LocalObservation::Available(profile) => {
                        let transport: Box<dyn PreparedModelTransport> =
                            Box::new(profile.transport);
                        LocalObservation::Available(PreparedModelProfile {
                            capability: profile.capability,
                            transport,
                        })
                    }
                    LocalObservation::Unavailable(reason) => LocalObservation::Unavailable(reason),
                },
            )
        })
    }
}
