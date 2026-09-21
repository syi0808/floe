//! Canonical delegated Expert model/source host.
//!
//! Delegated built-in Experts reason through shared Inference: the Expert
//! states what execution class it requires, Context projects the authorized
//! input, Inference selects the profile and owns the attempt, and Access
//! fences the dispatch. No provider, route, or usage ledger is selected here.

use std::{future::Future, pin::Pin, sync::Mutex};

use floe_agent_contract::{
    AGENT_VERSION, AgentFailure, AllowedCatalog, DataClass, DependencyCoverage,
    ExpertModelAnswer, ExpertModelCall, ExpertModelRequirement, ExpertReasoningStep,
    ExpertStep, ExpertStepOutcome, ExpertTranscriptEntry, InferencePolicyDecision,
    InvocationKey, ModelConversation, ModelConversationEntry, ModelPlacement, ModelRequest,
    ModelStep, ToolCall, ToolDescriptor, ToolResult, TransferConsent,
};
use floe_context::{AttentionView, CalendarContextView, NativeContextView, PeopleView, WellbeingView};
use floe_inference::{InferenceExecutionConstraint, InferenceExecutor};
use floe_kernel::{PersonId, TaskId};
use floe_provider_adapters::sources::ServerSourceClient;
use floe_provider_adapters::sources::server::CalendarContextRequest;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use crate::FloeCore;
use crate::local_context::LocalContextHost;

use super::super::personal_grants;

/// The Context/source policy delegated Experts run under.
///
/// This carries Context/source semantics only (data classes, freshness,
/// bounds): it no longer selects a provider placement and never authorizes
/// model transfer. Canonical Inference maps the Expert's requirement to an
/// execution constraint, and Access fences the dispatch.
pub(super) fn expert_policy() -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE.into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

/// Map an Expert-owned requirement to the Inference execution constraint.
///
/// The mapping is 1:1 by construction: the Expert states a class, Inference
/// selects a profile satisfying it. No provider is named here.
fn execution_constraint(
    requirement: ExpertModelRequirement,
) -> InferenceExecutionConstraint {
    match requirement {
        ExpertModelRequirement::Any => InferenceExecutionConstraint::Any,
        ExpertModelRequirement::DeviceOnly => InferenceExecutionConstraint::DeviceOnly,
        ExpertModelRequirement::RemoteOnly => InferenceExecutionConstraint::RemoteOnly,
    }
}

/// Where a delegated legacy Expert endpoint reads the saved server
/// connection for its model/source compatibility: the host keychain slot in
/// production, a fixed injected fixture in tests.
///
/// Constructor-injected per endpoint. The endpoint admits the loaded
/// connection against the invocation principal and the execution-context
/// device id on every execution; no credential ever travels in the
/// delegation itself. Temporary legacy Expert transport support until
/// Stage 3-A.
#[derive(Clone)]
pub(crate) enum EndpointConnectionStore {
    HostKeychain(floe_provider_adapters::control::SavedServerConnectionStore),
    #[cfg(test)]
    Fixture(Option<floe_inference::SavedServerConnection>),
}

impl EndpointConnectionStore {
    pub(crate) fn host_keychain() -> Self {
        Self::HostKeychain(floe_provider_adapters::control::SavedServerConnectionStore)
    }

    #[cfg(test)]
    pub(crate) fn fixture(saved: Option<floe_inference::SavedServerConnection>) -> Self {
        Self::Fixture(saved)
    }
}

impl floe_inference::SavedConnectionStore for EndpointConnectionStore {
    fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        match self {
            Self::HostKeychain(store) => store.load(),
            #[cfg(test)]
            Self::Fixture(saved) => Ok(saved.clone()),
        }
    }
}

/// A result recorder that also captures dependencies for the Expert's own
/// model dispatch.
///
/// Source-backed Expert input must not dispatch as `Independent`: every
/// dependency recorded for this message is captured here so the model host
/// can project it as the exact dispatch coverage. The store recording still
/// flows to the inner recorder unchanged for the Task report.
pub(super) struct CapturingRecorder<'a> {
    pub(super) inner: Option<&'a dyn ResultRecorder>,
    pub(super) captured: &'a Mutex<Vec<floe_context_contract::ContextDependency>>,
}

impl ResultRecorder for CapturingRecorder<'_> {
    fn record_independent(&self, turn_id: Uuid, result_id: Uuid) -> Result<(), AgentFailure> {
        if let Some(inner) = self.inner {
            inner.record_independent(turn_id, result_id)?;
        }
        Ok(())
    }

    fn record(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure> {
        self.captured
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .push(dependency.clone());
        if let Some(inner) = self.inner {
            inner.record(turn_id, result_id, dependency)?;
        }
        Ok(())
    }
}

/// The canonical catalog name for one Expert-declared output data class.
fn tool_output_class(class: DataClass) -> &'static str {
    match class {
        DataClass::Synthetic => "synthetic",
        DataClass::Personal => "personal",
        DataClass::TemporaryAiContext => "temporaryaicontext",
        DataClass::HighlySensitive => "highlysensitive",
        DataClass::DeviceOnlyRaw => "deviceonlyraw",
        DataClass::Credential => "credential",
    }
}

