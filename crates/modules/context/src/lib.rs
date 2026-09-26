mod application {
    pub mod archive;
    pub mod assembler;
    pub mod calendar_connector;
    pub mod calendar_timeline;
    pub mod consumed;
    pub mod coverage;
    pub mod day_context_views;
    pub mod expert_context;
    pub mod expert_sources;
    pub mod history;
    pub mod leases;
    pub mod model_coverage;
    pub mod model_projection;
    pub mod native_calendar;
    pub mod native_calendar_view;
    pub mod observations;
    pub mod personal_lineage;
    pub mod personal_sources;
    pub mod projection;
    pub mod remote_sources;
    pub mod remote_views;
    pub mod routing;
    pub mod service;
    pub mod source_candidates;
    pub mod source_view;
    pub mod tools;
}

mod ports {
    pub mod archive_reader;
    pub mod calendar_source;
    pub mod evidence_reader;
    pub mod personal_source;
    pub mod source_reader;
}

/// The consumer identity a general assistant read is made under.
pub const ASSISTANT_CONSUMER: &str = "assistant";

pub use application::archive::read_authorized_archive;
pub use application::assembler::acquire_memory_context;
pub use application::calendar_connector::{
    ConnectorProjectionError, project_calendar_connector, validate_connector_device,
};
pub use application::calendar_timeline::{CalendarTimelineViews, GovernedDependencyResolver};
pub use application::consumed::ConsumedLineage;
pub use application::coverage::{
    CoverageAccumulator, CoverageMessageFact, CoverageRegistry, message_coverage,
};
pub use application::day_context_views::{note_context_view, task_context_view};
pub use application::expert_context::{ExpertContextRequest, prepare_expert_context};
pub use application::expert_sources::{
    CalendarReviewClassification, DeclaredSourceRequirement, DeclaredSourceValue,
    LocalExpertSource, LocalExpertSourceDriver, classify_calendar_review,
    current_calendar_connector, observe_calendar_binding, read_declared_source,
};
pub use application::history::read_history_coverage;
pub use application::leases::{
    MAX_LEASE_BYTES, MAX_LIVE_LEASES, SourceLeaseRegistry, SourceLeaseReservation,
};
pub use application::model_coverage::{
    TurnCoverageDecision, project_history, revalidate_turn_coverage,
};
pub use application::model_projection::{
    ContextProjectionInput, ContextProjectionRole, assemble_context_projection, context_manifest,
};
pub use application::native_calendar::{
    AdmittedNativeCalendarRead, AdmittedNativeCalendarSource, CalendarConnectionReader,
    NativeCalendarGrantReader, NativeCalendarSourceRequest, NativeCalendarSubjectSource,
    NativeSubjectObservation, NativeSubjectRequest, admit_current_native_calendar_read,
    admit_native_calendar_source, preview_native_calendar_subject,
};
pub use application::native_calendar_view::{
    NativeCalendarViewRead, authorize_native_calendar_dependency, read_native_calendar_view,
};
pub use application::observations::{
    ALLOWED_VIEW_IDS, ObservationEntry, ObservationRegistry, PublishedCalendarObservation,
    TrustedPersonalObservation, valid_native_subject_fingerprint, validate_calendar_observation,
    validate_view,
};
pub use application::personal_lineage::{
    FeasibilityQueryLineage, attention_query_fingerprint, attention_subject_fingerprint,
    feasibility_query_fingerprint, people_query_fingerprint, wellbeing_query_fingerprint,
};
pub use application::personal_sources::{
    ATTENTION_CONNECTION, ATTENTION_CONNECTOR, ATTENTION_RESOURCE, FEASIBILITY_CONNECTION,
    FEASIBILITY_CONNECTOR, FEASIBILITY_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTION,
    WELLBEING_CONNECTOR, WELLBEING_RESOURCE, admit_attention, admit_attention_outcome,
    apple_execution_owner, attention_execution_owner, attention_source,
    authorize_personal_dependency, contacts_connection, contacts_execution_owner,
    feasibility_source, personal_dependency_holds, read_feasibility, read_feasibility_outcome,
    read_manager_people, read_manager_people_outcome, read_people, read_selected_people_outcome,
    read_wellbeing, read_wellbeing_outcome, wellbeing_source,
};
pub use application::projection::{CoverageProjection, project_coverage};
pub use application::remote_sources::{
    AdmittedRemoteRead, RemoteCalendarViewRead, RemoteViewTransport, authorize_remote_dependency,
    read_remote_calendar_view, read_remote_view, read_selected_remote_view,
};
pub use application::remote_views::{
    LOGISTICS_VIEW, MAIL_VIEW, WORK_VIEW, is_remote_view, remote_view_data_category,
    remote_view_dependency, remote_view_resource, split_remote_view_resource, validate_remote_view,
    validate_remote_view_query,
};
pub use application::service::{ContextService, PreparedContext};
pub use application::source_candidates::{
    LOCAL_CONTEXT_CONNECTOR, SourceCandidate, SourceCandidateRequest, discover_source_candidates,
    source_candidate_id, validate_local_source_selection,
};
pub use application::source_view::SourceView;
pub use application::tools::{
    ATTENTION_COARSE_READ, ContextToolService, LIFE_LOGISTICS_READ, MAIL_COMMUNICATION_READ,
    MANAGER_TOOL_DEFINITION_REVISION, PEOPLE_IDENTITY_READ, SCHEDULE_FEASIBILITY_READ,
    WELLBEING_DERIVED_READ, WORK_CONTEXT_READ, manager_direct_native_connector,
    manager_direct_remote_view, manager_tool_descriptors,
};
/// The authorization input Context's own coverage entry points take.
pub use floe_access::{
    DependencyAuthorization, DependencyLiveness, DependencyResolver, RemoteCallWindow,
    RemoteGrantBinding, RemoteGrantStore, RemoteGrantTransport, RemotePairingIdentity,
    RemoteSourceQuery, SignedSourcePreview,
};
pub use floe_agent_contract::{HistoryMessageSize, bounded_history_start};
pub use floe_context_contract::ContextDependency;
pub use floe_context_contract::{OptionalSource, acquire_optional_source, record_source_issue};
pub use ports::archive_reader::{
    ArchiveProjection, ArchiveReader, MAX_ARCHIVE_PROJECTION_BYTES, MAX_ARCHIVE_PROJECTION_MESSAGES,
};
pub use ports::calendar_source::{
    CalendarMirrorReader, CalendarObservation, CalendarObserveRequest, CalendarSource,
    ProjectedCalendarItem, ProjectedCalendarObservation,
};
pub use ports::evidence_reader::EvidenceReader;
pub use ports::personal_source::{
    AcquiredSource, AttentionAcquisition, AttentionAcquisitionMode, PersonalAcquisition,
    PersonalDomain, PersonalGrantRecords, PersonalSourceDriver, TrustedObservation,
};
pub use ports::source_reader::{SourceKey, SourceRead, SourceReadRequest, SourceReader};

pub use application::routing::{
    ContextRouteResult, ContextRoutingRuntime, ContextTransferClass, DeviceClass, DevicePresence,
    DeviceScope, LogicalViewRoute, LogicalViewRoutingPolicy, RouteDisagreement, RoutedAvailability,
    RoutedView, RuntimeDeviceState, RuntimeSourcePolicy, route_logical_views,
    route_logical_views_with_runtime,
};
/// The immutable projection an authorized read produces.
///
/// The view shapes, the evidence they yield and the memories that reach a
/// context are the context contract's; what one turn may see and which
/// placement may see it is the agent contract's. This module composes them —
/// it does not define them, and an Expert names them from the contract.
pub use floe_agent_contract::{
    AgentContext, ContextEvidence, InferencePolicyDecision, MAX_CONTEXT_EVIDENCE,
    MAX_CONTEXT_EVIDENCE_BYTES, MAX_CONTEXT_ISSUES,
};
pub use floe_context_contract::views::*;
pub use floe_context_contract::{
    ContextMemory, MAX_CONTEXT_MEMORIES, MAX_CONTEXT_MEMORY_BYTES, MemoryContextSnapshot,
};
