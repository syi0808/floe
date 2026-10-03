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
mod first_party_observe;
mod host;
#[cfg(unix)]
mod knowledge_services;
mod local_context;
#[cfg(unix)]
mod local_operations;
mod native_lane;
#[cfg(unix)]
mod owner_handles;
mod prompts;
mod services;
#[cfg(unix)]
mod vault_host;
#[cfg(unix)]
mod vault_services;
mod worker;

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

pub use floe_actions::{ActionAuthorityMode, CalendarAction, CalendarActionState};
pub use floe_context_contract::ConnectionId;
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
pub use floe_knowledge::{KnowledgeDecisionKind, MemoryOrigin, MemoryOverviewSnapshot};

pub use action_facade::CalendarActionCommand;
#[cfg(unix)]
pub use action_services::{ActionCommands, ActionInspection, ActionOperationResult, ActionQueries};
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
pub use day_services::{
    DayCommands, DayMutation, DayMutationRequest, DayMutationResult, DayQueries, DayRead,
};
pub use error::{CoreError, ErrorCode};
#[cfg(unix)]
pub use expert_services::{
    ExpertBindingSelectionIntent, ExpertCandidateCatalog, ExpertCommand, ExpertCommands,
    ExpertInspection, ExpertOperationResult, ExpertQueries, ExpertSourceCandidateView,
    RegistryConfiguration, RegistryConfigurationTarget, RegistryOverview,
};
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
pub use knowledge_services::{
    KnowledgeCommands, KnowledgeInspection, KnowledgeOperationResult, KnowledgeQueries,
};
pub use local_context::{
    LocalContextHost, NativeHostKind, NativeHostOutcome, NativeHostRegistrationRef,
};
pub use native_lane::{NativeHostLane, NativeHostLaneError};
#[cfg(unix)]
pub use owner_handles::{ReadyOwners, host_scope};
pub use services::CalendarActionsResult;
pub use services::ServiceError;

#[cfg(unix)]
pub use vault_services::{
    VaultLifecycleCommand, VaultLifecycleCommands, VaultLifecycleQueries, VaultLifecycleResult,
};
pub use worker::{
    CalendarActionOperation, CalendarActionProposal, CalendarProposalInspection,
    MemoryReviewDecision, MemoryReviewResult, VaultState,
};
pub(crate) use worker::{WorkerAction, WorkerResult};
