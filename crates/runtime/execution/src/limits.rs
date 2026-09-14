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

#[cfg(test)]
mod tests {
    use std::{future::Future, task::Poll, time::Duration};

    use crate::CancelReason;

    use super::*;

    fn limiter(max_pending: usize) -> CallLimiter {
        CallLimiter::new(CallLimits {
            max_running: 1,
            max_pending,
            max_context_bytes: 10,
            max_total_context_bytes: 20,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn queued_cancellation_releases_capacity_without_cancelling_other_calls() {
        let limiter = limiter(1);
        let parent = Cancellation::new();
        let child = parent.child_scope();
        let deadline = Instant::now() + Duration::from_secs(5);
        let active = limiter.acquire(5, deadline, &parent).await.unwrap();
        let mut waiting = Box::pin(limiter.acquire(5, deadline, &child));
        assert!(
            std::future::poll_fn(|context| Poll::Ready(waiting.as_mut().poll(context)))
                .await
                .is_pending()
        );
        assert!(matches!(
            limiter.acquire(5, deadline, &parent).await,
            Err(AgentFailure::QuotaExceeded)
        ));
        child.cancel();
        assert!(matches!(waiting.await, Err(AgentFailure::Cancelled)));
        assert!(!parent.is_cancelled());
        drop(active);
        assert!(limiter.acquire(10, deadline, &parent).await.is_ok());
    }

    #[tokio::test]
    async fn byte_limits_cover_pending_and_running_calls_and_drop_rolls_back() {
        let limiter = limiter(2);
        let cancellation = Cancellation::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let active = limiter.acquire(10, deadline, &cancellation).await.unwrap();
        let mut waiting = Box::pin(limiter.acquire(10, deadline, &cancellation));
        assert!(
            std::future::poll_fn(|context| Poll::Ready(waiting.as_mut().poll(context)))
                .await
                .is_pending()
        );
        assert!(matches!(
            limiter.acquire(1, deadline, &cancellation).await,
            Err(AgentFailure::QuotaExceeded)
        ));
        drop(waiting);
        drop(active);
        assert!(matches!(
            limiter.acquire(11, deadline, &cancellation).await,
            Err(AgentFailure::BudgetExceeded)
        ));
        assert!(limiter.acquire(10, deadline, &cancellation).await.is_ok());
    }

    #[tokio::test]
    async fn no_queue_mode_rejects_excess_without_waiting_and_preserves_deadline_reason() {
        let limiter = limiter(0);
        let cancellation = Cancellation::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let active = limiter.acquire(0, deadline, &cancellation).await.unwrap();
        assert!(matches!(
            limiter.acquire(0, deadline, &cancellation).await,
            Err(AgentFailure::QuotaExceeded)
        ));
        drop(active);
        cancellation.cancel_with_reason(CancelReason::Deadline);
        assert!(matches!(
            limiter.acquire(0, deadline, &cancellation).await,
            Err(AgentFailure::DeadlineExceeded)
        ));
    }

    #[tokio::test]
    async fn queue_deadline_releases_pending_and_byte_capacity() {
        let limiter = limiter(1);
        let cancellation = Cancellation::new();
        let active = limiter
            .acquire(10, Instant::now() + Duration::from_secs(5), &cancellation)
            .await
            .unwrap();
        assert!(matches!(
            limiter
                .acquire(10, Instant::now() + Duration::from_millis(1), &cancellation)
                .await,
            Err(AgentFailure::DeadlineExceeded)
        ));
        assert!(!cancellation.is_cancelled());
        drop(active);
        assert!(
            limiter
                .acquire(10, Instant::now() + Duration::from_secs(5), &cancellation)
                .await
                .is_ok()
        );
    }
}