/// Map Expert-declared capabilities to the canonical tool catalog.
///
/// Experts never delegate, so the card list stays empty: any Delegate output
/// fails closed in Inference. Legacy capability versions are semver strings
/// carried opaquely (only id matching ever bound them), so every declared
/// capability maps to revision 1 with its id, schema and output class intact.
fn expert_catalog(
    capabilities: &[floe_agent_contract::CapabilityDescriptor],
) -> Result<AllowedCatalog, AgentFailure> {
    let mut tools = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        if capability.schema_version != AGENT_VERSION
            || !capability.read_only
            || capability.id.trim().is_empty()
            || capability.version.trim().is_empty()
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let descriptor = ToolDescriptor {
            id: capability.id.clone(),
            definition_revision: 1,
            description: format!("Expert capability {}", capability.id),
            input_schema: capability
                .input_schema
                .clone()
                .map(|schema| schema.to_string())
                .unwrap_or_else(|| "{}".into()),
            output_data_class: tool_output_class(capability.output_data_class).into(),
        };
        descriptor.validate()?;
        tools.push(descriptor);
    }
    Ok(AllowedCatalog {
        cards: vec![],
        tools,
        revision: 1,
    })
}

/// Map an Expert's working transcript to the canonical model conversation.
///
/// Past capability results re-enter as Tool exchanges bound to the declared
/// catalog revisions; their source coverage travels separately through the
/// captured record dependencies, so exchanges stay `Independent` here without
/// losing the exact dispatch coverage.
fn expert_conversation(
    transcript: Vec<ExpertTranscriptEntry>,
    catalog: &AllowedCatalog,
) -> Result<ModelConversation, AgentFailure> {
    let mut current_turn = Vec::with_capacity(transcript.len());
    for entry in transcript {
        match entry {
            ExpertTranscriptEntry::Task { text } => {
                current_turn.push(ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text,
                });
            }
            ExpertTranscriptEntry::Preamble { text } => {
                current_turn.push(ModelConversationEntry::Preamble {
                    message_id: Uuid::new_v4(),
                    text,
                });
            }
            ExpertTranscriptEntry::Capability {
                call_id,
                capability_id,
                input,
                result,
            } => {
                if call_id.is_nil() {
                    return Err(AgentFailure::InvalidInput);
                }
                let definition_revision = catalog
                    .tools
                    .iter()
                    .find(|tool| tool.id == capability_id)
                    .map(|tool| tool.definition_revision)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                current_turn.push(ModelConversationEntry::ToolExchange {
                    call: ToolCall {
                        call_id,
                        invocation_key: InvocationKey::new(),
                        tool_id: capability_id,
                        definition_revision,
                        input,
                    },
                    result: ToolResult {
                        call_id,
                        text: result,
                        artifacts: vec![],
                        coverage: DependencyCoverage::Independent,
                        issue: None,
                    },
                });
            }
        }
    }
    Ok(ModelConversation {
        history: vec![],
        current_turn,
    })
}

/// The model an Expert reasons on, as the Expert's own contract states it.
///
/// An Expert asks one question and is owed one answer. Projecting the
/// authorized input is Context's work; selecting the profile, fencing the
/// dispatch, and settling the attempt is Inference's. This host only binds
/// the two: the Expert call becomes a Context projection plus a canonical
/// model request under a bounded child of the Task scope.
pub(crate) struct ExpertModelHost<'a> {
    pub(crate) executor: &'a dyn InferenceExecutor,
    pub(crate) scope: &'a floe_execution::ExecutionScope,
    pub(crate) captured: &'a Mutex<Vec<floe_context_contract::ContextDependency>>,
}

impl ExpertModelHost<'_> {
    fn captured_dependencies(
        &self,
    ) -> Result<Vec<floe_context_contract::ContextDependency>, AgentFailure> {
        self.captured
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| AgentFailure::StorageUnavailable)
    }

    fn child_scope(
        &self,
        deadline: tokio::time::Instant,
        max_tokens: u64,
        max_cost_micros: u64,
        invocation_id: Uuid,
    ) -> floe_execution::ExecutionScope {
        self.scope.child_scope(
            deadline,
            max_tokens,
            max_cost_micros,
            TaskId::from_uuid(invocation_id),
        )
    }
}

