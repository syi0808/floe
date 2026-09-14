//! Role-neutral execution loop. Session and task ownership stay in callers.
mod engine;
pub use engine::{Engine, EngineConfig, EnginePorts, EngineReport, FinalPayloadValidator};
