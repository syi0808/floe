//! Canonical Continue/Resume lineage. Retry remains a new Person Work intent.
use crate::{RunRecord, RunState};
use floe_kernel::AgentFailure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunSuccessorKind {
    Continue,
    Resume,
}

pub const MAX_RUN_LINEAGE_RECORDS: usize = (crate::MAX_RESUME_LINEAGE as usize + 1) * 4;

pub fn validate_run_successor(
    parent: &RunRecord,
    child: &RunRecord,
    kind: RunSuccessorKind,
) -> Result<(), AgentFailure> {
    parent.validate(parent.person_id)?;
    child.validate(parent.person_id)?;
    if parent.run_id == child.run_id
        || parent.session_id != child.session_id
        || parent.device_id != child.device_id
        || parent.user_message_id != child.user_message_id
        || child.initial_session_revision < parent.session_revision
        || child.retry_of.is_some()
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let valid = match kind {
        RunSuccessorKind::Continue => {
            matches!(
                (parent.state, parent.issue),
                (RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
                    | (RunState::Failed, Some(AgentFailure::BudgetExceeded))
            ) && child.continuation_of == Some(parent.run_id)
                && child.resume_of.is_none()
                && child.continuation_executor_generation == Some(parent.executor_generation)
                && parent.continuation_level.checked_add(1) == Some(child.continuation_level)
                && child.resume_lineage == parent.resume_lineage
                && child.initial_session_revision == parent.session_revision
        }
        RunSuccessorKind::Resume => {
            matches!(parent.state, RunState::Completed | RunState::Blocked)
                && child.resume_of == Some(parent.run_id)
                && child.continuation_of.is_none()
                && parent.resume_lineage.checked_add(1) == Some(child.resume_lineage)
                && child.continuation_level == 0
        }
    };
    if valid {
        Ok(())
    } else {
        Err(AgentFailure::StorageUnavailable)
    }
}

#[cfg(test)]
mod tests;
