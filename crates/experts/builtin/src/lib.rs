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
mod program_support;

pub use catalog::{
    BUILTIN_EXPERT_PACKAGE_VERSION, BUILTIN_EXPERT_PUBLISHER, BUILTIN_EXPERT_STATE_SCHEMA_VERSION,
    BuiltinContextSource, BuiltinExpertKind,
};
pub use floe_experts::RequirementReadOutcome;
pub use host::{
    Acquiring, BlockedExpertResult, BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertOutput,
    BuiltinExpertRequest, DeclaredSourceRead, StatefulExpertDraft, granted_context,
};
pub use registration::{BuiltinExpertRunner, manifests, registrations};
pub use shared::{
    ExpertJudgment, MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation,
};
