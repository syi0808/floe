use crate::{CallerContext, HostError, HostServices};
use std::{
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// One total caller wait, shared by the first and every repeated close:
/// 35s request drain + 60s lifecycle operation + 35s generation retirement +
/// 5s lifecycle runtime retirement + 35s Day retirement + 5s margin.
/// Expiry closes admission with an error; retained cleanup is not a clean drain.
const HOST_SHUTDOWN_BUDGET: Duration = Duration::from_secs(175);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostState {
    Open,
    Closing,
    Closed,
}
struct Lifecycle {
    state: HostState,
    active_requests: usize,
    shutdown_deadline: Option<Instant>,
    shutdown_failure: Option<HostError>,
    #[cfg(test)]
    retirement_complete: bool,
    close_hooks: Vec<Weak<dyn CloseAdmission>>,
}
pub(crate) trait CloseAdmission: Send + Sync {
    fn close_admission(&self);
}

/// Shared admission owns no application services. Retained callback lanes use a
/// short guard for each call and cannot extend the lifetime of the core handle.
pub(crate) struct HostAdmission {
    lifecycle: Mutex<Lifecycle>,
    drained: Condvar,
}
impl HostAdmission {
    fn new() -> Self {
        Self {
            lifecycle: Mutex::new(Lifecycle {
                state: HostState::Open,
                active_requests: 0,
                shutdown_deadline: None,
                shutdown_failure: None,
                #[cfg(test)]
                retirement_complete: false,
                close_hooks: Vec::new(),
            }),
            drained: Condvar::new(),
        }
    }
    pub(crate) fn enter(self: &Arc<Self>, request_id: Uuid) -> Result<AdmissionGuard, HostError> {
        if request_id.is_nil() {
            return Err(HostError::InvalidRequest);
        }
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        if lifecycle.state != HostState::Open {
            return Err(HostError::Closing);
        }
        lifecycle.active_requests = lifecycle
            .active_requests
            .checked_add(1)
            .ok_or(HostError::Shutdown)?;
        Ok(AdmissionGuard {
            admission: self.clone(),
        })
    }
    pub(crate) fn register_close_hook(
        &self,
        hook: Weak<dyn CloseAdmission>,
    ) -> Result<(), HostError> {
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        if lifecycle.state != HostState::Open {
            return Err(HostError::Closing);
        }
        lifecycle.close_hooks.retain(|hook| hook.strong_count() > 0);
        if lifecycle.close_hooks.len() >= 64 {
            return Err(HostError::InvalidRequest);
        }
        lifecycle.close_hooks.push(hook);
        Ok(())
    }

    fn retirement_lock(&self) -> MutexGuard<'_, Lifecycle> {
        match self.lifecycle.lock() {
            Ok(lifecycle) => lifecycle,
            Err(poisoned) => {
                let mut lifecycle = poisoned.into_inner();
                lifecycle.shutdown_failure = Some(HostError::Shutdown);
                lifecycle
            }
        }
    }

    fn finish(&self, result: Result<(), HostError>) {
        let mut lifecycle = self.retirement_lock();
        if let Err(failure) = result {
            lifecycle.shutdown_failure.get_or_insert(failure);
        }
        #[cfg(test)]
        {
            lifecycle.retirement_complete = true;
        }
        lifecycle.state = HostState::Closed;
        self.drained.notify_all();
    }
}
pub(crate) struct AdmissionGuard {
    admission: Arc<HostAdmission>,
}
impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        let mut lifecycle = self.admission.retirement_lock();
        if lifecycle.active_requests == 0 {
            lifecycle.shutdown_failure = Some(HostError::Shutdown);
        } else {
            lifecycle.active_requests -= 1;
        }
        if lifecycle.active_requests == 0 {
            self.admission.drained.notify_all();
        }
    }
}
pub struct AppHost<Services: HostServices> {
    // The sole retirement worker owns the lifetime root. Requests temporarily
    // retain strong references; the opaque core can never perform the last drop.
    services: Weak<Services>,
    caller: CallerContext,
    admission: Arc<HostAdmission>,
}
impl<Services: HostServices> AppHost<Services> {
    pub(crate) fn with_caller(
        services: Services,
        caller: CallerContext,
    ) -> Result<Self, HostError> {
        let services = Arc::new(services);
        let admission = Arc::new(HostAdmission::new());
        let worker_services = services.clone();
        let worker_admission = admission.clone();
        // Create the lifetime owner before exposing admission. A lazy spawn at
        // close could fail after requests had already borrowed the runtime.
        let worker = std::thread::Builder::new()
            .name("floe-host-retirement".into())
            .spawn(move || {
                let completion = RetirementCompletion {
                    admission: worker_admission.clone(),
                    completed: false,
                };
                let result = isolate_shutdown(|| {
                    retire_services(&worker_admission, worker_services.as_ref())
                });
                // Drop the final service root on this thread before publishing
                // success. A blocked destructor remains covered by the caller's
                // deadline and cannot strand memory borrowed by an active call.
                let release = isolate_shutdown(|| {
                    drop(worker_services);
                    Ok(())
                });
                completion.finish(result.and(release));
            });
        if worker.is_err() {
            // No request has been exposed. Retire the constructed resources,
            // including a partially activated owner, before reporting open failure.
            let _ = isolate_shutdown(|| services.shutdown());
            let _ = isolate_shutdown(|| {
                drop(services);
                Ok(())
            });
            return Err(HostError::Shutdown);
        }
        // There is deliberately no join on this handle. The worker owns its
        // resources until real retirement completes, including after a timeout.
        drop(worker);
        Ok(Self {
            services: Arc::downgrade(&services),
            caller,
            admission,
        })
    }
    pub fn request(&self, request_id: Uuid) -> Result<HostRequest<'_, Services>, HostError> {
        let guard = self.admission.enter(request_id)?;
        let services = self.services.upgrade().ok_or(HostError::Shutdown)?;
        Ok(HostRequest {
            services,
            caller: self.caller.clone(),
            request_id,
            _host: PhantomData,
            _guard: guard,
        })
    }
    pub fn shutdown(&self) -> Result<(), HostError> {
        self.shutdown_with_budget(HOST_SHUTDOWN_BUDGET)
    }

    #[cfg(test)]
    pub(crate) fn shutdown_with_budget_for_test(&self, budget: Duration) -> Result<(), HostError> {
        self.shutdown_with_budget(budget)
    }

    fn shutdown_with_budget(&self, budget: Duration) -> Result<(), HostError> {
        let mut lifecycle = self.admission.retirement_lock();
        match lifecycle.state {
            HostState::Open => {
                lifecycle.state = HostState::Closing;
                lifecycle.shutdown_deadline = Some(Instant::now() + budget);
                self.admission.drained.notify_all();
            }
            HostState::Closing => {}
            HostState::Closed => return lifecycle.shutdown_failure.map_or(Ok(()), Err),
        }
        let deadline = lifecycle.shutdown_deadline.unwrap_or_else(Instant::now);
        while lifecycle.state != HostState::Closed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                lifecycle.shutdown_failure = Some(HostError::Shutdown);
                lifecycle.state = HostState::Closed;
                self.admission.drained.notify_all();
                break;
            }
            lifecycle = match self.admission.drained.wait_timeout(lifecycle, remaining) {
                Ok((lifecycle, _)) => lifecycle,
                Err(poisoned) => {
                    let (mut lifecycle, _) = poisoned.into_inner();
                    lifecycle.shutdown_failure = Some(HostError::Shutdown);
                    lifecycle
                }
            };
        }
        lifecycle.shutdown_failure.map_or(Ok(()), Err)
    }

    #[cfg(test)]
    pub(crate) fn retirement_complete_for_test(&self) -> bool {
        self.admission.retirement_lock().retirement_complete
    }
}
impl<Services: HostServices> Drop for AppHost<Services> {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
pub struct HostRequest<'host, Services: HostServices> {
    // Fields drop in declaration order. Release services and caller BEFORE the
    // admission guard notifies retirement, preserving the worker's final Arc.
    services: Arc<Services>,
    caller: CallerContext,
    request_id: Uuid,
    _host: PhantomData<&'host AppHost<Services>>,
    _guard: AdmissionGuard,
}
impl<Services: HostServices> HostRequest<'_, Services> {
    pub fn caller(&self) -> &CallerContext {
        &self.caller
    }
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }
    pub fn services(&self) -> &Services {
        self.services.as_ref()
    }
    pub(crate) fn admission(&self) -> Arc<HostAdmission> {
        self._guard.admission.clone()
    }
}

