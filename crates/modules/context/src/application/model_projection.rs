//! Canonical Context-owned model projection assembly.
//!
//! The assembler turns already-filtered Conversation input plus current Context
//! inputs into the one immutable [`AuthorizedModelProjection`] one model
//! attempt runs on. It owns the envelope, the contextual data, the manifest,
//! the coverage fold, and the projection identity — and it is route-free: no
//! model profile, placement, recipient, endpoint, credential, or
//! external-transfer consent enters here. Model route/recipient admission
//! happens later, at Access model dispatch.

use floe_agent_contract::{
    AGENT_SCHEMA_VERSION, AgentCard, AgentContext, AgentFailure, AllowedCatalog,
    AuthorizedModelProjection, CapabilityDescriptor, ContextDependency, ContextEnvelope,
    ContextManifest, ContextualData, DataClass, DependencyCoverage, ModelConversation,
    ModelConversationEntry, ModelCorrection, ProjectionRef, RuntimeContext, ScopedInstructions,
    ToolDescriptor,
    prompts::PromptAssembly,
};
use floe_agent_contract::{
    AgentCardManifestEntry, EvidenceManifestEntry, MemoryManifestEntry, PromptManifestEntry,
};

/// Upper bound the projection advertises for one model response, matching the
/// historical root projector.
const MAX_PROJECTED_OUTPUT_BYTES: usize = 16384;

/// Which live context the projection carries.
///
/// The role owner (Conversation) maps its roles to this; the assembler never
/// sees role strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextProjectionRole {
    Manager,
    Finalization,
    /// A delegated Expert's own reasoning input: the full authorized context
    /// the Expert was given, like the Manager, but never the Manager role.
    Expert,
    /// The background Knowledge Learner's review input: the memories under
    /// review plus the digest turn, never foreground conversation.
    Learner,
}

/// Everything the canonical projection is assembled from.
///
/// History filtering already happened: `conversation` carries only retained
/// history, and `authorized_history_dependencies` are the exact dependencies
/// that retained history reauthorized under. There is deliberately no model
/// profile, placement, recipient, route, endpoint, credential, or consent
/// field: this input — and the projection it produces — is route-free.
pub struct ContextProjectionInput<'a> {
    pub role: ContextProjectionRole,
    pub purpose: &'a str,
    pub response_contract: &'a str,
    pub correction: Option<ModelCorrection>,
    /// Stable prompt assembly prepared by the Conversation role owner.
    pub prompt: PromptAssembly,
    pub conversation: ModelConversation,
    pub agent_context: &'a AgentContext,
    pub catalog: &'a AllowedCatalog,
    pub active_experts: &'a [AgentCard],
    pub authorized_history_dependencies: &'a [ContextDependency],
    /// Data classes of the admitted Session, never App policy.
    pub input_data_classes: Vec<DataClass>,
    pub max_output_bytes: usize,
}

/// Assemble the canonical authorized model projection.
pub fn assemble_context_projection(
    input: ContextProjectionInput<'_>,
) -> Result<AuthorizedModelProjection, AgentFailure> {
    validate_input(&input)?;
    let live = live_context(input.role, input.agent_context);
    let available_capabilities = capability_summaries(input.catalog)?;
    let active_experts = filter_experts(input.active_experts, input.catalog);
    let envelope = ContextEnvelope {
        schema_version: AGENT_SCHEMA_VERSION,
        stable_instructions: input.prompt.clone(),
        scoped_instructions: ScopedInstructions {
            purpose: input.purpose.to_owned(),
            response_contract: input.response_contract.to_owned(),
            available_capabilities,
            active_experts: active_experts.clone(),
            correction: input.correction.clone(),
        },
        contextual_data: ContextualData {
            projection_version: live.projection_version,
            memories: live.memories.clone(),
            optional_context_issues: live.optional_context_issues.clone(),
            evidence: live.evidence.clone(),
        },
        conversation: input.conversation.clone(),
        runtime: RuntimeContext {
            max_output_bytes: input.max_output_bytes.min(MAX_PROJECTED_OUTPUT_BYTES),
        },
        manifest: context_manifest(&input.prompt, &live, &active_experts),
    };
    let coverage = fold_coverage(
        input.authorized_history_dependencies,
        &input.conversation,
    )?;
    let projection = AuthorizedModelProjection {
        projection_ref: ProjectionRef::new(),
        projection_revision: 1,
        envelope,
        coverage,
        input_data_classes: input.input_data_classes.clone(),
    };
    projection.validate()?;
    Ok(projection)
}

