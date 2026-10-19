//! Voice owns durable lifecycle/ordering. These records are not microphone,
//! source-processing, budget-debit or provider credentials/capabilities.
use crate::{MediaCommand, MediaState, VoiceBinding, VoiceSession};
use floe_context_contract::{LiveContextProjection, LiveProjectionSource};
use floe_execution::{
    BoxFuture,
    budget::{ModelAttemptReceipt, ModelReservationCeiling},
};
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor};
use floe_model_contract::{
    LiveEndEvidence, LiveModelCapabilities, LiveModelPlan, LiveTransportLimits, ModelBindingDigest,
    ModelSelectionCommitment, ProcessingBoundary,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use uuid::Uuid;

fn digest<T: Serialize>(domain: &[u8], value: &T) -> Result<[u8; 32], AgentFailure> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?;
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(bytes);
    Ok(hash.finalize().into())
}

/// The prepared selection/context identity, without provider wire or credentials.
/// Full context is separately held by Context/Inference for live authorization;
/// a persisted digest cannot recreate a consumed transport after restart.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceStartIntent {
    pub binding: VoiceBinding,
    pub model_binding: ModelBindingDigest,
    pub selection_commitment: ModelSelectionCommitment,
    pub capabilities: LiveModelCapabilities,
    pub limits: LiveTransportLimits,
    pub boundary: ProcessingBoundary,
    pub projection_id: Uuid,
    pub projection_revision: u64,
    pub context_digest: [u8; 32],
    pub source: LiveProjectionSource,
    pub reservation: ModelReservationCeiling,
}
impl VoiceStartIntent {
    pub fn prepare(
        binding: VoiceBinding,
        plan: &LiveModelPlan,
        projection: &LiveContextProjection,
        reservation: ModelReservationCeiling,
    ) -> Result<Self, AgentFailure> {
        binding.validate()?;
        plan.validate()?;
        if binding.actor != plan.actor || binding.plan_id != plan.operation_id {
            return Err(AgentFailure::PolicyDenied);
        }
        projection.validate_for_plan(plan, binding.voice_session_id, binding.surface_session_id)?;
        if projection.payload.byte_len() as u64 > plan.limits.initial_context_bytes {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        let value = Self {
            binding,
            model_binding: plan.binding_digest,
            selection_commitment: plan.selection_commitment,
            capabilities: plan.capabilities.clone(),
            limits: plan.limits.clone(),
            boundary: plan.boundary,
            projection_id: projection.projection_id,
            projection_revision: projection.projection_revision,
            context_digest: projection.digest()?,
            source: projection.source.clone(),
            reservation,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.binding.validate()?;
        self.source.validate()?;
        self.reservation.validate()?;
        self.capabilities
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.limits
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.selection_commitment.validate()?;
        if self.model_binding.0 == [0; 32]
            || self.context_digest == [0; 32]
            || self.projection_id.is_nil()
            || self.projection_revision == 0
            || self.projection_revision > i64::MAX as u64
            || !self
                .capabilities
                .includes(&LiveModelCapabilities::conversational())
            || !matches!(self.source, LiveProjectionSource::SurfacePrefix { .. })
            || self.source.through().session_id != self.binding.surface_session_id
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        self.validate()?;
        digest(b"floe.voice.start.v1\0", self)
    }
}
impl fmt::Debug for VoiceStartIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceStartIntent")
            .field("boundary", &self.boundary)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceCallRecord {
    pub intent: VoiceStartIntent,
    pub revision: u64,
    pub session: VoiceSession,
    pub ingress_closed: bool,
    pub usage: Option<ModelAttemptReceipt>,
}
impl VoiceCallRecord {
    pub fn prepare(intent: VoiceStartIntent) -> Result<Self, AgentFailure> {
        intent.validate()?;
        let session = VoiceSession::new(intent.binding.clone())?;
        let value = Self {
            intent,
            revision: 1,
            session,
            ingress_closed: false,
            usage: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.intent.validate()?;
        self.session.validate()?;
        if let Some(receipt) = &self.usage {
            receipt.validate()?;
            if receipt.attempt_id != self.intent.binding.plan_id
                || (receipt.accounting.unknown_tokens
                    && receipt.charged_tokens > self.intent.reservation.tokens)
                || (receipt.accounting.unknown_cost
                    && receipt.charged_cost_micros > self.intent.reservation.cost_micros)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if self.ingress_closed && !self.session.stop_requested() {
            return Err(AgentFailure::InvalidInput);
        }
        if self.revision == 1
            && (self.usage.is_some()
                || self.session.media() != MediaState::Preparing
                || self.session.remote() != crate::RemoteState::NotStarted
                || self.session.stop_requested()
                || self.session.pending_media_command().is_none_or(|command| {
                    command.kind != crate::MediaCommandKind::PrepareMuted || command.generation != 1
                }))
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.revision == 0
            || self.revision > i64::MAX as u64
            || self.session.binding() != &self.intent.binding
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    /// Conservative recovery after a lost owner scope, never an observed bill.
    /// A recorded dispatch intent may have crossed the provider boundary.
    pub fn unresolved_usage(&self) -> ModelAttemptReceipt {
        if let Some(receipt) = self.usage {
            return receipt;
        }
        let dispatched = self.session.remote() != crate::RemoteState::NotStarted;
        ModelAttemptReceipt {
            attempt_id: self.intent.binding.plan_id,
            charged_tokens: if dispatched {
                self.intent.reservation.tokens
            } else {
                0
            },
            charged_cost_micros: if dispatched {
                self.intent.reservation.cost_micros
            } else {
                0
            },
            accounting: floe_execution::budget::ModelAccounting {
                observed_tokens: None,
                observed_cost_micros: None,
                unknown_tokens: dispatched,
                unknown_cost: dispatched,
            },
            dispatched,
        }
    }

    pub fn authorize_mutation(
        &self,
        actor: &OwnerActor,
        kind: &VoiceMutationKind,
    ) -> Result<(), AgentFailure> {
        actor.validate()?;
        if actor.person_id != self.intent.binding.actor.person_id
            || actor.device_id != self.intent.binding.actor.device_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let recovery_accounting = matches!(kind, VoiceMutationKind::SettleUsage { receipt }
            if self.session.stop_requested() && self.ingress_closed && *receipt == self.unresolved_usage());
        if actor.runtime_epoch != self.intent.binding.actor.runtime_epoch
            && !recovery_accounting
            && !matches!(
                kind,
                VoiceMutationKind::Stop {}
                    | VoiceMutationKind::RemoteEnd { .. }
                    | VoiceMutationKind::NativeReceipt {
                        command: MediaCommand {
                            kind: crate::MediaCommandKind::Stop,
                            ..
                        },
                        state: MediaState::Stopped
                    }
            )
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub fn apply(
        &self,
        actor: &OwnerActor,
        request: &VoiceMutation,
    ) -> Result<(Self, VoiceMutationReceipt), AgentFailure> {
        self.validate()?;
        request.validate()?;
        actor.validate()?;
        self.authorize_mutation(actor, &request.kind)?;
        if request.voice_session_id != self.intent.binding.voice_session_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let recovering = actor.runtime_epoch != self.intent.binding.actor.runtime_epoch;
        let policy_actor = self.intent.binding.actor.clone();
        if request.expected_revision != self.revision {
            return Err(AgentFailure::Conflict);
        }
        let mut next = self.clone();
        let mut remote_dispatch = false;
        let media_command = match &request.kind {
            VoiceMutationKind::SettleUsage { receipt } => {
                receipt.validate()?;
                if receipt.attempt_id != self.intent.binding.plan_id {
                    return Err(AgentFailure::PolicyDenied);
                }
                if self.usage.is_some_and(|saved| saved != *receipt) {
                    return Err(AgentFailure::Conflict);
                }
                if !receipt.dispatched
                    && (self.session.capture_started()
                        || matches!(
                            self.session.remote(),
                            crate::RemoteState::Connected | crate::RemoteState::Closed
                        ))
                {
                    return Err(AgentFailure::Conflict);
                }
                next.usage = Some(*receipt);
                // Terminal accounting closes admission to new capture/work,
                // without claiming the actual native or provider stop completed.
                Some(next.session.request_stop(&policy_actor)?)
            }
            VoiceMutationKind::CloseIngress {} => {
                if !next.session.stop_requested() {
                    return Err(AgentFailure::Conflict);
                }
                next.ingress_closed = true;
                None
            }
            VoiceMutationKind::NativeReceipt { command, state } => {
                next.session.observe_media(command, *state)?;
                None
            }
            VoiceMutationKind::BeginRemoteDispatch {} => {
                next.session.begin_remote_dispatch(&policy_actor)?;
                remote_dispatch = true;
                None
            }
            VoiceMutationKind::RemoteReady { binding } => {
                next.session.observe_remote_ready(binding)?;
                None
            }
            VoiceMutationKind::Unmute {} => Some(next.session.request_unmute(&policy_actor)?),
            VoiceMutationKind::Mute {} => Some(next.session.request_mute(&policy_actor)?),
            VoiceMutationKind::Stop {} => Some({
                let stop = next.session.request_stop(&policy_actor)?;
                if recovering {
                    next.ingress_closed = true;
                    if next.session.remote() != crate::RemoteState::NotStarted {
                        next.session
                            .observe_remote_end(&self.intent.binding, false)?;
                    }
                }
                stop
            }),
            VoiceMutationKind::RemoteEnd { binding, evidence } => Some(
                next.session
                    .observe_remote_end(binding, *evidence == LiveEndEvidence::ProviderConfirmed)?,
            ),
        };
        if recovering {
            next.ingress_closed = true;
        }
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        next.validate()?;
        let receipt = VoiceMutationReceipt {
            voice_session_id: request.voice_session_id,
            command_id: request.command_id,
            request_digest: request.digest(actor)?,
            revision: next.revision,
            media_command,
            remote_dispatch,
            state_digest: next.digest()?,
            caller_epoch: actor.runtime_epoch,
        };
        next.validate_receipt(&receipt)?;
        Ok((next, receipt))
    }
    pub fn digest(&self) -> Result<[u8; 32], AgentFailure> {
        self.validate()?;
        digest(b"floe.voice.state.v1\0", self)
    }
    pub fn validate_receipt(&self, receipt: &VoiceMutationReceipt) -> Result<(), AgentFailure> {
        receipt.validate()?;
        self.validate()?;
        if receipt.voice_session_id != self.intent.binding.voice_session_id
            || receipt.revision > self.revision
        {
            return Err(AgentFailure::InvalidInput);
        }
        if receipt.revision == self.revision && receipt.state_digest != self.digest()? {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(command) = &receipt.media_command {
            if command.binding != self.intent.binding
                || command.generation > self.session.command_generation()
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}
impl fmt::Debug for VoiceCallRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceCallRecord")
            .field("revision", &self.revision)
            .field("media", &self.session.media())
            .field("remote", &self.session.remote())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VoiceMutationKind {
    SettleUsage {
        receipt: ModelAttemptReceipt,
    },
    CloseIngress {},
    NativeReceipt {
        command: MediaCommand,
        state: MediaState,
    },
    BeginRemoteDispatch {},
    RemoteReady {
        binding: VoiceBinding,
    },
    Unmute {},
    Mute {},
    Stop {},
    RemoteEnd {
        binding: VoiceBinding,
        evidence: LiveEndEvidence,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceMutation {
    pub voice_session_id: Uuid,
    pub command_id: Uuid,
    pub expected_revision: u64,
    pub kind: VoiceMutationKind,
}
impl VoiceMutation {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.voice_session_id.is_nil()
            || self.command_id.is_nil()
            || self.expected_revision == 0
            || self.expected_revision > i64::MAX as u64
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn digest(&self, actor: &OwnerActor) -> Result<[u8; 32], AgentFailure> {
        self.validate()?;
        actor.validate()?;
        digest(
            b"floe.voice.mutation.v1\0",
            &(actor.person_id, &actor.device_id, actor.runtime_epoch, self),
        )
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceMutationReceipt {
    pub voice_session_id: Uuid,
    pub command_id: Uuid,
    pub request_digest: [u8; 32],
    pub revision: u64,
    pub media_command: Option<MediaCommand>,
    /// Records what the original transition did. A replay never authorizes a
    /// second start; use the enclosing admission's fresh-dispatch predicate.
    pub remote_dispatch: bool,
    pub state_digest: [u8; 32],
    pub caller_epoch: u64,
}
impl VoiceMutationReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.voice_session_id.is_nil()
            || self.command_id.is_nil()
            || self.request_digest == [0; 32]
            || self.state_digest == [0; 32]
            || self.caller_epoch == 0
            || self.revision < 2
            || self.revision > i64::MAX as u64
            || (self.remote_dispatch && self.media_command.is_some())
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(command) = &self.media_command {
            command.binding.validate()?;
            if command.generation == 0 {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct VoiceStartAdmission {
    pub record: VoiceCallRecord,
    pub replayed: bool,
}
impl VoiceStartAdmission {
    /// Initial admission stamp stays immutable even when the returned current
    /// record has advanced through native/provider observations.
    pub fn admission_revision(&self) -> u64 {
        1
    }
}
#[derive(Clone, Debug)]
pub struct VoiceMutationAdmission {
    pub record: VoiceCallRecord,
    pub receipt: VoiceMutationReceipt,
    pub replayed: bool,
}
impl VoiceMutationAdmission {
    /// Ordering fact only; Inference/Access and the prepared transport must
    /// still enforce actual dispatch, source, budget and media prerequisites.
    pub fn fresh_remote_dispatch(&self) -> bool {
        !self.replayed && self.receipt.remote_dispatch
    }
}

pub trait VoiceJournal: Send + Sync {
    fn reserve_start<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command: crate::VoiceStartCommand,
    ) -> BoxFuture<'a, Result<crate::VoiceStartCommandAdmission, CommandFailure<AgentFailure>>>;
    fn read_start<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<crate::VoiceStartCommandRecord>, AgentFailure>>;
    /// Cancels only pre-binding preparation. A bound call requires normal Stop.
    fn cancel_start<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command: crate::VoiceStartCommand,
    ) -> BoxFuture<'a, Result<crate::VoiceStartCommandRecord, CommandFailure<AgentFailure>>>;

    fn admit_start<'a>(
        &'a self,
        actor: &'a OwnerActor,
        intent: VoiceStartIntent,
    ) -> BoxFuture<'a, Result<VoiceStartAdmission, CommandFailure<AgentFailure>>>;
    fn read_call<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice_session_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<VoiceCallRecord>, AgentFailure>>;
    fn mutate<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: VoiceMutation,
    ) -> BoxFuture<'a, Result<VoiceMutationAdmission, CommandFailure<AgentFailure>>>;
}