impl floe_agent_contract::ExpertModel for ExpertModelHost<'_> {
    fn answer<'a>(
        &'a self,
        call: ExpertModelCall,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertModelAnswer, AgentFailure>> {
        Box::pin(async move {
            if call.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if call.deadline <= tokio::time::Instant::now() {
                return Err(AgentFailure::DeadlineExceeded);
            }
            call.prompt.validate()?;
            if call.assignment.trim().is_empty() || call.assignment.len() > 2048 {
                return Err(AgentFailure::InvalidInput);
            }
            let dependencies = self.captured_dependencies()?;
            let catalog = AllowedCatalog {
                cards: vec![],
                tools: vec![],
                revision: 1,
            };
            let conversation = ModelConversation {
                history: vec![],
                current_turn: vec![ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: call.assignment,
                }],
            };
            let projection = floe_context::assemble_context_projection(
                floe_context::ContextProjectionInput {
                    role: floe_context::ContextProjectionRole::Expert,
                    purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                    response_contract: "One answer to the Expert assignment.",
                    correction: None,
                    prompt: call.prompt,
                    conversation,
                    agent_context: &call.context,
                    catalog: &catalog,
                    active_experts: &[],
                    authorized_history_dependencies: &dependencies,
                    input_data_classes: call.policy.data_classes.clone(),
                    max_output_bytes: call.max_output_bytes,
                },
            )?;
            let child = self.child_scope(
                call.deadline,
                call.max_tokens,
                call.max_cost_micros,
                call.invocation_id,
            );
            let response = self
                .executor
                .execute(
                    ModelRequest {
                        attempt_id: Uuid::new_v4(),
                        principal: call.person_id.to_string(),
                        projection,
                        catalog,
                        purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE.into(),
                        consumer: floe_agent_contract::EXPERT_INFERENCE_CONSUMER.into(),
                        preferred_profile_id: None,
                        replay: vec![],
                    },
                    &child,
                    execution_constraint(call.requirement),
                )
                .await?;
            // One question, one reply: a preamble, a tool call or a
            // delegation is not an answer to an Expert's assignment.
            let [ModelStep::Answer { text, .. }] = response.steps.as_slice() else {
                return Err(AgentFailure::InvalidModelOutput);
            };
            Ok(ExpertModelAnswer {
                schema_version: AGENT_VERSION,
                answer: text.clone(),
                used_tokens: response.usage.tokens,
                cost_micros: response.usage.cost_micros,
            })
        })
    }
}

impl floe_agent_contract::ExpertReasoner for ExpertModelHost<'_> {
    fn step<'a>(
        &'a self,
        step: ExpertReasoningStep,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertStepOutcome, AgentFailure>> {
        Box::pin(async move {
            if step.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if step.deadline <= tokio::time::Instant::now() {
                return Err(AgentFailure::DeadlineExceeded);
            }
            step.prompt.validate()?;
            // The Expert's transcript is its own; it becomes a canonical
            // conversation only for as long as the model call lasts.
            let catalog = expert_catalog(&step.capabilities)?;
            let conversation = expert_conversation(step.transcript, &catalog)?;
            let dependencies = self.captured_dependencies()?;
            let projection = floe_context::assemble_context_projection(
                floe_context::ContextProjectionInput {
                    role: floe_context::ContextProjectionRole::Expert,
                    purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                    response_contract: "One Expert reasoning step.",
                    correction: None,
                    prompt: step.prompt,
                    conversation,
                    agent_context: &step.context,
                    catalog: &catalog,
                    active_experts: &[],
                    authorized_history_dependencies: &dependencies,
                    input_data_classes: step.policy.data_classes.clone(),
                    max_output_bytes: step.max_output_bytes,
                },
            )?;
            let child = self.child_scope(
                step.deadline,
                step.remaining_tokens,
                step.remaining_cost_micros,
                step.invocation_id,
            );
            let response = self
                .executor
                .execute(
                    ModelRequest {
                        attempt_id: Uuid::new_v4(),
                        principal: step.person_id.to_string(),
                        projection,
                        catalog,
                        purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE.into(),
                        consumer: floe_agent_contract::EXPERT_INFERENCE_CONSUMER.into(),
                        preferred_profile_id: None,
                        replay: vec![],
                    },
                    &child,
                    execution_constraint(step.requirement),
                )
                .await?;
            Ok(ExpertStepOutcome {
                schema_version: AGENT_VERSION,
                steps: response
                    .steps
                    .into_iter()
                    .map(|step| match step {
                        ModelStep::Preamble { text } => Ok(ExpertStep::Preamble { text }),
                        ModelStep::Answer { text, .. } => Ok(ExpertStep::Answer { text }),
                        ModelStep::CallTool { tool_id, input, .. } => {
                            Ok(ExpertStep::Call {
                                capability_id: tool_id,
                                input,
                            })
                        }
                        // An Expert has no one to delegate to.
                        ModelStep::Delegate { .. } => Err(AgentFailure::CapabilityDenied),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                replay: None,
                used_tokens: response.usage.tokens,
                cost_micros: response.usage.cost_micros,
            })
        })
    }
}

pub(super) struct PersonalViewSource<'a> {
    pub(super) server_source_allowed: bool,
    pub(super) source_client: Option<&'a ServerSourceClient>,
    pub(super) person_id: PersonId,
    pub(super) people_reader: Option<&'a dyn PersonalPeopleReaderApi>,
    pub(super) wellbeing_reader: Option<&'a dyn PersonalWellbeingReaderApi>,
    pub(super) remote_reader: Option<&'a dyn floe_context::SourceReader>,
    pub(super) recorder: Option<&'a dyn ResultRecorder>,
    pub(super) dependency_turn_id: Uuid,
    pub(super) dependency_result_id: Uuid,
    pub(super) consumer_name: &'a str,
}

