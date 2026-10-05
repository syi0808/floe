//! Physical write admission and uncertain-future retirement, shared by the two
//! encrypted stores. This guard owns no command or authorization policy.
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
