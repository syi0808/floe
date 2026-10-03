use std::future::Future;

use floe_kernel::AgentFailure;
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Instant;

use crate::{CancelReason, Cancellation};

pub async fn run_bounded<Value>(
    future: impl Future<Output = Result<Value, AgentFailure>>,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<Value, AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(cancellation_failure(cancellation));
    }
    if deadline <= Instant::now() {
        cancellation.cancel_with_reason(CancelReason::Deadline);
        return Err(AgentFailure::DeadlineExceeded);
    }
    let mut guard = CallCancellationGuard(Some(cancellation.clone()));
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(cancellation_failure(cancellation)),
        result = tokio::time::timeout_at(deadline, future) => {
            result.unwrap_or(Err(AgentFailure::DeadlineExceeded))
        },
    };
    if matches!(result, Err(AgentFailure::DeadlineExceeded)) {
        cancellation.cancel_with_reason(CancelReason::Deadline);
    }
    if !matches!(
        result,
        Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded)
    ) {
        guard.0 = None;
    }
    result
}

pub fn cancellation_failure(cancellation: &Cancellation) -> AgentFailure {
    match cancellation.reason() {
        Some(CancelReason::Deadline) => AgentFailure::DeadlineExceeded,
        Some(CancelReason::OwnerDropped) => AgentFailure::Interrupted,
        Some(CancelReason::User) | None => AgentFailure::Cancelled,
    }
}

/// A timeout leaves the handle with its owner; it does not detach or assert settlement.
pub async fn cancel_and_join<Value>(
    task: &mut JoinHandle<Value>,
    cancellation: &Cancellation,
    reason: CancelReason,
    deadline: Instant,
) -> Option<Result<Value, JoinError>> {
    cancellation.cancel_with_reason(reason);
    tokio::time::timeout_at(deadline, task).await.ok()
}

struct CallCancellationGuard(Option<Cancellation>);

impl Drop for CallCancellationGuard {
    fn drop(&mut self) {
        if let Some(cancellation) = &self.0 {
            cancellation.cancel_with_reason(CancelReason::OwnerDropped);
        }
    }
}
