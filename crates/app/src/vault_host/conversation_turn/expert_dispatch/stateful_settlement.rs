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
                || dependencies
                    .iter()
                    .any(|dependency| dependency.person_id() != request.person_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use floe_agent_contract::{AgentContext, DataClass, ExpertInsight};
    use floe_context_contract::{
        CalendarProvider, GrantConsumer, GrantOperation, GrantPurpose, ProcessingRestriction,
        SourceAuthority,
    };
    use floe_execution::Cancellation;
    use floe_experts::BuiltinExpertSetup;
    use floe_kernel::PersonId;
    use floe_vault::VaultKey;

    #[derive(Clone, Default)]
    struct SettlementKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for SettlementKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    #[tokio::test]
    async fn schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, SettlementKeys::default())
            .await
            .unwrap();
        vault
            .install_builtin_experts_enabled(
                BuiltinExpertSetup {
                    instance_id: vault.registry_instance_id(),
                    expected_revision: 0,
                    setup_id: Uuid::new_v4(),
                },
                &crate::vault_host::builtin_setup_specs(),
                Cancellation::default(),
            )
            .await
            .unwrap();
        let source_authority = SourceAuthority::new();
        let grant = vault
            .review_native_calendar_grant(
                "eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                &crate::first_party_observe::calendar_policy()
                    .unwrap()
                    .consumers,
                &"a".repeat(64),
                None,
            )
            .await
            .unwrap();
        let consumer = GrantConsumer::builtin(BuiltinExpertKind::Schedule.package_id()).unwrap();
        let admission = vault
            .authorize_current_native_calendar_grant(
                "eventkit-connection",
                CalendarProvider::EventKit,
                "test-device",
                &["home".into()],
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                consumer.clone(),
                ProcessingRestriction::LocalOnly,
                Some("a".repeat(64).as_str()),
            )
            .await
            .unwrap();
        assert_eq!(admission.grant_id, grant.id());
        assert_eq!(admission.authority, grant.authority());
        let observation_id = Uuid::new_v4();
        let observed_at = chrono::Utc::now();
        let dependency = ContextDependency::try_new(
            person_id,
            admission.grant_id,
            admission.authority,
            admission.source.clone(),
            admission.scope.resources().to_vec(),
            admission.scope.categories().to_vec(),
            GrantOperation::Read,
            GrantPurpose::Assistant,
            consumer,
            ProcessingRestriction::LocalOnly,
            admission.consumer_policy,
            observation_id,
            vec![1],
            Uuid::new_v4(),
            Uuid::new_v4(),
            observed_at,
            observed_at + chrono::Duration::minutes(59),
        )
        .unwrap();
        assert_eq!(dependency.grant_id(), grant.id());
        assert_eq!(dependency.grant_authority(), grant.authority());
        assert_eq!(dependency.source(), grant.source());
        assert_eq!(dependency.consumer_policy(), admission.consumer_policy);
        assert_eq!(
            dependency.consumer().identifier(),
            BuiltinExpertKind::Schedule.package_id()
        );
        let request = BuiltinExpertRequest {
            agent_id: BuiltinExpertKind::Schedule.package_id().into(),
            person_id,
            task_id: Uuid::new_v4(),
            invocation_id: Uuid::new_v4(),
            assignment: "Review today".into(),
            current_time_unix_ms: chrono::Utc::now().timestamp_millis(),
            context: AgentContext {
                projection_version: 1,
                persona: None,
                optional_context_issues: vec![],
                memories: vec![],
                evidence: vec![],
            },
            max_output_bytes: 16_384,
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let window_start = 1_800_000_000_000u64;
        let draft = floe_experts_builtin::StatefulExpertDraft {
            source_handle: format!("calendar.observe:{observation_id}"),
            data_class: DataClass::Personal,
            expires_at_unix_ms: u64::try_from(
                (observed_at + chrono::Duration::minutes(59)).timestamp_millis(),
            )
            .unwrap(),
            insights: vec![ExpertInsight::FocusWindow {
                starts_at_unix_ms: window_start,
                ends_at_unix_ms: window_start + 1_800_000,
            }],
            action_proposals: vec![floe_experts_builtin::StatefulFocusProposal {
                starts_at_unix_ms: window_start,
                ends_at_unix_ms: window_start + 1_800_000,
            }],
            summary: "One focus window".into(),
            model_calls: 1,
            view_calls: 1,
        };
        let settlement = VaultStatefulExpertSettlement { vault: &vault };
        let output =
            StatefulExpertSettlement::settle(&settlement, &request, draft, vec![dependency])
                .await
                .unwrap();
        let result: ExpertResult = serde_json::from_str(&output.data).unwrap();
        assert_eq!(result.evidence_id, observation_id);
        assert_eq!(result.package.id, BuiltinExpertKind::Schedule.package_id());
        assert_eq!(result.action_proposals.len(), 1);
        assert_eq!(result.action_proposals[0].evidence_id, observation_id);
        let mut fixture = serde_json::to_value(&result).unwrap();
        let fields = fixture.as_object_mut().unwrap();
        fields.insert(
            "invocation_id".into(),
            serde_json::json!("00000000-0000-4000-8000-000000000003"),
        );
        fields.insert(
            "instance_id".into(),
            serde_json::json!("00000000-0000-4000-8000-000000000006"),
        );
        fields.insert(
            "person_id".into(),
            serde_json::json!("00000000-0000-4000-8000-000000000001"),
        );
        fields.insert(
            "assignment_id".into(),
            serde_json::json!("00000000-0000-4000-8000-000000000007"),
        );
        fields.insert(
            "evidence_id".into(),
            serde_json::json!("00000000-0000-4000-8000-000000000008"),
        );
        fields.insert(
            "source_handle".into(),
            serde_json::json!("calendar.observe:00000000-0000-4000-8000-000000000008"),
        );
        fields.insert(
            "expires_at_unix_ms".into(),
            serde_json::json!(4102444800000_u64),
        );
        fixture["action_proposals"][0]["evidence_id"] =
            serde_json::json!("00000000-0000-4000-8000-000000000008");
        let serialized = serde_json::to_string(&fixture).unwrap();
        if std::env::var_os("FLOE_PRINT_EXPERT_RESULT_FIXTURE").is_some() {
            println!("EXPERT_RESULT_FIXTURE={serialized}");
        }
        let tracked = include_str!("../../../../../../fixtures/expert-result/schedule-v1.json");
        assert_eq!(serialized, tracked.trim_end());
    }
}
