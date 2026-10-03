mod application {
    pub mod archive;
    pub mod assembler;
    pub mod calendar_acquisition;
    mod calendar_lease;
    pub mod candidate_catalog;
    pub mod consumed;
    pub mod coverage;
    pub mod day_context_views;
    pub mod dependency_resolver;
    pub mod expert_context;
    pub mod expert_execution;
    pub mod expert_sources;
    pub mod history;
    pub mod learner_projection;
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
    mod source_adapters;
    pub mod source_candidates;
    pub mod source_review;
    pub mod source_view;
    mod trusted_consumers;

    pub use trusted_consumers::ContextTrustedConsumerCatalog;
}

mod ports {
    pub mod archive_reader;
    pub mod calendar_product;
    pub mod calendar_source;
    pub mod evidence_reader;
    pub mod expert_execution;
    pub mod personal_source;
    pub mod source_metadata;
    pub mod source_reader;
}

pub use application::archive::read_authorized_archive;
pub use application::assembler::acquire_memory_context;
pub use application::calendar_acquisition::{ContextCalendarAcquisition, ContextCore};
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
pub use application::learner_projection::ContextLearnerProjection;
pub use application::leases::{
    MAX_LEASE_BYTES, MAX_LIVE_LEASES, SourceLeaseRegistry, SourceLeaseReservation,
};
pub use application::model_coverage::{
    TurnCoverageDecision, project_history, revalidate_turn_coverage,
};
pub use application::model_projection::{
    ContextProjectionInput, ContextProjectionRole, assemble_context_projection,
};
pub use application::native_calendar::{
    AdmittedNativeCalendarRead, CalendarConnectionReader, NativeCalendarGrantReader,
    NativeSubjectObservation, admit_current_native_calendar_read,
};
pub use application::native_calendar_view::{
    NativeCalendarViewRead, authorize_native_calendar_dependency, read_native_calendar_view,
};
pub use application::observations::{
    ALLOWED_VIEW_IDS, ObservationEntry, ObservationRegistry, TrustedPersonalObservation,
    valid_native_subject_fingerprint, validate_view,
};
pub use application::personal_lineage::{
    attention_query_fingerprint, attention_subject_fingerprint, people_query_fingerprint,
    wellbeing_query_fingerprint,
};
pub use application::personal_sources::{
    ATTENTION_CONNECTOR, ATTENTION_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTOR,
    WELLBEING_RESOURCE, admit_selected_attention_outcome, apple_execution_owner,
    authorize_personal_dependency, read_selected_people_outcome, read_selected_wellbeing_outcome,
};
pub use application::projection::{CoverageProjection, project_coverage};
pub use application::remote_sources::{
    AdmittedRemoteRead, RemoteViewTransport, authorize_remote_dependency,
    configured_remote_source_selections, read_configured_remote_view, read_selected_remote_view,
};
pub use application::remote_views::{
    LOGISTICS_VIEW, MAIL_VIEW, WORK_VIEW, is_remote_view, remote_view_data_categories,
    remote_view_dependency, validate_remote_view, validate_remote_view_query,
};
pub use application::service::{ContextService, PreparedContext};
pub(crate) use application::source_candidates::validate_personal_source_selection;
pub use application::source_candidates::{
    LOCAL_CONTEXT_CONNECTOR, SourceCandidate, SourceCandidateRequest, discover_source_candidates,
    source_candidate_id, validate_local_source_selection,
};
pub use application::source_view::SourceView;
/// The authorization input Context's own coverage entry points take.
pub use floe_access::{
    DependencyAuthorization, DependencyLiveness, DependencyResolver, RemoteCallWindow,
    RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery, SignedSourcePreview,
};
pub use floe_agent_contract::{HistoryMessageSize, bounded_history_start};
pub use floe_context_contract::ContextDependency;
pub use floe_context_contract::{OptionalSource, acquire_optional_source, record_source_issue};
pub use ports::archive_reader::{
    ArchiveProjection, ArchiveReader, MAX_ARCHIVE_PROJECTION_BYTES, MAX_ARCHIVE_PROJECTION_MESSAGES,
};
pub use ports::calendar_source::{CalendarObservation, CalendarObserveRequest, CalendarSource};
pub use ports::evidence_reader::EvidenceReader;
pub use ports::expert_execution::{
    ExpertRemoteSource, ExpertRemoteTransport, ExpertSourceTransport,
};
pub use ports::personal_source::{
    AcquiredSource, AttentionAcquisition, AttentionAcquisitionMode, PersonalAcquisition,
    PersonalConnectionReader, PersonalDomain, PersonalSourceDriver, TrustedObservation,
};
pub use ports::source_reader::{
    SelectedSourceReader, SourceKey, SourceRead, SourceReadRequest, SourceReader,
};

pub use application::routing::{
    ContextRouteResult, ContextRoutingRuntime, ContextTransferClass, DeviceClass, DevicePresence,
    DeviceScope, LogicalViewRoute, LogicalViewRoutingPolicy, RouteDisagreement, RoutedAvailability,
    RoutedView, RuntimeDeviceState, RuntimeSourcePolicy, route_logical_views,
    route_logical_views_with_runtime,
};
/// The immutable projection an authorized read produces.
///
/// Shared contracts define the view and envelope values. Context assembles
/// them against the prepared plan and current source-processing requirements.
pub use floe_agent_contract::{
    AgentContext, ContextEvidence, InferencePolicyDecision, MAX_CONTEXT_EVIDENCE,
    MAX_CONTEXT_EVIDENCE_BYTES, MAX_CONTEXT_ISSUES,
};
pub use floe_context_contract::views::*;
pub use floe_context_contract::{
    ContextMemory, MAX_CONTEXT_MEMORIES, MAX_CONTEXT_MEMORY_BYTES, MemoryContextSnapshot,
};

pub use ports::calendar_product::{
    CalendarProductPage, CalendarProductPageOutcome, CalendarProductReadResult,
    CalendarProductTransport,
};

pub use application::candidate_catalog::ContextCandidateCatalog;
pub use application::expert_execution::{
    ContextExpertProjection, ContextExpertSources, ExpertContextDependencies,
};

pub use application::dependency_resolver::ContextDependencyResolver;
pub use application::ContextTrustedConsumerCatalog;

pub use application::source_review::ContextSourceReview;
pub use ports::source_metadata::SourceMetadataTransport;
