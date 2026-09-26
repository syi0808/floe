//! The builtin Experts.
//!
//! Each folder owns one Expert's judgment, its context views, its role prompt and
//! its registration. The generic directory, Task path and A2A transport stay in
//! `floe-experts`.

pub mod catalog;
pub mod commitments;
pub mod communication;
pub mod focus_attention;
mod host;
pub mod life_logistics;
pub mod prompts;
mod registration;
pub mod relationships;
pub mod schedule;
pub mod wellbeing;
pub mod work_context;

mod shared;

pub use catalog::{
    BUILTIN_EXPERT_PACKAGE_VERSION, BUILTIN_EXPERT_PUBLISHER, BUILTIN_EXPERT_STATE_SCHEMA_VERSION,
    BuiltinContextSource, BuiltinExpertKind, BuiltinSourceRequirement,
};
pub use host::{
    Acquiring, BlockedExpertResult, BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput,
    BuiltinExpertRequest, StatefulExpertDraft, granted_context,
};
#[cfg(test)]
mod test_host;
pub use registration::{manifests, registrations};
pub use shared::{
    ExpertJudgment, MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation,
};
