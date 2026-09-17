pub mod application {
    pub mod archive;
    pub mod day_context_views;
    pub mod routing;
    pub mod assembler;
    pub mod consumed;
    pub mod coverage;
    pub mod history;
    pub mod leases;
    pub mod personal_lineage;
    pub mod personal_sources;
    pub mod projection;
    pub mod remote_views;
    pub mod service;
    pub mod source_view;
}

pub mod ports {
    pub mod archive_reader;
    pub mod personal_source;
    pub mod evidence_reader;
    pub mod source_reader;
}

/// The consumer identity a general assistant read is made under.
pub const ASSISTANT_CONSUMER: &str = "assistant";

pub use application::archive::read_authorized_archive;
pub use application::assembler::acquire_memory_context;
pub use floe_context_contract::{
    OptionalSource, acquire_optional_source, record_source_issue,
};
pub use application::consumed::ConsumedLineage;
pub use application::coverage::{CoverageAccumulator, CoverageMessageFact, CoverageRegistry};
pub use application::history::read_history_coverage;
pub use floe_agent_contract::{HistoryMessageSize, bounded_history_start};
pub use application::leases::{
    MAX_LEASE_BYTES, MAX_LIVE_LEASES, SourceLeaseRegistry, SourceLeaseReservation,
};
pub use application::personal_lineage::{
    FeasibilityQueryLineage, attention_query_fingerprint, attention_subject_fingerprint,
    feasibility_query_fingerprint, people_query_fingerprint, wellbeing_query_fingerprint,
};
pub use application::personal_sources::{
    ATTENTION_CONNECTION, ATTENTION_CONNECTOR, ATTENTION_RESOURCE, FEASIBILITY_CONNECTION,
    FEASIBILITY_CONNECTOR, FEASIBILITY_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTION,
    ATTENTION_EXPERT_CONSUMER, WELLBEING_CONNECTOR, WELLBEING_RESOURCE, admit_attention,
    apple_execution_owner, attention_execution_owner, attention_source,
    authorize_personal_dependency, contacts_connection, contacts_execution_owner,
    feasibility_source, personal_dependency_holds, read_feasibility, read_people, read_wellbeing,
    wellbeing_source,
};
pub use ports::personal_source::{
    AcquiredSource, AttentionAcquisition, AttentionAcquisitionMode, PersonalAcquisition,
    PersonalDomain, PersonalGrantRecords, PersonalSourceDriver, TrustedObservation,
};
pub use application::projection::{CoverageProjection, project_coverage};
pub use application::remote_views::{
    LOGISTICS_VIEW, MAIL_VIEW, WORK_VIEW, is_remote_view, remote_view_connector_admissible,
    remote_view_data_category, remote_view_dependency, remote_view_resource,
    split_remote_view_resource, validate_remote_view, validate_remote_view_query,
};
pub use application::service::{ContextService, PreparedContext};
pub use application::source_view::SourceView;
pub use floe_context_contract::ContextDependency;
pub use ports::archive_reader::{
    ArchiveProjection, ArchiveReader, MAX_ARCHIVE_PROJECTION_BYTES,
    MAX_ARCHIVE_PROJECTION_MESSAGES,
};
pub use ports::evidence_reader::EvidenceReader;
pub use ports::source_reader::{SourceKey, SourceRead, SourceReadRequest, SourceReader};

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
pub use application::routing::{
    ContextRouteResult, ContextRoutingRuntime, ContextTransferClass, DeviceClass, DevicePresence,
    DeviceScope, LogicalViewRoute, LogicalViewRoutingPolicy, RouteDisagreement, RoutedAvailability,
    RoutedView, RuntimeDeviceState, RuntimeSourcePolicy, route_logical_views,
    route_logical_views_with_runtime,
};
