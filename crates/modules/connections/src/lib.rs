mod domain { pub mod source_operation; pub mod product; pub mod personal_source_spec; }
pub use domain::personal_source_spec::PersonalSourceSpec;
pub use domain::source_operation::*;
mod api;
pub mod application;
mod ports;
mod source;

pub use source::{
    ConnectionResource, ResourceMode, SourceConnection, SourceConnectionError, SourceState,
};

pub use api::{CalendarConnectionRef,ConnectorCatalogObservation,project_calendar_connections};
pub use application::source_connections::{SourceConnectionService, SourceServiceError};
pub use ports::{SourceRepository, SourceRepositoryError};

pub use application::connected_context::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConformanceCode, ConformanceViolation,
    ConnectionState, ConnectorCapabilityDescriptor, ConnectorConnectionSnapshot,
    ConnectorDescriptor, ConnectorSnapshot, DeviceBinding, ExecutionLocation, RetentionClass,
    SituationConformanceReport, SituationDescriptor, SituationTrigger, SourceFailure,
    SourceFailureKind, SourceIssue, ViewDescriptor, ViewSnapshot, evaluate_situation,
    validate_connector_snapshot,
};

pub use floe_context_contract::{ConnectionId, ConnectorId};

pub use ports::{SourceOperationRepository,ConnectionsRepository};

pub use application::source_operation::{ConnectionsService,ConnectionsDependencies,PrepareSourceReviews,PreparedSourceReviews,PrepareProjectionReviews,PreparedProjectionReviews,SourceReviewEvidence,SourceCleanup,SourceCleanupOutcome};

pub use ports::gateway_pairing::*;
pub use application::gateway_pairing::{GatewayPairingService,GatewaySetupRecord,PairingRepository,PairingRecord,PairingSnapshot,PairingState};

pub use domain::product::*;
pub use ports::remote_integration::*;
pub use ports::product_repository::ConnectionsProductRepository;

pub use application::product::source_ref;
