//! Private native WebRTC signaling/handle boundary. No Dart JSON, provider
//! credential, automatic permission prompt or unmute is part of this port.
use floe_execution::{BoxFuture, Cancellation};
use floe_kernel::{AgentFailure, OwnerActor};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveMediaBinding {
    pub actor: OwnerActor,
    pub voice_session_id: Uuid,
    pub media_lease_id: Uuid,
    pub plan_id: Uuid,
}
impl LiveMediaBinding {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        if [self.voice_session_id, self.media_lease_id, self.plan_id]
            .iter()
            .any(Uuid::is_nil)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Signaling is private adapter/native data, never a product or persisted value.
/// It is deliberately neither Serialize nor Clone and diagnostic text is redacted.
pub struct LiveSessionDescription(String);
impl LiveSessionDescription {
    pub fn new(value: String) -> Result<Self, AgentFailure> {
        if value.len() > 65_536 || !value.starts_with("v=0\r\n") || value.contains('\0') {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for LiveSessionDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveSessionDescription[redacted]")
    }
}

/// A backend's observation after it has fenced and stopped the exact local
/// audio lease. This is deliberately process-local and is never a provider or
/// Conversation event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveMediaStopObservation {
    pub binding: LiveMediaBinding,
    pub capture_stopped: bool,
    pub playback_stopped: bool,
}
impl LiveMediaStopObservation {
    pub fn validate_for(&self, expected: &LiveMediaBinding) -> Result<(), AgentFailure> {
        self.binding.validate()?;
        if &self.binding != expected {
            return Err(AgentFailure::PolicyDenied);
        }
        if !self.capture_stopped || !self.playback_stopped {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(())
    }
}

/// Registry-issued receipt proving which host-owned lease returned a successful
/// backend Stop observation. The caller persists the corresponding
/// Voice NativeReceipt before acknowledging this in-memory receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveMediaStopReceipt {
    host_instance_id: Uuid,
    binding: LiveMediaBinding,
}
impl LiveMediaStopReceipt {
    pub fn host_instance_id(&self) -> Uuid {
        self.host_instance_id
    }

    pub fn binding(&self) -> &LiveMediaBinding {
        &self.binding
    }
}

/// Lookup only an already-prepared, muted, exact native lease. This cannot
/// manufacture permission or a substitute lease if the host/generation changed.
pub trait LiveMediaRegistry: Send + Sync {
    fn lease(&self, binding: &LiveMediaBinding) -> Result<Arc<dyn LiveMediaLease>, AgentFailure>;
}
pub trait LiveMediaLease: Send + Sync {
    fn binding(&self) -> &LiveMediaBinding;
    fn validate_current(&self) -> Result<(), AgentFailure>;
    /// Offer creation stays muted and must never ask for microphone permission.
    fn offer<'a>(
        &'a self,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<LiveSessionDescription, AgentFailure>>;
    /// Applying an answer does not unmute. The exact Voice media command owns it.
    fn apply_answer<'a>(
        &'a self,
        answer: LiveSessionDescription,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
    /// Immediate local fence. A late answer/unmute/write must stay rejected.
    /// A fence is not the host's completed microphone/speaker Stop receipt.
    fn fence(&self);
    fn lifetime_cancellation(&self) -> Cancellation;
    /// Submit the exact local Stop once. This has no caller cancellation token:
    /// once requested, privacy teardown remains owned even if its observer or
    /// screen goes away. Implementations must enforce the deadline.
    fn request_stop_once<'a>(
        &'a self,
        deadline: Instant,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
    /// Read back the exact backend state. Re-observation is safe after a lost
    /// Stop result and must report capture and playback independently.
    fn observe_stop<'a>(
        &'a self,
        deadline: Instant,
    ) -> BoxFuture<'a, Result<LiveMediaStopObservation, AgentFailure>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LeaseState {
    Prepared,
    Fenced,
    StopIssued,
    Stopped(LiveMediaStopReceipt),
}
struct RegisteredLease {
    lease: Arc<dyn LiveMediaLease>,
    state: Mutex<LeaseState>,
    stop_gate: tokio::sync::Mutex<()>,
}

