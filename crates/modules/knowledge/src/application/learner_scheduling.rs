use std::sync::{Arc, Mutex};

use floe_execution::{CancelReason, Cancellation};
use floe_kernel::AgentFailure;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct LearnerScheduling(Arc<Mutex<SchedulingState>>);

#[derive(Default)]
struct SchedulingState {
    foreground_pending: bool,
    closed: bool,
    active: Option<(Uuid, Cancellation)>,
}

pub struct LearnerLease {
    scheduling: LearnerScheduling,
    id: Uuid,
    cancellation: Cancellation,
}

impl LearnerScheduling {
    pub fn foreground_submitted(&self) -> Result<(), AgentFailure> {
        let mut state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.closed {
            return Err(AgentFailure::Interrupted);
        }
        state.foreground_pending = true;
        if let Some((_, cancellation)) = &state.active {
            cancellation.cancel();
        }
        Ok(())
    }

    pub fn foreground_finished(&self) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .foreground_pending = false;
        Ok(())
    }

    pub fn foreground_pending(&self) -> Result<bool, AgentFailure> {
        let state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        Ok(state.foreground_pending || state.closed)
    }

    pub fn try_start(&self) -> Result<Option<LearnerLease>, AgentFailure> {
        let mut state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.closed || state.foreground_pending || state.active.is_some() {
            return Ok(None);
        }
        let id = Uuid::new_v4();
        let cancellation = Cancellation::default();
        state.active = Some((id, cancellation.clone()));
        Ok(Some(LearnerLease {
            scheduling: self.clone(),
            id,
            cancellation,
        }))
    }

    pub fn close(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        if let Some((_, cancellation)) = &state.active {
            cancellation.cancel_with_reason(CancelReason::OwnerDropped);
        }
    }
}

impl LearnerLease {
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
}

impl Drop for LearnerLease {
    fn drop(&mut self) {
        self.cancellation
            .cancel_with_reason(CancelReason::OwnerDropped);
        if let Ok(mut state) = self.scheduling.0.lock()
            && state.active.as_ref().is_some_and(|(id, _)| *id == self.id)
        {
            state.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_preempts_only_the_independent_learner_lease() {
        let scheduling = LearnerScheduling::default();
        let foreground = Cancellation::default();
        let expert = foreground.child_scope();
        let learner = scheduling.try_start().unwrap().unwrap();
        assert!(scheduling.try_start().unwrap().is_none());
        scheduling.foreground_submitted().unwrap();
        assert!(learner.cancellation().is_cancelled());
        assert!(!foreground.is_cancelled());
        assert!(!expert.is_cancelled());
        drop(learner);
        assert!(scheduling.try_start().unwrap().is_none());
        scheduling.foreground_finished().unwrap();
        let next = scheduling.try_start().unwrap().unwrap();
        assert!(!next.cancellation().is_cancelled());
        expert.cancel();
        assert!(!next.cancellation().is_cancelled());
    }

    #[test]
    fn release_cancels_orphan_work_and_close_prevents_new_leases() {
        let scheduling = LearnerScheduling::default();
        let learner = scheduling.try_start().unwrap().unwrap();
        let cancellation = learner.cancellation();
        drop(learner);
        assert_eq!(cancellation.reason(), Some(CancelReason::OwnerDropped));
        let learner = scheduling.try_start().unwrap().unwrap();
        scheduling.close();
        assert!(learner.cancellation().is_cancelled());
        drop(learner);
        assert!(scheduling.try_start().unwrap().is_none());
        assert!(scheduling.foreground_pending().unwrap());
        assert_eq!(
            scheduling.foreground_submitted(),
            Err(AgentFailure::Interrupted)
        );
    }
}
