pub mod authority;
pub mod calendar_read;
pub mod dependency;
pub mod grants;
pub mod model_dispatch;
pub mod native_calendar;
pub mod personal_read;
pub mod personal_sources;
pub mod release;
pub mod remote_authority;
pub mod remote_grants;
pub mod remote_view;

pub use authority::{
    ReadAuthorityEvidence, ReadAuthorityIdentity, validate_read_authority, validate_read_continuity,
};
pub use dependency::validate_grant_dependency;
pub use grants::{
    AccessGrantMutation, GrantPolicyError, apply_grant_mutation, authorize_grant, create_grant,
    validate_grant_expectation,
};
pub use native_calendar::{
    NativeCalendarConnection, NativeCalendarReview, admit_native_calendar_setup,
    admit_native_calendar_subject, is_local_calendar_provider, local_calendar_connection_id,
    local_calendar_connection_id_for_connector, local_calendar_connector,
    local_calendar_execution_owner, local_calendar_execution_owner_for_connector,
    local_calendar_provider, native_calendar_connection_unchanged,
    native_calendar_source_current, reviewed_native_subject,
};
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub use native_calendar::fixture_calendar_execution_owner;
pub use personal_read::{
    PersonalReadRequirement, active_read_grant, attention_consumer, grant_unchanged,
    subject_unchanged, valid_subject_fingerprint,
};
pub use personal_sources::{
    ATTENTION_CONNECTOR, ATTENTION_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTOR,
    WELLBEING_RESOURCE, apple_execution_owner, is_device_local_source,
};
pub use release::{ReleasePermit, ReleaseRecipient, admit_release, consume_release};
pub use remote_authority::{admit_device_pairing, admit_enrollment_pairing};
pub use remote_grants::{
    RemoteViewGrantPreview, RemoteViewGrantRequest, preview_remote_view_grant,
};
pub use remote_view::{
    RemoteProducerIdentity, RemoteViewSourceReference, admit_remote_view_binding,
    admit_remote_view_source, producer_is_pinned, remote_dependency_live,
    remote_dependency_resource, remote_dependency_source_admits, remote_view_source,
    source_matches_producer,
};
pub mod authorization_signing;
pub mod connection_review;
pub mod source_policy;

pub mod product_calendar_read;
