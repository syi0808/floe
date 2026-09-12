mod calendar;
mod capture;
mod connection_authority;
mod context_dependency;
mod data_access_grant;
mod entity;
mod id;
mod projection;

pub use calendar::{
    CalendarBatch, CalendarConnection, CalendarFailure, CalendarMirror, CalendarProvider,
    CalendarRange, CalendarRecord, CalendarScope, CalendarSelection, CalendarSource,
    CalendarSyncStatus,
};
pub use capture::{Capture, CaptureProcessing, CaptureSource, DomainRef};
pub use connection_authority::SourceAuthority;
pub use context_dependency::{
    ConsumerPolicyAuthority, ContextDependency, ContextDependencyError, CoverageAccumulator,
    DependencyCoverage, MAX_CONTEXT_DEPENDENCIES, MAX_CONTEXT_DEPENDENCY_BYTES,
    MAX_QUERY_FINGERPRINT_BYTES, ReplayRequest, ReplayTrust, admit_replay,
    validate_dependency_freshness, validate_stored_dependency,
};
pub use data_access_grant::{
    ConnectionId, ConnectorId, DataAccessGrant, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    GrantState, GrantTransitionError, GrantValidationError, MAX_CONNECTOR_ID_BYTES,
    MAX_CONSUMER_ID_BYTES, MAX_CONSUMERS, MAX_EXECUTION_OWNER_BYTES, MAX_RESOURCE_HANDLE_BYTES,
    MAX_RESOURCE_HANDLES, MAX_SCOPE_BYTES, ProcessingRestriction, ResourceHandle,
};
pub use entity::{
    AllDaySchedule, DomainError, Event, EventSchedule, Note, Priority, SourceRef, Task,
    TimedSchedule,
};
pub use id::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
pub use projection::{DaySnapshot, TimelineItem, project_day, project_day_with_end_offset};
