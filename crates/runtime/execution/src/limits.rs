use std::sync::Arc;

use floe_kernel::AgentFailure;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use crate::{Cancellation, tasks::cancellation_failure};

#[derive(Clone, Copy, Debug)]
pub struct CallLimits {
    pub max_running: usize,
    pub max_pending: usize,
    pub max_context_bytes: u32,
    pub max_total_context_bytes: u32,
}

#[derive(Clone, Debug)]
pub struct CallLimiter {
    running: Arc<Semaphore>,
    pending: Arc<Semaphore>,
    context: Arc<Semaphore>,
    max_context_bytes: u32,
}

#[derive(Debug)]
pub struct CallPermit {
    _running: OwnedSemaphorePermit,
    _context: OwnedSemaphorePermit,
}

impl CallLimiter {
    pub fn new(limits: CallLimits) -> Result<Self, AgentFailure> {
        if limits.max_running == 0
            || limits.max_running > Semaphore::MAX_PERMITS
            || limits.max_pending > Semaphore::MAX_PERMITS
            || limits.max_context_bytes > limits.max_total_context_bytes
            || u64::from(limits.max_total_context_bytes) > Semaphore::MAX_PERMITS as u64
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            running: Arc::new(Semaphore::new(limits.max_running)),
            pending: Arc::new(Semaphore::new(limits.max_pending)),
            context: Arc::new(Semaphore::new(limits.max_total_context_bytes as usize)),
            max_context_bytes: limits.max_context_bytes,
        })
    }

    pub async fn acquire(
        &self,
        context_bytes: usize,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<CallPermit, AgentFailure> {
        check_running(deadline, cancellation)?;
        let context_bytes =
            u32::try_from(context_bytes).map_err(|_| AgentFailure::BudgetExceeded)?;
        if context_bytes > self.max_context_bytes {
            return Err(AgentFailure::BudgetExceeded);
        }
        let context = self
            .context
            .clone()
            .try_acquire_many_owned(context_bytes)
            .map_err(|_| AgentFailure::QuotaExceeded)?;
        let running = match self.running.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                let _pending = self
                    .pending
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| AgentFailure::QuotaExceeded)?;
                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => return Err(cancellation_failure(cancellation)),
                    _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
                    permit = self.running.clone().acquire_owned() => {
                        permit.map_err(|_| AgentFailure::Interrupted)?
                    }
                }
            }
        };
        check_running(deadline, cancellation)?;
        Ok(CallPermit {
            _running: running,
            _context: context,
        })
    }
}

fn check_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        Err(cancellation_failure(cancellation))
    } else if Instant::now() >= deadline {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}
