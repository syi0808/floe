//! Canonical Context-owned model projection assembly.
//!
//! The assembler turns already-filtered Conversation input plus current Context
//! inputs into the one immutable [`AuthorizedModelProjection`] one model
//! attempt runs on. It owns the envelope, the contextual data, the manifest,
//! coverage and immutable prepared-plan identity. Current source-processing
//! requirements are checked before a model intent exists; Access repeats live
//! source and Gateway fences at handoff and release.

use floe_agent_contract::{
    AGENT_SCHEMA_VERSION, AgentContext, AgentFailure, AllowedCatalog, AttemptContext,
    AuthorizedModelProjection, CONTEXT_ENVELOPE_SCHEMA_VERSION, CapabilityDescriptor,
    ContextDependency, ContextEnvelope, ContextManifest, ContextualData, DataClass,
    DependencyCoverage, DiscoveryContext, ExpertEnvironmentManifestEntry, ModelConversation,
    ModelConversationEntry, ModelCorrection, ProjectionRef, RunInstructions, ToolDescriptor,
    prompts::PromptAssembly,
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
/// that retained history reauthorized under. The immutable prepared plan binds
/// this projection to its operation, processing boundary and transport digest.
/// Credentials and live dispatch admission remain with their owners.
pub struct ContextProjectionInput<'a> {
    pub role: ContextProjectionRole,
    pub plan: &'a floe_agent_contract::PreparedModelPlan,
    pub projection_operation_id: uuid::Uuid,
    pub purpose: &'a str,
    pub response_contract: &'a str,
    pub output_format: &'a floe_agent_contract::ModelOutputFormat,
    pub correction: Option<ModelCorrection>,
    /// Stable prompt assembly prepared by the Conversation role owner.
    pub prompt: PromptAssembly,
    pub conversation: ModelConversation,
    pub agent_context: &'a AgentContext,
    pub catalog: &'a AllowedCatalog,
    pub expert_environment: Option<ExpertEnvironmentManifestEntry>,
    pub authorized_history_dependencies: &'a [ContextDependency],
    /// Caller-admitted minimum or stricter classes, unioned with projected content.
    pub input_data_classes: Vec<DataClass>,
    pub max_output_bytes: usize,
}

