use std::sync::{Arc, Mutex};

use floe_execution::{CancelReason, Cancellation};
use floe_kernel::AgentFailure;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct LearnerScheduling(Arc<SchedulingShared>);

#[derive(Default)]
struct SchedulingShared {
    state: Mutex<SchedulingState>,
    drained: tokio::sync::Notify,
}

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
        let mut state = self.0.state.lock().map_err(|_| AgentFailure::Interrupted)?;
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
        let mut state = self.0.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        state.foreground_pending = state
            .foreground_pending
            .checked_sub(1)
            .ok_or(AgentFailure::Interrupted)?;
        Ok(())
    }

    pub fn foreground_pending(&self) -> Result<bool, AgentFailure> {
        let state = self.0.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        Ok(state.foreground_pending > 0 || state.closed)
    }

    pub fn try_start(&self) -> Result<Option<LearnerLease>, AgentFailure> {
        let mut state = self.0.state.lock().map_err(|_| AgentFailure::Interrupted)?;
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
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        self.0.drained.notify_waiters();
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
        if let Ok(mut state) = self.scheduling.0.state.lock()
            && state.active.as_ref().is_some_and(|(id, _)| *id == self.id)
        {
            state.active = None;
            self.scheduling.0.drained.notify_waiters();
        }
    }
}

/// An admitted foreground owner holds this while its execution is active.
pub struct KnowledgeForegroundLease {
    scheduling: LearnerScheduling,
}
impl KnowledgeForegroundLease {
    pub(crate) async fn acquire(
        scheduling: LearnerScheduling,
        scope: &floe_execution::ExecutionScope,
    ) -> Result<Self, AgentFailure> {
        scheduling.foreground_submitted()?;
        let lease = Self { scheduling };
        // Cancellation is a request, not proof that the learner released its
        // database transaction. Keep new background admissions fenced until
        // its retained lease drops, before returning foreground custody.
        scope
            .run(async {
                loop {
                    let drained = lease.scheduling.0.drained.notified();
                    tokio::pin!(drained);
                    drained.as_mut().enable();
                    {
                        let state = lease
                            .scheduling
                            .0
                            .state
                            .lock()
                            .map_err(|_| AgentFailure::Interrupted)?;
                        if state.closed {
                            return Err(AgentFailure::Interrupted);
                        }
                        if state.active.is_none() {
                            return Ok(());
                        }
                    }
                    drained.await;
                }
            })
            .await?;
        Ok(lease)
    }
}
impl Drop for KnowledgeForegroundLease {
    fn drop(&mut self) {
        let _ = self.scheduling.foreground_finished();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_execution::{
        ExecutionScope,
        budget::{BudgetConfig, BudgetLedger},
    };
    use std::time::Duration;

    fn scope() -> ExecutionScope {
        ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + Duration::from_secs(2),
            BudgetLedger::new(BudgetConfig::new(0, 0), Default::default()).root_lease(),
            floe_kernel::TraceContext::new(Uuid::new_v4()),
        )
    }

    #[tokio::test]
    async fn foreground_waits_for_cancelled_learner_custody_to_drain() {
        let scheduling = LearnerScheduling::default();
        let background = scheduling.try_start().unwrap().unwrap();
        let scope = scope();
        let foreground = KnowledgeForegroundLease::acquire(scheduling.clone(), &scope);
        tokio::pin!(foreground);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut foreground)
                .await
                .is_err()
        );
        assert!(background.cancellation().is_cancelled());
        assert!(scheduling.foreground_pending().unwrap());
        assert!(scheduling.try_start().unwrap().is_none());
        drop(background);
        let lease = foreground
            .await
            .expect("drained learner releases foreground");
        assert!(scheduling.try_start().unwrap().is_none());
        drop(lease);
        assert!(!scheduling.foreground_pending().unwrap());
        assert!(scheduling.try_start().unwrap().is_some());
    }

    #[tokio::test]
    async fn abandoned_or_closed_foreground_wait_releases_its_admission() {
        for close in [false, true] {
            let scheduling = LearnerScheduling::default();
            let background = scheduling.try_start().unwrap().unwrap();
            let scope = scope();
            let foreground = KnowledgeForegroundLease::acquire(scheduling.clone(), &scope);
            tokio::pin!(foreground);
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut foreground)
                    .await
                    .is_err()
            );
            if close {
                scheduling.close();
            } else {
                scope.cancellation().cancel();
            }
            let result = foreground.await;
            assert!(
                matches!(result, Err(AgentFailure::Interrupted)) && close
                    || matches!(result, Err(AgentFailure::Cancelled)) && !close
            );
            assert_eq!(scheduling.0.state.lock().unwrap().foreground_pending, 0);
            drop(background);
            assert_eq!(scheduling.try_start().unwrap().is_none(), close);
        }
    }
}
