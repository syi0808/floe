//! The builtin Experts.
//!
//! Each folder owns one Expert's judgment, its context views, its role prompt and
//! its registration. The generic directory, Task path and common Engine stay in
//! `floe-experts`.

pub mod catalog;
pub mod commitments;
pub mod communication;
pub mod focus_attention;
pub mod life_logistics;
pub mod prompts;
mod registration;
pub mod relationships;
pub mod schedule;
pub mod wellbeing;
pub mod work_context;

mod shared;
mod program_support;

pub use catalog::{
    BUILTIN_EXPERT_PACKAGE_VERSION, BUILTIN_EXPERT_PUBLISHER, BUILTIN_EXPERT_STATE_SCHEMA_VERSION,
    BuiltinContextSource, BuiltinExpertKind,
};
pub use registration::{manifests, registrations};
