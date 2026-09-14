pub mod application {
    pub mod assembler;
    pub mod coverage;
    pub mod consumed;
}

pub use application::assembler::{OptionalSource, acquire_memory_context, acquire_optional_source, record_source_issue};
pub use application::coverage::{CoverageAccumulator, CoverageMessageFact, CoverageRegistry};
pub use application::consumed::ConsumedLineage;
