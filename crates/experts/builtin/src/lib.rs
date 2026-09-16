//! The builtin Experts.
//!
//! Each folder owns one Expert's judgment, its context views, its role prompt and
//! its registration. The generic directory, Task path and A2A transport stay in
//! `floe-experts`.

pub mod catalog;
pub mod commitments;
pub mod communication;
pub mod focus_attention;
pub mod life_logistics;
pub mod prompts;
pub mod relationships;
pub mod schedule;
pub mod setup;
pub mod wellbeing;
pub mod work_context;

mod shared;

pub use catalog::{BUILTIN_EXPERT_PACKAGE_VERSION, BuiltinContextSource, BuiltinExpertKind};
pub use shared::{MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation};
