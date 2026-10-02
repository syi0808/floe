//! The best-effort gate before a linked fresh Run admission.
//!
//! Whether an origin currently wants a child is a pure judgment over the
//! origin receipt and its interaction group: Completed, with chain depth
//! left, every card terminal and at least one resolved. Admission itself
//! re-verifies all of this atomically (plus the Session revision and the
//! unique resume slot), so this gate only saves wasted admission attempts
//! and names honest suppression reasons; it never authorizes a child.

use crate::{
    ConversationInteraction, InteractionResumeRef, InteractionState, RunReceipt, RunState,
};

/// Why no linked child is currently wanted for an origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeSuppression {
    /// The origin never finished successfully; Working, Failed, Cancelled,
    /// TimedOut and Interrupted origins have no resume linkage.
    OriginNotCompleted,
    /// The origin finished but its chain already reached the automatic
    /// depth cap; only a fresh explicit turn continues from here.
    LineageExhausted,
    /// A card is still Pending or Resolving; automatic resume waits for
    /// the whole group to settle.
    GroupOpen,
    /// Every card settled with none resolved: deny-all and its cousins
    /// never start a child on their own.
    NothingResolved,
    /// The Session moved on since the origin finished. Automatic restart
    /// stays suppressed; an explicit Continue may still claim the slot at
    /// the current revision.
    NewerTurn,
}

/// The child linkage one origin currently admits, if any.
///
/// Reads only: the atomic admission re-verifies the origin, the group,
/// the revision and the slot before anything is created.
pub fn resume_gate(
    origin: &RunReceipt,
    group: &[ConversationInteraction],
) -> Result<InteractionResumeRef, ResumeSuppression> {
    if origin.state != RunState::Completed {
        return Err(ResumeSuppression::OriginNotCompleted);
    }
    let Some(link) = origin.resume() else {
        return Err(ResumeSuppression::LineageExhausted);
    };
    if group.is_empty()
        || !group
            .iter()
            .any(|entry| matches!(entry.state, InteractionState::Resolved { .. }))
    {
        return Err(ResumeSuppression::NothingResolved);
    }
    if group.iter().any(|entry| !entry.state.is_terminal()) {
        return Err(ResumeSuppression::GroupOpen);
    }
    Ok(link)
}
