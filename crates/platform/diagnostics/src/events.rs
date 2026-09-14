use std::{
    any::Any,
    panic::{self, PanicHookInfo},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError},
};

use uuid::Uuid;

use crate::current_context;

type PanicHook = dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanicRecord {
    pub error_id: Uuid,
    pub panic_type: &'static str,
}

static HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn hook_lock() -> &'static Mutex<()> {
    HOOK_LOCK.get_or_init(|| Mutex::new(()))
}

pub struct PanicHookGuard {
    state: Arc<HookState>,
    hook_identity: *const PanicHook,
    lock: Option<MutexGuard<'static, ()>>,
}

struct HookState {
    enabled: AtomicBool,
    previous: Mutex<Option<Box<PanicHook>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanicHookInstallError {
    AlreadyInstalled,
}

/// Installs a payload-free hook for one exclusive boundary owner.
///
/// The owner must keep the returned guard alive only while it is catching its
/// own boundary panics and must not call `set_hook` concurrently. Dropping the
/// guard restores the handler that was present before installation.
pub fn install_panic_hook() -> Result<PanicHookGuard, PanicHookInstallError> {
    let lock = match hook_lock().try_lock() {
        Ok(lock) => lock,
        Err(TryLockError::Poisoned(poisoned)) => {
            let lock = poisoned.into_inner();
            hook_lock().clear_poison();
            lock
        }
        Err(TryLockError::WouldBlock) => return Err(PanicHookInstallError::AlreadyInstalled),
    };
    let previous = panic::take_hook();
    let state = Arc::new(HookState {
        enabled: AtomicBool::new(true),
        previous: Mutex::new(Some(previous)),
    });
    let hook_state = state.clone();
    let hook: Box<PanicHook> = Box::new(move |info| {
        if hook_state.enabled.load(Ordering::Acquire) && current_context().is_some() {
            return;
        }
        if let Ok(previous) = hook_state.previous.lock()
            && let Some(previous) = previous.as_ref()
        {
            previous(info);
        }
    });
    let hook_identity = hook.as_ref() as *const PanicHook;
    panic::set_hook(hook);
    Ok(PanicHookGuard {
        state,
        hook_identity,
        lock: Some(lock),
    })
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        self.state.enabled.store(false, Ordering::Release);
        if std::thread::panicking() {
            drop(self.lock.take());
            return;
        }
        let current = panic::take_hook();
        if std::ptr::eq(current.as_ref(), self.hook_identity) {
            if let Ok(mut previous) = self.state.previous.lock() {
                if let Some(previous) = previous.take() {
                    panic::set_hook(previous);
                } else {
                    panic::set_hook(current);
                }
            } else {
                panic::set_hook(current);
            }
        } else {
            panic::set_hook(current);
        }
        drop(self.lock.take());
    }
}

pub fn panic_record(payload: Box<dyn Any + Send>) -> PanicRecord {
    PanicRecord {
        error_id: Uuid::new_v4(),
        panic_type: panic_type(payload.as_ref()),
    }
}

fn panic_type(payload: &(dyn Any + Send)) -> &'static str {
    if payload.is::<&str>() {
        "str"
    } else if payload.is::<String>() {
        "string"
    } else {
        "unknown"
    }
}
