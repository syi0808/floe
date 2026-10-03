use crate::{CoreError, ErrorCode};
use floe_context::SourceLeaseRegistry;
use floe_vault::TursoStore;
use std::sync::Arc;

pub struct FloeCore {
    pub(crate) store: Arc<TursoStore>,
    pub(crate) lease_registry: Arc<SourceLeaseRegistry>,
    pub(crate) day: Arc<floe_day::DayService>,
    pub(crate) product_gateway: Arc<floe_provider_adapters::gateway::ProductGatewayLeaseRegistry>,
}
pub use floe_day::Classification;
impl FloeCore {
    pub fn day_service(&self) -> Arc<floe_day::DayService> {
        self.day.clone()
    }
    pub fn source_service(&self) -> floe_connections::SourceConnectionService<'_, TursoStore> {
        floe_connections::SourceConnectionService::new(self.store.as_ref())
    }
}

pub(crate) fn day_error(error: floe_day::DayError) -> CoreError {
    let code = match error.code {
        floe_day::DayErrorCode::Validation => ErrorCode::Validation,
        floe_day::DayErrorCode::NotFound => ErrorCode::NotFound,
        floe_day::DayErrorCode::Conflict => ErrorCode::Conflict,
        floe_day::DayErrorCode::Storage => ErrorCode::Storage,
    };
    let mut result = CoreError::new(code, error.message);
    for (key, value) in error.metadata {
        result = result.with_metadata(key, value);
    }
    result
}
