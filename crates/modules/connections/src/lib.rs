mod api;
pub mod application;
mod ports;

pub use api::{
    CalendarConnectionRef, ConnectorCatalogObservation, PAIRING_REPORT_VERSION,
    PairingConfirmation, PairingConfirmationRequest, PairingIssuer, PairingStatus,
    PairingStatusRequest, ProducerIdentity, admit_pairing_issuer, admit_pairing_status,
    project_calendar_connections,
};
pub use application::pairing::{
    PAIRING_POLL_INTERVAL_MS, PairingDirective, PairingOperation, PairingOperationState,
    PairingService, cancel_pairing, observe_pairing,
};
pub use application::remote_pairing::{
    PairingIdentity, PairingOwnerKeys, admit_pairing_report, confirm_pairing, finalize_pairing,
    read_pairing_status,
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