struct RetirementCompletion {
    admission: Arc<HostAdmission>,
    completed: bool,
}
impl RetirementCompletion {
    fn finish(mut self, result: Result<(), HostError>) {
        self.admission.finish(result);
        self.completed = true;
    }
}
impl Drop for RetirementCompletion {
    fn drop(&mut self) {
        if !self.completed {
            self.admission.finish(Err(HostError::Shutdown));
        }
    }
}

fn retire_services(
    admission: &HostAdmission,
    services: &impl HostServices,
) -> Result<(), HostError> {
    let mut lifecycle = admission.retirement_lock();
    while lifecycle.state == HostState::Open {
        lifecycle = match admission.drained.wait(lifecycle) {
            Ok(lifecycle) => lifecycle,
            Err(poisoned) => {
                let mut lifecycle = poisoned.into_inner();
                lifecycle.shutdown_failure = Some(HostError::Shutdown);
                lifecycle
            }
        };
    }
    let hooks = std::mem::take(&mut lifecycle.close_hooks);
    drop(lifecycle);
    let mut result = Ok(());
    // One panicking callback cannot prevent the remaining registrations retiring.
    // Closing them first releases product calls waiting on native completions.
    for hook in hooks.into_iter().filter_map(|hook| hook.upgrade()) {
        let closed = isolate_shutdown(|| {
            hook.close_admission();
            Ok(())
        });
        result = result.and(closed);
    }
    let mut lifecycle = admission.retirement_lock();
    while lifecycle.active_requests != 0 {
        let remaining = lifecycle
            .shutdown_deadline
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            lifecycle.state = HostState::Closed;
            lifecycle.shutdown_failure = Some(HostError::Shutdown);
            admission.drained.notify_all();
            // Callers have their terminal error. Retained cleanup waits for the
            // actual last guard; it must never free still-borrowed service state.
            lifecycle = match admission.drained.wait(lifecycle) {
                Ok(lifecycle) => lifecycle,
                Err(poisoned) => poisoned.into_inner(),
            };
        } else {
            lifecycle = match admission.drained.wait_timeout(lifecycle, remaining) {
                Ok((lifecycle, _)) => lifecycle,
                Err(poisoned) => {
                    let (mut lifecycle, _) = poisoned.into_inner();
                    lifecycle.shutdown_failure = Some(HostError::Shutdown);
                    lifecycle
                }
            };
        }
    }
    drop(lifecycle);
    let retired = isolate_shutdown(|| services.shutdown());
    result.and(retired)
}

fn isolate_shutdown(operation: impl FnOnce() -> Result<(), HostError>) -> Result<(), HostError> {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|payload| {
        // Only the redacted panic type/incident is recorded. Even an unusual
        // panic payload destructor or failing diagnostic sink cannot unwind out.
        if let Err(secondary) = catch_unwind(AssertUnwindSafe(|| {
            crate::diagnostics::panic_error(payload);
        })) {
            std::mem::forget(secondary);
        }
        Err(HostError::Shutdown)
    })
}
