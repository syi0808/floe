//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod action_facade;
mod api;
mod bootstrap;
#[cfg(unix)]
mod composition;
mod connection_observe;
#[cfg(unix)]
mod context_services;
mod core;
mod diagnostics;
mod error;
mod host;
mod local_context;
mod native_lane;
#[cfg(unix)]
mod owner_handles;
mod prompts;
#[cfg(unix)]
mod ready_generation;
#[cfg(unix)]
mod router;
#[cfg(unix)]
mod runtime_control;
#[cfg(unix)]
mod runtime_preparation;
#[cfg(unix)]
mod storage_profile;
#[cfg(unix)]
pub use storage_profile::storage_profile_code;

pub use floe_context_contract::{CalendarProvider, CalendarScope, ResourceHandle, SourceAuthority};
/// Values carried by the host and typed product-router APIs.
pub use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarMirrorState, CalendarRange,
    CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, Capture, CaptureId,
    CaptureProcessing, CaptureSource, DaySnapshot, DomainError, DomainRef, Event, EventId,
    EventSchedule, Note, NoteId, Priority, Revision, SourceRef, Task, TaskId, TimedSchedule,
    TimelineItem,
};
pub use floe_diagnostics::{PanicRecord, TraceContext, instrument, panic_record};

pub use floe_context_contract::ConnectionId;
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
pub use floe_knowledge::{KnowledgeDecisionKind, MemoryOrigin, MemoryOverviewSnapshot};

#[cfg(unix)]
pub use api::{
    ActionAuthorityMode, ActionDecisionKind, ActionDestinationChoice, ActionIntent,
    ActionProposalPreview, ActionReviewRef, ActionSnapshot, ActionsAuthority, ActionsCommand,
    ActionsCommandResult, ActionsPage, ActionsQuery, ActionsQueryResult, ConnectionsCommand,
    ConnectionsCommandOutcome, ConnectionsQuery, ConnectionsQueryOutcome, ConversationCommand,
    ConversationCommandOutcome, ConversationQuery, ConversationQueryOutcome, DayCommand,
    DayCommandOutcome, DayProductQuery, DayQueryOutcome, ExpertCommand, ExpertCommandResult,
    ExpertQuery, ExpertQueryResult, MemoryCommand, MemoryQuery, MemoryQueryResult, ProductCommand,
    ProductCommandDisposition, ProductCommandFailure, ProductCommandOutcome, ProductCommandRequest,
    ProductFailure, ProductObservation, ProductObservationOutcome, ProductQuery,
    ProductQueryOutcome,
};
#[cfg(unix)]
pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
#[cfg(unix)]
pub use composition::{
    AppComposition, AppOpenError, AppOpenOptions, AppStorageFailure, ModelProviderFactory, open,
    open_default, open_default_with_options,
};
#[cfg(unix)]
pub use context_services::{
    AttentionCompletion, CalendarCompletion, NativeHostCommand, NativeHostCommands,
    NativeHostQueries, NativeHostQuery, PersonalCompletion,
};
pub use core::{Classification, FloeCore};
pub use error::{CoreError, ErrorCode};
/// The acquisition values one local-context command carries.
pub use floe_context::valid_native_subject_fingerprint;
pub use floe_day::{DayMutation, DayMutationRequest, DayMutationResult, DayQuery};
pub use floe_provider_adapters::sources::native_acquisition::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    CalendarSourceFailure, MAX_ACQUISITION_DEADLINE_MS, NativeCalendarBatch, NativeCalendarFailure,
    NativeCalendarRecord, NativeEventSchedule, NativeResourceGroup, NativeSourceResource,
    PersonalAcquisitionMode, PersonalAcquisitionRequest, PersonalAcquisitionResult, PersonalDomain,
    attention_failure, personal_failure,
};
pub use host::{AppHost, HostRequest};
pub use local_context::{
    LocalContextHost, NativeHostKind, NativeHostOutcome, NativeHostRegistrationRef,
};
pub use native_lane::{NativeHostLane, NativeHostLaneError};
#[cfg(unix)]
pub use owner_handles::{ReadyOwners, host_scope};

#[cfg(unix)]
pub use runtime_control::{
    RuntimeFailureProjection, RuntimePreparationCommandFailure, RuntimePreparationResult,
    RuntimeReadiness, RuntimeReadinessState, RuntimeRecovery,
};
