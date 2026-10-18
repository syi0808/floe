//! Durable one-shot presentation intent. Voice correlates a canonical Manager
//! result; Inference alone authorizes its data and performs the provider write.
use crate::{MediaState, RemoteState, VoiceBinding, VoiceCallRecord, VoiceIngressJournal};
use floe_context_contract::LiveProjectionSource;
use floe_execution::BoxFuture;
use floe_inference::LiveAppendIntent;
use floe_kernel::{AgentFailure, CommandFailure, CommandId, OwnerActor};
use floe_model_contract::{LiveAppendKind, LiveAppendPayload};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn digest(value: &impl Serialize) -> Result<[u8; 32], AgentFailure> {
    let mut hash = Sha256::new();
    hash.update(b"floe.voice.append.v1\0");
    hash.update(serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?);
    Ok(hash.finalize().into())
}
/// Stable across request retries and later Surface heads. A new request UUID
/// cannot turn the same result/delegation/presentation kind into a fresh effect.
pub fn append_operation_id(
    voice_session_id: Uuid,
    source: &LiveProjectionSource,
    kind: LiveAppendKind,
    delegation_id: &str,
) -> Result<Uuid, AgentFailure> {
    source.validate()?;
    let LiveProjectionSource::SurfaceResult {
        surface_message_id, ..
    } = source
    else {
        return Err(AgentFailure::InvalidInput);
    };
    if voice_session_id.is_nil()
        || matches!(kind, LiveAppendKind::Instruction)
        || delegation_id.is_empty()
        || delegation_id.len() > floe_model_contract::MAX_LIVE_ID_BYTES
        || delegation_id.trim() != delegation_id
        || delegation_id.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let identity = serde_json::to_vec(&(surface_message_id, kind, delegation_id))
        .map_err(|_| AgentFailure::InvalidInput)?;
    let mut bytes = b"floe.voice.append-operation.v1\0".to_vec();
    bytes.extend_from_slice(&identity);
    Ok(Uuid::new_v5(&voice_session_id, &bytes))
}
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceAppendIntent {
    pub binding: VoiceBinding,
    pub source: LiveProjectionSource,
    pub projection_digest: [u8; 32],
    pub payload: LiveAppendPayload,
}
impl VoiceAppendIntent {
    pub fn from_admitted(
        binding: VoiceBinding,
        request: &LiveAppendIntent,
    ) -> Result<Self, AgentFailure> {
        request.validate()?;
        if binding.actor != request.actor
            || binding.plan_id != request.plan_id
            || binding.voice_session_id != request.voice_session_id
            || binding.surface_session_id != request.surface_session_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let value = Self {
            binding,
            source: request.source.clone(),
            projection_digest: request.projection_digest,
            payload: request.payload.clone(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.binding.validate()?;
        self.source.validate()?;
        self.payload
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        let delegation = self
            .payload
            .delegation_id
            .as_deref()
            .ok_or(AgentFailure::InvalidInput)?;
        if self.projection_digest == [0; 32]
            || self.source.through().session_id != self.binding.surface_session_id
            || self.payload.operation_id
                != append_operation_id(
                    self.binding.voice_session_id,
                    &self.source,
                    self.payload.kind,
                    delegation,
                )?
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn validate_for_call(&self, call: &VoiceCallRecord) -> Result<(), AgentFailure> {
        self.validate()?;
        call.validate()?;
        if self.binding != call.intent.binding {
            return Err(AgentFailure::PolicyDenied);
        }
        if self.payload.text.len() as u64 > call.intent.limits.append_bytes {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        if call.ingress_closed
            || call.usage.is_some()
            || call.session.stop_requested()
            || call.session.remote() != RemoteState::Connected
            || !matches!(
                call.session.media(),
                MediaState::Muted | MediaState::Capturing
            )
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        self.validate()?;
        digest(self)
    }
}
impl std::fmt::Debug for VoiceAppendIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceAppendIntent")
            .field("kind", &self.payload.kind)
            .field("text_bytes", &self.payload.text.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VoiceAppendOutcome {
    Written,
    NotSent { failure: AgentFailure },
    Uncertain { failure: AgentFailure },
}
/// A missing outcome after interruption remains unknown. No recovery path may
/// turn it into NotSent, replay it, or claim acceptance/playback from Written.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceAppendReceipt {
    pub operation_id: Uuid,
    pub intent_digest: [u8; 32],
    pub outcome: VoiceAppendOutcome,
}
impl VoiceAppendReceipt {
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        if self.operation_id.is_nil() || self.intent_digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        digest(self)
    }
}
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceAppendRecord {
    pub intent: VoiceAppendIntent,
    pub admitted_revision: u64,
    pub state_digest: [u8; 32],
    pub work_command_id: CommandId,
    pub receipt: Option<VoiceAppendReceipt>,
}
impl VoiceAppendRecord {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.intent.validate()?;
        if self.admitted_revision == 0
            || self.admitted_revision > i64::MAX as u64
            || self.state_digest == [0; 32]
            || !self.work_command_id.is_valid()
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(receipt) = &self.receipt {
            if receipt.operation_id != self.intent.payload.operation_id
                || receipt.intent_digest != self.intent.digest()?
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}
pub struct VoiceAppendAdmission {
    pub record: VoiceAppendRecord,
    pub replayed: bool,
}
pub trait VoiceOutboundJournal: VoiceIngressJournal {
    fn claim_append<'a>(
        &'a self,
        actor: &'a OwnerActor,
        intent: VoiceAppendIntent,
    ) -> BoxFuture<'a, Result<VoiceAppendAdmission, CommandFailure<AgentFailure>>>;
    fn settle_append<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice_session_id: Uuid,
        receipt: VoiceAppendReceipt,
    ) -> BoxFuture<'a, Result<VoiceAppendRecord, CommandFailure<AgentFailure>>>;
    fn read_append<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice_session_id: Uuid,
        operation_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<VoiceAppendRecord>, AgentFailure>>;
}

struct ControllerCustody<'a> {
    controller: &'a crate::VoiceController,
    intent: std::sync::Mutex<Option<VoiceAppendIntent>>,
    fresh: std::sync::atomic::AtomicBool,
}
impl floe_inference::LiveAppendCustody for ControllerCustody<'_> {
    fn claim_append<'a>(
        &'a self,
        request: &'a LiveAppendIntent,
        _: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            if self.controller.cancellation().is_cancelled() {
                return Err(AgentFailure::Interrupted);
            }
            let intent =
                VoiceAppendIntent::from_admitted(self.controller.binding.clone(), request)?;
            *self.intent.lock().unwrap() = Some(intent.clone());
            let admission = crate::custody::retry_local(
                || async {
                    self.controller
                        .journal
                        .claim_append(&self.controller.binding.actor, intent.clone())
                        .await
                        .map_err(CommandFailure::into_failure)
                },
                crate::custody::busy,
            )
            .await?;
            let fresh = !admission.replayed;
            self.fresh
                .store(fresh, std::sync::atomic::Ordering::Release);
            Ok(fresh)
        })
    }
}
impl crate::VoiceController {
    /// This future belongs inside the registered driver, not a UI observer.
    /// A durable receipt can outlive retirement; a replay never starts a write.
    pub async fn deliver_result(
        &self,
        call: &mut floe_inference::AuthorizedLiveSession,
        request: floe_inference::LiveAppendRequest,
    ) -> Result<VoiceAppendReceipt, AgentFailure> {
        if call.plan().actor != self.binding.actor
            || call.plan().operation_id != self.binding.plan_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let custody = ControllerCustody {
            controller: self,
            intent: std::sync::Mutex::new(None),
            fresh: std::sync::atomic::AtomicBool::new(false),
        };
        let result = call.append(request, &custody).await;
        let intent = custody.intent.into_inner().unwrap();
        let Some(intent) = intent else {
            return Err(result.err().unwrap_or(AgentFailure::Conflict));
        };
        if !custody.fresh.load(std::sync::atomic::Ordering::Acquire) {
            // An existing unresolved intent might have reached the provider in
            // another attempt. Never overwrite it with a synthetic NotSent.
            let saved = crate::custody::retry_local(
                || {
                    self.journal.read_append(
                        &self.binding.actor,
                        self.binding.voice_session_id,
                        intent.payload.operation_id,
                    )
                },
                crate::custody::busy,
            )
            .await?;
            return match saved {
                Some(record) if record.intent == intent => record
                    .receipt
                    .ok_or(result.err().unwrap_or(AgentFailure::Conflict)),
                _ => Err(result.err().unwrap_or(AgentFailure::Conflict)),
            };
        }
        let outcome = match result {
            Ok(floe_inference::LiveAppendWrite::Written) => VoiceAppendOutcome::Written,
            Ok(floe_inference::LiveAppendWrite::Uncertain(failure)) => {
                VoiceAppendOutcome::Uncertain { failure }
            }
            Err(failure) => VoiceAppendOutcome::NotSent { failure },
        };
        let receipt = VoiceAppendReceipt {
            operation_id: intent.payload.operation_id,
            intent_digest: intent.digest()?,
            outcome,
        };
        let saved = crate::custody::retry_local(
            || async {
                self.journal
                    .settle_append(
                        &self.binding.actor,
                        self.binding.voice_session_id,
                        receipt.clone(),
                    )
                    .await
                    .map_err(CommandFailure::into_failure)
            },
            crate::custody::busy,
        )
        .await?;
        saved.receipt.ok_or(AgentFailure::StorageUnavailable)
    }
}
