//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod action_facade;
mod agent_fixture;
pub mod agent_run;
mod api;
mod bootstrap;
mod calendar_facade;
#[cfg(unix)]
mod composition;
mod core;
mod diagnostics;
mod error;
pub mod events;
mod host;
mod inference_routes;
pub mod local_context;
mod prompts;
mod services;
pub mod turn_request;
pub mod worker;
#[cfg(unix)]
mod vault_host;

/// The values this host's own API names at its boundary.
///
/// A binding reads a receipt and reports a failure; it does not reach past the
/// app into the modules behind it, so only the values these signatures carry
/// are named here — never a module, and never a concrete adapter.
pub use floe_conversation::{RunReceipt, RunState};
pub use floe_context_contract::{CalendarProvider, CalendarScope, SourceAuthority};
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
pub use floe_inference::{RemoteRoute, RoutePairing};
pub use floe_knowledge::{KnowledgeDecisionKind, MemoryOrigin, MemoryOverviewSnapshot};
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};

pub use action_facade::CalendarActionCommand;
pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
pub use bootstrap::local_identity_for_database;
#[cfg(unix)]
pub use composition::{AppComposition, AppOpenError, open};
pub use agent_run::{
    AgentFixtureRunCommand, AgentFixtureRunRequest, AgentFixtureRunSnapshot,
};
pub use agent_fixture::{
    AgentFixturePrompt, AgentFixtureResult, AgentFixtureTurn, recover_agent_sample,
    run_persisted_agent_sample,
};
pub use core::{Classification, FloeCore};
pub use services::CalendarActionsResult;
pub use turn_request::{ConversationTurnRequest, RemoteTurnRoute};
pub use worker::{
    CalendarActionOperation, CalendarActionProposal, CalendarProposalInspection,
    CalendarSubjectPreview, CalendarSubjectRequest, ConversationSessionOperation,
    FixtureOperation, MemoryReviewDecision, MemoryReviewOverview, RemoteCalendarGrantPreview,
    RemoteGrantOverview, RemotePairingChallenge, VaultState, WorkerAction, WorkerOperation,
    WorkerResult,
};
pub use error::{CoreError, ErrorCode};
pub use host::{AppHost, HostRequest};
pub use inference_routes::HostInferenceRoutes;
pub use local_context::{
    CalendarObservationPublication, LocalContextCommand, LocalContextHost, LocalContextOutcome,
};
/// The acquisition values one local-context command carries.
pub use floe_context::{PublishedCalendarObservation, valid_native_subject_fingerprint};
pub use floe_provider_adapters::sources::native_acquisition::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    CalendarSourceFailure, MAX_ACQUISITION_DEADLINE_MS, NativeCalendarBatch,
    NativeCalendarFailure, NativeCalendarRecord, NativeEventSchedule, PersonalAcquisitionRequest,
    PersonalAcquisitionResult, PersonalDomain, attention_failure, personal_failure,
};
pub use services::{
    CancelRun, CancelRunOutcome, CancelRunReceipt, CommandReceipt, ContinuationRef,
    ConversationCommands, ProfileSelection, ServiceError, StartTurn, TurnMode,
};
#[cfg(unix)]
pub use vault_host::{ConversationQuery, VaultBridge, VaultRequestFailure};
