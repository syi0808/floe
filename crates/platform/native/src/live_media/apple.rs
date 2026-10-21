//! Apple WebRTC lease bridge into the process-owned Swift host.
use super::{LiveMediaBinding, LiveMediaLease, LiveMediaStopObservation, LiveSessionDescription};
use floe_execution::{BoxFuture, CancelReason, Cancellation};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::time::Instant;
use uuid::Uuid;

const MAX_REQUEST_BYTES: usize = 131_072;
const MAX_RESPONSE_BYTES: usize = 131_072;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeBinding {
    person_id: String,
    device_id: String,
    runtime_epoch: u64,
    voice_session_id: Uuid,
    media_lease_id: Uuid,
    plan_id: Uuid,
}
impl From<&LiveMediaBinding> for NativeBinding {
    fn from(value: &LiveMediaBinding) -> Self {
        Self {
            person_id: value.actor.person_id.to_string(),
            device_id: value.actor.device_id.clone(),
            runtime_epoch: value.actor.runtime_epoch,
            voice_session_id: value.voice_session_id,
            media_lease_id: value.media_lease_id,
            plan_id: value.plan_id,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeReply {
    schema_version: u32,
    status: String,
    host_instance_id: String,
    binding: NativeBinding,
    offer_sdp: Option<String>,
    capture_stopped: Option<bool>,
    playback_stopped: Option<bool>,
    failure: Option<String>,
}

static APPLE_LIVE_MEDIA: crate::ByteCall = crate::ByteCall::new(crate::NativeLibrary {
    relative_path: "Frameworks/libfloe_local_model.dylib",
    invoke_symbol: c"floe_live_media",
    release_symbol: c"floe_live_media_free",
    #[cfg(target_os = "macos")]
    bundle_parents: crate::MACOS_BUNDLE_ROOT,
    #[cfg(target_os = "ios")]
    bundle_parents: crate::BUNDLE_SIBLING,
});

pub struct AppleLiveMediaLease {
    binding: LiveMediaBinding,
    host_instance_id: Uuid,
    lifetime: Cancellation,
    fenced: AtomicBool,
    stop_issued: AtomicBool,
    stop_confirmed: AtomicBool,
}
impl AppleLiveMediaLease {
    /// Prepare a PeerConnection without a local audio track or permission prompt.
    pub fn prepare_muted(
        binding: LiveMediaBinding,
        host_instance_id: Uuid,
    ) -> Result<Arc<Self>, AgentFailure> {
        binding.validate()?;
        if host_instance_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let lease = Arc::new(Self {
            binding,
            host_instance_id,
            lifetime: Cancellation::new(),
            fenced: AtomicBool::new(false),
            stop_issued: AtomicBool::new(false),
            stop_confirmed: AtomicBool::new(false),
        });
        lease.invoke("prepare_muted", None, None)?;
        Ok(lease)
    }

    fn request(
        &self,
        operation: &'static str,
        deadline_ms: Option<u64>,
        answer_sdp: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "operation": operation,
            "host_instance_id": self.host_instance_id.to_string(),
            "binding": NativeBinding::from(&self.binding),
            "deadline_ms": deadline_ms,
            "answer_sdp": answer_sdp,
        })
    }

    fn invoke(
        &self,
        operation: &'static str,
        deadline_ms: Option<u64>,
        answer_sdp: Option<&str>,
    ) -> Result<NativeReply, AgentFailure> {
        let request = serde_json::to_vec(&self.request(operation, deadline_ms, answer_sdp))
            .map_err(|_| AgentFailure::InvalidInput)?;
        if request.len() > MAX_REQUEST_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let bytes = APPLE_LIVE_MEDIA
            .call(&request, MAX_RESPONSE_BYTES)
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let reply: NativeReply =
            serde_json::from_slice(&bytes).map_err(|_| AgentFailure::PolicyDenied)?;
        self.validate_reply(&reply)?;
        if reply.status != "ok" {
            return Err(map_failure(reply.failure.as_deref()));
        }
        Ok(reply)
    }

    fn validate_reply(&self, reply: &NativeReply) -> Result<(), AgentFailure> {
        if reply.schema_version != 1
            || reply.host_instance_id != self.host_instance_id.to_string()
            || reply.binding != NativeBinding::from(&self.binding)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    fn remaining_ms(deadline: Instant) -> Result<u64, AgentFailure> {
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis();
        u64::try_from(remaining)
            .ok()
            .filter(|value| *value > 0 && *value <= 60_000)
            .ok_or(AgentFailure::DeadlineExceeded)
    }

    fn ensure_active(&self) -> Result<(), AgentFailure> {
        if self.fenced.load(Ordering::Acquire) || self.lifetime.is_cancelled() {
            Err(AgentFailure::Interrupted)
        } else {
            Ok(())
        }
    }

    fn spawn_call<'a>(
        &'a self,
        operation: &'static str,
        deadline: Instant,
        answer_sdp: Option<String>,
        require_offer: bool,
    ) -> BoxFuture<'a, Result<NativeReply, AgentFailure>> {
        Box::pin(async move {
            self.ensure_active()?;
            let request = serde_json::to_vec(&self.request(
                operation,
                Some(Self::remaining_ms(deadline)?),
                answer_sdp.as_deref(),
            ))
            .map_err(|_| AgentFailure::InvalidInput)?;
            let reply = tokio::task::spawn_blocking(move || {
                let bytes = APPLE_LIVE_MEDIA
                    .call(&request, MAX_RESPONSE_BYTES)
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                serde_json::from_slice::<NativeReply>(&bytes)
                    .map_err(|_| AgentFailure::PolicyDenied)
            })
            .await
            .map_err(|_| AgentFailure::Interrupted)??;
            self.validate_reply(&reply)?;
            if reply.status != "ok" {
                return Err(map_failure(reply.failure.as_deref()));
            }
            if require_offer && reply.offer_sdp.is_none() {
                return Err(AgentFailure::PolicyDenied);
            }
            self.ensure_active()?;
            Ok(reply)
        })
    }
}
impl LiveMediaLease for AppleLiveMediaLease {
    fn binding(&self) -> &LiveMediaBinding {
        &self.binding
    }