/// Assemble the canonical authorized model projection.
pub fn assemble_context_projection(
    input: ContextProjectionInput<'_>,
) -> Result<floe_agent_contract::ModelProjectionOutcome, AgentFailure> {
    validate_input(&input)?;
    let live = live_context(input.role, input.agent_context);
    let input_data_classes = effective_input_data_classes(&input.input_data_classes, &live)?;
    if input_data_classes.iter().any(|class| {
        matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw)
            || (*class == DataClass::TemporaryAiContext
                && input.plan.boundary == floe_agent_contract::ProcessingBoundary::Gateway)
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    let mut available_capabilities = capability_summaries(input.catalog)?;
    available_capabilities.sort_by(|left, right| left.id.cmp(&right.id));
    let mut active_experts = input.catalog.cards.clone();
    active_experts.sort_by(|left, right| left.card.id.cmp(&right.card.id));
    let mut envelope = ContextEnvelope {
        schema_version: CONTEXT_ENVELOPE_SCHEMA_VERSION,
        stable_instructions: input.prompt.clone(),
        run_instructions: RunInstructions {
            purpose: input.purpose.to_owned(),
            response_contract: input.response_contract.to_owned(),
            output_format: input.output_format.clone(),
        },
        discovery: DiscoveryContext {
            revision: input.catalog.revision,
            available_capabilities,
            active_experts,
        },
        contextual_data: ContextualData {
            projection_version: live.projection_version,
            memories: live.memories.clone(),
            optional_context_issues: live.optional_context_issues.clone(),
            evidence: live.evidence.clone(),
        },
        conversation: input.conversation.clone(),
        attempt: AttemptContext {
            correction: input.correction.clone(),
            max_output_bytes: input.max_output_bytes.min(MAX_PROJECTED_OUTPUT_BYTES),
        },
        manifest: ContextManifest {
            stable_prompt_sha256: String::new(),
            run_frame_sha256: String::new(),
            expert_environment: None,
            prompt_components: vec![],
            evidence: vec![],
            memories: vec![],
            agent_cards: vec![],
        },
    };
    envelope.manifest = envelope.derived_manifest(input.expert_environment)?;
    let coverage = fold_coverage(input.authorized_history_dependencies, &input.conversation)?;
    // A transformed Health view is admitted only with the exact still-live
    // host receipt carried by its source dependency. Class labels never attest it.
    for evidence in &live.evidence {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&evidence.untrusted_text) else {
            continue;
        };
        if value.get("view_id").and_then(serde_json::Value::as_str)
            == Some(floe_context_contract::WELLBEING_VIEW_ID)
        {
            let view: floe_context_contract::WellbeingView =
                serde_json::from_value(value).map_err(|_| AgentFailure::PolicyDenied)?;
            let DependencyCoverage::Dependent { dependencies } = &coverage else {
                return Err(AgentFailure::PolicyDenied);
            };
            let valid = dependencies.iter().any(|dependency| {
                dependency.source().connector().as_str() == floe_access::WELLBEING_CONNECTOR
                    && dependency.health_transform().is_some_and(|receipt| {
                        receipt
                            .validate_view(&input.plan.device_id, &view, chrono::Utc::now())
                            .is_ok()
                    })
            });
            if !valid || evidence.data_class != DataClass::HighlySensitive {
                return Err(AgentFailure::PolicyDenied);
            }
        }
    }
    let mut blockers = Vec::new();
    match &coverage {
        DependencyCoverage::Unknown
            if input.plan.boundary == floe_agent_contract::ProcessingBoundary::Gateway =>
        {
            return Err(AgentFailure::PolicyDenied);
        }
        DependencyCoverage::Dependent { dependencies } => {
            for dependency in dependencies {
                if dependency.person_id().to_string() != input.plan.principal
                    || dependency.expires_at() <= chrono::Utc::now()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                if dependency.source().connector().as_str() == floe_access::WELLBEING_CONNECTOR {
                    dependency
                        .validate_health_transform(&input.plan.device_id, chrono::Utc::now())?;
                    if !input_data_classes.contains(&DataClass::HighlySensitive) {
                        return Err(AgentFailure::PolicyDenied);
                    }
                }
                if input.plan.boundary == floe_agent_contract::ProcessingBoundary::Gateway
                    && !dependency
                        .processing()
                        .admits_gateway(dependency.categories())
                {
                    let blocker =
                        floe_context_contract::SourceAccessRequirement::from_processing_dependency(
                            dependency,
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    if !blockers.contains(&blocker) {
                        blockers.push(blocker);
                    }
                }
            }
        }
        _ => {}
    }
    if !blockers.is_empty() {
        use sha2::Digest;
        let blockers = floe_context_contract::SourceAccessBlockers::try_new(blockers)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let target_digest = sha2::Sha256::digest(
            serde_json::to_vec(&(input.plan, input.projection_operation_id, &blockers))
                .map_err(|_| AgentFailure::InvalidInput)?,
        )
        .into();
        return Ok(
            floe_agent_contract::ModelProjectionOutcome::NeedsSourceReview(
                floe_agent_contract::SourceProjectionReview {
                    projection_operation_id: input.projection_operation_id,
                    target_digest,
                    blockers,
                },
            ),
        );
    }
    let projection = AuthorizedModelProjection {
        plan_id: input.plan.operation_id,
        binding_digest: input.plan.binding_digest,
        projection_operation_id: input.projection_operation_id,
        projection_ref: ProjectionRef::new(),
        projection_revision: 1,
        envelope,
        coverage,
        input_data_classes,
    };
    projection.validate()?;
    Ok(floe_agent_contract::ModelProjectionOutcome::Ready(
        projection,
    ))
}

fn effective_input_data_classes(
    declared: &[DataClass],
    live: &AgentContext,
) -> Result<Vec<DataClass>, AgentFailure> {
    let mut classes = declared.to_vec();
    classes.extend(live.evidence.iter().map(|evidence| evidence.data_class));
    if live.persona.is_some() || !live.memories.is_empty() {
        classes.push(DataClass::Personal);
    }
    classes.sort();
    classes.dedup();
    if classes.is_empty() || classes.len() > floe_agent_contract::MAX_INPUT_DATA_CLASSES {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(classes)
}

fn validate_input(input: &ContextProjectionInput<'_>) -> Result<(), AgentFailure> {
    input.plan.validate()?;
    if !input.plan.capabilities.includes(&floe_agent_contract::ModelCapabilities::for_request(input.output_format, input.catalog)?) { return Err(AgentFailure::PolicyDenied); }
    if input.projection_operation_id.is_nil() || input.purpose != input.plan.purpose {
        return Err(AgentFailure::InvalidInput);
    }
    if input.purpose.trim().is_empty()
        || input.purpose.len() > floe_agent_contract::MAX_SCOPED_PURPOSE_BYTES
        || input.response_contract.len() > floe_agent_contract::MAX_RESPONSE_CONTRACT_BYTES
        || input.input_data_classes.is_empty()
        || input.input_data_classes.len() > floe_agent_contract::MAX_INPUT_DATA_CLASSES
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
fn capability_summaries(
    catalog: &AllowedCatalog,
) -> Result<Vec<CapabilityDescriptor>, AgentFailure> {
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
