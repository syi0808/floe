pub mod application;
mod data_access_grant;
pub mod ports;

pub use application::{
    ATTENTION_ASSISTANT_CONSUMER, ATTENTION_EXPERT_CONSUMER, ContactsAccessChange,
    ContactsAccessConfiguration, PersonalAccessChange, PersonalAccessConfiguration,
    PersonalAccessOverview, PersonalAccessState, attention_consumer, matches_source,
    source_and_scope,
};
pub use application::personal_grants::{
    apply as apply_personal_access, apply_contacts,
    validate_request as validate_personal_access_request,
};
pub use application::{
    ATTENTION_CONNECTION, ATTENTION_CONNECTOR, ATTENTION_RESOURCE, FEASIBILITY_CONNECTION,
    FEASIBILITY_CONNECTOR, FEASIBILITY_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTION,
    WELLBEING_CONNECTOR, WELLBEING_RESOURCE, apple_execution_owner, attention_execution_owner,
    attention_source, contacts_connection, contacts_execution_owner, contacts_source,
    feasibility_source, source_binding, wellbeing_source,
};
pub use application::{
    NativeCalendarConnection, NativeCalendarReview, admit_native_calendar_setup,
    admit_native_calendar_subject, is_native_calendar, native_calendar_connection_unchanged,
    reviewed_native_subject,
};
pub use application::{
    RemoteAuthorityEnrollment, RemoteAuthorityInspection, admit_enrollment_pairing,
    inspect_remote_authority, remote_enrollment_status, review_and_enroll_remote_authority,
};
pub use application::{
    REMOTE_CALENDAR_CONSUMER, REMOTE_CALENDAR_RECIPIENT, RemoteCalendarConnection,
    RemoteCalendarGrantPreview, RemoteCalendarGrantRequest, RemoteCalendarSourceReference,
    admits_remote_calendar_connection, hosted_calendar_connector, pause_remote_calendar_grant,
    preview_remote_calendar_grant, remote_calendar_grant, remote_calendar_scope,
    remote_calendar_source, review_and_activate_remote_calendar_grant,
};
pub use application::{
    AccessGrantMutation, FeasibilityGrantQuery, GrantPolicyError, PersonalReadRequirement, ReadAuthorityEvidence,
    ReadAuthorityIdentity, ReleasePermit, ReleaseRecipient, RemoteProducerIdentity,
    RemoteViewApproval, RemoteViewGrantExpectation, RemoteViewGrantPreview,
    RemoteViewGrantRequest, RemoteViewGrantReview, RemoteViewSourceReference, ReplayRequest,
    ReplayTrust, active_read_grant, active_resource_grant, admit_release, admit_remote_view_binding,
    admit_remote_view_source, admit_replay, apply_grant_mutation, authorize_grant, consume_release,
    create_grant, grant_unchanged, matches_review, producer_is_pinned,
    remote_dependency_binding_matches, remote_dependency_live, remote_dependency_resource,
    remote_dependency_source_admits, remote_view_scope, remote_view_source,
    preview_remote_view_grant, review_and_activate_remote_view_grant,
    review_remote_view_grant, source_matches_producer, subject_unchanged,
    valid_subject_fingerprint, validate_grant_dependency, validate_read_authority,
    validate_read_continuity,
};
pub use data_access_grant::{DataAccessGrant, GrantState, GrantTransitionError};
pub use floe_context_contract::{
    ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency, ContextDependencyError,
    DependencyCoverage, ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory,
    GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, GrantValidationError,
    MAX_CONNECTOR_ID_BYTES, MAX_CONSUMER_ID_BYTES, MAX_CONSUMERS, MAX_CONTEXT_DEPENDENCIES,
    MAX_CONTEXT_DEPENDENCY_BYTES, MAX_EXECUTION_OWNER_BYTES, MAX_RESOURCE_HANDLE_BYTES,
    MAX_RESOURCE_HANDLES, MAX_SCOPE_BYTES, ProcessingRestriction, ResourceHandle, SourceAuthority,
};
pub use floe_kernel::PersonId;
pub use ports::CurrentAuthority;
pub use ports::dependency_authorization::{
    DependencyAuthorization, DependencyLiveness, DependencyResolver,
};
pub use ports::personal_grants::{
    PersonalGrantStore, PersonalSubjectEvidence, PersonalSubjectInspector, PersonalSubjectProbe,
};
pub use ports::remote_grants::{
    BoxFuture, RemoteCalendarQuery, RemoteCallWindow, RemoteGrantBinding, RemoteGrantStore,
    RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery, SignedCalendarPreview,
    SignedSourcePreview,
};
pub use ports::remote_authorization::{
    RemoteAuthorityStore, RemoteAuthorityTransport, RemoteAuthorizationKeys, RemoteCalendarAuthorizationExpectation, RemoteEnrollmentSignature,
    RemoteEnrollmentStatus, RemoteOwnerPublicKey, RemotePairingChallenge,
};

pub use application::calendar_lease::{CalendarLeaseKey, calendar_lease_dependency};
pub use application::calendar_read::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, CalendarReadAdmission,
    admission_matches, admission_matches_dependency,
};
pub use floe_context_contract::{CalendarProvider, CalendarReadAccessStamp, CalendarScope};
