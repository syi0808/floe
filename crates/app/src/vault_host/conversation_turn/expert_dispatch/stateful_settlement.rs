use std::{future::Future, pin::Pin};

use floe_agent_contract::{AGENT_VERSION, AgentFailure, ExpertFocusProposal, ExpertResult};
use floe_context_contract::ContextDependency;
use floe_experts::{AgentRegistry, PackageImplementation};
use floe_experts_builtin::{
    BuiltinExpertKind, BuiltinExpertOutput, BuiltinExpertRequest, StatefulExpertDraft,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use tokio::time::Instant;
use uuid::Uuid;

pub(in crate::vault_host::conversation_turn) trait StatefulExpertSettlement: Sync {
    fn settle<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        draft: StatefulExpertDraft,
        dependencies: Vec<ContextDependency>,
    ) -> Pin<Box<dyn Future<Output = Result<BuiltinExpertOutput, AgentFailure>> + Send + 'a>>;
}

pub(super) struct VaultStatefulExpertSettlement<'a, Keys> {
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
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
            let expected_revision = registry.revision();
            let current_snapshot = registry.snapshot();
            let setup = current_snapshot
                .builtin_setups
                .iter()
                .find(|setup| setup.person_id == request.person_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            let assignment = setup
                .assignments
                .iter()
                .find(|assignment| assignment.expert.as_str() == request.agent_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            let assignment_id = assignment.expert_assignment_id;
            let expected_expert = floe_experts::AgentId::try_new(request.agent_id.clone())
                .ok_or(AgentFailure::CapabilityDenied)?;
            let resolved = registry.resolve_builtin(
                registry.instance_id(),
                request.person_id,
                assignment_id,
                expected_revision,
                &expected_expert,
            )?;
            if resolved.package.reference.id != request.agent_id
                || !matches!(
                    &resolved.package.implementation,
                    PackageImplementation::Builtin { expert } if expert.as_str() == request.agent_id
                )
                || draft.data_class != resolved.data_class
                || draft.expires_at_unix_ms
                    <= u64::try_from(chrono::Utc::now().timestamp_millis())
                        .map_err(|_| AgentFailure::StaleContext)?
                || dependencies.is_empty()
                || dependencies.iter().any(|dependency| dependency.person_id() != request.person_id)
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let evidence_id = evidence_for_source(&draft.source_handle, &dependencies)?;
            let mut result = ExpertResult {
                schema_version: AGENT_VERSION,
                invocation_id: request.invocation_id,
                instance_id: registry.instance_id(),
                person_id: request.person_id,
                assignment_id,
                package: resolved.package.reference.clone(),
                evidence_id,
                source_handle: draft.source_handle,
                data_class: draft.data_class,
                expires_at_unix_ms: draft.expires_at_unix_ms,
                insights: draft.insights,
                action_proposals: draft
                    .action_proposals
                    .into_iter()
                    .map(|proposal| ExpertFocusProposal {
                        starts_at_unix_ms: proposal.starts_at_unix_ms,
                        ends_at_unix_ms: proposal.ends_at_unix_ms,
                        evidence_id,
                    })
                    .collect(),
                summary: Some(draft.summary.clone()),
                model_calls: draft.model_calls,
                state_revision: 0,
                view_calls: draft.view_calls,
            };
            result.state_revision = registry.complete(&resolved, request.invocation_id)?;
            registry.validate_recorded_result(&result)?;
            let result_data =
                serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?;
            if result_data.len() > request.max_output_bytes.min(16_384) {
                return Err(AgentFailure::BudgetExceeded);
            }
            let settlement = registry
                .settle_registered_expert_invocation(
                    request.agent_id.clone(),
                    expected_revision,
                    assignment_id,
                    request.invocation_id,
                    dependencies,
                    result_data,
                )
                .into_endpoint_settlement()?;
            BuiltinExpertOutput::from_result(
                BuiltinExpertKind::from_package_id(&request.agent_id)
                    .ok_or(AgentFailure::CapabilityDenied)?
                    .result_artifact_name(),
                draft.summary,
                &result,
            )
            .map(|output| output.with_settlement(settlement))
        })
    }
}

/// The observation backing `source_handle`, from the captured dependencies.
///
/// Native Calendar views carry `calendar.observe:{observation_id}`; the exact
/// captured dependency with that observation backs the result. Other sources
/// must have exactly one captured dependency.
fn evidence_for_source(
    source_handle: &str,
    dependencies: &[ContextDependency],
) -> Result<Uuid, AgentFailure> {
    if let Some(observation) = source_handle.strip_prefix("calendar.observe:") {
        let observation_id =
            Uuid::parse_str(observation).map_err(|_| AgentFailure::StaleContext)?;
        if observation_id.is_nil() {
            return Err(AgentFailure::StaleContext);
        }
        let matches: Vec<_> = dependencies
            .iter()
            .filter(|dependency| dependency.observation_id() == observation_id)
            .collect();
        let [dependency] = matches.as_slice() else {
            return Err(AgentFailure::PolicyDenied);
        };
        Ok(dependency.observation_id())
    } else {
        let [dependency] = dependencies else {
            return Err(AgentFailure::PolicyDenied);
        };
        let evidence_id = dependency.observation_id();
        if evidence_id.is_nil() {
            return Err(AgentFailure::StaleContext);
        }
        Ok(evidence_id)
    }
}

#[cfg(test)]
pub(in crate::vault_host::conversation_turn) struct RejectStatefulSettlement;

#[cfg(test)]
impl StatefulExpertSettlement for RejectStatefulSettlement {
    fn settle<'a>(
        &'a self,
        _: &'a BuiltinExpertRequest,
        _: StatefulExpertDraft,
        _: Vec<ContextDependency>,
    ) -> Pin<Box<dyn Future<Output = Result<BuiltinExpertOutput, AgentFailure>> + Send + 'a>> {
        Box::pin(async { Err(AgentFailure::CapabilityDenied) })
    }
}
