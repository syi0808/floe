//! Session creation admission is distinct from failure and read projection.
use crate::SessionReceipt;
use floe_kernel::AgentFailure;

/// Immutable occupants of the Conversation command identity namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationCommandKind {
    SessionStart,
    Run,
    Cancel,
    InteractionDecision,
    InteractionRefresh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionStartRefusal {
    ForeignCommand(ConversationCommandKind),
    Capacity,
}
impl SessionStartRefusal {
    pub fn reason(self) -> AgentFailure {
        match self {
            Self::ForeignCommand(_) => AgentFailure::Conflict,
            Self::Capacity => AgentFailure::BudgetExceeded,
        }
    }
}

/// Returned only after the admission transaction and access check completed.
/// A refusal is a structural proof: occupied IDs are immutable and Start rows
/// are never evicted. A rollback or an arbitrary error cannot create this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStartAdmission {
    Started(SessionReceipt),
    Replayed(SessionReceipt),
    NotApplied(SessionStartRefusal),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStartFailure {
    /// Refused before physical admission; not proof about a previous attempt.
    NotAdmitted(AgentFailure),
    /// Includes interrupted transactions and any post-effect projection failure.
    Indeterminate(AgentFailure),
}
