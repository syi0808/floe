mod domain;
pub use domain::connection_review::*;
mod application;
mod data_access_grant;
mod ports;

pub use application::attention_consumer;
pub use application::model_dispatch::{
    ModelDispatchFence, ModelDispatchPermit, admit_model_dispatch, consume_model_dispatch,
    revalidate_model_dispatch,
};
pub use application::{
    ATTENTION_CONNECTOR, ATTENTION_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTOR,
    WELLBEING_RESOURCE, apple_execution_owner, is_device_local_source,
};
pub use application::{
    AccessGrantMutation, GrantPolicyError, PersonalReadRequirement, ReadAuthorityEvidence,
    ReadAuthorityIdentity, ReleasePermit, ReleaseRecipient, RemoteProducerIdentity,
    RemoteViewGrantPreview, RemoteViewGrantRequest, RemoteViewSourceReference, active_read_grant,
    admit_release, admit_remote_view_binding, admit_remote_view_source, apply_grant_mutation,
    authorize_grant, consume_release, create_grant, grant_unchanged, preview_remote_view_grant,
    producer_is_pinned, remote_dependency_live, remote_dependency_resource,
    remote_dependency_source_admits, remote_view_source, source_matches_producer,
    subject_unchanged, valid_subject_fingerprint, validate_grant_dependency,
    validate_grant_expectation, validate_read_authority, validate_read_continuity,
};
pub use application::{
    NativeCalendarConnection, NativeCalendarReview, admit_native_calendar_setup,
    admit_native_calendar_subject, is_native_calendar, local_calendar_execution_owner,
    local_calendar_connection_id,
    native_calendar_connection_unchanged, native_calendar_connector,
    native_calendar_connection_id, native_calendar_execution_owner, native_calendar_provider,
    native_calendar_source_current, reviewed_native_subject,
};
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub use application::fixture_calendar_execution_owner;
pub use application::{admit_device_pairing, admit_enrollment_pairing};
pub use data_access_grant::{DataAccessGrant, GrantState, GrantTransitionError};
pub use floe_context_contract::{
    ConnectionId, ConnectorId, ContextDependency, ContextDependencyError, DependencyCoverage,
    ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation,
    GrantPurpose, GrantScope, GrantSourceBinding, GrantValidationError, MAX_CONNECTOR_ID_BYTES,
    MAX_CONSUMER_ID_BYTES, MAX_CONSUMERS, MAX_CONTEXT_DEPENDENCIES, MAX_CONTEXT_DEPENDENCY_BYTES,
    MAX_EXECUTION_OWNER_BYTES, MAX_RESOURCE_HANDLE_BYTES, MAX_SCOPE_BYTES, ProcessingRestriction,
    ResourceHandle, SourceAuthority,
};
pub use floe_kernel::PersonId;
pub use ports::CurrentAuthority;
pub use ports::dependency_authorization::{
    DependencyAuthorization, DependencyLiveness, DependencyResolver,
};
pub use ports::gateway_admission::{GatewayAdmission, VerifiedGatewayBinding};
pub use ports::model_dispatch::{ModelDispatchRequest, ModelDispatchTarget};
pub use ports::personal_subject::{
    PersonalSubjectEvidence, PersonalSubjectInspector, PersonalSubjectProbe,
};
pub use ports::remote_grants::{
    BoxFuture, RemoteCallWindow, RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery,
    SignedSourcePreview,
};

pub use application::calendar_read::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, admission_matches_dependency,
    admits_native_calendar_read, current_native_calendar_grant, native_calendar_resource,
};
pub use floe_context_contract::{CalendarProvider, CalendarReadAccessStamp, CalendarScope};

pub use application::authorization_signing::validate_authorization_grant;
pub use application::connection_review::{
    AccessClock, AccessService, PrepareConnectionReview, SystemAccessClock, ViewReviewRequest,
};
pub use application::source_policy::{
    remote_connector_ids_for_view, source_view_ids, trusted_view_capability,
};
pub use domain::gateway_identity::{RemoteOwnerPublicKey, RemoteViewAuthorizationExpectation};
pub use ports::authorization_signer::{
    AssistantAuthorizationSigningCommand, AuthorizationProofVerifier, AuthorizationSignature,
    AuthorizationSigner, AuthorizationSigningCommand, VerifiedAuthorizationClaims,
};
pub use ports::gateway_trust::{GatewayCredentialExpectation, GatewayTrustReader};
pub use ports::grant_repository::GrantRepository;
pub use ports::source_preview::SourcePreviewVerifier;
pub use ports::trusted_consumer_catalog::{
    TrustedConsumerCatalog, TrustedConsumerRegistration, TrustedViewCapability,
};

pub use application::product_calendar_read::ProductCalendarReadAuthority;
pub use domain::product_calendar_read::*;
pub use ports::product_source_authority::ProductSourceAuthority;

pub use domain::product_calendar_authorization::*;
