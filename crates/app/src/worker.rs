//! What this host's vault worker is asked to do, and what it reports back.
//!
//! The worker queue carries the Person's own commands, stated in the words of
//! the owners that will run them. Nothing here is a wire shape: the binding
//! parses a request into one of these and projects the result back out, so that
//! the queue, the job map and the dispatcher never speak a protocol.

use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use uuid::Uuid;


/// The Person's vault, as this process currently holds it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VaultState {
    #[default]
    Missing,
    Locked,
    Ready,
    Unavailable,
}

/// What a calendar action command is asked to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CalendarActionOperation {
    Capabilities,
    GetAuthority,
    SetAuthority {
        calendar_create: floe_actions::ActionAuthorityMode,
    },
    Execute {
        action_id: Uuid,
    },
    Recover {
        action_id: Uuid,
    },
    List,
    Get {
        action_id: Uuid,
    },
    Propose(Box<CalendarActionProposal>),
    Direct(Box<CalendarActionProposal>),
    Decide {
        action_id: Uuid,
        approve: bool,
    },
}

/// One calendar write the Person is being asked about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarActionProposal {
    pub calendar_id: String,
    pub title: String,
    pub starts_at: String,
    pub ends_at: String,
    pub timezone: String,
    pub event_id: Option<String>,
    pub event_revision: Option<u64>,
    pub delete: bool,
}

/// One decision the Person made about a learned memory, and what they are
/// shown afterwards.
///
/// Both are Knowledge's own values; the worker only carries them.
pub use floe_knowledge::{MemoryReviewDecision, MemoryReviewResult};

/// One proposal the Person asked to inspect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarProposalInspection {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub invocation_id: Uuid,
    pub action: Option<floe_actions::CalendarAction>,
}

/// One command the worker runs against this Person's vault.
///
/// Every variant names an owner's own request; none of them carries a wire.
pub enum WorkerAction {
    Status,
    Create,
    Unlock,
    Lock,
    Registry {
        change: Option<floe_experts::RegistryConfiguration>,
    },
    ExpertCandidates {
        assignment_id: Uuid,
        requirement_key: String,
        device_id: String,
    },
    ExpertReplaceBinding {
        change: crate::ExpertBindingSelectionIntent,
        device_id: String,
    },
    CalendarAction {
        operation: CalendarActionOperation,
    },
    InspectProposal {
        session_id: Uuid,
        invocation_id: Uuid,
    },
    MemoryReview {
        decision: Option<MemoryReviewDecision>,
    },
    Memory,
}

impl WorkerAction {
    /// The stage name a failure on this command is reported under.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Create => "create",
            Self::Unlock => "unlock",
            Self::Lock => "lock",
            Self::Registry { .. } => "registry",
            Self::ExpertCandidates { .. } => "expert_candidates",
            Self::ExpertReplaceBinding { .. } => "expert_binding",
            Self::CalendarAction { .. } => "calendar_action",
            Self::InspectProposal { .. } => "inspect_proposal",
            Self::MemoryReview { .. } => "memory_review",
            Self::Memory => "memory",
        }
    }

    /// Whether this command needs the host to itself, with no vault open.
    pub fn is_exclusive_host(&self) -> bool {
        matches!(self, Self::Create | Self::Unlock | Self::Lock)
    }

    /// Whether this command may run beside another on the open vault.
    pub fn is_concurrent_host(&self) -> bool {
        matches!(
            self,
            Self::Status
                | Self::Registry { .. }
                | Self::ExpertCandidates { .. }
                | Self::ExpertReplaceBinding { .. }
                | Self::InspectProposal { .. }
                | Self::MemoryReview { .. }
                | Self::Memory
        )
    }
}

/// How far one command got, and everything it produced.
///
/// Every slot is an owner's own value; a caller decides how to say it.
#[derive(Clone, Debug)]
pub struct WorkerResult {
    pub request_id: Uuid,
    pub person_id: PersonId,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub registry: Option<floe_experts::RegistryOverview>,
    pub expert_candidates: Option<crate::ExpertCandidateCatalog>,
    pub proposal: Option<CalendarProposalInspection>,
    pub memory_review: Option<MemoryReviewResult>,
    pub memory: Option<floe_knowledge::MemoryOverviewSnapshot>,
    pub calendar_actions: Option<crate::CalendarActionsResult>,
    pub failure: Option<AgentFailure>,
}

