//! Person-facing custody and work admission, independent of the Core recorder.
use floe_conversation_contract::MessageId;
use floe_kernel::{AgentFailure, CommandId, OwnerActor, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Queued work bound, never a lifetime transcript limit. Active execution is
/// separately limited by the existing Session/Core recorder.
pub const MAX_PENDING_SURFACE_WORK: usize = 64;
pub const MAX_SURFACE_PAGE_ITEMS: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceAnchor {
    pub session_id: Uuid,
    pub sequence: u64,
    pub digest: [u8; 32],
}
impl SurfaceAnchor {
    pub fn empty(session_id: Uuid) -> Result<Self, AgentFailure> {
        if session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut hash = Sha256::new();
        hash.update(b"floe.surface.prefix\0");
        hash.update(session_id.as_bytes());
        Ok(Self {
            session_id,
            sequence: 0,
            digest: hash.finalize().into(),
        })
    }
    pub(super) fn advance(&self, request_digest: [u8; 32]) -> Result<Self, AgentFailure> {
        self.validate()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(AgentFailure::BudgetExceeded)?;
        Ok(Self {
            session_id: self.session_id,
            sequence,
            digest: digest(b"floe.surface.link\0", &(*self, request_digest))?,
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil() || self.sequence > i64::MAX as u64 {
            return Err(AgentFailure::InvalidInput);
        }
        if self.sequence == 0 && *self != Self::empty(self.session_id)? {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
}

/// These are host-normalized origins, not provider roles or source grants.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceInputOrigin {
    Text,
    Voice {
        voice_session_id: Uuid,
        utterance_id: String,
    },
}
impl SurfaceInputOrigin {
    fn validate(&self) -> Result<(), AgentFailure> {
        if let Self::Voice {
            voice_session_id,
            utterance_id,
        } = self
        {
            if voice_session_id.is_nil()
                || utterance_id.is_empty()
                || utterance_id.len() > 256
                || utterance_id.trim() != utterance_id
                || utterance_id.chars().any(char::is_control)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceInput {
    pub message_id: MessageId,
    pub text: String,
    pub origin: SurfaceInputOrigin,
}
impl SurfaceInput {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.origin.validate()?;
        if !self.message_id.is_valid() || crate::normalize_turn_text(&self.text)? != self.text {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceInputReceipt {
    pub person_id: PersonId,
    pub device_id: String,
    pub command_id: CommandId,
    pub before: SurfaceAnchor,
    pub through: SurfaceAnchor,
    pub input: SurfaceInput,
    pub request_digest: [u8; 32],
}

/// Text requests work immediately. A settled voice utterance may be retained
/// without work; a later metadata-only delegation binds its exact message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppendSurfaceInput {
    pub command_id: CommandId,
    pub expected: SurfaceAnchor,
    pub input: SurfaceInput,
}
impl AppendSurfaceInput {
    pub fn admit(&self, actor: &OwnerActor) -> Result<SurfaceInputReceipt, AgentFailure> {
        actor.validate()?;
        self.expected.validate()?;
        self.input.validate()?;
        if !self.command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let request_digest = digest(
            b"floe.surface.input\0",
            &(
                actor.person_id,
                &actor.device_id,
                self.command_id,
                self.expected,
                &self.input,
            ),
        )?;
        let through = self.expected.advance(request_digest)?;
        Ok(SurfaceInputReceipt {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            command_id: self.command_id,
            before: self.expected,
            through,
            input: self.input.clone(),
            request_digest,
        })
    }
}
impl SurfaceInputReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        // Runtime epochs fence live callers, not immutable receipt identity.
        let actor = OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: 1,
        };
        let expected = AppendSurfaceInput {
            command_id: self.command_id,
            expected: self.before,
            input: self.input.clone(),
        }
        .admit(&actor)?;
        if *self != expected {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorkId(Uuid);
impl WorkId {
    pub fn from_command(command_id: CommandId) -> Result<Self, AgentFailure> {
        if !command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self(Uuid::new_v5(
            &command_id.as_uuid(),
            b"floe.conversation.work",
        )))
    }
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkOrigin {
    Text,
    Voice {
        voice_session_id: Uuid,
        delegation_id: String,
    },
}
impl WorkOrigin {
    fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::Text => Ok(()),
            Self::Voice {
                voice_session_id,
                delegation_id,
            } => SurfaceInputOrigin::Voice {
                voice_session_id: *voice_session_id,
                utterance_id: delegation_id.clone(),
            }
            .validate(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmitSurfaceWork {
    pub command_id: CommandId,
    pub anchor: SurfaceAnchor,
    pub input_id: MessageId,
    pub origin: WorkOrigin,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceWorkReceipt {
    pub work_id: WorkId,
    pub person_id: PersonId,
    pub device_id: String,
    pub command_id: CommandId,
    pub anchor: SurfaceAnchor,
    pub input_id: MessageId,
    pub origin: WorkOrigin,
    pub request_digest: [u8; 32],
}
impl SubmitSurfaceWork {
    pub fn admit(&self, actor: &OwnerActor) -> Result<SurfaceWorkReceipt, AgentFailure> {
        actor.validate()?;
        self.anchor.validate()?;
        self.origin.validate()?;
        if self.anchor.sequence == 0 || !self.input_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(SurfaceWorkReceipt {
            work_id: WorkId::from_command(self.command_id)?,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            command_id: self.command_id,
            anchor: self.anchor,
            input_id: self.input_id,
            origin: self.origin.clone(),
            request_digest: digest(
                b"floe.surface.work\0",
                &(
                    actor.person_id,
                    &actor.device_id,
                    self.command_id,
                    self.anchor,
                    self.input_id,
                    &self.origin,
                ),
            )?,
        })
    }
}
impl SurfaceWorkReceipt {
    /// Distinct from user/voice admission identity. Stable across claim retries.
    pub fn execution_command_id(&self) -> CommandId {
        CommandId::from_uuid(Uuid::new_v5(&self.work_id.as_uuid(), b"manager-execution"))
            .expect("UUID v5 is non-nil")
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let actor = OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: 1,
        };
        let expected = SubmitSurfaceWork {
            command_id: self.command_id,
            anchor: self.anchor,
            input_id: self.input_id,
            origin: self.origin.clone(),
        }
        .admit(&actor)?;
        if *self != expected {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// Delivery is distinct from Manager completion. An uncertain append must be
/// reconciled by the same ID, never converted to Heard or blindly resent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationState {
    Ready,
    Sending,
    Accepted,
    Played,
    Interrupted,
    Unknown,
}
impl PresentationState {
    pub fn transition(self, next: Self) -> Result<Self, AgentFailure> {
        use PresentationState::*;
        if self == next
            || matches!(
                (self, next),
                (Ready, Sending)
                    | (Sending, Accepted | Unknown)
                    | (Accepted, Played | Interrupted | Unknown)
                    | (Unknown, Accepted | Played | Interrupted)
            )
        {
            Ok(next)
        } else {
            Err(AgentFailure::Conflict)
        }
    }
}

pub(super) fn digest(domain: &[u8], value: &impl Serialize) -> Result<[u8; 32], AgentFailure> {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?);
    Ok(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor() -> OwnerActor {
        OwnerActor {
            person_id: PersonId::new(),
            device_id: "device-a".into(),
            runtime_epoch: 1,
        }
    }
    fn input() -> AppendSurfaceInput {
        AppendSurfaceInput {
            command_id: CommandId::new(),
            expected: SurfaceAnchor::empty(Uuid::new_v4()).unwrap(),
            input: SurfaceInput {
                message_id: MessageId::new(),
                text: "오늘 일정 확인해줘".into(),
                origin: SurfaceInputOrigin::Text,
            },
        }
    }
    #[test]
    fn surface_receipt_is_exact_and_epoch_independent() {
        let mut actor = actor();
        let request = input();
        let receipt = request.admit(&actor).unwrap();
        receipt.validate().unwrap();
        actor.runtime_epoch += 1;
        assert_eq!(request.admit(&actor).unwrap(), receipt);
        actor.device_id = "other".into();
        assert_ne!(
            request.admit(&actor).unwrap().request_digest,
            receipt.request_digest
        );
        let mut corrupt = receipt;
        corrupt.through.sequence += 1;
        assert!(corrupt.validate().is_err());
    }
    #[test]
    fn new_inputs_do_not_rewrite_prior_work_anchor() {
        let actor = actor();
        let mut request = input();
        let first = request.admit(&actor).unwrap();
        let work = SubmitSurfaceWork {
            command_id: CommandId::new(),
            anchor: first.through,
            input_id: first.input.message_id,
            origin: WorkOrigin::Text,
        };
        let receipt = work.admit(&actor).unwrap();
        receipt.validate().unwrap();
        request.expected = first.through;
        request.command_id = CommandId::new();
        request.input.message_id = MessageId::new();
        request.input.text = "내일 날씨도".into();
        let second = request.admit(&actor).unwrap();
        assert_eq!(receipt.anchor, first.through);
        assert_ne!(receipt.anchor, second.through);
        assert_eq!(work.admit(&actor).unwrap(), receipt);
    }
    #[test]
    fn uncertain_delivery_never_implies_playback_or_retry() {
        use PresentationState::*;
        assert_eq!(Sending.transition(Unknown), Ok(Unknown));
        assert!(Unknown.transition(Sending).is_err());
        assert!(Accepted.transition(Ready).is_err());
        assert!(Ready.transition(Played).is_err());
        assert_eq!(Unknown.transition(Played), Ok(Played));
        assert!(Interrupted.transition(Played).is_err());
    }
    #[test]
    fn reject_noncanonical_input_and_empty_anchor_work() {
        let actor = actor();
        let mut request = input();
        request.input.text.push(' ');
        assert!(request.admit(&actor).is_err());
        let work = SubmitSurfaceWork {
            command_id: CommandId::new(),
            anchor: request.expected,
            input_id: MessageId::new(),
            origin: WorkOrigin::Text,
        };
        assert!(work.admit(&actor).is_err());
        request.expected.sequence = u64::MAX;
        assert!(request.admit(&actor).is_err());
    }
}

/// Queue custody only. Execution progress and result remain on the linked Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SurfaceWorkState {
    Queued,
    Linked(floe_kernel::RunId),
    Cancelled,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceWorkSnapshot {
    pub receipt: SurfaceWorkReceipt,
    pub state: SurfaceWorkState,
    /// Latest canonical Continue/Resume descendant, observed without rewriting
    /// the immutable initial claim link. Progress remains owned by this Run.
    pub current_run_id: Option<floe_kernel::RunId>,
}

/// Text admission retains the input and queues work atomically. Voice instead
/// retains settled speech before a later delegation decides whether work exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmitTextWork {
    pub command_id: CommandId,
    pub expected: SurfaceAnchor,
    pub text: String,
}
impl SubmitTextWork {
    pub fn prepare(
        &self,
        actor: &OwnerActor,
    ) -> Result<(SurfaceInputReceipt, SurfaceWorkReceipt), AgentFailure> {
        if !self.command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let input = AppendSurfaceInput {
            command_id: CommandId::from_uuid(Uuid::new_v5(
                &self.command_id.as_uuid(),
                b"surface-text-input",
            ))
            .ok_or(AgentFailure::InvalidInput)?,
            expected: self.expected,
            input: SurfaceInput {
                message_id: MessageId::from_uuid(self.command_id.as_uuid())
                    .ok_or(AgentFailure::InvalidInput)?,
                text: crate::normalize_turn_text(&self.text)?,
                origin: SurfaceInputOrigin::Text,
            },
        }
        .admit(actor)?;
        let work = SubmitSurfaceWork {
            command_id: self.command_id,
            anchor: input.through,
            input_id: input.input.message_id,
            origin: WorkOrigin::Text,
        }
        .admit(actor)?;
        Ok((input, work))
    }
}
