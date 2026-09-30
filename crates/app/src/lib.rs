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
mod connection_observe;
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
mod first_party_observe;
mod host;
#[cfg(unix)]
mod knowledge_services;
mod local_context;
#[cfg(unix)]
mod local_operations;
mod personal_source_spec;
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

pub use floe_context_contract::{CalendarProvider, CalendarScope, ResourceHandle, SourceAuthority};
/// The values this host's own API names at its boundary.
///
/// A binding reads a receipt and reports a failure; it does not reach past the
/// app into the modules behind it, so only the values these signatures carry
/// are named here — never a module, and never a concrete adapter.
pub use floe_conversation::{RunReceipt, RunState};
pub use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarMirrorInput, CalendarMirrorState,
    CalendarRange, CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, Capture,
    CaptureId, CaptureProcessing, CaptureSource, DaySnapshot, DomainError, DomainRef, Event,
    EventId, EventSchedule, Note, NoteId, Priority, Revision, SourceRef, Task, TaskId,
    TimedSchedule, TimelineItem,
};
pub use floe_diagnostics::{PanicRecord, TraceContext, instrument, panic_record};

/// The owner values one worker command carries, and one worker result reports.
///
/// The binding reads and writes these against its wire; it never reaches into
/// the owner that decided them.
pub use floe_access::{
    DataAccessGrant, GrantAuthority, GrantId, GrantState, ProcessingRestriction,
    RemoteEnrollmentStatus, RemoteOwnerPublicKey, RemoteProducerIdentity,
};
pub use floe_actions::{ActionAuthorityMode, CalendarAction, CalendarActionState};
pub use floe_agent_contract::UserInteractionKind;
pub use floe_connections::{
    CalendarConnectionRef, ConnectionId, ConnectionResource, ConnectorId, PairingIssuer,
    PairingStatus, ResourceMode, SourceConnection, SourceState,
};
pub use floe_context_contract::{
    DataClass, GrantConsumer, ProcessingSourceScope, RecipientLineage,
};
pub use floe_conversation::AgentOutcome;
pub use floe_conversation::{
    ConversationInteraction, InlineObserveTarget, InteractionOrigin, InteractionRequirement,
    InteractionRequirementKind, InteractionState, NavigationDestination, NavigationOnlyTarget,
    RecipientConsentTarget, ReviewedTarget,
};
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
pub use floe_knowledge::{KnowledgeDecisionKind, MemoryOrigin, MemoryOverviewSnapshot};

pub use action_facade::CalendarActionCommand;
#[cfg(unix)]
pub use action_services::{ActionCommands, ActionInspection, ActionOperationResult, ActionQueries};
pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
#[cfg(unix)]
pub use composition::{AppComposition, AppOpenError, open};
#[cfg(unix)]
pub use connection_observe::{ConnectionObserveCommands, ConnectionObserveResult};
pub use connection_observe::{
    ConnectionObserveExpectation, ConnectionObserveMember, ConnectionObserveOperation,
    ConnectionObserveOverview, ConnectionObserveReviewedMember, ConnectionObserveStatus,
};
#[cfg(unix)]
pub use connection_services::{
    ConnectionsQueries, ConnectionsResult, NativeCalendarSourceCommands,
    NativeCalendarSourceMutation, NativePersonalSourceCommands, NativePersonalSourceSetup,
    RemoteCalendarSourceCommands, RemoteCalendarSourceMutation,
};
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
    NativeCalendarRecord, NativeEventSchedule, PersonalAcquisitionRequest,
    PersonalAcquisitionResult, PersonalDomain, attention_failure, personal_failure,
};
pub use host::{AppHost, HostRequest};
#[cfg(unix)]
pub use knowledge_services::{
    KnowledgeCommands, KnowledgeInspection, KnowledgeOperationResult, KnowledgeQueries,
};
pub use local_context::{
    CalendarObservationPublication, LocalContextCommand, LocalContextHost, LocalContextOutcome,
};
pub use remote_services::{
    PairingTarget, RemoteAccessCommand, RemoteAccessCommands, RemoteAccessResult,
    RemotePairingCommand, RemotePairingCommands, RemotePairingResult,
};
pub(crate) use remote_services::{
    RemoteConnectionObserveExpectation, RemoteObserveMemberExpectation,
};
pub use services::CalendarActionsResult;
pub use services::{
    CancelRun, CancelRunOutcome, CancelRunReceipt, CommandReceipt, ContinuationRef,
    ConversationCommands, ConversationEvent, ConversationEvents, ConversationQueries, EventPayload,
    EventRead, InteractionDecision, MAX_SESSION_INTERACTIONS, ProfileSelection, ReadConversation,
    ReadConversationEvents, RefreshInteraction, RefreshInteractionOutcome,
    RefreshInteractionResult, ResolveInteraction, ResolveInteractionOutcome,
    ResolveInteractionResult, ResumeInteraction, RunEventRecord, ServiceError, StartTurn, TurnMode,
};
#[cfg(unix)]
pub use session_services::{
    ConversationSessionCommand, ConversationSessionCommands, ConversationSessionQueries,
    ConversationSessionResult,
};
pub use turn_request::{ConversationResumeRequest, ConversationTurnRequest};
#[cfg(unix)]
pub use vault_services::{
    VaultLifecycleCommand, VaultLifecycleCommands, VaultLifecycleQueries, VaultLifecycleResult,
};
#[cfg(test)]
pub(crate) use worker::WorkerOperation;
pub use worker::{
    CalendarActionOperation, CalendarActionProposal, CalendarProposalInspection,
    ConversationSessionOperation, MemoryReviewDecision, MemoryReviewResult, RemotePairingChallenge,
    VaultState,
};
pub(crate) use worker::{WorkerAction, WorkerResult};