impl PersonalViewSource<'_> {
    fn record_result_independent(&self) -> Result<(), AgentFailure> {
        if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
            recorder.record_independent(self.dependency_turn_id, self.dependency_result_id)?;
        }
        Ok(())
    }

    pub(super) async fn people_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<PeopleView, AgentFailure> {
        self.record_result_independent()?;
        let reader = self
            .people_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let (view, dependency) = reader
            .read(self.person_id, self.consumer_name, deadline, cancellation)
            .await?;
        if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
            recorder.record(
                self.dependency_turn_id,
                self.dependency_result_id,
                dependency,
            )?;
        }
        Ok(view)
    }

    pub(super) async fn wellbeing_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<WellbeingView, AgentFailure> {
        self.record_result_independent()?;
        let reader = self
            .wellbeing_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let (view, dependency) = reader
            .read(
                self.person_id,
                self.consumer_name,
                self.dependency_result_id,
                deadline,
                cancellation,
            )
            .await?;
        if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
            recorder.record(
                self.dependency_turn_id,
                self.dependency_result_id,
                dependency,
            )?;
        }
        Ok(view)
    }

    pub(super) async fn calendar_views(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<CalendarContextView>, AgentFailure> {
        if !self.server_source_allowed {
            return Ok(vec![]);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        let now = i64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?;
        let source_client = self
            .source_client
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        // Source enumeration happens here, when the Expert actually needs it:
        // the catalog is observed lazily, never pre-resolved for the turn.
        let connections = source_client
            .observe_calendar_connections(deadline, cancellation)
            .await?;
        let mut views = Vec::new();
        for connection in &connections {
            match source_client
                .read_calendar_context_view(
                    CalendarContextRequest {
                        connector_id: &connection.connector_id,
                        connection_id: &connection.connection_id,
                        connection_revision: connection.connection_revision,
                        range_start_unix_ms: now.saturating_sub(86_400_000),
                        range_end_unix_ms: now.saturating_add(86_400_000),
                        cursor: "",
                        limit: floe_context::MAX_CALENDAR_CONTEXT_ITEMS,
                    },
                    deadline,
                    cancellation,
                )
                .await
            {
                Ok(view) => views.push(view),
                Err(AgentFailure::CapabilityUnavailable) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(views)
    }

    pub(super) async fn confirmed_interaction_views(
        &self,
        people: &PeopleView,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<floe_context::ConfirmedInteractionView>, AgentFailure> {
        if !self.server_source_allowed {
            return Ok(vec![]);
        }
        let source_client = self
            .source_client
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        match source_client
            .read_confirmed_interaction_view(people, deadline, cancellation)
            .await
        {
            Ok(view) => Ok(vec![view]),
            Err(AgentFailure::CapabilityUnavailable) => Ok(vec![]),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn work_context_views(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<floe_context::WorkContextView>, AgentFailure> {
        if !self.server_source_allowed {
            return Ok(vec![]);
        }
        let Some(reader) = self.remote_reader else {
            return Err(AgentFailure::CapabilityUnavailable);
        };
        match read_context_source(
            reader,
            self.person_id,
            "work.context",
            self.consumer_name,
            serde_json::json!({"schema_version": AGENT_VERSION}),
            deadline,
            cancellation,
        )
        .await
        {
            Ok(source_view) => {
                if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
                    recorder.record(
                        self.dependency_turn_id,
                        self.dependency_turn_id,
                        source_view.dependency().clone(),
                    )?;
                }
                let view = serde_json::from_value(source_view.payload().clone())
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                Ok(vec![view])
            }
            Err(AgentFailure::CapabilityUnavailable) => Ok(vec![]),
            Err(error) => Err(error),
        }
    }

}

pub(super) async fn read_context_source(
    reader: &dyn floe_context::SourceReader,
    person_id: PersonId,
    source_id: &str,
    consumer: &str,
    query: serde_json::Value,
    deadline: tokio::time::Instant,
    cancellation: &floe_execution::Cancellation,
) -> Result<floe_context::SourceView<serde_json::Value>, AgentFailure> {
    let prepared = floe_context::ContextService::new(Some(reader)).prepare(person_id)?;
    let source_request = prepared.source_request(
        source_id,
        floe_context_contract::GrantConsumer::builtin(consumer)
            .map_err(|_| AgentFailure::InvalidInput)?,
        floe_context_contract::GrantPurpose::Assistant,
        query,
        deadline,
        cancellation.clone(),
    )?;
    prepared.read_source(&source_request).await
}

pub(super) trait ResultRecorder: Send + Sync {
    fn record_independent(&self, turn_id: Uuid, result_id: Uuid) -> Result<(), AgentFailure>;

    fn record(
        &self,
        turn_id: uuid::Uuid,
        result_id: uuid::Uuid,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure>;
}

pub(super) struct StoreResultRecorder<'a, Keys: VaultKeyProvider> {
    pub(super) store: &'a floe_conversation::GovernedSessionStore<'a, EncryptedAgentVault<Keys>>,
}

impl<Keys: VaultKeyProvider> floe_experts::TaskCoverageRecorder for StoreResultRecorder<'_, Keys> {
    fn record_independent(&self, turn_id: Uuid, result_id: Uuid) -> Result<(), AgentFailure> {
        self.store.record_result_independent(turn_id, result_id)
    }

    fn record(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure> {
        self.store
            .record_result_dependency(turn_id, result_id, dependency)
    }
}

impl<Keys: VaultKeyProvider> ResultRecorder for StoreResultRecorder<'_, Keys> {
    fn record_independent(&self, turn_id: Uuid, result_id: Uuid) -> Result<(), AgentFailure> {
        self.store.record_result_independent(turn_id, result_id)
    }

    fn record(
        &self,
        turn_id: uuid::Uuid,
        result_id: uuid::Uuid,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure> {
        self.store
            .record_result_dependency(turn_id, result_id, dependency)
    }
}

pub(super) trait PersonalAttentionReaderApi: Send + Sync {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'static str,
        call_id: uuid::Uuid,
        turn_id: uuid::Uuid,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (AttentionView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

pub(super) trait PersonalPeopleReaderApi: Send + Sync {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (PeopleView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

pub(super) trait PersonalWellbeingReaderApi: Send + Sync {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        call_id: Uuid,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (WellbeingView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

pub(super) struct PersonalAttentionReader<'a, Keys: VaultKeyProvider> {
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) local_context: &'a LocalContextHost,
    pub(super) device_id: &'a str,
}

impl<Keys: VaultKeyProvider> PersonalAttentionReaderApi for PersonalAttentionReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'static str,
        call_id: uuid::Uuid,
        turn_id: uuid::Uuid,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (AttentionView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if tokio::time::Instant::now() >= deadline || cancellation.is_cancelled() {
                return Err(if cancellation.is_cancelled() {
                    AgentFailure::Cancelled
                } else {
                    AgentFailure::DeadlineExceeded
                });
            }
            let (view, dependency) = floe_context::admit_attention(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                floe_access::attention_consumer(consumer)?,
                call_id,
                deadline,
                cancellation,
            )
            .await?;
            let _ = turn_id;
            Ok((view, dependency))
        })
    }
}

pub(super) struct PersonalPeopleReader<'a, Keys: VaultKeyProvider> {
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) local_context: &'a LocalContextHost,
    pub(super) device_id: &'a str,
}

pub(super) struct PersonalWellbeingReader<'a, Keys: VaultKeyProvider> {
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) local_context: &'a LocalContextHost,
    pub(super) device_id: &'a str,
}

impl<Keys: VaultKeyProvider> PersonalWellbeingReaderApi for PersonalWellbeingReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        call_id: Uuid,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (WellbeingView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            floe_context::read_wellbeing(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                consumer,
                call_id,
                deadline,
                cancellation,
            )
            .await
        })
    }
}

