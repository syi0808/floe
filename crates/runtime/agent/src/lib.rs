//! Role-neutral execution loop. Session and task ownership stay in callers.
mod capability;
mod engine;
mod journal;
pub use journal::{JournalBlockage, JournalExecutionBinding, JournalLineage, JournalProjection,
    JournalProjectionMode, aggregate_model_accounting, journal_digest, project_execution_journal,
    validate_journal_capacity};
pub use capability::execute_recorded;
pub use engine::{
    Engine, EngineBlock, EngineBlockage, EngineConfig, EngineOutcome, EnginePorts, EngineReport,
    FinalPayloadValidator, InvocationKind, ValidatedFinalPayload, stable_call_id, stable_invocation_key,
    stable_preamble_id, stable_task_id,
};
