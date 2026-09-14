use std::sync::{Condvar, Mutex};

use uuid::Uuid;

use crate::{CallerContext, HostError, HostServices};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostState {
    Open,
    Closing,
    Closed,
}

struct Lifecycle {
    state: HostState,
    active_requests: usize,
    shutdown_failure: Option<HostError>,
}

pub struct AppHost<Services: HostServices> {
    services: Services,
    caller: Option<CallerContext>,
    lifecycle: Mutex<Lifecycle>,
    drained: Condvar,
}

impl<Services: HostServices> AppHost<Services> {
    pub fn legacy(services: Services) -> Self {
        Self {
            services,
            caller: None,
            lifecycle: Mutex::new(Lifecycle {
                state: HostState::Open,
                active_requests: 0,
                shutdown_failure: None,
            }),
            drained: Condvar::new(),
        }
    }

    pub(crate) fn with_caller(services: Services, caller: CallerContext) -> Self {
        Self {
            services,
            caller: Some(caller),
            lifecycle: Mutex::new(Lifecycle {
                state: HostState::Open,
                active_requests: 0,
                shutdown_failure: None,
            }),
            drained: Condvar::new(),
        }
    }

    pub fn request(&self, request_id: Uuid) -> Result<HostRequest<'_, Services>, HostError> {
        if request_id.is_nil() {
            return Err(HostError::InvalidRequest);
        }
        let caller = self.caller.as_ref().ok_or(HostError::UnsupportedCaller)?;
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        if lifecycle.state != HostState::Open {
            return Err(HostError::Closing);
        }
        lifecycle.active_requests = lifecycle
            .active_requests
            .checked_add(1)
            .ok_or(HostError::Shutdown)?;
        Ok(HostRequest {
            host: self,
            caller,
            request_id,
        })
    }

    pub fn legacy_services(&self) -> &Services {
        &self.services
    }

    pub fn shutdown(&self) -> Result<(), HostError> {
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        match lifecycle.state {
            HostState::Open => lifecycle.state = HostState::Closing,
            HostState::Closing => {
                while lifecycle.state == HostState::Closing {
                    lifecycle = self
                        .drained
                        .wait(lifecycle)
                        .map_err(|_| HostError::Shutdown)?;
                }
                return lifecycle.shutdown_failure.map_or(Ok(()), Err);
            }
            HostState::Closed => return lifecycle.shutdown_failure.map_or(Ok(()), Err),
        }
        while lifecycle.active_requests != 0 {
            lifecycle = self
                .drained
                .wait(lifecycle)
                .map_err(|_| HostError::Shutdown)?;
        }
        drop(lifecycle);
        let result = self.services.shutdown();
        let mut lifecycle = self.lifecycle.lock().map_err(|_| HostError::Shutdown)?;
        lifecycle.shutdown_failure = result.err();
        lifecycle.state = HostState::Closed;
        self.drained.notify_all();
        lifecycle.shutdown_failure.map_or(Ok(()), Err)
    }

    fn release_request(&self) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            lifecycle.active_requests = lifecycle.active_requests.saturating_sub(1);
            if lifecycle.active_requests == 0 {
                self.drained.notify_all();
            }
        }
    }
}

impl<Services: HostServices> Drop for AppHost<Services> {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub struct HostRequest<'host, Services: HostServices> {
    host: &'host AppHost<Services>,
    caller: &'host CallerContext,
    request_id: Uuid,
}

impl<Services: HostServices> HostRequest<'_, Services> {
    pub fn caller(&self) -> &CallerContext {
        self.caller
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn services(&self) -> &Services {
        &self.host.services
    }
}

impl<Services: HostServices> Drop for HostRequest<'_, Services> {
    fn drop(&mut self) {
        self.host.release_request();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::*;
    use crate::{LocalIdentityClaim, LocalIdentityProvider};

    struct Identity;

    impl LocalIdentityProvider for Identity {
        fn verified_local_identity(&self) -> Result<LocalIdentityClaim, HostError> {
            Ok(LocalIdentityClaim {
                person_id: Uuid::new_v4(),
                device_id: "mac-local".into(),
            })
        }
    }

    struct Services(Arc<AtomicUsize>);

    impl HostServices for Services {
        fn shutdown(&self) -> Result<(), HostError> {
            self.0.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }

    #[test]
    fn verified_identity_is_host_owned_and_shutdown_is_idempotent() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let host = AppHost::bootstrap(Services(Arc::clone(&shutdowns)), &Identity).unwrap();
        let request_id = Uuid::new_v4();
        let request = host.request(request_id).unwrap();
        assert_eq!(request.request_id(), request_id);
        assert_eq!(request.caller().device_id(), "mac-local");
        assert!(!request.caller().person_id().is_nil());
        drop(request);
        host.shutdown().unwrap();
        host.shutdown().unwrap();
        assert_eq!(shutdowns.load(Ordering::Acquire), 1);
        assert_eq!(host.request(Uuid::new_v4()).err(), Some(HostError::Closing));
    }

    #[test]
    fn legacy_host_cannot_admit_app_wire_requests() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let host = AppHost::legacy(Services(shutdowns));
        assert_eq!(
            host.request(Uuid::new_v4()).err(),
            Some(HostError::UnsupportedCaller)
        );
    }

    #[test]
    fn shutdown_blocks_new_admission_and_drains_active_requests() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let host =
            Arc::new(AppHost::bootstrap(Services(Arc::clone(&shutdowns)), &Identity).unwrap());
        let request = host.request(Uuid::new_v4()).unwrap();
        let shutdown_started = Arc::new(AtomicBool::new(false));
        let shutdown_host = Arc::clone(&host);
        let shutdown_started_task = Arc::clone(&shutdown_started);
        let shutdown = std::thread::spawn(move || {
            shutdown_started_task.store(true, Ordering::Release);
            shutdown_host.shutdown()
        });
        while !shutdown_started.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            match host.request(Uuid::new_v4()) {
                Err(HostError::Closing) => break,
                Ok(admitted) => drop(admitted),
                Err(failure) => panic!("unexpected admission failure: {failure:?}"),
            }
            assert!(std::time::Instant::now() < deadline);
        }
        assert_eq!(shutdowns.load(Ordering::Acquire), 0);
        drop(request);
        shutdown.join().unwrap().unwrap();
        assert_eq!(shutdowns.load(Ordering::Acquire), 1);
    }

    #[test]
    fn product_path_bootstrap_uses_native_identity_while_test_paths_stay_legacy() {
        let root = tempfile::tempdir().unwrap();
        let person_id = Uuid::new_v4();
        let person_directory = root.path().join("people").join(person_id.to_string());
        std::fs::create_dir_all(&person_directory).unwrap();
        std::fs::write(root.path().join("local_device_id"), "local-device-1").unwrap();
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let host = AppHost::bootstrap_local_or_legacy(
            Services(Arc::clone(&shutdowns)),
            &person_directory.join("floe.db"),
        )
        .unwrap();
        let request = host.request(Uuid::new_v4()).unwrap();
        assert_eq!(request.caller().person_id(), person_id);
        assert_eq!(request.caller().device_id(), "local-device-1");
        drop(request);

        let legacy =
            AppHost::bootstrap_local_or_legacy(Services(shutdowns), &root.path().join("test.db"))
                .unwrap();
        assert_eq!(
            legacy.request(Uuid::new_v4()).err(),
            Some(HostError::UnsupportedCaller)
        );
    }
}