impl<Keys: VaultKeyProvider> PersonalPeopleReaderApi for PersonalPeopleReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (PeopleView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let grants = self.vault.list_data_access_grants(128).await?;
            let grant =
                floe_access::people_read_grant(&grants, person_id, self.device_id, consumer)?;
            let selected_handles = self
                .vault
                .personal_grant_selected_handles(grant.id())
                .await?;
            if selected_handles.is_empty() {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let subject = self
                .vault
                .personal_grant_subject_fingerprint(grant.id())
                .await?;
            floe_context::read_people(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                grant.source().clone(),
                &selected_handles,
                &subject,
                consumer,
                deadline,
                cancellation,
            )
            .await
        })
    }
}

pub(super) trait ConversationContextReaderApi: Send + Sync {
    fn memory<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<floe_knowledge::MemoryContextSnapshot, AgentFailure>>
                + Send
                + 'a,
        >,
    >;

    fn tasks<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<NativeContextView, AgentFailure>> + Send + 'a>>;
}

pub(super) struct ConversationContextReader<'a, Keys: VaultKeyProvider> {
    pub(super) core: &'a FloeCore,
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) person_id: PersonId,
}

impl<Keys: VaultKeyProvider> ConversationContextReaderApi for ConversationContextReader<'_, Keys> {
    fn memory<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<floe_knowledge::MemoryContextSnapshot, AgentFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(floe_context::acquire_memory_context(
            self.vault,
            chrono::Utc::now(),
        ))
    }

    fn tasks<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<NativeContextView, AgentFailure>> + Send + 'a>> {
        let handle = uuid::Uuid::new_v5(&self.person_id.0, b"floe.tasks");
        Box::pin(
            floe_context::task_context_view(
                &self.core.store,
                self.person_id,
                handle,
                chrono::Utc::now(),
                16,
                8 * 1024,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::{ExpertModel, ExpertReasoner};
    use std::collections::VecDeque;

    struct FakeExecutor {
        calls: Mutex<
            Vec<(
                floe_agent_contract::ModelRequest,
                floe_inference::InferenceExecutionConstraint,
            )>,
        >,
        script: Mutex<VecDeque<Result<Vec<ModelStep>, AgentFailure>>>,
    }

    impl FakeExecutor {
        fn new(script: Vec<Result<Vec<ModelStep>, AgentFailure>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                script: Mutex::new(script.into_iter().collect()),
            }
        }

        fn calls(
            &self,
        ) -> Vec<(
            floe_agent_contract::ModelRequest,
            floe_inference::InferenceExecutionConstraint,
        )> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl floe_inference::InferenceExecutor for FakeExecutor {
        fn execute<'a>(
            &'a self,
            request: floe_agent_contract::ModelRequest,
            _scope: &'a floe_execution::ExecutionScope,
            constraint: floe_inference::InferenceExecutionConstraint,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ModelResponse, AgentFailure>,
        > {
            self.calls.lock().unwrap().push((request.clone(), constraint));
            let next = self.script.lock().unwrap().pop_front().unwrap();
            Box::pin(async move {
                Ok(floe_agent_contract::ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: next?,
                    usage: floe_agent_contract::ModelUsage {
                        tokens: 11,
                        cost_micros: 22,
                    },
                })
            })
        }
    }

    fn test_scope() -> floe_execution::ExecutionScope {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(1_000_000, 1_000_000_000),
            Default::default(),
        );
        floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()),
        )
    }

    fn test_context() -> floe_agent_contract::AgentContext {
        floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        }
    }

    fn test_call() -> ExpertModelCall {
        ExpertModelCall {
            person_id: PersonId::new(),
            invocation_id: Uuid::new_v4(),
            prompt: floe_experts_builtin::prompts::focus_expert_prompt(),
            policy: expert_policy(),
            context: test_context(),
            assignment: "Protect the current focus period.".into(),
            requirement: ExpertModelRequirement::DeviceOnly,
            max_output_bytes: 8192,
            max_tokens: 4096,
            max_cost_micros: 1_000,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            cancellation: floe_execution::Cancellation::default(),
        }
    }

    fn test_capability(id: &str) -> floe_agent_contract::CapabilityDescriptor {
        floe_agent_contract::CapabilityDescriptor {
            schema_version: AGENT_VERSION,
            id: id.into(),
            version: "1.0.0".into(),
            read_only: true,
            output_data_class: DataClass::Personal,
            input_schema: Some(serde_json::json!({"type": "object"})),
        }
    }

    fn test_step() -> ExpertReasoningStep {
        ExpertReasoningStep {
            person_id: PersonId::new(),
            invocation_id: Uuid::new_v4(),
            prompt: floe_experts_builtin::prompts::focus_expert_prompt(),
            policy: expert_policy(),
            context: test_context(),
            requirement: ExpertModelRequirement::Any,
            transcript: vec![ExpertTranscriptEntry::Task {
                text: "Review the selected context.".into(),
            }],
            capabilities: vec![test_capability("calendar.read")],
            replay: vec![],
            remaining_tokens: 4096,
            remaining_cost_micros: 1_000,
            max_output_bytes: 8192,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            cancellation: floe_execution::Cancellation::default(),
        }
    }

    #[tokio::test]
    async fn single_answer_accepts_exactly_one_answer_with_inference_usage() {
        let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Answer {
            text: "Protect focus.".into(),
            artifacts: vec![],
        }])]);
        let scope = test_scope();
        let captured = Mutex::new(Vec::new());
        let host = ExpertModelHost {
            executor: &executor,
            scope: &scope,
            captured: &captured,
        };
        let answer = ExpertModel::answer(&host, test_call()).await.unwrap();
        assert_eq!(answer.schema_version, AGENT_VERSION);
        assert_eq!(answer.answer, "Protect focus.");
        assert_eq!(answer.used_tokens, 11);
        assert_eq!(answer.cost_micros, 22);
        let calls = executor.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].0.purpose,
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE
        );
        assert_eq!(
            calls[0].0.consumer,
            floe_agent_contract::EXPERT_INFERENCE_CONSUMER
        );
        assert_eq!(
            calls[0].1,
            floe_inference::InferenceExecutionConstraint::DeviceOnly
        );
        assert!(calls[0].0.catalog.tools.is_empty());
        assert!(calls[0].0.catalog.cards.is_empty());
    }

    #[tokio::test]
    async fn single_answer_rejects_non_answers() {
        for steps in [
            vec![],
            vec![ModelStep::Preamble {
                text: "Thinking.".into(),
            }],
            vec![ModelStep::CallTool {
                tool_id: "calendar.read".into(),
                definition_revision: 1,
                input: "{}".into(),
            }],
            vec![ModelStep::Delegate {
                agent_id: "floe.builtin.focus.v1".into(),
                definition_revision: 1,
                message: "hi".into(),
                context_refs: vec![],
            }],
            vec![
                ModelStep::Answer {
                    text: "one".into(),
                    artifacts: vec![],
                },
                ModelStep::Answer {
                    text: "two".into(),
                    artifacts: vec![],
                },
            ],
        ] {
            let executor = FakeExecutor::new(vec![Ok(steps)]);
            let scope = test_scope();
            let captured = Mutex::new(Vec::new());
            let host = ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &captured,
            };
            assert_eq!(
                ExpertModel::answer(&host, test_call()).await.err(),
                Some(AgentFailure::InvalidModelOutput)
            );
        }
    }

    #[tokio::test]
    async fn single_answer_maps_each_requirement_to_its_constraint() {
        for (requirement, constraint) in [
            (
                ExpertModelRequirement::Any,
                floe_inference::InferenceExecutionConstraint::Any,
            ),
            (
                ExpertModelRequirement::DeviceOnly,
                floe_inference::InferenceExecutionConstraint::DeviceOnly,
            ),
            (
                ExpertModelRequirement::RemoteOnly,
                floe_inference::InferenceExecutionConstraint::RemoteOnly,
            ),
        ] {
            let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Answer {
                text: "ok".into(),
                artifacts: vec![],
            }])]);
            let scope = test_scope();
            let captured = Mutex::new(Vec::new());
            let host = ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &captured,
            };
            let mut call = test_call();
            call.requirement = requirement;
            ExpertModel::answer(&host, call).await.unwrap();
            assert_eq!(executor.calls()[0].1, constraint);
        }
    }

    #[tokio::test]
    async fn source_backed_input_dispatches_with_exact_captured_coverage() {
        let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Answer {
            text: "ok".into(),
            artifacts: vec![],
        }])]);
        let scope = test_scope();
        let person_id = PersonId::new();
        let now = chrono::Utc::now();
        let dependency = floe_context_contract::ContextDependency::try_new(
            person_id,
            floe_context_contract::GrantId::new(),
            floe_context_contract::GrantAuthority::new(),
            floe_context_contract::GrantSourceBinding::try_new(
                person_id,
                floe_context_contract::ConnectionId::try_new("connection").unwrap(),
                floe_context_contract::ConnectorId::try_new("connector").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("owner").unwrap(),
                floe_context_contract::SourceAuthority::new(),
            )
            .unwrap(),
            vec![floe_context_contract::ResourceHandle::try_new("resource").unwrap()],
            vec![floe_context_contract::GrantDataCategory::Metadata],
            floe_context_contract::GrantOperation::Read,
            floe_context_contract::GrantPurpose::Assistant,
            floe_context_contract::GrantConsumer::builtin("expert").unwrap(),
            floe_context_contract::ProcessingRestriction::LocalOnly,
            floe_context_contract::ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            vec![7; 32],
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap();
        let captured = Mutex::new(vec![dependency.clone()]);
        let host = ExpertModelHost {
            executor: &executor,
            scope: &scope,
            captured: &captured,
        };
        ExpertModel::answer(&host, test_call()).await.unwrap();
        let calls = executor.calls();
        match &calls[0].0.projection.coverage {
            floe_agent_contract::DependencyCoverage::Dependent { dependencies } => {
                assert_eq!(dependencies.as_slice(), &[dependency]);
            }
            coverage => panic!("source-backed input must not be {coverage:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_expert_input_fails_closed_without_dispatch() {
        let scope = test_scope();
        // Empty assignment, empty prompt, empty data classes, oversized
        // output, and cancelled/expired calls never reach Inference.
        // Evidence shape stays the view readers' and the Expert pre-check's
        // responsibility, exactly as on the canonical root path.
        let mut empty_assignment = test_call();
        empty_assignment.assignment = "   ".into();
        let mut empty_prompt = test_call();
        empty_prompt.prompt.components.clear();
        let mut empty_classes = test_call();
        empty_classes.policy.data_classes.clear();
        let mut too_large = test_call();
        too_large.max_output_bytes = usize::MAX;
        let cancelled = {
            let call = test_call();
            call.cancellation.cancel();
            call
        };
        let mut expired = test_call();
        expired.deadline = tokio::time::Instant::now();
        for (index, call) in [
            empty_assignment,
            empty_prompt,
            empty_classes,
            too_large,
            cancelled,
            expired,
        ]
        .into_iter()
        .enumerate()
        {
            let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Answer {
                text: "unreachable".into(),
                artifacts: vec![],
            }])]);
            let captured = Mutex::new(Vec::new());
            let host = ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &captured,
            };
            let result = ExpertModel::answer(&host, call).await;
            assert!(result.is_err(), "case {index} unexpectedly succeeded");
            assert!(executor.calls().is_empty());
        }
    }

    #[tokio::test]
    async fn reasoning_step_maps_declared_capabilities_and_denies_delegation() {
        let call_id = Uuid::new_v4();
        let executor = FakeExecutor::new(vec![Ok(vec![
            ModelStep::Preamble {
                text: "Checking.".into(),
            },
            ModelStep::CallTool {
                tool_id: "calendar.read".into(),
                definition_revision: 1,
                input: "{}".into(),
            },
        ])]);
        let scope = test_scope();
        let captured = Mutex::new(Vec::new());
        let host = ExpertModelHost {
            executor: &executor,
            scope: &scope,
            captured: &captured,
        };
        let mut step = test_step();
        step.transcript.push(ExpertTranscriptEntry::Capability {
            call_id,
            capability_id: "calendar.read".into(),
            input: "{}".into(),
            result: "no conflicts".into(),
        });
        let outcome = ExpertReasoner::step(&host, step).await.unwrap();
        assert_eq!(outcome.schema_version, AGENT_VERSION);
        assert_eq!(outcome.used_tokens, 11);
        assert_eq!(outcome.cost_micros, 22);
        assert_eq!(
            outcome.steps.as_slice(),
            [
                ExpertStep::Preamble {
                    text: "Checking.".into()
                },
                ExpertStep::Call {
                    capability_id: "calendar.read".into(),
                    input: "{}".into(),
                },
            ]
        );
        let calls = executor.calls();
        assert_eq!(calls[0].0.catalog.tools.len(), 1);
        assert_eq!(calls[0].0.catalog.tools[0].id, "calendar.read");

        let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Delegate {
            agent_id: "floe.builtin.focus.v1".into(),
            definition_revision: 1,
            message: "you take it".into(),
            context_refs: vec![],
        }])]);
        let host = ExpertModelHost {
            executor: &executor,
            scope: &scope,
            captured: &captured,
        };
        assert_eq!(
            ExpertReasoner::step(&host, test_step()).await.err(),
            Some(AgentFailure::CapabilityDenied)
        );
    }

    #[tokio::test]
    async fn reasoning_step_rejects_undeclared_or_unreadable_capabilities() {
        let scope = test_scope();
        // Transcript references a capability the step did not declare.
        let mut undeclared = test_step();
        undeclared.transcript.push(ExpertTranscriptEntry::Capability {
            call_id: Uuid::new_v4(),
            capability_id: "calendar.write".into(),
            input: "{}".into(),
            result: "done".into(),
        });
        // Non-read-only and wrong-schema capabilities are denied like the
        // legacy transport denied them.
        let mut writable = test_step();
        writable.capabilities = vec![floe_agent_contract::CapabilityDescriptor {
            read_only: false,
            ..test_capability("calendar.read")
        }];
        let mut wrong_schema = test_step();
        wrong_schema.capabilities = vec![floe_agent_contract::CapabilityDescriptor {
            schema_version: AGENT_VERSION + 1,
            ..test_capability("calendar.read")
        }];
        // A reasoning step without its task has no canonical conversation.
        let mut taskless = test_step();
        taskless.transcript = vec![ExpertTranscriptEntry::Preamble {
            text: "no task".into(),
        }];
        for step in [undeclared, writable, wrong_schema, taskless] {
            let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Answer {
                text: "unreachable".into(),
                artifacts: vec![],
            }])]);
            let captured = Mutex::new(Vec::new());
            let host = ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &captured,
            };
            assert!(ExpertReasoner::step(&host, step).await.is_err());
            assert!(executor.calls().is_empty());
        }
    }

    #[test]
    fn capturing_recorder_forwards_to_the_store_and_keeps_a_copy() {
        struct Probe {
            records: Mutex<Vec<floe_context_contract::ContextDependency>>,
        }
        impl ResultRecorder for Probe {
            fn record_independent(&self, _: Uuid, _: Uuid) -> Result<(), AgentFailure> {
                Ok(())
            }
            fn record(
                &self,
                _: Uuid,
                _: Uuid,
                dependency: floe_context_contract::ContextDependency,
            ) -> Result<(), AgentFailure> {
                self.records.lock().unwrap().push(dependency);
                Ok(())
            }
        }
        let probe = Probe {
            records: Mutex::new(Vec::new()),
        };
        let captured = Mutex::new(Vec::new());
        let recorder = CapturingRecorder {
            inner: Some(&probe),
            captured: &captured,
        };
        let person_id = PersonId::new();
        let now = chrono::Utc::now();
        let dependency = floe_context_contract::ContextDependency::try_new(
            person_id,
            floe_context_contract::GrantId::new(),
            floe_context_contract::GrantAuthority::new(),
            floe_context_contract::GrantSourceBinding::try_new(
                person_id,
                floe_context_contract::ConnectionId::try_new("connection").unwrap(),
                floe_context_contract::ConnectorId::try_new("connector").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("owner").unwrap(),
                floe_context_contract::SourceAuthority::new(),
            )
            .unwrap(),
            vec![floe_context_contract::ResourceHandle::try_new("resource").unwrap()],
            vec![floe_context_contract::GrantDataCategory::Metadata],
            floe_context_contract::GrantOperation::Read,
            floe_context_contract::GrantPurpose::Assistant,
            floe_context_contract::GrantConsumer::builtin("expert").unwrap(),
            floe_context_contract::ProcessingRestriction::LocalOnly,
            floe_context_contract::ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            vec![7; 32],
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap();
        recorder
            .record(Uuid::new_v4(), Uuid::new_v4(), dependency.clone())
            .unwrap();
        assert_eq!(captured.lock().unwrap().as_slice(), &[dependency.clone()]);
        assert_eq!(probe.records.lock().unwrap().as_slice(), &[dependency]);
        let bare = CapturingRecorder {
            inner: None,
            captured: &captured,
        };
        bare.record_independent(Uuid::new_v4(), Uuid::new_v4())
            .unwrap();
    }
}
