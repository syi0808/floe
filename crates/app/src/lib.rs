//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod action_facade;
#[cfg(unix)]
mod action_services;
mod api;
mod bootstrap;
#[cfg(unix)]
mod composition;
mod connection_observe;
#[cfg(unix)]
mod context_services;
mod core;
#[cfg(unix)]
mod day_services;
mod diagnostics;
mod error;
#[cfg(unix)]
mod expert_services;
mod host;
#[cfg(unix)]
mod knowledge_services;
mod local_context;
mod native_lane;
#[cfg(unix)]
mod owner_handles;
mod prompts;
#[cfg(unix)]
mod vault_services;
#[cfg(unix)]
mod vault_lifecycle;
#[cfg(unix)]
mod ready_generation;

pub use floe_context_contract::{CalendarProvider, CalendarScope, ResourceHandle, SourceAuthority};
/// Values carried by the remaining host service signatures. The S1
/// Conversation and Connections bindings call their typed owners directly.
pub use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarMirrorInput, CalendarMirrorState,
    CalendarRange, CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, Capture,
    CaptureId, CaptureProcessing, CaptureSource, DaySnapshot, DomainError, DomainRef, Event,
    EventId, EventSchedule, Note, NoteId, Priority, Revision, SourceRef, Task, TaskId,
    TimedSchedule, TimelineItem,
};
pub use floe_diagnostics::{PanicRecord, TraceContext, instrument, panic_record};

pub use floe_actions::ActionAuthorityMode;
pub use floe_context_contract::ConnectionId;
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
pub use floe_knowledge::{KnowledgeDecisionKind, MemoryOrigin, MemoryOverviewSnapshot};

#[cfg(unix)]
pub use action_services::{ActionsCommand, ActionsCommandResult, ActionsCommands, ActionsQuery, ActionsQueryResult, ActionsQueries};
pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
#[cfg(unix)]
pub use composition::{AppComposition, AppOpenError, open};
#[cfg(unix)]
pub use context_services::{
    AttentionCompletion, CalendarCompletion, NativeHostCommand, NativeHostCommands,
    NativeHostQueries, NativeHostQuery, PersonalCompletion,
};
pub use core::{Classification, FloeCore};
#[cfg(unix)]
pub use day_services::{DayCommands, DayQueries};
pub use floe_day::{DayQuery, DayMutation, DayMutationRequest, DayMutationResult};
pub use error::{CoreError, ErrorCode};
#[cfg(unix)]
pub use expert_services::{ExpertCommand, ExpertCommandResult, ExpertCommands, ExpertQuery, ExpertQueryResult, ExpertQueries};
/// The acquisition values one local-context command carries.
pub use floe_context::valid_native_subject_fingerprint;
pub use floe_provider_adapters::sources::native_acquisition::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    CalendarSourceFailure, MAX_ACQUISITION_DEADLINE_MS, NativeCalendarBatch, NativeCalendarFailure,
    NativeCalendarRecord, NativeEventSchedule, NativeSourceResource, PersonalAcquisitionMode,
    PersonalAcquisitionRequest, PersonalAcquisitionResult, PersonalDomain, attention_failure,
    personal_failure,
};
pub use host::{AppHost, HostRequest};
#[cfg(unix)]
pub use knowledge_services::{KnowledgeCommands, KnowledgeQuery, KnowledgeQueryResult, KnowledgeQueries};
pub use local_context::{
    LocalContextHost, NativeHostKind, NativeHostOutcome, NativeHostRegistrationRef,
};
pub use native_lane::{NativeHostLane, NativeHostLaneError};
#[cfg(unix)]
pub use owner_handles::{ReadyOwners, host_scope};

#[cfg(unix)]
pub use vault_services::{
    VaultLifecycleCommand, VaultLifecycleCommands, VaultLifecycleQueries, VaultLifecycleResult, VaultState,
};
