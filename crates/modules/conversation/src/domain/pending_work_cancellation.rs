//! Cancellation of queued Work only. Once a Run exists, its existing owner is
//! the sole cancellation authority; this command cannot rewrite linked custody.
use crate::WorkId;
use crate::domain::surface::{advance_surface, digest};
use floe_kernel::{AgentFailure, CommandId, OwnerActor, PersonId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelPendingWork {
    pub command_id: CommandId,
    pub work_command_id: CommandId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingWorkCancellationReceipt {
    pub command_id: CommandId,
    pub work_command_id: CommandId,
    pub work_id: WorkId,
    pub person_id: PersonId,
    pub device_id: String,
    pub request_digest: [u8; 32],
}
impl CancelPendingWork {
    pub fn receipt(
        &self,
        actor: &OwnerActor,
    ) -> Result<PendingWorkCancellationReceipt, AgentFailure> {
        actor.validate()?;
        if !self.command_id.is_valid()
            || !self.work_command_id.is_valid()
            || self.command_id == self.work_command_id
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(PendingWorkCancellationReceipt {
            command_id: self.command_id,
            work_command_id: self.work_command_id,
            work_id: WorkId::from_command(self.work_command_id)?,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            request_digest: digest(
                b"floe.surface.cancel-pending-work\0",
                &(
                    self.command_id,
                    self.work_command_id,
                    actor.person_id,
                    &actor.device_id,
                ),
            )?,
        })
    }
}
impl PendingWorkCancellationReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let expected = CancelPendingWork {
            command_id: self.command_id,
            work_command_id: self.work_command_id,
        }
        .receipt(&OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: 1,
        })?;
        if *self != expected {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_exact_and_does_not_depend_on_runtime_epoch() {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture".into(),
            runtime_epoch: 1,
        };
        let request = CancelPendingWork {
            command_id: CommandId::new(),
            work_command_id: CommandId::new(),
        };
        let receipt = request.receipt(&actor).unwrap();
        receipt.validate().unwrap();
        let before = crate::SurfaceAnchor::empty(uuid::Uuid::new_v4()).unwrap();
        let entry = PendingWorkCancellationEntry::new(before, receipt.clone()).unwrap();
        entry.validate().unwrap();
        assert_eq!(entry.through.sequence, 1);
        let mut corrupt = entry;
        corrupt.through.digest[0] ^= 1;
        assert!(corrupt.validate().is_err());
        let mut newer = actor.clone();
        newer.runtime_epoch = 2;
        assert_eq!(request.receipt(&newer).unwrap(), receipt);
        let mut changed = receipt.clone();
        changed.work_command_id = CommandId::new();
        assert!(changed.validate().is_err());
        let mut changed = receipt;
        changed.device_id = "another-device".into();
        assert!(changed.validate().is_err());
    }
}

/// The cancellation command is also an ordered Person intent. A later Work
/// sees it; an older immutable anchor is never rewritten by the current state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingWorkCancellationEntry {
    pub before: crate::SurfaceAnchor,
    pub through: crate::SurfaceAnchor,
    pub cancellation: PendingWorkCancellationReceipt,
}
impl PendingWorkCancellationEntry {
    pub fn new(
        before: crate::SurfaceAnchor,
        cancellation: PendingWorkCancellationReceipt,
    ) -> Result<Self, AgentFailure> {
        cancellation.validate()?;
        let through = advance_surface(
            before,
            digest(b"floe.surface.cancelled-work-entry\0", &cancellation)?,
        )?;
        Ok(Self {
            before,
            through,
            cancellation,
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if Self::new(self.before, self.cancellation.clone())? != *self {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
    pub fn message_id(&self) -> floe_conversation_contract::MessageId {
        floe_conversation_contract::MessageId::from_uuid(uuid::Uuid::new_v5(
            &self.cancellation.command_id.as_uuid(),
            b"surface-work-cancellation-message",
        ))
        .expect("UUID v5")
    }
}
