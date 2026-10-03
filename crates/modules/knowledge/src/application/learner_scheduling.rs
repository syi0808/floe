use std::sync::{Arc, Mutex};

use floe_execution::{CancelReason, Cancellation};
use floe_kernel::AgentFailure;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct LearnerScheduling(Arc<Mutex<SchedulingState>>);

#[derive(Default)]
struct SchedulingState {
    foreground_pending: usize,
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
        state.foreground_pending = state
            .foreground_pending
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        if let Some((_, cancellation)) = &state.active {
            cancellation.cancel();
        }
        Ok(())
    }

    pub fn foreground_finished(&self) -> Result<(), AgentFailure> {
        let mut state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        state.foreground_pending = state
            .foreground_pending
            .checked_sub(1)
            .ok_or(AgentFailure::Interrupted)?;
        Ok(())
    }

    pub fn foreground_pending(&self) -> Result<bool, AgentFailure> {
        let state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        Ok(state.foreground_pending > 0 || state.closed)
    }

    pub fn try_start(&self) -> Result<Option<LearnerLease>, AgentFailure> {
        let mut state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.closed || state.foreground_pending > 0 || state.active.is_some() {
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

/// An admitted foreground owner holds this while its execution is active.
pub struct KnowledgeForegroundLease {
    scheduling: LearnerScheduling,
}
impl KnowledgeForegroundLease {
    pub(crate) fn acquire(scheduling: LearnerScheduling) -> Result<Self, AgentFailure> {
        scheduling.foreground_submitted()?;
        Ok(Self { scheduling })
    }
}
impl Drop for KnowledgeForegroundLease {
    fn drop(&mut self) {
        let _ = self.scheduling.foreground_finished();
    }
}
