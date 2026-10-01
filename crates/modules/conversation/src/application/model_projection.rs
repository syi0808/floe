//! Canonical Conversation/Context model projection.
//!
//! Conversation owns the transcript filtering and the role prompt; Context owns
//! the envelope assembly. This adapter binds them: it reauthorizes the typed
//! history through Context decisions, prepares the role prompt, and hands the
//! filtered input to the Context assembler. It takes no model policy, profile,
//! placement, route, recipient, or credential state.

use floe_agent_contract::{
    AgentContext, AgentFailure, AuthorizedModelProjection, BoxFuture, DataClass,
    ModelProjectionPort, ModelProjectionRequest,
    prompts::{PromptAssembly, PromptComponentKind},
};
use floe_context::{DependencyResolver, EvidenceReader};
use uuid::Uuid;

use super::history_projection::project_model_conversation_history;
use crate::{FINALIZATION_ROLE_ID, FINALIZATION_ROLE_PROMPT, prompts::manager_prompt};

/// Canonical root projector: Conversation filtering plus Context assembly.
pub struct ConversationModelProjection<Evidence, Resolver> {
    evidence: Evidence,
    resolver: Resolver,
    session_id: Uuid,
    agent_context: AgentContext,
    session_data_classes: Vec<DataClass>,
    manager_prompt: PromptAssembly,
    finalization_prompt: PromptAssembly,
    expert_environment: floe_agent_contract::ExpertEnvironmentManifestEntry,
}

impl<Evidence, Resolver> ConversationModelProjection<Evidence, Resolver> {
    /// Bind the projector to one Session's evidence, authority, live context,
    /// admitted data classes, and exact Run environment. Composition injects the
    /// concrete values; no policy is decided here.
    pub fn new(
        evidence: Evidence,
        resolver: Resolver,
        session_id: Uuid,
        agent_context: AgentContext,
        session_data_classes: Vec<DataClass>,
        environment: floe_experts::RunExpertEnvironmentIdentity,
    ) -> Result<Self, AgentFailure> {
        if session_id.is_nil()
            || session_data_classes.is_empty()
            || session_data_classes.len() > floe_agent_contract::MAX_INPUT_DATA_CLASSES
        {
            return Err(AgentFailure::InvalidInput);
        }
        agent_context.validate()?;
        environment.validate()?;
        let manager_prompt = manager_prompt(agent_context.persona.as_ref())?;
        let mut finalization_prompt = manager_prompt.clone();
        let component = finalization_prompt
            .components
            .iter_mut()
            .find(|component| component.kind == PromptComponentKind::Role)
            .ok_or(AgentFailure::InvalidInput)?;
        component.content = FINALIZATION_ROLE_PROMPT.into();
        manager_prompt.validate()?;
        finalization_prompt.validate()?;
        let expert_environment = floe_agent_contract::ExpertEnvironmentManifestEntry {
            revision: environment.revision,
            digest: environment.digest,
        };
        Ok(Self {
            evidence,
            resolver,
            session_id,
            agent_context,
            session_data_classes,
            manager_prompt,
            finalization_prompt,
            expert_environment,
        })
    }
}

impl<Evidence, Resolver: DependencyResolver> ConversationModelProjection<Evidence, Resolver> {
    pub fn coverage_resolver(&self) -> &dyn DependencyResolver {
        &self.resolver
    }
}

