//! Resume an already-admitted owner operation after determinate storage contention.
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use std::{future::Future, time::Duration};

pub(super) async fn retry_storage_contention<T, F, Fut>(
    scope: &ExecutionScope,
    mut operation: F,
) -> Result<T, AgentFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AgentFailure>>,
{
    let mut delay = Duration::from_millis(25);
    loop {
        if scope.cancellation().is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if tokio::time::Instant::now() >= scope.deadline() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        match operation().await {
            Err(AgentFailure::StorageBusy) => {}
            result => return result,
        }
        let wake = (tokio::time::Instant::now() + delay).min(scope.deadline());
        tokio::select! {
            _ = scope.cancellation().cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(wake) => {},
        }
        delay = (delay * 2).min(Duration::from_millis(500));
    }
}
