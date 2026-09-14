pub mod application {
    pub mod assembler;
    pub mod consumed;
    pub mod coverage;
    pub mod leases;
}

pub use application::assembler::{
    OptionalSource, acquire_memory_context, acquire_optional_source, record_source_issue,
};
pub use application::consumed::ConsumedLineage;
pub use application::coverage::{CoverageAccumulator, CoverageMessageFact, CoverageRegistry};
pub use application::leases::{
    MAX_LEASE_BYTES, MAX_LIVE_LEASES, SourceLeaseRegistry, SourceLeaseReservation,
};