/// One registry is retained by the native host process, across Rust runtime
/// generations. It admits one physical lease at a time and keeps a confirmed
/// Stop receipt until the Voice journal acknowledges it.
pub struct ProcessLiveMediaRegistry {
    host_instance_id: Uuid,
    leases: Mutex<HashMap<Uuid, Arc<RegisteredLease>>>,
    fenced_generations: Mutex<HashSet<(floe_kernel::PersonId, String, u64)>>,
}
impl Default for ProcessLiveMediaRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl ProcessLiveMediaRegistry {
    pub fn new() -> Self {
        Self {
            host_instance_id: Uuid::new_v4(),
            leases: Mutex::new(HashMap::new()),
            fenced_generations: Mutex::new(HashSet::new()),
        }
    }

    pub fn host_instance_id(&self) -> Uuid {
        self.host_instance_id
    }

    /// Snapshot exact process-held leases for application shutdown custody.
    /// Callers must still fence and stop each returned binding through this
    /// registry; the snapshot is never proof that a lease is still active.
    pub fn registered_bindings(&self) -> Vec<LiveMediaBinding> {
        lock_recover(&self.leases)
            .values()
            .map(|entry| entry.lease.binding().clone())
            .collect()
    }

    /// Retain a lease that the platform already prepared muted after the
    /// durable Start command was admitted. Replays may re-register only the
    /// same object with the same identity.
    pub fn register(&self, lease: Arc<dyn LiveMediaLease>) -> Result<(), AgentFailure> {
        let binding = lease.binding().clone();
        binding.validate()?;
        lease.validate_current()?;
        if lease.lifetime_cancellation().is_cancelled() {
            return Err(AgentFailure::Interrupted);
        }
        // Serialize admission with generation retirement. Keep the generation
        // guard while inserting so a fence cannot scan just before a stale
        // lease appears in the registry.
        let fenced_generations = lock_recover(&self.fenced_generations);
        let generation = (
            binding.actor.person_id,
            binding.actor.device_id.clone(),
            binding.actor.runtime_epoch,
        );
        if fenced_generations.contains(&generation) {
            return Err(AgentFailure::Interrupted);
        }
        let mut leases = lock_recover(&self.leases);
        if let Some(current) = leases.get(&binding.media_lease_id) {
            if current.lease.binding() == &binding && Arc::ptr_eq(&current.lease, &lease) {
                return Ok(());
            }
            return Err(AgentFailure::Conflict);
        }
        if !leases.is_empty() {
            return Err(AgentFailure::Conflict);
        }
        leases.insert(
            binding.media_lease_id,
            Arc::new(RegisteredLease {
                lease,
                state: Mutex::new(LeaseState::Prepared),
                stop_gate: tokio::sync::Mutex::new(()),
            }),
        );
        Ok(())
    }

    /// Fence every lease owned by one retiring runtime generation immediately.
    /// Physical Stop is separately awaited and receipted by `stop_and_confirm`.
    pub fn fence_generation(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        let mut fenced_generations = lock_recover(&self.fenced_generations);
        fenced_generations.insert((
            actor.person_id,
            actor.device_id.clone(),
            actor.runtime_epoch,
        ));
        let entries = lock_recover(&self.leases)
            .values()
            .filter(|entry| entry.lease.binding().actor == *actor)
            .cloned()
            .collect::<Vec<_>>();
        for entry in entries {
            fence_entry(&entry);
        }
        Ok(())
    }

    /// Fence one exact lease before its Voice Stop intent is journaled. The
    /// following stop_and_confirm call still owns physical Stop and readback.
    pub fn fence_binding(&self, binding: &LiveMediaBinding) -> Result<(), AgentFailure> {
        let entry = self.entry(binding)?;
        fence_entry(&entry);
        Ok(())
    }

