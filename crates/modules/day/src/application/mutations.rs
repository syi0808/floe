//! Admission of idempotent Day commands. Storage owns one atomic transaction;
//! the Day command owns the transition and exact safe result projection.
use floe_execution::ExecutionScope;
use floe_kernel::OwnerActor;
use crate::{DayError, DayMutationCommand, DayMutationRequest, DayMutationResult, DayService};
impl DayService {
    pub async fn mutate(&self, actor: &OwnerActor, request: DayMutationRequest, scope: &ExecutionScope) -> Result<DayMutationResult, DayError> {
        self.admit_actor(actor)?;
        let command = DayMutationCommand { person_id: actor.person_id, device_id: actor.device_id.clone(), executor_generation: self.lifecycle.generation, request };
        command.validate()?;
        let _admission = tokio::select! {
            biased;
            _ = scope.cancellation().cancelled() => return Err(DayError::storage("Day command cancelled")),
            result = tokio::time::timeout_at(scope.deadline(), self.lifecycle.admission.lock()) => result.map_err(|_| DayError::storage("Day command admission expired"))?,
        };
        self.admit_actor(actor)?;
        // Shutdown waits this same admission guard before retiring the executor.
        // Repository replay precedes its active-executor/current-target reads.
        let fence = crate::DayWriteFence::new(actor.clone(), self.lifecycle.generation, self.lifecycle.cancellation.clone(), scope);
        self.repository.mutate(command, &fence).await
    }
}
