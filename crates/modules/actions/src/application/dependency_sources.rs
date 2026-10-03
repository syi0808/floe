use crate::{
    ActionDependencySourceFence, ActionOrigin, ActionRecord, ActionsService,
    validate_expert_action_evidence,
};
use floe_agent_contract::DependencyCoverage;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};

impl ActionsService {
    pub(super) async fn capture_dependency_sources(
        &self,
        actor: &OwnerActor,
        coverage: &DependencyCoverage,
        scope: &ExecutionScope,
    ) -> Result<Vec<ActionDependencySourceFence>, AgentFailure> {
        coverage
            .validate()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let DependencyCoverage::Dependent { dependencies } = coverage else {
            return Err(AgentFailure::PolicyDenied);
        };
        let mut fences = Vec::with_capacity(dependencies.len());
        for dependency in dependencies {
            let connection_id = dependency.source().connection_id();
            let before = scope
                .run(
                    self.sources
                        .read_reservation_fence(actor.person_id, &connection_id),
                )
                .await?;
            if before.fenced {
                return Err(AgentFailure::PolicyDenied);
            }
            let source = scope
                .run(self.sources.load(actor.person_id, &connection_id))
                .await?
                .ok_or(AgentFailure::StaleContext)?;
            let after = scope
                .run(
                    self.sources
                        .read_reservation_fence(actor.person_id, &connection_id),
                )
                .await?;
            if before != after {
                return Err(AgentFailure::StaleContext);
            }
            let fence = ActionDependencySourceFence {
                dependency: dependency.clone(),
                source,
                reservation: before,
            };
            fence.validate(actor.person_id)?;
            fences.push(fence);
        }
        self.revalidate_dependency_sources(actor, &fences, scope)
            .await?;
        Ok(fences)
    }

    pub(super) async fn prepare_dependency_sources(
        &self,
        actor: &OwnerActor,
        record: &ActionRecord,
        scope: &ExecutionScope,
    ) -> Result<Vec<ActionDependencySourceFence>, AgentFailure> {
        let ActionOrigin::Expert {
            evidence_ref,
            artifact_id,
            ..
        } = &record.origin
        else {
            return Ok(Vec::new());
        };
        let evidence = self
            .proposals
            .read(actor, evidence_ref, *artifact_id, scope)
            .await?;
        validate_expert_action_evidence(record, &evidence)?;
        self.capture_dependency_sources(actor, &evidence.coverage, scope)
            .await
    }

    pub(super) async fn revalidate_dependency_sources(
        &self,
        actor: &OwnerActor,
        fences: &[ActionDependencySourceFence],
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        for fence in fences {
            fence.validate(actor.person_id)?;
            let connection_id = fence.source.connection_id();
            let before = scope
                .run(
                    self.sources
                        .read_reservation_fence(actor.person_id, connection_id),
                )
                .await?;
            let current = scope
                .run(self.sources.load(actor.person_id, connection_id))
                .await?;
            let after = scope
                .run(
                    self.sources
                        .read_reservation_fence(actor.person_id, connection_id),
                )
                .await?;
            if before != fence.reservation
                || after != fence.reservation
                || current.as_ref() != Some(&fence.source)
            {
                return Err(AgentFailure::StaleContext);
            }
        }
        Ok(())
    }
}