    fn validate_current(&self) -> Result<(), AgentFailure> {
        self.ensure_active()?;
        self.invoke("validate", None, None).map(|_| ())
    }

    fn offer<'a>(
        &'a self,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<LiveSessionDescription, AgentFailure>> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let reply = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
                result = self.spawn_call("offer", deadline, None, true) => result?,
            };
            LiveSessionDescription::new(reply.offer_sdp.ok_or(AgentFailure::PolicyDenied)?)
        })
    }

    fn apply_answer<'a>(
        &'a self,
        answer: LiveSessionDescription,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
                result = self.spawn_call("apply_answer", deadline, Some(answer.as_str().to_owned()), false) =>
                    result.map(|_| ()),
            }
        })
    }

    fn fence(&self) {
        if self.fenced.swap(true, Ordering::AcqRel) {
            return;
        }
        self.lifetime.cancel_with_reason(CancelReason::OwnerDropped);
        let _ = self.invoke("fence", None, None);
    }

    fn lifetime_cancellation(&self) -> Cancellation {
        self.lifetime.clone()
    }

    fn request_stop_once<'a>(
        &'a self,
        deadline: Instant,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            let deadline_ms = Self::remaining_ms(deadline)?;
            if self.stop_issued.swap(true, Ordering::AcqRel) {
                return Ok(());
            }
            let request = serde_json::to_vec(&self.request("stop_once", Some(deadline_ms), None))
                .map_err(|_| AgentFailure::InvalidInput)?;
            let reply = tokio::task::spawn_blocking(move || {
                let bytes = APPLE_LIVE_MEDIA
                    .call(&request, MAX_RESPONSE_BYTES)
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                serde_json::from_slice::<NativeReply>(&bytes)
                    .map_err(|_| AgentFailure::PolicyDenied)
            })
            .await
            .map_err(|_| AgentFailure::Interrupted)??;
            self.validate_reply(&reply)?;
            if reply.status != "ok" {
                return Err(map_failure(reply.failure.as_deref()));
            }
            Ok(())
        })
    }

    fn observe_stop<'a>(
        &'a self,
        deadline: Instant,
    ) -> BoxFuture<'a, Result<LiveMediaStopObservation, AgentFailure>> {
        Box::pin(async move {
            let request = serde_json::to_vec(&self.request(
                "observe_stop",
                Some(Self::remaining_ms(deadline)?),
                None,
            ))
            .map_err(|_| AgentFailure::InvalidInput)?;
            let reply = tokio::task::spawn_blocking(move || {
                let bytes = APPLE_LIVE_MEDIA
                    .call(&request, MAX_RESPONSE_BYTES)
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                serde_json::from_slice::<NativeReply>(&bytes)
                    .map_err(|_| AgentFailure::PolicyDenied)
            })
            .await
            .map_err(|_| AgentFailure::Interrupted)??;
            self.validate_reply(&reply)?;
            if reply.status != "ok" {
                return Err(map_failure(reply.failure.as_deref()));
            }
            let observation = LiveMediaStopObservation {
                binding: self.binding.clone(),
                capture_stopped: reply.capture_stopped.unwrap_or(false),
                playback_stopped: reply.playback_stopped.unwrap_or(false),
            };
            observation.validate_for(&self.binding)?;
            self.stop_confirmed.store(true, Ordering::Release);
            Ok(observation)
        })
    }
}
impl Drop for AppleLiveMediaLease {
    fn drop(&mut self) {
        if self.stop_confirmed.load(Ordering::Acquire) {
            let _ = self.invoke("release", None, None);
        } else {
            self.fence();
        }
    }
}

fn map_failure(code: Option<&str>) -> AgentFailure {
    match code {
        Some("invalid_input") => AgentFailure::InvalidInput,
        Some("conflict") => AgentFailure::Conflict,
        Some("not_found") => AgentFailure::NotFound,
        Some("deadline_exceeded") => AgentFailure::DeadlineExceeded,
        Some("cancelled") => AgentFailure::Cancelled,
        Some("policy_denied") => AgentFailure::PolicyDenied,
        _ => AgentFailure::CapabilityUnavailable,
    }
}
