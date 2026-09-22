//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod action_facade;
#[cfg(unix)]
mod action_services;
mod api;
mod bootstrap;
mod calendar_facade;
#[cfg(unix)]
mod composition;
#[cfg(unix)]
mod connection_services;
#[cfg(unix)]
mod context_services;
mod core;
#[cfg(unix)]
mod day_services;
mod diagnostics;
mod error;
mod events;
#[cfg(unix)]
mod expert_services;
mod host;
#[cfg(unix)]
mod knowledge_services;
#[cfg(unix)]
mod local_access_services;
mod local_context;
#[cfg(unix)]
mod local_operations;
mod prompts;
mod remote_services;
mod services;
#[cfg(unix)]
mod session_services;
mod turn_request;
#[cfg(unix)]
mod vault_host;
#[cfg(unix)]
mod vault_services;
mod worker;

pub use floe_context_contract::{CalendarProvider, CalendarScope, SourceAuthority};
/// The values this host's own API names at its boundary.
///
/// A binding reads a receipt and reports a failure; it does not reach past the
/// app into the modules behind it, so only the values these signatures carry
/// are named here — never a module, and never a concrete adapter.
pub use floe_conversation::{RunReceipt, RunState};
pub use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarConnection, CalendarFailure, CalendarRange,
    CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, Capture, CaptureId,
    CaptureProcessing, CaptureSource, DaySnapshot, DomainError, DomainRef, Event, EventId,
    EventSchedule, Note, NoteId, Priority, Revision, SourceRef, Task, TaskId, TimedSchedule,
    TimelineItem,
};
pub use floe_diagnostics::{PanicRecord, TraceContext, instrument, panic_record};

/// The owner values one worker command carries, and one worker result reports.
///
/// The binding reads and writes these against its wire; it never reaches into
/// the owner that decided them.
pub use floe_access::{
    ContactsAccessChange, ContactsAccessConfiguration, DataAccessGrant, FeasibilityGrantQuery,
    GrantAuthority, GrantId, GrantState, PersonalAccessChange, PersonalAccessConfiguration,
    PersonalAccessOverview, PersonalAccessState, ProcessingRestriction, RemoteEnrollmentStatus,
    RemoteOwnerPublicKey, RemoteProducerIdentity, RemoteViewGrantPreview,
};
pub use floe_actions::{ActionAuthorityMode, CalendarAction, CalendarActionState};
pub use floe_connections::{CalendarConnectionRef, PairingIssuer, PairingStatus};
pub use floe_conversation::AgentOutcome;
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
pub use floe_knowledge::{KnowledgeDecisionKind, MemoryOrigin, MemoryOverviewSnapshot};

pub use action_facade::CalendarActionCommand;
#[cfg(unix)]
pub use action_services::{ActionCommands, ActionInspection, ActionOperationResult, ActionQueries};
pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
#[cfg(unix)]
pub use composition::{AppComposition, AppOpenError, open};
#[cfg(unix)]
pub use connection_services::{ConnectionsQueries, ConnectionsResult};
#[cfg(unix)]
pub use context_services::{
    AttentionCompletion, CalendarCompletion, ContextCommand, ContextQuery, LocalContextCommands,
    LocalContextQueries, PersonalCompletion,
};
pub use core::{Classification, FloeCore};
#[cfg(unix)]
pub use day_services::{
    DayCommands, DayMutation, DayMutationRequest, DayMutationResult, DayQueries, DayRead,
};
pub use error::{CoreError, ErrorCode};
#[cfg(unix)]
pub use expert_services::{
    CalendarExpertInstall, CalendarExpertOverview, ExpertCommand, ExpertCommands, ExpertInspection,
    ExpertOperationResult, ExpertQueries, RegistryConfiguration, RegistryConfigurationTarget,
    RegistryOverview,
};
/// The acquisition values one local-context command carries.
pub use floe_context::valid_native_subject_fingerprint;
pub use floe_provider_adapters::sources::native_acquisition::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    CalendarSourceFailure, MAX_ACQUISITION_DEADLINE_MS, NativeCalendarBatch, NativeCalendarFailure,
    NativeCalendarRecord, NativeEventSchedule, PersonalAcquisitionRequest,
    PersonalAcquisitionResult, PersonalDomain, attention_failure, personal_failure,
};
pub use host::{AppHost, HostRequest};
#[cfg(unix)]
pub use knowledge_services::{
    KnowledgeCommands, KnowledgeInspection, KnowledgeOperationResult, KnowledgeQueries,
};
#[cfg(unix)]
pub use local_access_services::{
    CalendarGrantChange, CalendarGrantConfiguration, CalendarSubjectIntent, LocalAccessCommand,
    LocalAccessCommands, LocalAccessInspection, LocalAccessQueries, LocalAccessResult,
};
pub use local_context::{
    CalendarObservationPublication, LocalContextCommand, LocalContextHost, LocalContextOutcome,
};
pub use remote_services::{
    PairingTarget, RemoteAccessCommand, RemoteAccessCommands, RemoteAccessResult,
    RemotePairingCommand, RemotePairingCommands, RemotePairingResult,
};
pub use services::CalendarActionsResult;
pub use services::{
    CancelRun, CancelRunOutcome, CancelRunReceipt, CommandReceipt, ContinuationRef,
    ConversationCommands, ConversationEvent, ConversationEvents, ConversationQueries, EventPayload,
    EventRead, ProfileSelection, ReadConversation, ReadConversationEvents, RunEventRecord,
    ServiceError, StartTurn, TurnMode,
};
#[cfg(unix)]
pub use session_services::{
    ConversationSessionCommand, ConversationSessionCommands, ConversationSessionQueries,
    ConversationSessionResult,
};
pub use turn_request::ConversationTurnRequest;
#[cfg(unix)]
pub use vault_host::{VaultBridge, VaultRequestFailure};
#[cfg(unix)]
pub use vault_services::{
    VaultLifecycleCommand, VaultLifecycleCommands, VaultLifecycleQueries, VaultLifecycleResult,
};
pub use worker::{
    CalendarActionOperation, CalendarActionProposal, CalendarProposalInspection,
    CalendarSubjectPreview, CalendarSubjectRequest, ConversationSessionOperation,
    MemoryReviewDecision, MemoryReviewResult, RemoteCalendarGrantPreview, RemoteGrantOverview,
    RemotePairingChallenge, VaultState, WorkerAction, WorkerOperation, WorkerResult,
};
