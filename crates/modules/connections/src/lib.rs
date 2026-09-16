mod api;
pub mod application;
mod ports;

pub use api::{
    CalendarConnectionRef, ConnectorCatalogObservation, PairingConfirmation,
    PairingConfirmationRequest, PairingIssuer, PairingStatus, PairingStatusRequest,
    ProducerIdentity, project_calendar_connections,
};
pub use application::pairing::{
    PAIRING_POLL_INTERVAL_MS, PairingDirective, PairingOperation, PairingOperationState,
    PairingService, cancel_pairing, observe_pairing,
};
pub use ports::RemoteControl;

pub use application::connected_context::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConformanceCode, ConformanceViolation,
    ConnectionState, ConnectorCapabilityDescriptor, ConnectorConnectionSnapshot,
    ConnectorDescriptor, ConnectorSnapshot, DeviceBinding, ExecutionLocation, RetentionClass,
    SituationConformanceReport, SituationDescriptor, SituationTrigger, SourceFailure,
    SourceFailureKind, SourceIssue, ViewDescriptor, ViewSnapshot, evaluate_situation,
    validate_connector_snapshot,
};

pub use application::authorization::{
    AUTHORIZATION_DEADLINE, AUTHORIZATION_POLL_INTERVAL, AuthorizationDirective,
    AuthorizationError, AuthorizationOperation, AuthorizationState, ObservedAttempt,
    ObservedConnectorStatus, cancel_authorization, observe_authorization, start_authorization,
    valid_authorization_url,
};
pub use floe_context_contract::ConnectionId;
