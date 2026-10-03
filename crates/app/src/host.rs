use std::sync::{Arc, Condvar, Mutex, Weak};
use uuid::Uuid;
use crate::{CallerContext, HostError, HostServices};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostState { Open, Closing, Closed }
struct Lifecycle {
    state: HostState,
    active_requests: usize,
    shutdown_failure: Option<HostError>,
    close_hooks: Vec<Weak<dyn CloseAdmission>>,
}
pub(crate) trait CloseAdmission: Send + Sync { fn close_admission(&self); }

/// Shared admission owns no application services. Retained callback lanes use a
/// short guard for each call and cannot extend the lifetime of the core handle.
pub(crate) struct HostAdmission { lifecycle: Mutex<Lifecycle>, drained: Condvar }
impl HostAdmission {
    fn new() -> Self { Self { lifecycle: Mutex::new(Lifecycle { state: HostState::Open,
        active_requests: 0, shutdown_failure: None, close_hooks: Vec::new() }), drained: Condvar::new() } }
    pub(crate) fn enter(self: &Arc<Self>, request_id: Uuid) -> Result<AdmissionGuard, HostError> {
        if request_id.is_nil() { return Err(HostError::InvalidRequest); }
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        if lifecycle.state != HostState::Open { return Err(HostError::Closing); }
        lifecycle.active_requests = lifecycle.active_requests.checked_add(1).ok_or(HostError::Shutdown)?;
        Ok(AdmissionGuard { admission: self.clone() })
    }
    pub(crate) fn register_close_hook(&self, hook: Weak<dyn CloseAdmission>) -> Result<(), HostError> {
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        if lifecycle.state != HostState::Open { return Err(HostError::Closing); }
        lifecycle.close_hooks.retain(|hook| hook.strong_count() > 0);
        if lifecycle.close_hooks.len() >= 64 { return Err(HostError::InvalidRequest); }
        lifecycle.close_hooks.push(hook);
        Ok(())
    }
}
pub(crate) struct AdmissionGuard { admission: Arc<HostAdmission> }
impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        if let Ok(mut lifecycle) = self.admission.lifecycle.lock() {
            lifecycle.active_requests = lifecycle.active_requests.saturating_sub(1);
            if lifecycle.active_requests == 0 { self.admission.drained.notify_all(); }
        }
    }
}
pub struct AppHost<Services: HostServices> {
    services: Services,
    caller: CallerContext,
    admission: Arc<HostAdmission>,
}
impl<Services: HostServices> AppHost<Services> {
    pub(crate) fn with_caller(services: Services, caller: CallerContext) -> Self {
        Self { services, caller, admission: Arc::new(HostAdmission::new()) }
    }
    pub fn request(&self, request_id: Uuid) -> Result<HostRequest<'_, Services>, HostError> {
        let guard = self.admission.enter(request_id)?;
        Ok(HostRequest { host: self, caller: &self.caller, request_id, _guard: guard })
    }
    pub fn shutdown(&self) -> Result<(), HostError> {
        let mut lifecycle = self.admission.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        match lifecycle.state {
            HostState::Open => lifecycle.state = HostState::Closing,
            HostState::Closing => {
                while lifecycle.state == HostState::Closing {
                    lifecycle = self.admission.drained.wait(lifecycle).map_err(|_| HostError::Shutdown)?;
                }
                return lifecycle.shutdown_failure.map_or(Ok(()), Err);
            }
            HostState::Closed => return lifecycle.shutdown_failure.map_or(Ok(()), Err),
        }
        let hooks = lifecycle.close_hooks.iter().filter_map(Weak::upgrade).collect::<Vec<_>>();
        drop(lifecycle);
        // Cancelling callback registrations first releases product requests that
        // are awaiting a native reply. No new admission is possible after Closing.
        for hook in hooks { hook.close_admission(); }
        let mut lifecycle = self.admission.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        while lifecycle.active_requests != 0 {
            lifecycle = self.admission.drained.wait(lifecycle).map_err(|_| HostError::Shutdown)?;
        }
        drop(lifecycle);
        let result = self.services.shutdown();
        let mut lifecycle = self.admission.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        lifecycle.shutdown_failure = result.err(); lifecycle.state = HostState::Closed;
        self.admission.drained.notify_all();
        lifecycle.shutdown_failure.map_or(Ok(()), Err)
    }
}
impl<Services: HostServices> Drop for AppHost<Services> { fn drop(&mut self) { let _ = self.shutdown(); } }
pub struct HostRequest<'host, Services: HostServices> {
    host: &'host AppHost<Services>, caller: &'host CallerContext, request_id: Uuid, _guard: AdmissionGuard,
}
impl<Services: HostServices> HostRequest<'_, Services> {
    pub fn caller(&self) -> &CallerContext { self.caller }
    pub fn request_id(&self) -> Uuid { self.request_id }
    pub fn services(&self) -> &Services { &self.host.services }
    pub(crate) fn admission(&self) -> Arc<HostAdmission> { self.host.admission.clone() }
}
