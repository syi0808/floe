//! Role-neutral execution loop. Session and task ownership stay in callers.
mod capability;
mod engine;
pub use capability::execute_recorded;
pub use engine::{Engine, EngineConfig, EnginePorts, EngineReport, FinalPayloadValidator};
