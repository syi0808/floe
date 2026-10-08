//! Admission of idempotent Day commands. Storage owns one atomic transaction;
//! the Day command owns the transition and exact safe result projection.
use crate::{DayError, DayMutationCommand, DayMutationRequest, DayMutationResult, DayService};
use floe_execution::ExecutionScope;
use floe_kernel::{CommandFailure, OwnerActor};
impl DayService {
    pub async fn mutate(
        &self,
        actor: &OwnerActor,
        request: DayMutationRequest,
        scope: &ExecutionScope,
    ) -> Result<DayMutationResult, CommandFailure<DayError>> {
        self.admit_actor(actor)
            .map_err(CommandFailure::NotAdmitted)?;
        let command = DayMutationCommand {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            executor_generation: self.lifecycle.generation,
            request,
        };
        if command.request.command_id.is_nil() {
            return Err(CommandFailure::NotApplied(DayError::validation(
                "invalid Day command identity",
            )));
        }
        let _admission = tokio::select! {
            biased;
            _ = scope.cancellation().cancelled() => return Err(CommandFailure::NotAdmitted(DayError::storage("Day command cancelled"))),
            result = tokio::time::timeout_at(scope.deadline(), self.lifecycle.admission.lock()) => result.map_err(|_| CommandFailure::NotAdmitted(DayError::storage("Day command admission expired")))?,
        };
        self.admit_actor(actor)
            .map_err(CommandFailure::NotAdmitted)?;
        // Shutdown waits this same admission guard before retiring the executor.
        // Repository replay precedes its active-executor/current-target reads.
        let fence = crate::DayWriteFence::new(
            actor.clone(),
            self.lifecycle.generation,
            self.lifecycle.cancellation.clone(),
            scope,
        );
        self.repository.mutate(command, &fence).await
    }
}