fn validate_input(input: &ContextProjectionInput<'_>) -> Result<(), AgentFailure> {
    if input.purpose.trim().is_empty()
        || input.purpose.len() > floe_agent_contract::MAX_SCOPED_PURPOSE_BYTES
        || input.response_contract.len() > floe_agent_contract::MAX_RESPONSE_CONTRACT_BYTES
        || input.input_data_classes.is_empty()
        || input.input_data_classes.len()
            > floe_agent_contract::MAX_INPUT_DATA_CLASSES
        || input.max_output_bytes == 0
        || input.max_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
    {
        return Err(AgentFailure::InvalidInput);
    }
    input.prompt.validate()?;
    input.conversation.validate()?;
    input.agent_context.validate()?;
    input
        .catalog
        .tools
        .iter()
        .try_for_each(ToolDescriptor::validate)?;
    input
        .catalog
        .cards
        .iter()
        .try_for_each(floe_agent_contract::AgentDefinition::validate)?;
    if let Some(correction) = &input.correction {
        correction.validate()?;
    }
    Ok(())
}

/// The live context the projection carries: full for the Manager, for a
/// delegated Expert and for the background Learner, bounded and empty for
/// finalization. Settled conversation observations are untouched in all
/// cases — they travel in the conversation, not here.
fn live_context(role: ContextProjectionRole, context: &AgentContext) -> AgentContext {
    match role {
        ContextProjectionRole::Manager
        | ContextProjectionRole::Expert
        | ContextProjectionRole::Learner => context.clone(),
        ContextProjectionRole::Finalization => AgentContext {
            projection_version: context.projection_version,
            persona: None,
            memories: vec![],
            optional_context_issues: context.optional_context_issues.clone(),
            evidence: vec![],
        },
    }
}

/// The non-authoritative envelope summary of the canonical tool catalog: one
/// read-only capability entry per catalog tool. The catalog itself — owned by
/// Context — is what the Engine validates batches against.
fn capability_summaries(catalog: &AllowedCatalog) -> Result<Vec<CapabilityDescriptor>, AgentFailure> {
    catalog
        .tools
        .iter()
        .map(|tool| {
            tool.validate()?;
            let input_schema = serde_json::from_str::<serde_json::Value>(&tool.input_schema)
                .map_err(|_| AgentFailure::InvalidInput)?;
            if !input_schema.is_object() {
                return Err(AgentFailure::InvalidInput);
            }
            Ok(CapabilityDescriptor {
                schema_version: AGENT_SCHEMA_VERSION,
                id: tool.id.clone(),
                version: tool.definition_revision.to_string(),
                read_only: true,
                output_data_class: parse_data_class(&tool.output_data_class)?,
                input_schema: Some(input_schema),
            })
        })
        .collect()
}

fn parse_data_class(name: &str) -> Result<DataClass, AgentFailure> {
    match name.to_ascii_lowercase().as_str() {
        "synthetic" => Ok(DataClass::Synthetic),
        "personal" => Ok(DataClass::Personal),
        "temporaryaicontext" => Ok(DataClass::TemporaryAiContext),
        "highlysensitive" => Ok(DataClass::HighlySensitive),
        "deviceonlyraw" => Ok(DataClass::DeviceOnlyRaw),
        "credential" => Ok(DataClass::Credential),
        _ => Err(AgentFailure::InvalidInput),
    }
}

/// Active experts the current catalog still carries, by card id. Catalog
/// membership is the only filter: no App policy participates.
fn filter_experts(experts: &[AgentCard], catalog: &AllowedCatalog) -> Vec<AgentCard> {
    experts
        .iter()
        .filter(|card| {
            catalog
                .cards
                .iter()
                .any(|definition| definition.card.id == card.id)
        })
        .cloned()
        .collect()
}