    /// Rejoin the exact Stop operation. The spawned owner task survives a
    /// dropped observer; successful evidence stays available until ACKed after
    /// the Voice journal commits its NativeReceipt.
    pub async fn stop_and_confirm(
        self: &Arc<Self>,
        binding: &LiveMediaBinding,
        deadline: Instant,
    ) -> Result<LiveMediaStopReceipt, AgentFailure> {
        binding.validate()?;
        let entry = self.entry(binding)?;
        fence_entry(&entry);
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let host_instance_id = self.host_instance_id;
        let binding = binding.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        runtime.spawn(async move {
            let _gate = entry.stop_gate.lock().await;
            let result = async {
                let existing = match lock_recover(&entry.state).clone() {
                    LeaseState::Stopped(receipt) => Some(receipt),
                    LeaseState::Prepared | LeaseState::Fenced => None,
                    LeaseState::StopIssued => None,
                };
                if let Some(receipt) = existing {
                    Ok(receipt)
                } else {
                    if Instant::now() >= deadline {
                        return Err(AgentFailure::DeadlineExceeded);
                    }
                    let should_issue = {
                        let mut state = lock_recover(&entry.state);
                        match &*state {
                            LeaseState::Fenced => {
                                // Record intent before crossing the platform
                                // boundary. A lost result is observation-only;
                                // it never repeats the physical Stop command.
                                *state = LeaseState::StopIssued;
                                true
                            }
                            LeaseState::StopIssued | LeaseState::Stopped(_) => false,
                            LeaseState::Prepared => return Err(AgentFailure::Conflict),
                        }
                    };
                    if should_issue {
                        let _ = entry.lease.request_stop_once(deadline).await;
                    }
                    let observation = entry.lease.observe_stop(deadline).await?;
                    observation.validate_for(&binding)?;
                    let receipt = LiveMediaStopReceipt {
                        host_instance_id,
                        binding,
                    };
                    *lock_recover(&entry.state) = LeaseState::Stopped(receipt.clone());
                    Ok(receipt)
                }
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.unwrap_or(Err(AgentFailure::Interrupted))
    }

    /// Release a stopped lease only after the caller has durably committed its
    /// exact Voice receipt. A replay after an already completed ACK is harmless.
    pub fn acknowledge_stop(&self, receipt: &LiveMediaStopReceipt) -> Result<bool, AgentFailure> {
        if receipt.host_instance_id != self.host_instance_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut leases = lock_recover(&self.leases);
        let Some(entry) = leases.get(&receipt.binding.media_lease_id) else {
            return Ok(false);
        };
        let entry = entry.clone();
        if entry.lease.binding() != &receipt.binding {
            return Err(AgentFailure::PolicyDenied);
        }
        match &*lock_recover(&entry.state) {
            LeaseState::Stopped(saved) if saved == receipt => {
                leases.remove(&receipt.binding.media_lease_id);
                Ok(true)
            }
            LeaseState::Stopped(_) => Err(AgentFailure::Conflict),
            LeaseState::Prepared | LeaseState::Fenced | LeaseState::StopIssued => {
                Err(AgentFailure::Conflict)
            }
        }
    }

    fn entry(&self, binding: &LiveMediaBinding) -> Result<Arc<RegisteredLease>, AgentFailure> {
        let leases = lock_recover(&self.leases);
        let entry = leases
            .get(&binding.media_lease_id)
            .cloned()
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if entry.lease.binding() != binding {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(entry)
    }
}
impl LiveMediaRegistry for ProcessLiveMediaRegistry {
    fn lease(&self, binding: &LiveMediaBinding) -> Result<Arc<dyn LiveMediaLease>, AgentFailure> {
        binding.validate()?;
        let entry = self.entry(binding)?;
        if !matches!(&*lock_recover(&entry.state), LeaseState::Prepared) {
            return Err(AgentFailure::Interrupted);
        }
        entry.lease.validate_current()?;
        Ok(entry.lease.clone())
    }
}

fn fence_entry(entry: &RegisteredLease) {
    let should_fence = {
        let mut state = lock_recover(&entry.state);
        match &*state {
            LeaseState::Prepared => {
                *state = LeaseState::Fenced;
                true
            }
            LeaseState::Fenced | LeaseState::StopIssued | LeaseState::Stopped(_) => false,
        }
    };
    if should_fence {
        entry.lease.fence();
        entry
            .lease
            .lifetime_cancellation()
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    fn actor(epoch: u64) -> OwnerActor {
        OwnerActor {
            person_id: floe_kernel::PersonId(Uuid::new_v4()),
            device_id: "native-live-test-device".into(),
            runtime_epoch: epoch,
        }
    }

    fn binding(actor: OwnerActor) -> LiveMediaBinding {
        LiveMediaBinding {
            actor,
            voice_session_id: Uuid::new_v4(),
            media_lease_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
        }
    }

    struct FakeLease {
        binding: LiveMediaBinding,
        cancellation: Cancellation,
        stop_mode: AtomicU8,
        stop_requests: AtomicUsize,
        stop_observations: AtomicUsize,
    }
    impl FakeLease {
        fn new(binding: LiveMediaBinding) -> Self {
            Self {
                binding,
                cancellation: Cancellation::new(),
                stop_mode: AtomicU8::new(2),
                stop_requests: AtomicUsize::new(0),
                stop_observations: AtomicUsize::new(0),
            }
        }
    }
    impl LiveMediaLease for FakeLease {
        fn binding(&self) -> &LiveMediaBinding {
            &self.binding
        }
        fn validate_current(&self) -> Result<(), AgentFailure> {
            if self.cancellation.is_cancelled() {
                Err(AgentFailure::Interrupted)
            } else {
                Ok(())
            }
        }
        fn offer<'a>(
            &'a self,
            _: Instant,
            _: &'a Cancellation,
        ) -> BoxFuture<'a, Result<LiveSessionDescription, AgentFailure>> {
            Box::pin(async { LiveSessionDescription::new("v=0\r\n".into()) })
        }
        fn apply_answer<'a>(
            &'a self,
            _: LiveSessionDescription,
            _: Instant,
            _: &'a Cancellation,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async { Ok(()) })
        }
        fn fence(&self) {
            self.cancellation.cancel();
        }
        fn lifetime_cancellation(&self) -> Cancellation {
            self.cancellation.clone()
        }
        fn request_stop_once<'a>(&'a self, _: Instant) -> BoxFuture<'a, Result<(), AgentFailure>> {
            self.stop_requests.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }
        fn observe_stop<'a>(
            &'a self,
            _: Instant,
        ) -> BoxFuture<'a, Result<LiveMediaStopObservation, AgentFailure>> {
            self.stop_observations.fetch_add(1, Ordering::SeqCst);
            let mode = self.stop_mode.load(Ordering::SeqCst);
            let binding = self.binding.clone();
            Box::pin(async move {
                Ok(LiveMediaStopObservation {
                    binding,
                    capture_stopped: mode >= 1,
                    playback_stopped: mode >= 2,
                })
            })
        }
    }

    #[test]
    fn signaling_is_bounded_and_redacted() {
        let value = LiveSessionDescription::new("v=0\r\ns=private-address\r\n".into()).unwrap();
        assert!(!format!("{value:?}").contains("private-address"));
        assert!(LiveSessionDescription::new("not SDP".into()).is_err());
        assert!(LiveSessionDescription::new(format!("v=0\r\n{}", "x".repeat(65_536))).is_err());
        assert!(LiveSessionDescription::new("v=0\r\n\0".into()).is_err());
    }

    #[tokio::test]
    async fn process_registry_owns_exact_lease_and_requires_capture_and_playback_stop() {
        let registry = Arc::new(ProcessLiveMediaRegistry::new());
        let call_binding = binding(actor(1));
        let lease = Arc::new(FakeLease::new(call_binding.clone()));
        let lease_handle: Arc<dyn LiveMediaLease> = lease.clone();
        registry.register(lease_handle.clone()).unwrap();
        registry.register(lease_handle.clone()).unwrap();
        assert!(Arc::ptr_eq(
            &registry.lease(&call_binding).unwrap(),
            &lease_handle
        ));

        let mut foreign = call_binding.clone();
        foreign.actor.runtime_epoch = 2;
        assert!(matches!(
            registry.lease(&foreign),
            Err(AgentFailure::PolicyDenied)
        ));

        let competing = Arc::new(FakeLease::new(binding(actor(1))));
        let competing: Arc<dyn LiveMediaLease> = competing;
        assert_eq!(registry.register(competing), Err(AgentFailure::Conflict));

        registry.fence_generation(&call_binding.actor).unwrap();
        assert!(matches!(
            registry.lease(&call_binding),
            Err(AgentFailure::Interrupted)
        ));
        lease.stop_mode.store(1, Ordering::SeqCst);
        assert_eq!(
            registry
                .stop_and_confirm(
                    &call_binding,
                    Instant::now() + std::time::Duration::from_secs(1)
                )
                .await,
            Err(AgentFailure::CapabilityUnavailable),
            "capture stopped without playback confirmation is not a Stop receipt"
        );
        assert_eq!(lease.stop_requests.load(Ordering::SeqCst), 1);
        assert_eq!(lease.stop_observations.load(Ordering::SeqCst), 1);

        lease.stop_mode.store(2, Ordering::SeqCst);
        let receipt = registry
            .stop_and_confirm(
                &call_binding,
                Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .unwrap();
        let replay = registry
            .stop_and_confirm(
                &call_binding,
                Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(receipt, replay);
        assert_eq!(lease.stop_requests.load(Ordering::SeqCst), 1);
        assert_eq!(lease.stop_observations.load(Ordering::SeqCst), 2);
        assert_eq!(receipt.host_instance_id(), registry.host_instance_id());
        assert_eq!(receipt.binding(), &call_binding);
        let other_host = ProcessLiveMediaRegistry::new();
        assert_eq!(
            other_host.acknowledge_stop(&receipt),
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            registry.register(Arc::new(FakeLease::new(binding(actor(3))))),
            Err(AgentFailure::Conflict),
            "a stopped lease stays owned until its Voice receipt is durable"
        );
        assert!(registry.acknowledge_stop(&receipt).unwrap());
        assert!(!registry.acknowledge_stop(&receipt).unwrap());
        assert!(registry.lease(&call_binding).is_err());
        assert_eq!(
            registry.register(Arc::new(FakeLease::new(binding(
                call_binding.actor.clone()
            )))),
            Err(AgentFailure::Interrupted),
            "a retired runtime generation cannot register a late physical lease"
        );

        let next = Arc::new(FakeLease::new(binding(actor(2))));
        registry.register(next).unwrap();
    }

    #[tokio::test]
    async fn dropped_stop_observer_does_not_cancel_process_owned_teardown() {
        let registry = Arc::new(ProcessLiveMediaRegistry::new());
        let call_binding = binding(actor(1));
        let lease = Arc::new(FakeLease::new(call_binding.clone()));
        let lease_handle: Arc<dyn LiveMediaLease> = lease.clone();
        registry.register(lease_handle).unwrap();

        let child = registry.clone();
        let exact = call_binding.clone();
        let observer = tokio::spawn(async move {
            child
                .stop_and_confirm(&exact, Instant::now() + std::time::Duration::from_secs(1))
                .await
        });
        observer.abort();

        let receipt = registry
            .stop_and_confirm(
                &call_binding,
                Instant::now() + std::time::Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(receipt.binding(), &call_binding);
        assert_eq!(lease.stop_requests.load(Ordering::SeqCst), 1);
        assert_eq!(lease.stop_observations.load(Ordering::SeqCst), 1);
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use apple::AppleLiveMediaLease;
