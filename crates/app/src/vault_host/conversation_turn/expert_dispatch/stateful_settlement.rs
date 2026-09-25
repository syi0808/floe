use std::{future::Future, pin::Pin};

use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_experts::{AgentRegistry, PackageImplementation};
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
                let coverage = floe_agent_contract::DependencyCoverage::dependent(
                    (*contributor).clone(),
                )
                .map_err(|_| AgentFailure::PolicyDenied)?;
                let evidence = floe_actions::ExpertCalendarProposal {
                    schema_version: 1,
                    instance_id: registry.instance_id(),
                    person_id: request.person_id,
                    assignment_id,
                    package: resolved.package.reference.clone(),
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
                    expected_revision,
                    assignment_id,
                    request.invocation_id,
                    dependencies,
                    draft.result.clone(),
                )
                .into_endpoint_settlement()?;
            Ok(BuiltinExpertOutput {
                result: draft.result,
                artifacts,
                settlement: Some(settlement),
            })
        })
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

    use floe_agent_contract::{
        AgentContext, AgentEndpoint, BoxFuture, DelegationExecutionContext, DelegationPort,
        DelegationRequest, DependencyCoverage, EndpointInvocation, ExecutionScope, ExpertReport,
        InvocationKey, TaskId, TraceContext,
    };
    use floe_context_contract::{
        CalendarProvider, GrantConsumer, GrantOperation, GrantPurpose, ProcessingRestriction,
        SourceAuthority,
    };
    use floe_execution::Cancellation;
    use floe_experts::{
        A2AMessage, A2AMessageRole, A2APart, A2ASendMessageRequest, BuiltinExpertSetup,
        Directory, DirectoryEntry, TaskCoordinator, task_receipt_to_a2a,
    };
    use floe_experts_builtin::BuiltinExpertKind;
    use floe_kernel::PersonId;
    use floe_vault::VaultKey;
    use uuid::Uuid;

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

    struct FixtureEndpoint {
        output: Mutex<Option<BuiltinExpertOutput>>,
        coverage: DependencyCoverage,
    }

    impl AgentEndpoint for FixtureEndpoint {
        fn execute<'a>(
            &'a self,
            invocation: EndpointInvocation,
            _scope: &'a ExecutionScope,
        ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
            Box::pin(async move {
                let mut output = self
                    .output
                    .lock()
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    .take()
                    .ok_or(AgentFailure::Conflict)?;
                for artifact in &mut output.artifacts {
                    if artifact.coverage == DependencyCoverage::Unknown {
                        artifact.coverage = self.coverage.clone();
                    }
                }
                Ok(ExpertReport {
                    task_id: invocation.request.task_id,
                    principal: invocation.request.principal,
                    agent_id: invocation.request.selected_agent_id,
                    definition_revision: invocation.request.selected_definition_revision,
                    result: output.result,
                    artifacts: output.artifacts,
                    coverage: self.coverage.clone(),
                    settlement: output.settlement,
                })
            })
        }
    }

    fn normalize_product_value(
        value: &mut serde_json::Value,
        ids: &mut std::collections::BTreeMap<String, String>,
    ) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, field) in fields {
                    if key == "expires_at_unix_ms" {
                        *field = serde_json::json!(4_102_444_800_000u64);
                    } else if key == "starts_at_unix_ms" {
                        *field = serde_json::json!(1_800_000_000_000u64);
                    } else if key == "ends_at_unix_ms" {
                        *field = serde_json::json!(1_800_001_800_000u64);
                    } else {
                        normalize_product_value(field, ids);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    normalize_product_value(item, ids);
                }
            }
            serde_json::Value::String(text) => {
                if Uuid::parse_str(text).is_ok() {
                    let next = ids.len() + 1;
                    let normalized = ids
                        .entry(text.clone())
                        .or_insert_with(|| format!("00000000-0000-4000-8000-{next:012x}"));
                    *text = normalized.clone();
                } else if let Ok(mut nested) = serde_json::from_str::<serde_json::Value>(text)
                    && nested.is_object()
                {
                    normalize_product_value(&mut nested, ids);
                    *text = serde_json::to_string(&nested).unwrap();
                }
            }
            _ => {}
        }
    }

    #[tokio::test]
    async fn schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault = Arc::new(
            EncryptedAgentVault::create(root.path(), person_id, SettlementKeys::default())
                .await
                .unwrap(),
        );
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
        let package = BuiltinExpertOutput::from_result(
            "Schedule assessment",
            floe_experts_builtin::schedule::RESULT_MEDIA_TYPE,
            "One focus window".into(),
            &floe_experts_builtin::schedule::ScheduleAssessment {
                insights: vec![floe_experts_builtin::schedule::ScheduleInsight::FocusWindow {
                    starts_at_unix_ms: window_start,
                    ends_at_unix_ms: window_start + 1_800_000,
                }],
            },
        )
        .unwrap();
        let draft = StatefulExpertDraft {
            result: package.result,
            artifacts: package.artifacts,
            calendar_proposal: Some(floe_actions::ExpertCalendarProposalDraft {
                starts_at_unix_ms: window_start,
                ends_at_unix_ms: window_start + 1_800_000,
            }),
        };
        let settlement = VaultStatefulExpertSettlement { vault: &vault };
        let duplicate_draft = StatefulExpertDraft {
            result: draft.result.clone(),
            artifacts: draft.artifacts.clone(),
            calendar_proposal: draft.calendar_proposal.clone(),
        };
        assert_eq!(
            StatefulExpertSettlement::settle(
                &settlement,
                &request,
                duplicate_draft,
                vec![dependency.clone(), dependency.clone()],
            )
            .await
            .err(),
            Some(AgentFailure::PolicyDenied)
        );
        let missing_draft = StatefulExpertDraft {
            result: draft.result.clone(),
            artifacts: draft.artifacts.clone(),
            calendar_proposal: draft.calendar_proposal.clone(),
        };
        assert_eq!(
            StatefulExpertSettlement::settle(&settlement, &request, missing_draft, vec![])
                .await
                .err(),
            Some(AgentFailure::CapabilityDenied)
        );
        let output =
            StatefulExpertSettlement::settle(&settlement, &request, draft, vec![dependency.clone()])
                .await
                .unwrap();
        assert_eq!(output.result, "One focus window");
        let artifacts: Vec<_> = output
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.parts.iter().any(|part| matches!(
                    part,
                    floe_agent_contract::ArtifactPart::Data { media_type, .. }
                        if media_type == floe_actions::EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE
                ))
            })
            .collect();
        let [artifact] = artifacts.as_slice() else {
            panic!("exactly one Actions proposal");
        };
        let floe_agent_contract::DependencyCoverage::Dependent { dependencies } =
            &artifact.coverage else {
            panic!("exact contributor coverage");
        };
        let [contributor] = dependencies.as_slice() else {
            panic!("exactly one contributor");
        };
        assert_eq!(contributor.observation_id(), observation_id);
        let floe_agent_contract::ArtifactPart::Data { data, .. } = &artifact.parts[0] else {
            panic!("typed proposal");
        };
        let proposal: floe_actions::ExpertCalendarProposal = serde_json::from_str(data).unwrap();
        assert_eq!(proposal.evidence_id, observation_id);
        assert_eq!(proposal.package.id, BuiltinExpertKind::Schedule.package_id());
        assert_eq!(proposal.draft.starts_at_unix_ms, window_start);

        let coverage = DependencyCoverage::dependent(dependency).unwrap();
        let directory = Directory::default();
        let card = vault
            .enabled_builtin_expert_cards()
            .await
            .unwrap()
            .into_iter()
            .find(|card| card.id == request.agent_id)
            .unwrap();
        directory
            .register(
                DirectoryEntry {
                    definition: floe_agent_contract::AgentDefinition {
                        card,
                        definition_revision: 1,
                    },
                    reviewed: true,
                    enabled: true,
                    admitted_principals: vec![person_id.to_string()],
                    purposes: vec!["everyday-assistance".into()],
                },
                Arc::new(FixtureEndpoint {
                    output: Mutex::new(Some(output)),
                    coverage: coverage.clone(),
                }),
            )
            .unwrap();
        let repository = Arc::new(floe_vault::VaultTaskRepository::new(Arc::clone(&vault)));
        let (coordinator, recovered) = TaskCoordinator::activate(
            directory,
            repository,
            "everyday-assistance",
            floe_agent_contract::MAX_OUTPUT_BYTES,
        )
        .await
        .unwrap();
        assert!(recovered.is_empty());
        let task_id = TaskId::from_uuid(request.task_id).unwrap();
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(50_000, 100_000),
            Default::default(),
        );
        let scope = ExecutionScope::root(
            Cancellation::default(),
            Instant::now() + Duration::from_secs(5),
            ledger.work_lease(),
            TraceContext::new(Uuid::new_v4()).with_task_id(task_id),
        );
        let receipt = coordinator
            .delegate(
                DelegationRequest {
                    task_id,
                    parent_run_id: None,
                    principal: person_id.to_string(),
                    invocation_key: InvocationKey::from_uuid(request.invocation_id).unwrap(),
                    selected_agent_id: request.agent_id.clone(),
                    selected_definition_revision: 1,
                    message: request.assignment.clone(),
                    context_refs: vec![],
                    execution_context: DelegationExecutionContext {
                        session_id: Uuid::new_v4(),
                        device_id: "test-device".into(),
                        agent_context: request.context.clone(),
                        max_output_bytes: request.max_output_bytes,
                    },
                },
                &scope,
            )
            .await
            .unwrap();
        assert_eq!(receipt.snapshot.state, floe_agent_contract::TaskState::Completed);
        assert_eq!(receipt.snapshot.coverage, coverage);
        assert!(receipt.snapshot.artifacts.iter().all(|artifact| artifact.coverage != DependencyCoverage::Unknown));
        let context_id = Uuid::new_v4();
        let product = task_receipt_to_a2a(
            A2ASendMessageRequest {
                usage: Default::default(),
                schema_version: 1,
                person_id,
                session_id: Uuid::new_v4(),
                parent_turn_id: Uuid::new_v4(),
                agent_id: request.agent_id.clone(),
                message: A2AMessage {
                    message_id: Uuid::new_v4(),
                    context_id,
                    task_id: Some(request.task_id),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: request.assignment.clone(),
                    }],
                },
                max_output_bytes: request.max_output_bytes,
                deadline: Instant::now() + Duration::from_secs(5),
                cancellation: Cancellation::default(),
            },
            receipt,
        )
        .unwrap();
        let mut value = serde_json::to_value(floe_conversation::AgentMessage::Delegation {
            turn_id: Uuid::new_v4(),
            task: product,
        })
        .unwrap();
        normalize_product_value(&mut value, &mut std::collections::BTreeMap::new());
        let encoded = format!("{}\n", serde_json::to_string(&value).unwrap());
        if std::env::var_os("FLOE_PRINT_EXPERT_REPORT_FIXTURE").is_some() {
            println!("DELEGATION_FIXTURE={encoded}");
            return;
        }
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/expert-report/delegation-v1.json"
        );
        assert_eq!(encoded, std::fs::read_to_string(path).unwrap());
    }
}
