//! The best-effort gate before a linked fresh Run admission.
//!
//! Whether an origin currently wants a child is a pure judgment over the
//! origin receipt and its interaction group: Completed or Blocked, with chain depth
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
    /// marks the request superseded; it cannot overwrite that newer revision.
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
    if !matches!(origin.state, RunState::Completed | RunState::Blocked) {
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

/// Pure group policy used inside both resolution and terminal transactions.
pub fn build_resume_required(
    origin: &RunReceipt,
    group: &[ConversationInteraction],
) -> Result<Option<crate::ResumeRequired>, floe_kernel::AgentFailure> {
    use floe_kernel::AgentFailure;
    use sha2::{Digest, Sha256};
    origin.validate()?;
    for interaction in group {
        interaction.validate()?;
        if interaction.person_id.to_string() != origin.principal
            || interaction.session_id != origin.session_id
            || interaction.origin_run_id != origin.run_id
        {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    let Ok(link) = resume_gate(origin, group) else {
        return Ok(None);
    };
    let mut ordered = group.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|interaction| interaction.id);
    let group_digest: [u8; 32] = Sha256::digest(
        serde_json::to_vec(&(
            origin.run_id,
            origin.user_message_id,
            ordered
                .iter()
                .map(|interaction| {
                    (
                        interaction.id,
                        interaction.revision,
                        interaction.target_digest,
                        &interaction.state,
                    )
                })
                .collect::<Vec<_>>(),
        ))
        .map_err(|_| AgentFailure::StorageUnavailable)?,
    )
    .into();
    let pending = crate::ResumeRequired {
        origin_run_id: origin.run_id,
        person_id: floe_kernel::PersonId::from_uuid(
            uuid::Uuid::parse_str(&origin.principal)
                .map_err(|_| AgentFailure::StorageUnavailable)?,
        )
        .ok_or(AgentFailure::StorageUnavailable)?,
        device_id: origin.device_id.clone(),
        session_id: origin.session_id,
        user_message_id: origin.user_message_id,
        group_digest,
        expected_session_revision: origin.session_revision,
        lineage: link.lineage,
    };
    pending.validate()?;
    Ok(Some(pending))
}
