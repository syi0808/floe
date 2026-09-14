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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn expired_scope_never_polls_the_operation() {
        let parent = Cancellation::new();
        let child = parent.child_scope();
        let calls = AtomicUsize::new(0);
        let result = run_bounded(
            async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            Instant::now(),
            &child,
        )
        .await;
        assert_eq!(result, Err(AgentFailure::DeadlineExceeded));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(child.reason(), Some(CancelReason::Deadline));
        assert!(!parent.is_cancelled());
    }

    #[tokio::test]
    async fn inherited_deadline_remains_a_deadline_failure() {
        let parent = Cancellation::new();
        let child = parent.child_scope();
        let signal = parent.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            run_bounded(
                async {
                    started.send(()).unwrap();
                    std::future::pending::<Result<(), AgentFailure>>().await
                },
                Instant::now() + Duration::from_secs(5),
                &child,
            )
            .await
        });
        ready.await.unwrap();
        signal.cancel_with_reason(CancelReason::Deadline);
        assert_eq!(task.await.unwrap(), Err(AgentFailure::DeadlineExceeded));
    }

    #[tokio::test]
    async fn dropping_polled_call_only_cancels_its_scope() {
        let parent = Cancellation::new();
        let child = parent.child_scope();
        let sibling = parent.child_scope();
        let started = Arc::new(tokio::sync::Notify::new());
        let signal = started.clone();
        let mut call = Box::pin(run_bounded(
            async move {
                signal.notify_one();
                std::future::pending::<Result<(), AgentFailure>>().await
            },
            Instant::now() + Duration::from_secs(5),
            &child,
        ));
        tokio::select! {
            result = &mut call => panic!("unexpected completion: {result:?}"),
            _ = started.notified() => {}
        }
        drop(call);
        assert_eq!(child.reason(), Some(CancelReason::OwnerDropped));
        assert!(!parent.is_cancelled());
        assert!(!sibling.is_cancelled());
    }

    #[tokio::test]
    async fn timed_out_join_retains_handle_until_actual_settlement() {
        let parent = Cancellation::new();
        let child = parent.child_scope();
        let (release, wait) = tokio::sync::oneshot::channel();
        let mut task = tokio::spawn(async move { wait.await.unwrap() });
        assert!(
            cancel_and_join(&mut task, &child, CancelReason::User, Instant::now())
                .await
                .is_none()
        );
        assert!(!task.is_finished());
        assert!(!parent.is_cancelled());
        release.send(42).unwrap();
        assert_eq!(task.await.unwrap(), 42);
    }
}
