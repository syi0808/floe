use std::{future::Future, pin::Pin};

use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_experts::AgentRegistry;
use floe_experts_builtin::{BuiltinExpertOutput, BuiltinExpertRequest, StatefulExpertDraft};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use tokio::time::Instant;

pub(in crate::vault_host::conversation_turn) trait StatefulExpertSettlement:
    Sync
{
    fn settle<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        draft: StatefulExpertDraft,
        dependencies: Vec<ContextDependency>,
    ) -> Pin<Box<dyn Future<Output = Result<BuiltinExpertOutput, AgentFailure>> + Send + 'a>>;
}

pub(super) struct VaultStatefulExpertSettlement<'a, Keys> {
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) admission: &'a floe_experts::ExpertAdmissionIdentity,
    pub(super) selection: &'a floe_experts::ExpertExecutionSelection,
}

impl<Keys: VaultKeyProvider> StatefulExpertSettlement for VaultStatefulExpertSettlement<'_, Keys> {
    fn settle<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        draft: StatefulExpertDraft,
        dependencies: Vec<ContextDependency>,
    ) -> Pin<Box<dyn Future<Output = Result<BuiltinExpertOutput, AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            if request.person_id != self.vault.person_id()
                || request.cancellation.is_cancelled()
                || request.deadline <= Instant::now()
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let snapshot = self
                .vault
                .expert_registry()
                .await?
                .ok_or(AgentFailure::CapabilityDenied)?;
            let mut registry = AgentRegistry::restore(snapshot, self.vault.registry_instance_id())?;
            if self.admission.registry_instance_id != registry.instance_id()
                || self.admission.package.id != request.agent_id
            {
                return Err(AgentFailure::Conflict);
            }
            let assignment_id = self.admission.assignment_id;
            let resolved = registry.resolve_admitted(request.person_id, self.admission)?;
            if resolved.manifest.package.id != request.agent_id
                || resolved.manifest.package != self.admission.package
                || resolved.assignment.installation_id != self.admission.installation_id
                || dependencies.is_empty()
                || dependencies
                    .iter()
                    .any(|dependency| dependency.person_id() != request.person_id)
                || draft.result.trim().is_empty()
                || draft.result.len() > request.max_output_bytes
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let state_revision = registry.complete(&resolved, request.invocation_id)?;
            let mut artifacts = draft.artifacts;
            super::reject_raw_action_artifacts(&artifacts)?;
            if let Some(proposal) = draft.calendar_proposal {
                proposal.validate()?;
                let contributors: Vec<_> = dependencies
                    .iter()
                    .filter(|dependency| {
                        dependency.person_id() == request.person_id
                            && dependency.source().person_id() == request.person_id
                            && dependency.consumer().identifier() == request.agent_id
                            && dependency.operation() == floe_access::GrantOperation::Read
                            && dependency.purpose() == floe_access::GrantPurpose::Assistant
                            && matches!(
                                dependency.source().connector().as_str(),
                                "calendar.event_kit"
                                    | "calendar.android"
                                    | "calendar.google"
                                    | "calendar.microsoft"
                            )
                            && !dependency.resources().is_empty()
                            && dependency.expires_at() > chrono::Utc::now()
                    })
                    .collect();
                let [contributor] = contributors.as_slice() else {
                    return Err(AgentFailure::PolicyDenied);
                };
                if !self.selection.requirements.iter().any(|requirement| {
                    requirement.capability == "calendar.timeline"
                        && requirement.selected.iter().any(|selected| {
                            selected.connector_id == *contributor.source().connector()
                                && selected.connection_id == contributor.source().connection_id()
                                && selected.execution_owner_id
                                    == *contributor.source().execution_owner()
                                && contributor.resources().contains(&selected.resource)
                        })
                }) {
                    return Err(AgentFailure::PolicyDenied);
                }
                let coverage =
                    floe_agent_contract::DependencyCoverage::dependent((*contributor).clone())
                        .map_err(|_| AgentFailure::PolicyDenied)?;
                let evidence = floe_actions::ExpertCalendarProposal {
                    schema_version: 1,
                    instance_id: registry.instance_id(),
                    person_id: request.person_id,
                    assignment_id,
                    package: resolved.manifest.package.clone(),
                    task_id: request.task_id,
                    invocation_id: request.invocation_id,
                    state_revision,
                    evidence_id: contributor.observation_id(),
                    data_class: resolved.data_class,
                    expires_at_unix_ms: u64::try_from(contributor.expires_at().timestamp_millis())
                        .map_err(|_| AgentFailure::StaleContext)?,
                    draft: proposal,
                };
                artifacts.push(evidence.artifact(coverage)?);
            }
            let settlement = registry
                .settle_registered_expert_invocation(
                    request.agent_id.clone(),
                    request.person_id,
                    self.admission.clone(),
                    request.invocation_id,
                    dependencies,
                    draft.result.clone(),
                )?
                .into_endpoint_settlement()?;
            Ok(BuiltinExpertOutput {
                result: draft.result,
                artifacts,
                settlement: Some(settlement),
            })
        })
    }
}
