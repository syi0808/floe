//! Normalized final events become Voice custody and canonical Conversation input
//! or Work. Hints are lookup data, never new User instructions or action approval.
use crate::{VoiceCallRecord, VoiceJournal};
use floe_conversation::{
    AppendSurfaceInput, MAX_TURN_TEXT_BYTES, SubmitSurfaceWork, SurfaceInput, SurfaceInputOrigin,
    SurfaceInputReceipt, SurfaceWorkReceipt, WorkOrigin, normalize_turn_text,
};
use floe_conversation_contract::{MessageId, SurfaceAnchor};
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, CommandFailure, CommandId, OwnerActor};
use floe_model_contract::{LiveEventKind, LiveProviderEvent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn event_kind(event: &LiveProviderEvent) -> Result<&'static str, AgentFailure> {
    event.validate().map_err(|_| AgentFailure::InvalidInput)?;
    Ok(
        match event.durable_key().ok_or(AgentFailure::InvalidInput)?.kind {
            LiveEventKind::PersonUtterance => "person_final",
            LiveEventKind::AssistantUtterance => "assistant_final",
            LiveEventKind::Delegation => "delegation",
        },
    )
}
pub fn event_digest(event: &LiveProviderEvent) -> Result<[u8; 32], AgentFailure> {
    event_kind(event)?;
    let mut h = Sha256::new();
    h.update(b"floe.voice.event.v1\0");
    h.update(serde_json::to_vec(event).map_err(|_| AgentFailure::InvalidInput)?);
    Ok(h.finalize().into())
}
pub fn input_lookup_digest(voice: Uuid, text: &str) -> Result<[u8; 32], AgentFailure> {
    if voice.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    let text = normalize_turn_text(text)?;
    let mut h = Sha256::new();
    h.update(b"floe.voice.input-lookup.v1\0");
    h.update(voice.as_bytes());
    h.update(text.as_bytes());
    Ok(h.finalize().into())
}
fn id(voice: Uuid, domain: &[u8], key: &[u8]) -> Uuid {
    let mut bytes = domain.to_vec();
    bytes.extend_from_slice(key);
    Uuid::new_v5(&voice, &bytes)
}
pub fn input_command_id(voice: Uuid, utterance: &str) -> CommandId {
    CommandId::from_uuid(id(
        voice,
        b"floe.voice.input-command\0",
        utterance.as_bytes(),
    ))
    .expect("UUID v5")
}
pub fn work_command_id(voice: Uuid, input: MessageId) -> CommandId {
    CommandId::from_uuid(id(
        voice,
        b"floe.voice.input-work\0",
        input.as_uuid().as_bytes(),
    ))
    .expect("UUID v5")
}
fn input_message_id(voice: Uuid, utterance: &str) -> MessageId {
    MessageId::from_uuid(id(
        voice,
        b"floe.voice.input-message\0",
        utterance.as_bytes(),
    ))
    .expect("UUID v5")
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceInputRejection {
    TooLarge,
    InvalidText,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceUnroutedReason {
    MissingInput,
    AmbiguousInput,
    SessionStopped,
    QueueFull,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VoiceEventOutcome {
    Input {
        command_id: CommandId,
    },
    RejectedInput {
        reason: VoiceInputRejection,
    },
    PresentationRetained {},
    Work {
        input_command_id: CommandId,
        work_command_id: CommandId,
    },
    Unrouted {
        reason: VoiceUnroutedReason,
        input_command_id: Option<CommandId>,
    },
}
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceEventRecord {
    pub voice_session_id: Uuid,
    pub sequence: u64,
    pub admitted_revision: u64,
    pub state_digest: [u8; 32],
    pub event: LiveProviderEvent,
    pub outcome: VoiceEventOutcome,
}
impl std::fmt::Debug for VoiceEventRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceEventRecord")
            .field("sequence", &self.sequence)
            .field("kind", &event_kind(&self.event))
            .finish_non_exhaustive()
    }
}
impl VoiceEventRecord {
    pub fn validate(&self, call: &VoiceCallRecord) -> Result<(), AgentFailure> {
        call.validate()?;
        event_kind(&self.event)?;
        if self.voice_session_id != call.intent.binding.voice_session_id
            || self.sequence == 0
            || self.sequence > i64::MAX as u64
            || self.admitted_revision == 0
            || self.admitted_revision > call.revision
            || self.state_digest == [0; 32]
            || (self.admitted_revision == call.revision && self.state_digest != call.digest()?)
        {
            return Err(AgentFailure::InvalidInput);
        }
        match (&self.event, &self.outcome) {
            (
                LiveProviderEvent::PersonUtteranceFinal { utterance_id, text },
                VoiceEventOutcome::Input { command_id },
            ) if *command_id == input_command_id(self.voice_session_id, utterance_id)
                && normalize_turn_text(text).is_ok() => {}
            (
                LiveProviderEvent::PersonUtteranceFinal { text, .. },
                VoiceEventOutcome::RejectedInput { reason },
            ) if input_rejection(text) == Some(*reason) => {}
            (
                LiveProviderEvent::AssistantUtteranceFinal { .. },
                VoiceEventOutcome::PresentationRetained {},
            ) => {}
            (
                LiveProviderEvent::DelegationRequested { .. },
                VoiceEventOutcome::Work {
                    input_command_id,
                    work_command_id,
                },
            ) if input_command_id.is_valid() && work_command_id.is_valid() => {}
            (
                LiveProviderEvent::DelegationRequested { .. },
                VoiceEventOutcome::Unrouted {
                    reason,
                    input_command_id,
                },
            ) => {
                let known = matches!(
                    reason,
                    VoiceUnroutedReason::SessionStopped | VoiceUnroutedReason::QueueFull
                );
                if known != input_command_id.is_some()
                    || input_command_id.is_some_and(|id| !id.is_valid())
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            _ => return Err(AgentFailure::InvalidInput),
        }
        Ok(())
    }
}
pub fn input_rejection(text: &str) -> Option<VoiceInputRejection> {
    if text.trim_matches(char::is_whitespace).len() > MAX_TURN_TEXT_BYTES {
        Some(VoiceInputRejection::TooLarge)
    } else if normalize_turn_text(text).is_err() {
        Some(VoiceInputRejection::InvalidText)
    } else {
        None
    }
}
#[derive(Clone, Debug)]
pub enum VoiceInputCorrelation {
    Missing,
    Ambiguous,
    Exact(SurfaceInputReceipt),
}
pub enum VoiceIngressPlan {
    AppendInput(AppendSurfaceInput),
    Retain(VoiceEventOutcome),
    SubmitWork {
        input: SurfaceInputReceipt,
        request: SubmitSurfaceWork,
    },
    ExistingWork {
        input: SurfaceInputReceipt,
        work: SurfaceWorkReceipt,
    },
}
pub fn validate_input_link(
    call: &VoiceCallRecord,
    event: &LiveProviderEvent,
    input: &SurfaceInputReceipt,
) -> Result<(), AgentFailure> {
    input.validate()?;
    let SurfaceInputOrigin::Voice {
        voice_session_id,
        utterance_id,
    } = &input.input.origin
    else {
        return Err(AgentFailure::PolicyDenied);
    };
    if input.person_id != call.intent.binding.actor.person_id
        || input.device_id != call.intent.binding.actor.device_id
        || input.through.session_id != call.intent.binding.surface_session_id
        || *voice_session_id != call.intent.binding.voice_session_id
        || input.command_id != input_command_id(*voice_session_id, utterance_id)
        || input.input.message_id != input_message_id(*voice_session_id, utterance_id)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    match event {
        LiveProviderEvent::PersonUtteranceFinal {
            utterance_id: id,
            text,
        } if id == utterance_id && normalize_turn_text(text)? == input.input.text => Ok(()),
        LiveProviderEvent::DelegationRequested {
            utterance_ref: Some(id),
            ..
        } if id == utterance_id => Ok(()),
        LiveProviderEvent::DelegationRequested {
            utterance_ref: None,
            input_hint: Some(hint),
            ..
        } if normalize_turn_text(hint)? == input.input.text => Ok(()),
        _ => Err(AgentFailure::PolicyDenied),
    }
}
pub fn validate_work_link(
    call: &VoiceCallRecord,
    input: &SurfaceInputReceipt,
    work: &SurfaceWorkReceipt,
) -> Result<(), AgentFailure> {
    work.validate()?;
    if work.person_id != input.person_id
        || work.device_id != input.device_id
        || work.input_id != input.input.message_id
        || work.anchor != input.through
        || work.command_id
            != work_command_id(call.intent.binding.voice_session_id, input.input.message_id)
        || !matches!(&work.origin,WorkOrigin::Voice{voice_session_id,..} if *voice_session_id==call.intent.binding.voice_session_id)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
pub fn plan_ingress(
    call: &VoiceCallRecord,
    actor: &OwnerActor,
    event: &LiveProviderEvent,
    head: SurfaceAnchor,
    correlation: VoiceInputCorrelation,
    existing: Option<SurfaceWorkReceipt>,
) -> Result<VoiceIngressPlan, AgentFailure> {
    call.validate()?;
    event_kind(event)?;
    actor.validate()?;
    head.validate()?;
    if actor != &call.intent.binding.actor
        || head.session_id != call.intent.binding.surface_session_id
        || call.ingress_closed
        || call.session.remote() == crate::RemoteState::NotStarted
    {
        return Err(AgentFailure::PolicyDenied);
    }
    match event {
        LiveProviderEvent::PersonUtteranceFinal { utterance_id, text } => {
            if !call.session.capture_started() {
                return Err(AgentFailure::PolicyDenied);
            }
            if let Some(reason) = input_rejection(text) {
                return Ok(VoiceIngressPlan::Retain(VoiceEventOutcome::RejectedInput {
                    reason,
                }));
            }
            Ok(VoiceIngressPlan::AppendInput(AppendSurfaceInput {
                command_id: input_command_id(call.intent.binding.voice_session_id, utterance_id),
                expected: head,
                input: SurfaceInput {
                    message_id: input_message_id(
                        call.intent.binding.voice_session_id,
                        utterance_id,
                    ),
                    text: normalize_turn_text(text)?,
                    origin: SurfaceInputOrigin::Voice {
                        voice_session_id: call.intent.binding.voice_session_id,
                        utterance_id: utterance_id.clone(),
                    },
                },
            }))
        }
        LiveProviderEvent::AssistantUtteranceFinal { .. } => Ok(VoiceIngressPlan::Retain(
            VoiceEventOutcome::PresentationRetained {},
        )),
        LiveProviderEvent::DelegationRequested { delegation_id, .. } => {
            if !call.session.capture_started() {
                return Err(AgentFailure::PolicyDenied);
            }
            let input = match correlation {
                VoiceInputCorrelation::Exact(input) => input,
                VoiceInputCorrelation::Missing => {
                    return Ok(VoiceIngressPlan::Retain(VoiceEventOutcome::Unrouted {
                        reason: VoiceUnroutedReason::MissingInput,
                        input_command_id: None,
                    }));
                }
                VoiceInputCorrelation::Ambiguous => {
                    return Ok(VoiceIngressPlan::Retain(VoiceEventOutcome::Unrouted {
                        reason: VoiceUnroutedReason::AmbiguousInput,
                        input_command_id: None,
                    }));
                }
            };
            validate_input_link(call, event, &input)?;
            if let Some(work) = existing {
                validate_work_link(call, &input, &work)?;
                return Ok(VoiceIngressPlan::ExistingWork { input, work });
            }
            if call.session.stop_requested() {
                return Ok(VoiceIngressPlan::Retain(VoiceEventOutcome::Unrouted {
                    reason: VoiceUnroutedReason::SessionStopped,
                    input_command_id: Some(input.command_id),
                }));
            }
            let request = SubmitSurfaceWork {
                command_id: work_command_id(
                    call.intent.binding.voice_session_id,
                    input.input.message_id,
                ),
                anchor: input.through,
                input_id: input.input.message_id,
                origin: WorkOrigin::Voice {
                    voice_session_id: call.intent.binding.voice_session_id,
                    delegation_id: delegation_id.clone(),
                },
            };
            Ok(VoiceIngressPlan::SubmitWork { input, request })
        }
        _ => Err(AgentFailure::InvalidInput),
    }
}
#[derive(Clone, Debug)]
pub struct VoiceIngressAdmission {
    pub record: VoiceEventRecord,
    pub input: Option<SurfaceInputReceipt>,
    pub work: Option<SurfaceWorkReceipt>,
    pub replayed: bool,
}

pub trait VoiceIngressJournal: VoiceJournal {
    fn ingest<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice_session_id: Uuid,
        event: LiveProviderEvent,
    ) -> BoxFuture<'a, Result<VoiceIngressAdmission, CommandFailure<AgentFailure>>>;
    fn read_event<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice_session_id: Uuid,
        event_kind: &'a str,
        event_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<VoiceIngressAdmission>, AgentFailure>>;
}
