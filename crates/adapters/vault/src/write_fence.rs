//! Physical write admission and uncertain-future retirement, shared by the two
//! encrypted stores. This guard owns no command or authorization policy.
use floe_kernel::AgentFailure;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WriteAdmissionFailure {
    Busy,
    Unavailable,
}

pub(crate) struct JournalWriteGuard<'a> {
    unavailable: &'a AtomicBool,
    _lock: MutexGuard<'a, ()>,
    armed: bool,
}
impl<'a> JournalWriteGuard<'a> {
    pub(crate) fn acquire(
        unavailable: &'a AtomicBool,
        writes: &'a Mutex<()>,
    ) -> Result<Self, WriteAdmissionFailure> {
        let lock = writes.try_lock().map_err(|_| WriteAdmissionFailure::Busy)?;
        if unavailable.load(Ordering::Acquire) {
            return Err(WriteAdmissionFailure::Unavailable);
        }
        Ok(Self {
            unavailable,
            _lock: lock,
            armed: false,
        })
    }
    pub(crate) fn arm(&mut self) -> Result<(), WriteAdmissionFailure> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(WriteAdmissionFailure::Unavailable);
        }
        self.armed = true;
        Ok(())
    }
    pub(crate) fn settled(&mut self) {
        self.armed = false;
    }
}
impl Drop for JournalWriteGuard<'_> {
    fn drop(&mut self) {
        // Drop runs before _lock is released. A queued exact retry cannot write
        // a negative receipt while an abandoned SQL future might still settle.
        if self.armed {
            self.unavailable.store(true, Ordering::Release);
        }
    }
}

/// Retry only a proven local no-write/rolled-back Busy result, or a Busy read
/// after exact receipt publication. Every invocation retains the same immutable
/// payload and CAS identity. Commit uncertainty and all other failures escape;
/// this never retries the model/tool execution that produced the outcome.
pub(crate) async fn retry_exact_local_write<T, F, Fut>(mut write: F) -> Result<T, AgentFailure>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, AgentFailure>>,
{
    let mut attempt = 0u32;
    loop {
        match write().await {
            Err(AgentFailure::StorageBusy) if attempt < 3 => {
                tokio::time::sleep(std::time::Duration::from_millis(25 << attempt)).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod write_retry_tests {
    use super::*;
    use std::cell::Cell;
    #[tokio::test]
    async fn exact_local_write_retries_only_busy_and_remains_bounded() {
        for failure in [
            AgentFailure::StorageUnavailable,
            AgentFailure::VaultUnavailable,
            AgentFailure::Conflict,
            AgentFailure::PolicyDenied,
            AgentFailure::Cancelled,
            AgentFailure::DeadlineExceeded,
            AgentFailure::StorageBusy,
        ] {
            let attempts = Cell::new(0);
            let result: Result<(), _> = retry_exact_local_write(|| {
                attempts.set(attempts.get() + 1);
                std::future::ready(Err(failure))
            })
            .await;
            assert_eq!(result, Err(failure));
            assert_eq!(
                attempts.get(),
                if failure == AgentFailure::StorageBusy {
                    4
                } else {
                    1
                }
            );
        }
    }
}
