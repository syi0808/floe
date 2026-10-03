//! Private lifetime of one admitted local Day transaction.
use floe_execution::{Cancellation, ExecutionScope};
use floe_kernel::{OwnerActor, PersonId};
use tokio::time::Instant;
use uuid::Uuid;
use crate::DayError;

pub struct DayWriteFence { actor: OwnerActor, generation: Uuid, owner_cancellation: Cancellation, caller_cancellation: Cancellation, deadline: Instant }
impl DayWriteFence {
    pub(crate) fn new(actor: OwnerActor, generation: Uuid, owner_cancellation: Cancellation, scope: &ExecutionScope) -> Self {
        Self { actor, generation, owner_cancellation, caller_cancellation: scope.cancellation().clone(), deadline: scope.deadline().min(Instant::now() + std::time::Duration::from_secs(60)) }
    }
    pub fn check(&self, person: PersonId, device: &str, generation: Uuid) -> Result<(), DayError> {
        if self.actor.person_id != person || self.actor.device_id != device || self.generation != generation { return Err(DayError::conflict("Day write admission changed")); }
        if self.owner_cancellation.is_cancelled() || self.caller_cancellation.is_cancelled() { return Err(DayError::storage("Day write admission closed")); }
        if Instant::now() >= self.deadline { return Err(DayError::storage("Day write admission expired")); }
        Ok(())
    }
}
