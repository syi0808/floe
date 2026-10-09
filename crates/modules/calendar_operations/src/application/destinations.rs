use crate::*;
use floe_connections::SourceConnection;
use floe_context_contract::ConnectionId;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};
use std::collections::HashSet;
use uuid::Uuid;

impl CalendarOperationsService {
    /// Metadata-only choices. The opaque selector binds the exact current
    /// source fence and native label; it never grants permission on its own.
    pub async fn destinations(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<Vec<ActionDestinationChoice>, AgentFailure> {
        self.admit_actor(actor, scope)?;
        // Preserve the same locked-Vault availability as every Action operation.
        self.repository.read_authority(actor.person_id).await?;
        Ok(self
            .destination_candidates(actor, scope)
            .await?
            .into_iter()
            .map(|(choice, _, _)| choice)
            .collect())
    }
    /// A proposal can select only current writable destinations represented by
    /// its authenticated Calendar contributor. Listing never admits an Action.
    pub async fn proposal_preview(
        &self,
        actor: &OwnerActor,
        receipt: floe_agent_contract::TaskExecutionReceiptRef,
        artifact_id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<ActionProposalPreview, AgentFailure> {
        self.admit_actor(actor, scope)?;
        self.repository.read_authority(actor.person_id).await?;
        let seed = ActionOrigin::proposal_identity_seed(actor.person_id, &receipt, artifact_id)?;
        let id = action_uuid(b"floe.actions.action.v1\0", actor.person_id, seed);
        if let Some(record) = self.repository.get(actor.person_id, id).await? {
            self.validate_record_actor(actor, &record)?;
            if !matches!(&record.origin, ActionOrigin::Expert { evidence_ref, artifact_id: stored, .. }
                if evidence_ref == &receipt && *stored == artifact_id)
            {
                return Err(AgentFailure::Conflict);
            }
            return Ok(ActionProposalPreview::Existing {
                action: self.project(&record)?,
            });
        }
        let evidence = self
            .proposal_evidence(actor, &receipt, artifact_id, scope)
            .await?;
        let (schedule, _) = Self::proposal_schedule(actor, &evidence, self.clock.now())?;
        self.repository
            .validate_proposal_coverage(actor.person_id, &evidence.coverage)
            .await?;
        let destinations = self
            .destination_candidates(actor, scope)
            .await?
            .into_iter()
            .filter(|(_, destination, fence)| {
                Self::proposal_matches_destination(&evidence, destination)
                    && Self::dependency_matches_fence(&evidence.dependency, fence)
            })
            .map(|(choice, _, _)| choice)
            .collect();
        Ok(ActionProposalPreview::Ready {
            title: super::EXPERT_PROPOSAL_TITLE.to_owned(),
            schedule,
            destinations,
        })
    }

    pub(super) async fn resolve_destination(
        &self,
        actor: &OwnerActor,
        destination_ref: Uuid,
        scope: &ExecutionScope,
    ) -> Result<CalendarDestination, AgentFailure> {
        if destination_ref.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut matches = self
            .destination_candidates(actor, scope)
            .await?
            .into_iter()
            .filter(|(choice, _, _)| choice.destination_ref == destination_ref);
        let (_, destination, _) = matches.next().ok_or(AgentFailure::Conflict)?;
        if matches.next().is_some() {
            return Err(AgentFailure::Conflict);
        }
        Ok(destination)
    }
    pub(super) async fn target_destination(
        &self,
        actor: &OwnerActor,
        connection_id: &ConnectionId,
        calendar_id: &str,
        scope: &ExecutionScope,
    ) -> Result<CalendarDestination, AgentFailure> {
        let mut matches = self
            .destination_candidates(actor, scope)
            .await?
            .into_iter()
            .map(|(_, destination, _)| destination)
            .filter(|destination| {
                &destination.connection_id == connection_id
                    && destination.calendar_id == calendar_id
            });
        let destination = matches.next().ok_or(AgentFailure::PolicyDenied)?;
        if matches.next().is_some() {
            return Err(AgentFailure::Conflict);
        }
        Ok(destination)
    }
    async fn destination_candidates(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<
        Vec<(
            ActionDestinationChoice,
            CalendarDestination,
            ActionSourceFence,
        )>,
        AgentFailure,
    > {
        self.admit_actor(actor, scope)?;
        let sources = self.sources.list_calendar_sources(actor.person_id).await?;
        if sources.len() > 64 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut choices = Vec::new();
        let mut identities = HashSet::new();
        for source in sources {
            self.admit_actor(actor, scope)?;
            if !source.is_serving() {
                continue;
            }
            let execution_owner = source.execution_owner_id().as_str();
            let expected_owner = floe_access::local_calendar_execution_owner_for_connector(
                source.connector_id().as_str(),
                &actor.device_id,
            );
            if expected_owner.as_deref() != Some(execution_owner) {
                continue;
            }
            if self
                .sources
                .source_is_fenced(actor.person_id, source.connection_id())
                .await?
            {
                return Err(AgentFailure::Conflict);
            }
            let fence = Self::source_fence(actor, &source)?;
            let observations = self
                .executor
                .destinations(actor, &fence, scope)
                .await
                .map_err(|reason| match reason {
                    ActionBlockedReason::PermissionDenied => AgentFailure::CapabilityDenied,
                    ActionBlockedReason::PolicyDenied => AgentFailure::PolicyDenied,
                    ActionBlockedReason::SourceChanged => AgentFailure::Conflict,
                    _ => AgentFailure::CapabilityUnavailable,
                })?;
            let observed: HashSet<_> = observations
                .iter()
                .map(|value| value.calendar_id.as_str())
                .collect();
            if observations.len() != fence.resources.len()
                || observed.len() != observations.len()
                || !fence
                    .resources
                    .iter()
                    .all(|resource| observed.contains(resource.as_str()))
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            if self
                .sources
                .source_is_fenced(actor.person_id, source.connection_id())
                .await?
            {
                return Err(AgentFailure::Conflict);
            }
            let after = self
                .sources
                .load(actor.person_id, source.connection_id())
                .await?
                .ok_or(AgentFailure::Conflict)?;
            if Self::source_fence(actor, &after)? != fence {
                return Err(AgentFailure::Conflict);
            }
            for observation in observations {
                if !crate::domain::record::bounded(&observation.calendar_name, 512) {
                    return Err(AgentFailure::CapabilityUnavailable);
                }
                if !observation.can_modify {
                    continue;
                }
                let digest = action_digest(
                    b"floe.actions.destination.v1\0",
                    &(
                        actor.person_id,
                        &actor.device_id,
                        &fence,
                        &observation.calendar_id,
                        &observation.calendar_name,
                    ),
                )?;
                let mut bytes = [0; 16];
                bytes.copy_from_slice(&digest[..16]);
                bytes[6] = (bytes[6] & 15) | 0x50;
                bytes[8] = (bytes[8] & 63) | 0x80;
                let destination_ref = Uuid::from_bytes(bytes);
                if !identities.insert(destination_ref) {
                    return Err(AgentFailure::Conflict);
                }
                let choice = ActionDestinationChoice {
                    destination_ref,
                    label: observation.calendar_name.clone(),
                };
                let provider = floe_access::local_calendar_provider(source.connector_id().as_str())
                    .ok_or(AgentFailure::PolicyDenied)?;
                let destination = CalendarDestination {
                    provider,
                    connection_id: fence.connection_id.clone(),
                    connection_revision: fence.revision,
                    calendar_id: observation.calendar_id,
                    calendar_name: observation.calendar_name,
                };
                choices.push((choice, destination, fence.clone()));
                if choices.len() > 256 {
                    return Err(AgentFailure::BudgetExceeded);
                }
            }
        }
        choices.sort_by(|(left, _, _), (right, _, _)| {
            left.label
                .cmp(&right.label)
                .then(left.destination_ref.cmp(&right.destination_ref))
        });
        Ok(choices)
    }
    pub(super) fn source_fence(
        actor: &OwnerActor,
        source: &SourceConnection,
    ) -> Result<ActionSourceFence, AgentFailure> {
        if source.person_id() != actor.person_id
            || floe_access::local_calendar_provider(source.connector_id().as_str()).is_none()
            || !source.is_serving()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut resources: Vec<_> = source
            .resources()
            .iter()
            .map(|resource| resource.handle().as_str().to_owned())
            .collect();
        resources.sort();
        let fence = ActionSourceFence {
            connection_id: source.connection_id().clone(),
            revision: source.revision(),
            authority: source.source_authority(),
            execution_owner: source.execution_owner_id().as_str().to_owned(),
            resources,
            native_subject_fingerprint: source
                .native_subject_fingerprint()
                .ok_or(AgentFailure::PolicyDenied)?
                .to_owned(),
        };
        fence.validate_identity(&actor.device_id)?;
        Ok(fence)
    }
}
