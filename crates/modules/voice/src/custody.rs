//! Retry only completed Busy results for one already-known immutable request.
//! In-flight storage is never dropped to meet this retry window; generation
//! shutdown retains its separate bounded drain and uncertain-custody handling.
use floe_kernel::{AgentFailure, CommandFailure};
use std::{future::Future, time::Duration};
use tokio::time::Instant;
const LOCAL_CUSTODY_RETRY_WINDOW: Duration = Duration::from_secs(2);
pub(crate) async fn retry_local<T, E, F, Fut>(
    mut operation: F,
    busy: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let deadline = Instant::now() + LOCAL_CUSTODY_RETRY_WINDOW;
    let mut delay = Duration::from_millis(20);
    loop {
        match operation().await {
            Err(error) if busy(&error) && Instant::now() < deadline => {
                tokio::time::sleep_until((Instant::now() + delay).min(deadline)).await;
                if Instant::now() >= deadline {
                    return Err(error);
                }
                delay = (delay * 2).min(Duration::from_millis(100));
            }
            result => return result,
        }
    }
}
pub(crate) fn busy(error: &AgentFailure) -> bool {
    *error == AgentFailure::StorageBusy
}
pub(crate) fn command_busy(error: &CommandFailure<AgentFailure>) -> bool {
    matches!(
        error,
        CommandFailure::NotAdmitted(AgentFailure::StorageBusy)
            | CommandFailure::NotApplied(AgentFailure::StorageBusy)
            | CommandFailure::Admitted(AgentFailure::StorageBusy)
            | CommandFailure::Indeterminate(AgentFailure::StorageBusy)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[tokio::test]
    async fn retry_retains_failure_classification_and_only_retries_busy() {
        for failure in [
            AgentFailure::StorageUnavailable,
            AgentFailure::Conflict,
            AgentFailure::PolicyDenied,
            AgentFailure::Cancelled,
        ] {
            let attempts = Cell::new(0);
            let result: Result<(), _> = retry_local(
                || {
                    attempts.set(attempts.get() + 1);
                    std::future::ready(Err(CommandFailure::Indeterminate(failure)))
                },
                command_busy,
            )
            .await;
            assert_eq!(result, Err(CommandFailure::Indeterminate(failure)));
            assert_eq!(attempts.get(), 1);
        }
        let attempts = Cell::new(0);
        let value = retry_local(
            || {
                attempts.set(attempts.get() + 1);
                std::future::ready(if attempts.get() < 3 {
                    Err(CommandFailure::NotAdmitted(AgentFailure::StorageBusy))
                } else {
                    Ok(7)
                })
            },
            command_busy,
        )
        .await
        .unwrap();
        assert_eq!(value, 7);
        assert_eq!(attempts.get(), 3);
    }
}