/// Canonical manifest: what went into this projection. Same shape as the
/// historical helper, owned by Context for the canonical path.
pub fn context_manifest(
    prompt: &PromptAssembly,
    context: &AgentContext,
    active_experts: &[AgentCard],
) -> ContextManifest {
    ContextManifest {
        prompt_components: prompt
            .components
            .iter()
            .map(|component| PromptManifestEntry {
                kind: component.kind,
                source: component.source.clone(),
                revision: component.revision,
            })
            .collect(),
        evidence: context
            .evidence
            .iter()
            .map(|evidence| EvidenceManifestEntry {
                source_handle: evidence.source_handle.clone(),
                data_class: evidence.data_class,
                expires_at_unix_ms: evidence.expires_at_unix_ms,
            })
            .collect(),
        memories: context
            .memories
            .iter()
            .map(|memory| MemoryManifestEntry {
                target_id: memory.target_id,
                revision: memory.revision,
                source_refs: memory.source_refs.clone(),
            })
            .collect(),
        agent_cards: active_experts
            .iter()
            .map(|card| AgentCardManifestEntry {
                id: card.id.clone(),
                version: card.version.clone(),
            })
            .collect(),
    }
}

/// The exact coverage of the retained model input: the reauthorized history
/// dependencies plus the live exchanges and artifacts of the current turn.
/// Exact duplicates merge idempotently; conflicting coverage fails closed.
fn fold_coverage(
    history: &[ContextDependency],
    conversation: &ModelConversation,
) -> Result<DependencyCoverage, AgentFailure> {
    let mut coverage = DependencyCoverage::Independent;
    for dependency in history {
        coverage = coverage
            .merge(
                &DependencyCoverage::dependent(dependency.clone())
                    .map_err(|_| AgentFailure::InvalidInput)?,
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
    }
    for entry in &conversation.current_turn {
        let (exchange_coverage, artifacts) = match entry {
            ModelConversationEntry::ToolExchange { result, .. } => {
                (&result.coverage, result.artifacts.as_slice())
            }
            ModelConversationEntry::DelegationExchange { receipt, .. } => (
                &receipt.snapshot.coverage,
                receipt.snapshot.artifacts.as_slice(),
            ),
            ModelConversationEntry::User { .. }
            | ModelConversationEntry::Preamble { .. }
            | ModelConversationEntry::Assistant { .. } => continue,
        };
        coverage = coverage
            .merge(exchange_coverage)
            .map_err(|_| AgentFailure::InvalidInput)?;
        for artifact in artifacts {
            coverage = coverage
                .merge(&artifact.coverage)
                .map_err(|_| AgentFailure::InvalidInput)?;
        }
    }
    Ok(coverage)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use floe_agent_contract::prompts::{
        PromptComponent, PromptComponentKind, PromptRole,
    };
    use floe_agent_contract::{
        A2A_PROTOCOL_VERSION, AgentDefinition, AllowedCatalog, Artifact, ArtifactPart,
        ContextEvidence, ContextMemory, DelegationRequest, EpistemicStatus, InvocationKey,
        ModelPlacement, PersonalMemoryKind, TaskId, TaskReceipt, TaskSnapshot, TaskState,
        ToolCall, ToolResult,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, LearningEvidenceRef, ProcessingRestriction, ResourceHandle,
        SourceAuthority,
    };
    use uuid::Uuid;

    use super::*;

    fn dependency(person_id: floe_agent_contract::PersonId) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - Duration::minutes(1),
            now + Duration::minutes(5),
        )
        .unwrap()
    }

    fn prompt() -> PromptAssembly {
        let assembly = PromptAssembly {
            schema_version: floe_agent_contract::AGENT_VERSION,
            role: PromptRole::Manager,
            components: vec![
                PromptComponent {
                    kind: PromptComponentKind::BehaviorKernel,
                    source: "test-kernel".into(),
                    revision: 1,
                    content: "kernel".into(),
                },
                PromptComponent {
                    kind: PromptComponentKind::Role,
                    source: "test-role".into(),
                    revision: 1,
                    content: "role".into(),
                },
                PromptComponent {
                    kind: PromptComponentKind::CapabilityProtocol,
                    source: "test-protocol".into(),
                    revision: 1,
                    content: "protocol".into(),
                },
            ],
        };
        assembly.validate().unwrap();
        assembly
    }

    fn agent_card(id: &str) -> AgentCard {
        AgentCard {
            schema_version: AGENT_SCHEMA_VERSION,
            protocol_version: A2A_PROTOCOL_VERSION.into(),
            id: id.into(),
            version: "1.0.0".into(),
            name: id.into(),
            description: "test expert".into(),
            supported_placements: vec![ModelPlacement::DeviceLocal],
            domain_tags: vec![],
            skills: vec![],
        }
    }

    fn tool_descriptor(id: &str) -> ToolDescriptor {
        ToolDescriptor {
            id: id.into(),
            definition_revision: 3,
            description: format!("read {id}"),
            input_schema: "{}".into(),
            output_data_class: "personal".into(),
        }
    }

    fn catalog() -> AllowedCatalog {
        AllowedCatalog {
            cards: vec![AgentDefinition {
                card: agent_card("schedule"),
                definition_revision: 2,
            }],
            tools: vec![
                tool_descriptor("people.identity.read"),
                tool_descriptor("mail.communication.read"),
            ],
            revision: 7,
        }
    }

    fn agent_context() -> AgentContext {
        AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![ContextMemory {
                target_id: Uuid::new_v4(),
                revision: 1,
                kind: PersonalMemoryKind::Preference,
                statement: "prefers mornings".into(),
                epistemic_status: EpistemicStatus::Fact,
                confidence_millis: 900,
                observed_at_unix_ms: 1,
                valid_from_unix_ms: None,
                valid_until_unix_ms: None,
                source_refs: vec![LearningEvidenceRef {
                    session_id: Uuid::new_v4(),
                    turn_id: Uuid::new_v4(),
                }],
            }],
            optional_context_issues: vec![],
            evidence: vec![ContextEvidence {
                source_handle: "health:fixture".into(),
                data_class: DataClass::Personal,
                untrusted_text: "resting".into(),
                expires_at_unix_ms: u64::MAX,
            }],
        }
    }

    fn tool_exchange(coverage: DependencyCoverage) -> ModelConversationEntry {
        let call_id = Uuid::new_v4();
        ModelConversationEntry::ToolExchange {
            call: ToolCall {
                call_id,
                invocation_key: InvocationKey::new(),
                tool_id: "people.identity.read".into(),
                definition_revision: 3,
                input: "{}".into(),
            },
            result: ToolResult {
                call_id,
                text: "observation".into(),
                artifacts: vec![],
                coverage,
                issue: None,
            },
        }
    }

    fn input<'a>(
        role: ContextProjectionRole,
        prompt_value: PromptAssembly,
        conversation: ModelConversation,
        context: &'a AgentContext,
        catalog_value: &'a AllowedCatalog,
        experts: &'a [AgentCard],
        history: &'a [ContextDependency],
    ) -> ContextProjectionInput<'a> {
        ContextProjectionInput {
            role,
            purpose: "everyday_assistance",
            response_contract: "User-facing text.",
            correction: None,
            prompt: prompt_value,
            conversation,
            agent_context: context,
            catalog: catalog_value,
            active_experts: experts,
            authorized_history_dependencies: history,
            input_data_classes: vec![DataClass::Personal],
            max_output_bytes: 4096,
        }
    }

    fn conversation() -> ModelConversation {
        ModelConversation {
            history: vec![ModelConversationEntry::Assistant {
                message_id: Uuid::new_v4(),
                text: "retained".into(),
            }],
            current_turn: vec![ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "question".into(),
            }],
        }
    }

    #[test]
    fn manager_projection_carries_purpose_contract_and_correction() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![dependency(floe_agent_contract::PersonId::new())];
        let mut projection_input = input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        );
        projection_input.correction = Some(ModelCorrection {
            text: "try again".into(),
        });
        let projection = assemble_context_projection(projection_input).unwrap();
        assert_eq!(
            projection.envelope.scoped_instructions.purpose,
            "everyday_assistance"
        );
        assert_eq!(
            projection.envelope.scoped_instructions.response_contract,
            "User-facing text."
        );
        assert_eq!(
            projection.envelope.scoped_instructions.correction,
            Some(ModelCorrection {
                text: "try again".into()
            })
        );
        assert_eq!(projection.envelope.contextual_data.memories.len(), 1);
        assert_eq!(projection.envelope.contextual_data.evidence.len(), 1);
        assert_eq!(projection.envelope.runtime.max_output_bytes, 4096);
        assert_eq!(projection.input_data_classes, vec![DataClass::Personal]);
        assert_eq!(projection.projection_revision, 1);
    }

    #[test]
    fn expert_projection_carries_full_live_context() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![];
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Expert,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        assert_eq!(projection.envelope.contextual_data.memories.len(), 1);
        assert_eq!(projection.envelope.contextual_data.evidence.len(), 1);
        assert_eq!(projection.coverage, DependencyCoverage::Independent);
    }

    #[test]
    fn learner_projection_carries_memories_under_review() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![];
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Learner,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        assert_eq!(projection.envelope.contextual_data.memories.len(), 1);
        assert_eq!(projection.envelope.contextual_data.evidence.len(), 1);
        assert_eq!(projection.coverage, DependencyCoverage::Independent);
    }

    #[test]
    fn finalization_empties_live_context_but_keeps_settled_observations() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![];
        let mut settled = conversation();
        settled.current_turn.push(tool_exchange(DependencyCoverage::Independent));
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Finalization,
            prompt(),
            settled,
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        assert!(projection.envelope.contextual_data.memories.is_empty());
        assert!(projection.envelope.contextual_data.evidence.is_empty());
        assert!(projection.envelope.manifest.memories.is_empty());
        assert!(projection.envelope.manifest.evidence.is_empty());
        // Settled conversation observations travel in the conversation.
        assert_eq!(projection.envelope.conversation.current_turn.len(), 2);
        assert_eq!(projection.coverage, DependencyCoverage::Independent);
    }

    #[test]
    fn coverage_folds_history_and_current_exchanges_exactly() {
        let person = floe_agent_contract::PersonId::new();
        let history_dep = dependency(person);
        let tool_dep = dependency(person);
        let artifact_dep = dependency(person);
        let delegation_dep = dependency(person);
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![];
        let history = vec![history_dep.clone()];
        let tool_call_id = Uuid::new_v4();
        let task_id = TaskId::new();
        let conversation = ModelConversation {
            history: vec![],
            current_turn: vec![
                ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "question".into(),
                },
                ModelConversationEntry::ToolExchange {
                    call: ToolCall {
                        call_id: tool_call_id,
                        invocation_key: InvocationKey::new(),
                        tool_id: "people.identity.read".into(),
                        definition_revision: 3,
                        input: "{}".into(),
                    },
                    result: ToolResult {
                        call_id: tool_call_id,
                        text: "observation".into(),
                        artifacts: vec![Artifact {
                            artifact_id: Uuid::new_v4(),
                            name: "note".into(),
                            parts: vec![ArtifactPart::Text {
                                text: "detail".into(),
                            }],
                            coverage: DependencyCoverage::dependent(artifact_dep.clone()).unwrap(),
                        }],
                        coverage: DependencyCoverage::dependent(tool_dep.clone()).unwrap(),
                        issue: None,
                    },
                },
                ModelConversationEntry::DelegationExchange {
                    request: DelegationRequest {
                        task_id,
                        parent_run_id: None,
                        principal: "person:test".into(),
                        invocation_key: InvocationKey::new(),
                        selected_agent_id: "schedule".into(),
                        selected_definition_revision: 2,
                        message: "summarize".into(),
                        context_refs: vec![],
                        execution_context:
                            floe_agent_contract::DelegationExecutionContext {
                                session_id: Uuid::new_v4(),
                                device_id: "test-device".into(),
                                agent_context: floe_agent_contract::AgentContext {
                                    projection_version: 1,
                                    persona: None,
                                    memories: vec![],
                                    optional_context_issues: vec![],
                                    evidence: vec![],
                                },
                                max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
                            },
                    },
                    receipt: TaskReceipt {
                        task_id,
                        snapshot: TaskSnapshot {
                            task_id,
                            parent_run_id: None,
                            principal: "person:test".into(),
                            agent_id: "schedule".into(),
                            definition_revision: 2,
                            state: TaskState::Completed,
                            result: Some("summary".into()),
                            artifacts: vec![],
                            coverage: DependencyCoverage::dependent(delegation_dep.clone())
                                .unwrap(),
                            issue: None,
                        },
                        replay: None,
                    },
                },
            ],
        };
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation,
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        let mut expected = DependencyCoverage::Independent;
        for held in [&history_dep, &tool_dep, &artifact_dep, &delegation_dep] {
            expected = expected
                .merge(&DependencyCoverage::dependent(held.clone()).unwrap())
                .unwrap();
        }
        assert_eq!(projection.coverage, expected);
    }

    #[test]
    fn identical_authorized_input_projects_identically() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![dependency(floe_agent_contract::PersonId::new())];
        let conversation_value = conversation();
        let first = assemble_context_projection(input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation_value.clone(),
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        let second = assemble_context_projection(input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation_value,
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        // Only the fresh projection identity differs; everything authorized is
        // identical. There is no route/recipient/profile input that could
        // change the projection.
        assert_eq!(first.envelope, second.envelope);
        assert_eq!(first.coverage, second.coverage);
        assert_eq!(first.input_data_classes, second.input_data_classes);
        assert_ne!(
            first.projection_ref.as_uuid(),
            second.projection_ref.as_uuid()
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

    #[test]
    fn forbidden_route_and_secret_fields_are_absent() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![dependency(floe_agent_contract::PersonId::new())];
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        let encoded = serde_json::to_value(&projection).unwrap();
        let mut keys = Vec::new();
        object_keys(&encoded, &mut keys);
        // Prompt prose may mention recipients; field names must not. Agent
        // card capability declarations (`supported_placements`) are static
        // card data, not a route decision, so only exact keys are banned.
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

    #[test]
    fn capability_summary_derives_from_catalog_tools() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![];
        let history = vec![];
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        let capabilities = &projection.envelope.scoped_instructions.available_capabilities;
        assert_eq!(capabilities.len(), 2);
        assert_eq!(capabilities[0].id, "people.identity.read");
        assert_eq!(capabilities[0].version, "3");
        assert!(capabilities[0].read_only);
        assert_eq!(
            capabilities[0].output_data_class,
            DataClass::Personal
        );
        assert_eq!(
            capabilities[0].input_schema,
            Some(serde_json::json!({}))
        );
        // The catalog is stable regardless of model route: remote tools are
        // listed even though no route was consulted.
        assert_eq!(capabilities[1].id, "mail.communication.read");
    }

    #[test]
    fn unknown_tool_output_class_fails_closed() {
        let context = agent_context();
        let mut catalog_value = catalog();
        catalog_value.tools[0].output_data_class = "mystery".into();
        let experts = vec![];
        let history = vec![];
        assert_eq!(
            assemble_context_projection(input(
                ContextProjectionRole::Manager,
                prompt(),
                conversation(),
                &context,
                &catalog_value,
                &experts,
                &history,
            ))
            .err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn experts_are_filtered_by_catalog_membership_only() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule"), agent_card("retired")];
        let history = vec![];
        let projection = assemble_context_projection(input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        ))
        .unwrap();
        let active = &projection.envelope.scoped_instructions.active_experts;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "schedule");
        assert_eq!(projection.envelope.manifest.agent_cards.len(), 1);
        assert_eq!(projection.envelope.manifest.agent_cards[0].id, "schedule");
    }

    #[test]
    fn output_bytes_are_bounded_and_manifest_mirrors_inputs() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![agent_card("schedule")];
        let history = vec![];
        let mut projection_input = input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        );
        projection_input.max_output_bytes = usize::MAX;
        assert_eq!(
            assemble_context_projection(projection_input).err(),
            Some(AgentFailure::InvalidInput)
        );
        let mut projection_input = input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        );
        projection_input.max_output_bytes = 32 * 1024;
        let projection = assemble_context_projection(projection_input).unwrap();
        assert_eq!(projection.envelope.runtime.max_output_bytes, 16384);
        assert_eq!(projection.envelope.manifest.prompt_components.len(), 3);
        assert_eq!(projection.envelope.manifest.evidence.len(), 1);
        assert_eq!(projection.envelope.manifest.memories.len(), 1);
    }

    #[test]
    fn empty_session_data_classes_are_rejected() {
        let context = agent_context();
        let catalog_value = catalog();
        let experts = vec![];
        let history = vec![];
        let mut projection_input = input(
            ContextProjectionRole::Manager,
            prompt(),
            conversation(),
            &context,
            &catalog_value,
            &experts,
            &history,
        );
        projection_input.input_data_classes.clear();
        assert_eq!(
            assemble_context_projection(projection_input).err(),
            Some(AgentFailure::InvalidInput)
        );
    }
}
