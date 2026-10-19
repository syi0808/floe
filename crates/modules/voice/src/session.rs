use floe_kernel::{AgentFailure, OwnerActor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceBinding {
    #[serde(with = "actor_wire")]
    pub actor: OwnerActor,
    pub surface_session_id: Uuid,
    pub voice_session_id: Uuid,
    pub media_lease_id: Uuid,
    pub plan_id: Uuid,
}
impl VoiceBinding {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        if [
            self.surface_session_id,
            self.voice_session_id,
            self.media_lease_id,
            self.plan_id,
        ]
        .iter()
        .any(Uuid::is_nil)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommandKind {
    PrepareMuted,
    Unmute,
    Mute,
    Stop,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCommand {
    pub binding: VoiceBinding,
    pub generation: u64,
    pub kind: MediaCommandKind,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaState {
    Preparing,
    Muted,
    Capturing,
    Stopped,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteState {
    NotStarted,
    Dispatching,
    Connected,
    CloseRequested,
    Closed,
    Unknown,
}

/// One live local media lease. Unknown remote finalization does not become a
/// claim of local capture, playback, remote usage settlement or zero charge.
#[derive(Clone, Deserialize, Serialize)]
#[serde(try_from = "StoredVoiceSession", into = "StoredVoiceSession")]
pub struct VoiceSession {
    binding: VoiceBinding,
    media: MediaState,
    remote: RemoteState,
    command: MediaCommand,
    command_confirmed: bool,
    stop_requested: bool,
    capture_started: bool,
    input_active: bool,
    output_active: bool,
}
// Persistence representation only. Activity is ephemeral and is never restored
// as evidence that a microphone or speaker is currently active after restart.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredVoiceSession {
    binding: VoiceBinding,
    media: MediaState,
    remote: RemoteState,
    command: MediaCommand,
    command_confirmed: bool,
    stop_requested: bool,
    capture_started: bool,
}
impl From<VoiceSession> for StoredVoiceSession {
    fn from(value: VoiceSession) -> Self {
        Self {
            binding: value.binding,
            media: value.media,
            remote: value.remote,
            command: value.command,
            command_confirmed: value.command_confirmed,
            stop_requested: value.stop_requested,
            capture_started: value.capture_started,
        }
    }
}
impl TryFrom<StoredVoiceSession> for VoiceSession {
    type Error = &'static str;
    fn try_from(value: StoredVoiceSession) -> Result<Self, Self::Error> {
        let session = Self {
            binding: value.binding,
            media: value.media,
            remote: value.remote,
            command: value.command,
            command_confirmed: value.command_confirmed,
            stop_requested: value.stop_requested,
            capture_started: value.capture_started,
            input_active: false,
            output_active: false,
        };
        session
            .validate()
            .map_err(|_| "invalid stored voice lifecycle")?;
        Ok(session)
    }
}
pub(crate) mod actor_wire {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        person_id: floe_kernel::PersonId,
        device_id: String,
        runtime_epoch: u64,
    }
    pub fn serialize<S: serde::Serializer>(actor: &OwnerActor, s: S) -> Result<S::Ok, S::Error> {
        Fields {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            runtime_epoch: actor.runtime_epoch,
        }
        .serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<OwnerActor, D::Error> {
        let fields = Fields::deserialize(d)?;
        let actor = OwnerActor {
            person_id: fields.person_id,
            device_id: fields.device_id,
            runtime_epoch: fields.runtime_epoch,
        };
        actor
            .validate()
            .map_err(|_| serde::de::Error::custom("invalid stored voice actor"))?;
        Ok(actor)
    }
}
impl VoiceSession {
    /// Validate reachable persisted policy state, not current native readiness.
    /// Reopening a valid row never restores permission, an active driver or a
    /// live media receipt in a different runtime epoch.
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.binding.validate()?;
        if self.command.binding != self.binding
            || self.command.generation == 0
            || (self.stop_requested != (self.command.kind == MediaCommandKind::Stop))
            || (self.stop_requested
                && matches!(
                    self.remote,
                    RemoteState::Dispatching | RemoteState::Connected
                ))
            || (matches!(
                self.remote,
                RemoteState::CloseRequested | RemoteState::Closed | RemoteState::Unknown
            ) && !self.stop_requested)
            || (self.media == MediaState::Capturing
                && matches!(
                    self.remote,
                    RemoteState::NotStarted | RemoteState::Dispatching
                ))
            || (self.media == MediaState::Preparing
                && (self.command_confirmed
                    || self.remote != RemoteState::NotStarted
                    || !matches!(
                        self.command.kind,
                        MediaCommandKind::PrepareMuted | MediaCommandKind::Stop
                    )))
            || (self.media == MediaState::Stopped
                && (!self.stop_requested || !self.command_confirmed))
            || (self.command.kind == MediaCommandKind::PrepareMuted && self.command.generation != 1)
            || (self.command.kind == MediaCommandKind::Unmute
                && (self.remote != RemoteState::Connected || self.stop_requested))
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.command_confirmed {
            let expected = match self.command.kind {
                MediaCommandKind::PrepareMuted | MediaCommandKind::Mute => MediaState::Muted,
                MediaCommandKind::Unmute => MediaState::Capturing,
                MediaCommandKind::Stop => MediaState::Stopped,
            };
            if self.media != expected {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if (self.media == MediaState::Capturing && !self.capture_started)
            || (self.capture_started
                && matches!(
                    self.remote,
                    RemoteState::NotStarted | RemoteState::Dispatching
                ))
        {
            return Err(AgentFailure::InvalidInput);
        }
        if (self.input_active && self.media != MediaState::Capturing)
            || (self.output_active
                && matches!(self.media, MediaState::Preparing | MediaState::Stopped))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn new(binding: VoiceBinding) -> Result<Self, AgentFailure> {
        binding.validate()?;
        let command = MediaCommand {
            binding: binding.clone(),
            generation: 1,
            kind: MediaCommandKind::PrepareMuted,
        };
        Ok(Self {
            binding,
            media: MediaState::Preparing,
            remote: RemoteState::NotStarted,
            command,
            command_confirmed: false,
            stop_requested: false,
            capture_started: false,
            input_active: false,
            output_active: false,
        })
    }
    pub fn binding(&self) -> &VoiceBinding {
        &self.binding
    }
    pub fn media(&self) -> MediaState {
        self.media
    }
    pub fn remote(&self) -> RemoteState {
        self.remote
    }
    pub fn pending_media_command(&self) -> Option<&MediaCommand> {
        (!self.command_confirmed).then_some(&self.command)
    }
    pub fn input_active(&self) -> bool {
        self.input_active
    }
    pub fn output_active(&self) -> bool {
        self.output_active
    }
    pub fn local_lease_released(&self) -> bool {
        self.media == MediaState::Stopped && self.command_confirmed
    }
    pub fn capture_started(&self) -> bool {
        self.capture_started
    }

    pub(crate) fn command_generation(&self) -> u64 {
        self.command.generation
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    fn check(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.validate()?;
        actor.validate()?;
        if actor != &self.binding.actor {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    fn issue(&mut self, kind: MediaCommandKind) -> Result<MediaCommand, AgentFailure> {
        if !self.command_confirmed && self.command.kind == kind {
            return Ok(self.command.clone());
        }
        let generation = self
            .command
            .generation
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        self.command = MediaCommand {
            binding: self.binding.clone(),
            generation,
            kind,
        };
        self.command_confirmed = false;
        Ok(self.command.clone())
    }
    /// Only an exact native lease/generation receipt advances confirmed media.
    /// Intent alone never means the microphone or speaker actually stopped.
    pub fn observe_media(
        &mut self,
        command: &MediaCommand,
        state: MediaState,
    ) -> Result<(), AgentFailure> {
        self.validate()?;
        if command != &self.command {
            return Err(AgentFailure::Conflict);
        }
        let expected = match command.kind {
            MediaCommandKind::PrepareMuted | MediaCommandKind::Mute => MediaState::Muted,
            MediaCommandKind::Unmute => MediaState::Capturing,
            MediaCommandKind::Stop => MediaState::Stopped,
        };
        if state != expected {
            return Err(AgentFailure::Conflict);
        }
        self.media = state;
        if state == MediaState::Capturing {
            self.capture_started = true;
        }
        self.command_confirmed = true;
        if state != MediaState::Capturing {
            self.input_active = false;
        }
        if state == MediaState::Stopped {
            self.output_active = false;
        }
        Ok(())
    }
    /// This transition is local policy only. The application must first persist
    /// the exact startup intent and consume Inference/Access admission.
    pub fn begin_remote_dispatch(&mut self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.check(actor)?;
        if self.stop_requested
            || self.media != MediaState::Muted
            || !self.command_confirmed
            || self.remote != RemoteState::NotStarted
        {
            return Err(AgentFailure::Conflict);
        }
        self.remote = RemoteState::Dispatching;
        Ok(())
    }
    pub fn observe_remote_ready(&mut self, binding: &VoiceBinding) -> Result<(), AgentFailure> {
        self.validate()?;
        if binding != &self.binding
            || self.stop_requested
            || !matches!(
                self.remote,
                RemoteState::Dispatching | RemoteState::Connected
            )
        {
            return Err(AgentFailure::Conflict);
        }
        self.remote = RemoteState::Connected;
        // Readiness never issues Unmute, restores capture or clears a mute.
        Ok(())
    }
    pub fn request_unmute(&mut self, actor: &OwnerActor) -> Result<MediaCommand, AgentFailure> {
        self.check(actor)?;
        if self.stop_requested || self.remote != RemoteState::Connected {
            return Err(AgentFailure::Conflict);
        }
        if !self.command_confirmed && self.command.kind == MediaCommandKind::Unmute {
            return Ok(self.command.clone());
        }
        if self.media != MediaState::Muted || !self.command_confirmed {
            return Err(AgentFailure::Conflict);
        }
        self.issue(MediaCommandKind::Unmute)
    }
    pub fn request_mute(&mut self, actor: &OwnerActor) -> Result<MediaCommand, AgentFailure> {
        self.check(actor)?;
        if self.stop_requested || self.media == MediaState::Stopped {
            return Err(AgentFailure::Conflict);
        }
        if self.media == MediaState::Preparing
            && self.command.kind == MediaCommandKind::PrepareMuted
        {
            return Ok(self.command.clone());
        }
        self.issue(MediaCommandKind::Mute)
    }
    /// Stop fences later unmute immediately but retains actual media state until
    /// an exact native stop receipt. It emits no Conversation/Run cancellation.
    pub fn request_stop(&mut self, actor: &OwnerActor) -> Result<MediaCommand, AgentFailure> {
        self.check(actor)?;
        if self.local_lease_released() {
            return Ok(self.command.clone());
        }
        let command = self.issue(MediaCommandKind::Stop)?;
        self.stop_requested = true;
        if matches!(
            self.remote,
            RemoteState::Dispatching | RemoteState::Connected
        ) {
            self.remote = RemoteState::CloseRequested;
        }
        Ok(command)
    }
    /// The adapter supplies positive remote evidence or explicitly retains its
    /// uncertainty. Neither state says playback occurred or usage is finalized.
    pub fn observe_remote_end(
        &mut self,
        binding: &VoiceBinding,
        confirmed: bool,
    ) -> Result<MediaCommand, AgentFailure> {
        self.validate()?;
        if binding != &self.binding || self.remote == RemoteState::NotStarted {
            return Err(AgentFailure::Conflict);
        }
        // First establish the local stop intent. Remote disappearance is not
        // permission to leave capture running while waiting for another event.
        let stop = self.request_stop(&self.binding.actor.clone())?;
        if self.remote != RemoteState::Closed {
            self.remote = if confirmed {
                RemoteState::Closed
            } else {
                RemoteState::Unknown
            };
        }
        Ok(stop)
    }
    /// Full duplex observations are independent; no Listening|Speaking XOR.
    pub fn observe_activity(
        &mut self,
        binding: &VoiceBinding,
        input: bool,
        output: bool,
    ) -> Result<(), AgentFailure> {
        self.validate()?;
        if binding != &self.binding
            || self.local_lease_released()
            || self.remote != RemoteState::Connected
            || self.media == MediaState::Preparing
            || self.stop_requested
            || (input && self.media != MediaState::Capturing)
        {
            return Err(AgentFailure::Conflict);
        }
        self.input_active = input;
        self.output_active = output;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_kernel::PersonId;
    fn session() -> VoiceSession {
        VoiceSession::new(VoiceBinding {
            actor: OwnerActor {
                person_id: PersonId::new(),
                device_id: "fixture-device".into(),
                runtime_epoch: 1,
            },
            surface_session_id: Uuid::new_v4(),
            voice_session_id: Uuid::new_v4(),
            media_lease_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
        })
        .unwrap()
    }
    fn connect(s: &mut VoiceSession) {
        let prepare = s.pending_media_command().unwrap().clone();
        s.observe_media(&prepare, MediaState::Muted).unwrap();
        s.begin_remote_dispatch(&s.binding.actor.clone()).unwrap();
        s.observe_remote_ready(&s.binding.clone()).unwrap();
    }
    #[test]
    fn startup_requires_confirmed_mute_and_ready_does_not_unmute() {
        let mut s = session();
        let actor = s.binding.actor.clone();
        assert_eq!(s.begin_remote_dispatch(&actor), Err(AgentFailure::Conflict));
        connect(&mut s);
        assert_eq!(s.media(), MediaState::Muted);
        assert!(s.pending_media_command().is_none());
        s.observe_remote_ready(&s.binding.clone()).unwrap();
        assert_eq!(s.media(), MediaState::Muted);
        assert!(!s.input_active());
    }
    #[test]
    fn uncertain_remote_close_does_not_hold_a_confirmed_stopped_media_lease() {
        let mut s = session();
        connect(&mut s);
        let actor = s.binding.actor.clone();
        let unmute = s.request_unmute(&actor).unwrap();
        s.observe_media(&unmute, MediaState::Capturing).unwrap();
        s.observe_activity(&s.binding.clone(), true, true).unwrap();
        assert!(s.input_active() && s.output_active());
        let stop = s.request_stop(&actor).unwrap();
        assert_eq!(
            s.media(),
            MediaState::Capturing,
            "intent is not actual stop"
        );
        assert!(!s.local_lease_released());
        s.observe_remote_end(&s.binding.clone(), false).unwrap();
        assert_eq!(s.remote(), RemoteState::Unknown);
        assert!(!s.local_lease_released());
        s.observe_media(&stop, MediaState::Stopped).unwrap();
        assert!(s.local_lease_released());
        assert!(!s.input_active() && !s.output_active());
        assert_eq!(s.remote(), RemoteState::Unknown);
        assert_eq!(s.request_unmute(&actor), Err(AgentFailure::Conflict));
    }
    #[test]
    fn late_unmute_and_ready_cannot_revive_a_stopping_session() {
        let mut s = session();
        connect(&mut s);
        let actor = s.binding.actor.clone();
        let unmute = s.request_unmute(&actor).unwrap();
        assert_eq!(s.request_unmute(&actor).unwrap(), unmute);
        let stop = s.request_stop(&actor).unwrap();
        assert_eq!(s.request_stop(&actor).unwrap(), stop);
        assert_eq!(
            s.observe_media(&unmute, MediaState::Capturing),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            s.observe_remote_ready(&s.binding.clone()),
            Err(AgentFailure::Conflict)
        );
        s.observe_media(&stop, MediaState::Stopped).unwrap();
        s.observe_media(&stop, MediaState::Stopped).unwrap();
    }
    #[test]
    fn actor_epoch_and_media_lease_are_exact_fences() {
        let mut s = session();
        connect(&mut s);
        let actor = s.binding.actor.clone();
        for variant in 0..3 {
            let mut foreign = actor.clone();
            match variant {
                0 => foreign.runtime_epoch += 1,
                1 => foreign.device_id = "other-device".into(),
                _ => foreign.person_id = PersonId::new(),
            }
            assert_eq!(s.request_unmute(&foreign), Err(AgentFailure::PolicyDenied));
            assert_eq!(s.request_stop(&foreign), Err(AgentFailure::PolicyDenied));
        }
        let command = s.request_unmute(&actor).unwrap();
        let mut foreign = command.clone();
        foreign.binding.media_lease_id = Uuid::new_v4();
        assert_eq!(
            s.observe_media(&foreign, MediaState::Capturing),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            s.observe_media(&command, MediaState::Stopped),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(s.media(), MediaState::Muted);
    }
    #[test]
    fn remote_confirmation_cannot_release_unknown_local_media() {
        let mut s = session();
        connect(&mut s);
        let actor = s.binding.actor.clone();
        s.request_stop(&actor).unwrap();
        s.observe_remote_end(&s.binding.clone(), true).unwrap();
        assert_eq!(s.remote(), RemoteState::Closed);
        assert!(!s.local_lease_released());
        assert!(s.pending_media_command().is_some());
    }
    #[test]
    fn stop_before_dispatch_prevents_remote_start() {
        let mut s = session();
        let actor = s.binding.actor.clone();
        let original = s.pending_media_command().unwrap().clone();
        let stop = s.request_stop(&actor).unwrap();
        assert_eq!(
            s.observe_media(&original, MediaState::Muted),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(s.begin_remote_dispatch(&actor), Err(AgentFailure::Conflict));
        s.observe_media(&stop, MediaState::Stopped).unwrap();
        assert_eq!(s.remote(), RemoteState::NotStarted);
        assert!(s.local_lease_released());
    }
    #[test]
    fn remote_loss_alone_fences_unmute_and_requests_native_stop() {
        let mut s = session();
        connect(&mut s);
        let actor = s.binding.actor.clone();
        let unmute = s.request_unmute(&actor).unwrap();
        s.observe_media(&unmute, MediaState::Capturing).unwrap();
        let stop = s.observe_remote_end(&s.binding.clone(), false).unwrap();
        assert!(s.stop_requested());
        assert_eq!(stop.kind, MediaCommandKind::Stop);
        assert_eq!(s.request_unmute(&actor), Err(AgentFailure::Conflict));
        assert_eq!(
            s.media(),
            MediaState::Capturing,
            "native stop still needs confirmation"
        );
        s.observe_media(&stop, MediaState::Stopped).unwrap();
        assert!(s.local_lease_released());
        assert_eq!(s.remote(), RemoteState::Unknown);
    }
    #[test]
    fn preparing_mute_reuses_preparation_and_activity_waits_for_live_transport() {
        let mut s = session();
        let actor = s.binding.actor.clone();
        let original = s.pending_media_command().unwrap().clone();
        assert_eq!(s.request_mute(&actor).unwrap(), original);
        assert_eq!(
            s.observe_activity(&s.binding.clone(), false, true),
            Err(AgentFailure::Conflict)
        );
        s.observe_media(&original, MediaState::Muted).unwrap();
        assert_eq!(
            s.observe_activity(&s.binding.clone(), false, true),
            Err(AgentFailure::Conflict)
        );
    }
    fn roundtrip(value: &VoiceSession) -> VoiceSession {
        value.validate().unwrap();
        let json = serde_json::to_value(value).unwrap();
        assert!(json.get("input_active").is_none() && json.get("output_active").is_none());
        let restored: VoiceSession = serde_json::from_value(json).unwrap();
        assert_eq!(restored.binding, value.binding);
        assert_eq!(restored.media, value.media);
        assert_eq!(restored.remote, value.remote);
        assert_eq!(restored.command, value.command);
        assert_eq!(restored.command_confirmed, value.command_confirmed);
        assert_eq!(restored.stop_requested, value.stop_requested);
        assert!(!restored.input_active && !restored.output_active);
        restored
    }
    #[test]
    fn stored_lifecycle_preserves_stop_uncertainty_without_restoring_activity_or_new_epoch_authority()
     {
        let mut s = session();
        roundtrip(&s);
        connect(&mut s);
        roundtrip(&s);
        let actor = s.binding.actor.clone();
        let unmute = s.request_unmute(&actor).unwrap();
        roundtrip(&s);
        s.observe_media(&unmute, MediaState::Capturing).unwrap();
        s.observe_activity(&s.binding.clone(), true, true).unwrap();
        roundtrip(&s);
        let stop = s.observe_remote_end(&s.binding.clone(), false).unwrap();
        let mut pending = roundtrip(&s);
        assert!(!pending.local_lease_released());
        assert_eq!(pending.media(), MediaState::Capturing);
        let mut next_epoch = actor.clone();
        next_epoch.runtime_epoch += 1;
        assert_eq!(
            pending.request_unmute(&next_epoch),
            Err(AgentFailure::PolicyDenied)
        );
        pending.observe_media(&stop, MediaState::Stopped).unwrap();
        let stopped = roundtrip(&pending);
        assert!(stopped.local_lease_released());
        assert_eq!(stopped.remote(), RemoteState::Unknown);
    }
    #[test]
    fn malformed_persisted_lifecycle_cannot_claim_capture_stop_or_foreign_command_receipts() {
        let original = serde_json::to_value(session()).unwrap();
        for variation in 0..8 {
            let mut altered = original.clone();
            match variation {
                0 => altered["media"] = serde_json::json!("capturing"),
                1 => altered["remote"] = serde_json::json!("connected"),
                2 => altered["stop_requested"] = serde_json::json!(true),
                3 => altered["command"]["generation"] = serde_json::json!(0),
                4 => {
                    altered["command"]["binding"]["actor"]["person_id"] =
                        serde_json::json!(PersonId::new())
                }
                5 => altered["command"]["generation"] = serde_json::json!(2),
                6 => altered["media"] = serde_json::json!("stopped"),
                _ => altered["input_active"] = serde_json::json!(true),
            }
            assert!(
                serde_json::from_value::<VoiceSession>(altered).is_err(),
                "malformed variation {variation}"
            );
        }
    }
}
