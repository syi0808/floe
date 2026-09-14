pub mod application {
    pub mod assembler;
    pub mod consumed;
    pub mod coverage;
    pub mod history;
    pub mod leases;
    pub mod projection;
    pub mod service;
    pub mod source_view;
}

pub mod ports {
    pub mod evidence_reader;
    pub mod source_reader;
}

pub use application::assembler::{
    OptionalSource, acquire_memory_context, acquire_optional_source, record_source_issue,
};
pub use application::consumed::ConsumedLineage;
pub use application::coverage::{CoverageAccumulator, CoverageMessageFact, CoverageRegistry};
pub use application::history::{HistoryMessageSize, bounded_history_start, read_history_coverage};
pub use application::leases::{
    MAX_LEASE_BYTES, MAX_LIVE_LEASES, SourceLeaseRegistry, SourceLeaseReservation,
};
pub use application::projection::{CoverageProjection, project_coverage};
pub use application::service::{ContextService, PreparedContext};
pub use application::source_view::SourceView;
pub use ports::evidence_reader::EvidenceReader;
pub use ports::source_reader::{SourceKey, SourceRead, SourceReadRequest, SourceReader};
