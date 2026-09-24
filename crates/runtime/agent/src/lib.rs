//! Role-neutral execution loop. Session and task ownership stay in callers.
mod capability;
mod engine;
pub use capability::execute_recorded;
pub use engine::{
    Engine, EngineBlocked, EngineConfig, EngineOutcome, EnginePorts, EngineReport,
    FinalPayloadValidator, InvocationKind, stable_call_id, stable_invocation_key,
    stable_preamble_id, stable_task_id,
};