impl<Evidence, Resolver> ModelProjectionPort for ConversationModelProjection<Evidence, Resolver>
where
    Evidence: EvidenceReader,
    Resolver: DependencyResolver,
{
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            let role = match request.role.role_id.as_str() {
                "manager" => floe_context::ContextProjectionRole::Manager,
                role if role == FINALIZATION_ROLE_ID => {
                    floe_context::ContextProjectionRole::Finalization
                }
                _ => return Err(AgentFailure::InvalidInput),
            };
            if request.catalog.revision != self.expert_environment.revision {
                return Err(AgentFailure::InvalidInput);
            }
            let prompt = match role {
                floe_context::ContextProjectionRole::Manager => self.manager_prompt.clone(),
                _ => self.finalization_prompt.clone(),
            };
            let authorization = floe_context::DependencyAuthorization {
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            let projected = project_model_conversation_history(
                &self.evidence,
                self.session_id,
                &request.conversation,
                Some(&self.resolver),
                &authorization,
            )
            .await?;
            floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
                role,
                purpose: floe_inference::CANONICAL_MODEL_PURPOSE,
                response_contract: &request.role.output_contract,
                correction: request.correction.clone(),
                prompt,
                conversation: projected.conversation,
                agent_context: &self.agent_context,
                catalog: &request.catalog,
                expert_environment: Some(self.expert_environment.clone()),
                authorized_history_dependencies: &projected.authorized_history_dependencies,
                input_data_classes: self.session_data_classes.clone(),
                max_output_bytes: request.max_output_bytes,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use floe_agent_contract::{
        AgentCard, AllowedCatalog, ContextDependency, DataClass, DependencyCoverage,
        ModelConversation, ModelConversationEntry, ModelProjectionRequest, RoleSpec,
        ToolDescriptor,
    };
    use floe_context::{DependencyAuthorization, DependencyResolver, EvidenceReader};
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantAuthority, GrantConsumer,
        GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantSourceBinding,
        ProcessingRestriction, ResourceHandle,
    };

    use super::*;
    use crate::{FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_ID, MANAGER_OUTPUT_CONTRACT};

    fn dependency() -> ContextDependency {
        let person = floe_kernel::PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            floe_context_contract::SourceAuthority::new(),
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }

    struct MapReader {
        coverage: Mutex<HashMap<Uuid, DependencyCoverage>>,
    }

    impl EvidenceReader for MapReader {
        fn read_turn_coverage(
            &self,
            _session_id: Uuid,
            turn_id: Uuid,
        ) -> impl std::future::Future<Output = Result<DependencyCoverage, AgentFailure>> + Send
        {
            let coverage = self
                .coverage
                .lock()
                .unwrap()
                .get(&turn_id)
                .cloned()
                .unwrap_or(DependencyCoverage::Unknown);
            async move { Ok(coverage) }
        }
    }

    struct AcceptAll;

    impl DependencyResolver for AcceptAll {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(()) })
        }
    }

    struct DenyAll;

    impl DependencyResolver for DenyAll {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move { Err(AgentFailure::PolicyDenied) })
        }
    }

    fn agent_context() -> AgentContext {
        AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        }
    }

    fn catalog() -> AllowedCatalog {
        AllowedCatalog {
            cards: vec![],
            tools: vec![ToolDescriptor {
                id: "test.identity-evidence".into(),
                definition_revision: 3,
                description: "read people".into(),
                input_schema: "{}".into(),
                output_data_class: "personal".into(),
            }],
            revision: 1,
        }
    }

    fn scope() -> floe_execution::ExecutionScope {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            ledger.work_lease(),
            floe_kernel::TraceContext::new(Uuid::new_v4()),
        )
    }

    #[tokio::test]
    async fn run_projector_reuses_stable_program_and_exact_environment() {
        let environment = floe_experts::RunExpertEnvironmentIdentity {
            revision: 1,
            digest: [7; 32],
        };
        let context = agent_context();
        let projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::new()),
            },
            AcceptAll,
            Uuid::new_v4(),
            context.clone(),
            vec![DataClass::Personal],
            environment,
        )
        .unwrap();
        let conversation = ModelConversation {
            history: vec![],
            current_turn: vec![ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "question".into(),
            }],
        };
        let request = manager_request(conversation);
        let first = projector.project(request.clone(), &scope()).await.unwrap();
        let mut retry = request.clone();
        retry.correction = Some(floe_agent_contract::ModelCorrection {
            text: "retry".into(),
        });
        retry
            .conversation
            .history
            .push(ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "earlier".into(),
            });
        let second = projector.project(retry, &scope()).await.unwrap();
        assert_eq!(first.envelope.stable_instructions, projector.manager_prompt);
        assert_eq!(
            first.envelope.stable_instructions.render(),
            second.envelope.stable_instructions.render()
        );
        assert_eq!(
            first.envelope.manifest.stable_prompt_sha256,
            second.envelope.manifest.stable_prompt_sha256
        );
        assert_eq!(
            first.envelope.manifest.run_frame_sha256,
            second.envelope.manifest.run_frame_sha256
        );
        for projection in [&first, &second] {
            assert_eq!(
                projection.envelope.manifest.expert_environment,
                Some(floe_agent_contract::ExpertEnvironmentManifestEntry {
                    revision: environment.revision,
                    digest: environment.digest
                })
            );
        }
        let next_projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::new()),
            },
            AcceptAll,
            Uuid::new_v4(),
            context,
            vec![DataClass::Personal],
            floe_experts::RunExpertEnvironmentIdentity {
                revision: 2,
                digest: [8; 32],
            },
        )
        .unwrap();
        let mut next_request = request.clone();
        next_request.catalog.revision = 2;
        let next = next_projector
            .project(next_request, &scope())
            .await
            .unwrap();
        assert_eq!(
            first.envelope.manifest.stable_prompt_sha256,
            next.envelope.manifest.stable_prompt_sha256
        );
        assert_ne!(
            first.envelope.manifest.run_frame_sha256,
            next.envelope.manifest.run_frame_sha256
        );
        assert_ne!(
            first.envelope.manifest.expert_environment,
            next.envelope.manifest.expert_environment
        );
        assert_eq!(
            next_projector
                .project(request.clone(), &scope())
                .await
                .err(),
            Some(AgentFailure::InvalidInput)
        );
        let mut finalization = request;
        finalization.role.role_id = FINALIZATION_ROLE_ID.into();
        finalization.role.output_contract = FINALIZATION_OUTPUT_CONTRACT.into();
        let final_projection = projector.project(finalization, &scope()).await.unwrap();
        let role = final_projection
            .envelope
            .stable_instructions
            .components
            .iter()
            .find(|component| component.kind == PromptComponentKind::Role)
            .unwrap();
        assert_eq!(role.content, FINALIZATION_ROLE_PROMPT);
        assert_eq!(
            final_projection.envelope.run_instructions.response_contract,
            FINALIZATION_OUTPUT_CONTRACT
        );
        assert_ne!(
            first.envelope.manifest.stable_prompt_sha256,
            final_projection.envelope.manifest.stable_prompt_sha256
        );
        let mut persona_prompt = projector.manager_prompt.clone();
        let persona = persona_prompt
            .components
            .iter_mut()
            .find(|component| component.kind == PromptComponentKind::Persona)
            .unwrap();
        persona.content.push_str(" Short answers.");
        assert_ne!(
            persona_prompt.stable_prompt_sha256(),
            first.envelope.manifest.stable_prompt_sha256
        );
    }

    fn manager_request(conversation: ModelConversation) -> ModelProjectionRequest {
        ModelProjectionRequest {
            principal: "person:test".into(),
            role: RoleSpec {
                role_id: "manager".into(),
                instructions: "Answer.".into(),
                output_contract: MANAGER_OUTPUT_CONTRACT.into(),
            },
            conversation,
            catalog: catalog(),
            max_output_bytes: 4096,
            correction: None,
        }
    }

    fn history_pair() -> (ModelConversation, Uuid, Uuid, ContextDependency) {
        let user_id = Uuid::new_v4();
        let assistant_id = Uuid::new_v4();
        let held = dependency();
        let conversation = ModelConversation {
            history: vec![
                ModelConversationEntry::User {
                    message_id: user_id,
                    text: "what did the source say?".into(),
                },
                ModelConversationEntry::Assistant {
                    message_id: assistant_id,
                    text: "the source said yes".into(),
                },
            ],
            current_turn: vec![ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "and now?".into(),
            }],
        };
        (conversation, user_id, assistant_id, held)
    }

    #[tokio::test]
    async fn manager_stable_prompt_is_independent_of_active_expert_catalog() {
        let first = AgentCard {
            schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: "example.dynamic-advisor".into(),
            version: "1.0.0".into(),
            name: "Synthetic advisor".into(),
            description: "Assesses supplied fictional constraints.".into(),
            domain_tags: vec![],
            skills: vec![],
            supported_placements: vec![floe_agent_contract::ModelPlacement::DeviceLocal],
        };
        let second = AgentCard {
            id: "example.other-advisor".into(),
            name: "Other synthetic advisor".into(),
            description: "Provides an independent perspective on supplied fictional plans.".into(),
            ..first.clone()
        };
        let conversation = ModelConversation {
            history: vec![],
            current_turn: vec![ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "Assess the supplied fictional plan.".into(),
            }],
        };
        for cards in [
            vec![],
            vec![first.clone()],
            vec![first.clone(), second.clone()],
            vec![second, first],
        ] {
            let projector = ConversationModelProjection::new(
                MapReader {
                    coverage: Mutex::new(HashMap::new()),
                },
                AcceptAll,
                Uuid::new_v4(),
                agent_context(),
                vec![DataClass::Personal],
                floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [1; 32],
                },
            )
            .unwrap();
            let mut request = manager_request(conversation.clone());
            request.role = crate::prompts::manager_role_spec();
            request.catalog.cards = cards
                .iter()
                .cloned()
                .map(|card| floe_agent_contract::AgentDefinition {
                    card,
                    definition_revision: 1,
                })
                .collect();
            let projection = projector.project(request, &scope()).await.unwrap();
            assert_eq!(
                projection.envelope.stable_instructions,
                manager_prompt(None).unwrap()
            );
            assert_eq!(
                projection
                    .envelope
                    .discovery
                    .active_experts
                    .iter()
                    .map(|definition| definition.card.clone())
                    .collect::<Vec<_>>(),
                {
                    let mut sorted = cards;
                    sorted.sort_by(|left, right| left.id.cmp(&right.id));
                    sorted
                }
            );
        }
    }

    #[tokio::test]
    async fn manager_role_only_spec_preserves_stable_projection() {
        for persona in [
            None,
            Some(floe_knowledge::prompts::PersonaProfile {
                revision: 2,
                source: "test.custom-persona".into(),
                instructions: "Answer concisely in Korean.".into(),
            }),
        ] {
            let (conversation, user_id, assistant_id, held) = history_pair();
            let mut context = agent_context();
            context.persona = persona;
            let previous_role = RoleSpec {
                role_id: "manager".into(),
                instructions: manager_prompt(context.persona.as_ref()).unwrap().render(),
                output_contract: MANAGER_OUTPUT_CONTRACT.into(),
            };
            let projector = ConversationModelProjection::new(
                MapReader {
                    coverage: Mutex::new(HashMap::from([
                        (user_id, DependencyCoverage::Independent),
                        (assistant_id, DependencyCoverage::dependent(held).unwrap()),
                    ])),
                },
                AcceptAll,
                Uuid::new_v4(),
                context,
                vec![DataClass::Personal],
                floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [1; 32],
                },
            )
            .unwrap();
            let mut previous_request = manager_request(conversation);
            previous_request.role = previous_role;
            let mut canonical_request = previous_request.clone();
            canonical_request.role = crate::prompts::manager_role_spec();
            let scope = scope();
            let previous = projector.project(previous_request, &scope).await.unwrap();
            let canonical = projector.project(canonical_request, &scope).await.unwrap();
            assert_eq!(previous.envelope, canonical.envelope);
            assert_eq!(previous.coverage, canonical.coverage);
            assert_eq!(previous.input_data_classes, canonical.input_data_classes);
            assert_eq!(previous.projection_revision, canonical.projection_revision);
        }
    }

    #[tokio::test]
    async fn manager_role_only_spec_rejects_invalid_persona_before_projection_release() {
        for persona in [
            floe_knowledge::prompts::PersonaProfile {
                revision: 0,
                ..Default::default()
            },
            floe_knowledge::prompts::PersonaProfile {
                source: " ".into(),
                ..Default::default()
            },
            floe_knowledge::prompts::PersonaProfile {
                instructions: " ".into(),
                ..Default::default()
            },
            floe_knowledge::prompts::PersonaProfile {
                instructions: "a".repeat(4097),
                ..Default::default()
            },
        ] {
            let mut context = agent_context();
            context.persona = Some(persona);
            let projector = ConversationModelProjection::new(
                MapReader {
                    coverage: Mutex::new(HashMap::new()),
                },
                AcceptAll,
                Uuid::new_v4(),
                context,
                vec![DataClass::Personal],
                floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [1; 32],
                },
            );
            assert_eq!(projector.err().unwrap(), AgentFailure::InvalidInput);
        }
    }

    #[tokio::test]
    async fn manager_projects_filtered_history_with_canonical_purpose() {
        let (conversation, user_id, assistant_id, held) = history_pair();
        let projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::from([
                    (user_id, DependencyCoverage::Independent),
                    (
                        assistant_id,
                        DependencyCoverage::dependent(held.clone()).unwrap(),
                    ),
                ])),
            },
            AcceptAll,
            Uuid::new_v4(),
            agent_context(),
            vec![DataClass::Personal],
            floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
        )
        .unwrap();
        let projection = projector
            .project(manager_request(conversation), &scope())
            .await
            .unwrap();
        assert_eq!(
            projection.envelope.run_instructions.purpose,
            floe_inference::CANONICAL_MODEL_PURPOSE
        );
        assert_eq!(projection.input_data_classes, vec![DataClass::Personal]);
        assert_eq!(projection.envelope.conversation.history.len(), 2);
        assert_eq!(
            projection.coverage,
            DependencyCoverage::dependent(held).unwrap()
        );
        let role = projection
            .envelope
            .stable_instructions
            .components
            .iter()
            .find(|component| component.kind == PromptComponentKind::Role)
            .unwrap();
        assert!(!role.content.contains(FINALIZATION_ROLE_PROMPT));
    }

    #[tokio::test]
    async fn revoked_history_drops_derived_before_assembly() {
        let (conversation, user_id, assistant_id, held) = history_pair();
        let projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::from([
                    (user_id, DependencyCoverage::Independent),
                    (assistant_id, DependencyCoverage::dependent(held).unwrap()),
                ])),
            },
            DenyAll,
            Uuid::new_v4(),
            agent_context(),
            vec![DataClass::Personal],
            floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
        )
        .unwrap();
        let projection = projector
            .project(manager_request(conversation), &scope())
            .await
            .unwrap();
        assert_eq!(projection.envelope.conversation.history.len(), 1);
        assert!(matches!(
            projection.envelope.conversation.history[0],
            ModelConversationEntry::User { .. }
        ));
        assert_eq!(projection.coverage, DependencyCoverage::Independent);
    }

    #[tokio::test]
    async fn finalization_uses_finalization_prompt_and_empty_live_context() {
        let (conversation, user_id, assistant_id, held) = history_pair();
        let mut context = agent_context();
        context.evidence = vec![floe_agent_contract::ContextEvidence {
            source_handle: "health:fixture".into(),
            data_class: DataClass::Personal,
            untrusted_text: "resting".into(),
            expires_at_unix_ms: u64::MAX,
        }];
        let projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::from([
                    (user_id, DependencyCoverage::Independent),
                    (assistant_id, DependencyCoverage::dependent(held).unwrap()),
                ])),
            },
            AcceptAll,
            Uuid::new_v4(),
            context,
            vec![DataClass::Personal],
            floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
        )
        .unwrap();
        let mut request = manager_request(conversation);
        request.role.role_id = FINALIZATION_ROLE_ID.into();
        request.role.output_contract = FINALIZATION_OUTPUT_CONTRACT.into();
        let projection = projector.project(request, &scope()).await.unwrap();
        let role = projection
            .envelope
            .stable_instructions
            .components
            .iter()
            .find(|component| component.kind == PromptComponentKind::Role)
            .unwrap();
        assert!(role.content.starts_with(FINALIZATION_ROLE_PROMPT));
        assert!(projection.envelope.contextual_data.evidence.is_empty());
        // Retained history and the live turn still reach the final answer.
        assert_eq!(projection.envelope.conversation.history.len(), 2);
        assert_eq!(projection.envelope.conversation.current_turn.len(), 1);
    }

    #[tokio::test]
    async fn unknown_role_is_rejected() {
        let (conversation, _, _, _) = history_pair();
        let projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::new()),
            },
            AcceptAll,
            Uuid::new_v4(),
            agent_context(),
            vec![DataClass::Personal],
            floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
        )
        .unwrap();
        let mut request = manager_request(conversation);
        request.role.role_id = "analyst".into();
        assert_eq!(
            projector.project(request, &scope()).await.err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn constructor_rejects_missing_session_or_data_classes() {
        assert_eq!(
            ConversationModelProjection::new(
                MapReader {
                    coverage: Mutex::new(HashMap::new()),
                },
                AcceptAll,
                Uuid::nil(),
                agent_context(),
                vec![DataClass::Personal],
                floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [1; 32]
                },
            )
            .err(),
            Some(AgentFailure::InvalidInput)
        );
        assert_eq!(
            ConversationModelProjection::new(
                MapReader {
                    coverage: Mutex::new(HashMap::new()),
                },
                AcceptAll,
                Uuid::new_v4(),
                agent_context(),
                vec![],
                floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [1; 32]
                },
            )
            .err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    fn object_keys(value: &serde_json::Value, keys: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, nested) in map {
                    keys.push(key.clone());
                    object_keys(nested, keys);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    object_keys(item, keys);
                }
            }
            _ => {}
        }
    }

    #[tokio::test]
    async fn adapter_output_carries_no_route_or_secret_fields() {
        let (conversation, user_id, assistant_id, held) = history_pair();
        let projector = ConversationModelProjection::new(
            MapReader {
                coverage: Mutex::new(HashMap::from([
                    (user_id, DependencyCoverage::Independent),
                    (assistant_id, DependencyCoverage::dependent(held).unwrap()),
                ])),
            },
            AcceptAll,
            Uuid::new_v4(),
            agent_context(),
            vec![DataClass::Personal],
            floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
        )
        .unwrap();
        let projection = projector
            .project(manager_request(conversation), &scope())
            .await
            .unwrap();
        let encoded = serde_json::to_value(&projection).unwrap();
        let mut keys = Vec::new();
        object_keys(&encoded, &mut keys);
        for forbidden in [
            "recipient",
            "endpoint",
            "base_url",
            "bearer",
            "credential",
            "credentials",
            "consent",
            "external_transfer_consent",
            "allowed_placements",
            "placement",
            "route",
            "remote_route",
            "profile",
            "profile_id",
            "token",
        ] {
            assert!(
                !keys.iter().any(|key| key == forbidden),
                "projection must not carry a {forbidden} field: {keys:?}"
            );
        }
    }
}
