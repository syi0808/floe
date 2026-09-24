//! Canonical delegated Expert model/source host.
//!
//! Delegated built-in Experts reason through shared Inference: the Expert
//! states what execution class it requires, Context projects the authorized
//! input, Inference selects the profile and owns the attempt, and Access
//! fences the dispatch. No provider, route, or usage ledger is selected here.

use std::{future::Future, pin::Pin, sync::Mutex};

use floe_agent_contract::{
    AGENT_VERSION, AgentFailure, AllowedCatalog, DataClass, DependencyCoverage, ExpertModelAnswer,
    ExpertModelCall, ExpertModelRequirement, ExpertReasoningStep, ExpertStep, ExpertStepOutcome,
    ExpertTranscriptEntry, InferencePolicyDecision, InvocationKey, ModelConversation,
    ModelConversationEntry, ModelPlacement, ModelRequest, ModelStep, ToolCall, ToolDescriptor,
    ToolResult, TransferConsent,
};
use floe_context::{
    AttentionView, CalendarContextView, NativeContextView, PeopleView, PersonalGrantRecords,
    WellbeingView,
};
use floe_inference::{InferenceExecutionConstraint, InferenceExecutor};
use floe_kernel::{PersonId, TaskId};
use floe_provider_adapters::sources::ServerSourceClient;
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
fn execution_constraint(requirement: ExpertModelRequirement) -> InferenceExecutionConstraint {
    match requirement {
        ExpertModelRequirement::Any => InferenceExecutionConstraint::Any,
        ExpertModelRequirement::DeviceOnly => InferenceExecutionConstraint::DeviceOnly,
        ExpertModelRequirement::RemoteOnly => InferenceExecutionConstraint::RemoteOnly,
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
                observation,
            } => {
                if call_id.is_nil() {
                    return Err(AgentFailure::InvalidInput);
                }
                observation.validate(4096)?;
                let (text, artifacts, issue) = match observation {
                    floe_agent_contract::ExpertCapabilityObservation::Success { result } => {
                        (result, vec![], None)
                    }
                    floe_agent_contract::ExpertCapabilityObservation::Unavailable {
                        reason_code,
                    } => (
                        reason_code,
                        vec![],
                        Some(floe_agent_contract::OutcomeIssue {
                            failure: AgentFailure::CapabilityUnavailable,
                            retryable: false,
                        }),
                    ),
                    floe_agent_contract::ExpertCapabilityObservation::NeedsUserAction {
                        interaction,
                        summary,
                    } => {
                        let data = serde_json::to_string(&interaction)
                            .map_err(|_| AgentFailure::InvalidModelOutput)?;
                        (
                            summary,
                            vec![floe_agent_contract::Artifact {
                                artifact_id: Uuid::new_v4(),
                                name: "user_interaction".into(),
                                parts: vec![floe_agent_contract::ArtifactPart::Data {
                                    media_type: floe_agent_contract::USER_INTERACTION_MEDIA_TYPE
                                        .into(),
                                    data,
                                }],
                                coverage: DependencyCoverage::Independent,
                            }],
                            Some(floe_agent_contract::OutcomeIssue {
                                failure: AgentFailure::CapabilityUnavailable,
                                retryable: false,
                            }),
                        )
                    }
                };
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
                        text,
                        artifacts,
                        coverage: DependencyCoverage::Independent,
                        issue,
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
            let projection =
                floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
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
                })?;
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
            let projection =
                floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
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
                })?;
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
                        ModelStep::CallTool { tool_id, input, .. } => Ok(ExpertStep::Call {
                            capability_id: tool_id,
                            input,
                        }),
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
    pub(super) source_client: Option<&'a ServerSourceClient>,
    pub(super) calendar_reader: Option<&'a dyn CalendarContextReaderApi>,
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
    ) -> Result<floe_context_contract::SourceReadOutcome<PeopleView>, AgentFailure> {
        self.record_result_independent()?;
        let reader = self
            .people_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        match reader
            .read(self.person_id, self.consumer_name, deadline, cancellation)
            .await?
        {
            floe_context_contract::SourceReadOutcome::Ready((view, dependency)) => {
                if let (Some(recorder), false) =
                    (self.recorder, self.dependency_turn_id.is_nil())
                {
                    recorder.record(
                        self.dependency_turn_id,
                        self.dependency_result_id,
                        dependency,
                    )?;
                }
                Ok(floe_context_contract::SourceReadOutcome::Ready(view))
            }
            floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
                Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                    reason,
                ))
            }
            floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
                Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                    blockers,
                ))
            }
        }
    }

    pub(super) async fn wellbeing_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<floe_context_contract::SourceReadOutcome<WellbeingView>, AgentFailure> {
        self.record_result_independent()?;
        let reader = self
            .wellbeing_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        match reader
            .read(
                self.person_id,
                self.consumer_name,
                self.dependency_result_id,
                deadline,
                cancellation,
            )
            .await?
        {
            floe_context_contract::SourceReadOutcome::Ready((view, dependency)) => {
                if let (Some(recorder), false) =
                    (self.recorder, self.dependency_turn_id.is_nil())
                {
                    recorder.record(
                        self.dependency_turn_id,
                        self.dependency_result_id,
                        dependency,
                    )?;
                }
                Ok(floe_context_contract::SourceReadOutcome::Ready(view))
            }
            floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
                Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                    reason,
                ))
            }
            floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
                Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                    blockers,
                ))
            }
        }
    }

    pub(super) async fn calendar_views(
        &self,
        query: &floe_context_contract::CalendarViewQuery,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<floe_context_contract::SourceReadOutcome<Vec<CalendarContextView>>, AgentFailure>
    {
        query.validate()?;
        let reader = self
            .calendar_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        self.record_result_independent()?;
        let outcome = reader
            .read(
                self.person_id,
                self.consumer_name,
                query,
                deadline,
                cancellation,
            )
            .await?;
        let reads = match outcome {
            floe_context_contract::SourceReadOutcome::Ready(reads) => reads,
            floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
                return Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                    reason,
                ));
            }
            floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
                return Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                    blockers,
                ));
            }
        };
        let mut views = Vec::new();
        for (view, dependency) in reads {
            floe_context::validate_calendar_context_view_for_query(
                &view,
                query,
                chrono::Utc::now().timestamp_millis(),
            )?;
            if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
                recorder.record(
                    self.dependency_turn_id,
                    self.dependency_result_id,
                    dependency,
                )?;
            }
            views.push(view);
        }
        Ok(floe_context_contract::SourceReadOutcome::Ready(views))
    }

    pub(super) async fn confirmed_interaction_views(
        &self,
        people: &PeopleView,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<floe_context::ConfirmedInteractionView>, AgentFailure> {
        if self.source_client.is_none() {
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
    ) -> Result<
        floe_context_contract::SourceReadOutcome<Vec<floe_context::WorkContextView>>,
        AgentFailure,
    > {
        if self.source_client.is_none() {
            return Ok(floe_context_contract::SourceReadOutcome::Ready(vec![]));
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
        .await?
        {
            floe_context_contract::SourceReadOutcome::Ready(source_view) => {
                if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
                    for binding in source_view.bindings() {
                        recorder.record(
                            self.dependency_turn_id,
                            self.dependency_turn_id,
                            binding.dependency.clone(),
                        )?;
                    }
                }
                let view = serde_json::from_value(source_view.payload().clone())
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                Ok(floe_context_contract::SourceReadOutcome::Ready(vec![view]))
            }
            // Unavailable paired state reads as absent, as before; only a
            // concrete blocker propagates for review.
            floe_context_contract::SourceReadOutcome::Unavailable(_) => {
                Ok(floe_context_contract::SourceReadOutcome::Ready(vec![]))
            }
            floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
                Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                    blockers,
                ))
            }
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
) -> Result<
    floe_context_contract::SourceReadOutcome<floe_context::SourceView<serde_json::Value>>,
    AgentFailure,
> {
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
                        floe_context_contract::SourceReadOutcome<(
                            AttentionView,
                            floe_context_contract::ContextDependency,
                        )>,
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
                        floe_context_contract::SourceReadOutcome<(
                            PeopleView,
                            floe_context_contract::ContextDependency,
                        )>,
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
                        floe_context_contract::SourceReadOutcome<(
                            WellbeingView,
                            floe_context_contract::ContextDependency,
                        )>,
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

pub(super) trait CalendarContextReaderApi: Send + Sync {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        query: &'a floe_context_contract::CalendarViewQuery,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        floe_context_contract::SourceReadOutcome<
                            Vec<(
                                CalendarContextView,
                                floe_context_contract::ContextDependency,
                            )>,
                        >,
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

/// How a calendar admission failure classified against current grant facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CalendarReviewClassification {
    reason: floe_context_contract::SourceAccessRequirementKind,
    observed: Option<floe_context_contract::ObservedGrant>,
}

/// Classify a calendar admission failure against the live grants that bind
/// this connection: none means the grant is missing, one paused or drifted
/// grant is reviewable, and more than one is a duplicate authority the review
/// cannot pick between, so it fails closed with no card.
fn classify_calendar_review(
    grants: &[floe_access::DataAccessGrant],
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
) -> Result<CalendarReviewClassification, AgentFailure> {
    let mut binding = grants.iter().filter(|grant| {
        grant.state() != floe_access::GrantState::Revoked
            && grant.source().person_id() == person_id
            && grant.source().connector().as_str() == connector_id
            && grant.source().connection_id().as_str() == connection_id
    });
    let Some(grant) = binding.next() else {
        return Ok(CalendarReviewClassification {
            reason: floe_context_contract::SourceAccessRequirementKind::EnableObserve,
            observed: None,
        });
    };
    if binding.next().is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    let observed = floe_context_contract::ObservedGrant::try_new(grant.id(), grant.authority())
        .map_err(|_| AgentFailure::StaleContext)?;
    let reason = if grant.state() == floe_access::GrantState::Paused {
        floe_context_contract::SourceAccessRequirementKind::EnableObserve
    } else {
        floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource
    };
    Ok(CalendarReviewClassification {
        reason,
        observed: Some(observed),
    })
}

/// The connector identity this calendar connection grants bind, when the
/// provider has one.
fn calendar_connector_id(connection: &floe_day::CalendarConnection) -> Option<&'static str> {
    floe_access::native_calendar_connector(connection.provider)
        .or_else(|| floe_access::hosted_calendar_connector(connection.provider))
}

/// Observe the one live grant binding this connection for reconnect review.
/// Duplicates fail closed; absence stays a navigation-free reconnect review
/// against the known connection.
fn observe_calendar_binding(
    grants: &[floe_access::DataAccessGrant],
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
) -> Result<Option<floe_context_contract::ObservedGrant>, AgentFailure> {
    let mut binding = grants.iter().filter(|grant| {
        grant.state() != floe_access::GrantState::Revoked
            && grant.source().person_id() == person_id
            && grant.source().connector().as_str() == connector_id
            && grant.source().connection_id().as_str() == connection_id
    });
    let Some(grant) = binding.next() else {
        return Ok(None);
    };
    if binding.next().is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    floe_context_contract::ObservedGrant::try_new(grant.id(), grant.authority())
        .map(Some)
        .map_err(|_| AgentFailure::StaleContext)
}

fn calendar_access_requirement<Value>(
    connection: Option<&floe_day::CalendarConnection>,
    consumer: &str,
    reason: floe_context_contract::SourceAccessRequirementKind,
    observed_grant: Option<floe_context_contract::ObservedGrant>,
) -> Result<floe_context_contract::SourceReadOutcome<Value>, AgentFailure> {
    let connector = connection.and_then(calendar_connector_id);
    let connector_id = connector
        .map(|value| floe_context_contract::ConnectorId::try_new(value.to_owned()))
        .transpose()
        .map_err(|_| AgentFailure::StaleContext)?;
    let connection_id = connection
        .map(|connection| {
            floe_context_contract::ConnectionId::try_new(connection.connection_id.clone())
        })
        .transpose()
        .map_err(|_| AgentFailure::StaleContext)?;
    let resources = connection
        .and_then(|connection| {
            connection
                .calendars
                .iter()
                .map(|calendar| {
                    floe_context_contract::ResourceHandle::try_new(calendar.calendar_id.clone())
                })
                .collect::<Result<Vec<_>, _>>()
                .ok()
        })
        .unwrap_or_default();
    let source_authority = connection
        .map(|connection| connection.source_authority)
        .filter(|authority| authority.is_valid());
    let inline_resolution = connector_id.is_some()
        && connection_id.is_some()
        && !resources.is_empty()
        && source_authority.is_some()
        && reason != floe_context_contract::SourceAccessRequirementKind::Reconnect;
    let requirement = floe_context_contract::SourceAccessRequirement::try_new(
        floe_experts_builtin::BuiltinContextSource::Calendar.source_id(),
        connector_id,
        connection_id,
        floe_context_contract::GrantOperation::Read,
        floe_context_contract::GrantConsumer::builtin(consumer)
            .map_err(|_| AgentFailure::CapabilityDenied)?,
        floe_context_contract::GrantPurpose::Assistant,
        resources,
        None,
        reason,
        source_authority,
        observed_grant,
        inline_resolution,
    )
    .map_err(|_| AgentFailure::StaleContext)?;
    let blockers = floe_context_contract::SourceAccessBlockers::try_new(vec![requirement])
        .map_err(|_| AgentFailure::StaleContext)?;
    Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
        blockers,
    ))
}

fn calendar_read_outcome<Value>(
    result: Result<Value, AgentFailure>,
    connection: &floe_day::CalendarConnection,
    consumer: &str,
    review: &CalendarReviewClassification,
    reconnect_observed: Option<floe_context_contract::ObservedGrant>,
) -> Result<floe_context_contract::SourceReadOutcome<Value>, AgentFailure> {
    match result {
        Ok(value) => Ok(floe_context_contract::SourceReadOutcome::Ready(value)),
        Err(AgentFailure::CapabilityUnavailable) => {
            Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
            ))
        }
        Err(AgentFailure::AccessReviewRequired) => calendar_access_requirement(
            Some(connection),
            consumer,
            review.reason,
            review.observed,
        ),
        Err(AgentFailure::CredentialExpired) => calendar_access_requirement(
            Some(connection),
            consumer,
            floe_context_contract::SourceAccessRequirementKind::Reconnect,
            reconnect_observed,
        ),
        Err(error) => Err(error),
    }
}

pub(super) struct CurrentCalendarContextReader<'a, Keys: VaultKeyProvider> {
    pub(super) core: &'a FloeCore,
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) source_client: Option<&'a ServerSourceClient>,
    pub(super) device_id: &'a str,
}

impl<Keys: VaultKeyProvider> CalendarContextReaderApi for CurrentCalendarContextReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        query: &'a floe_context_contract::CalendarViewQuery,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        floe_context_contract::SourceReadOutcome<
                            Vec<(
                                CalendarContextView,
                                floe_context_contract::ContextDependency,
                            )>,
                        >,
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if person_id != self.vault.person_id() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let connection = self
                .core
                .calendar_connection(person_id)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let Some(connection) = connection else {
                return calendar_access_requirement(
                    None,
                    consumer,
                    floe_context_contract::SourceAccessRequirementKind::SelectResource,
                    None,
                );
            };
            if connection.device_id != self.device_id || connection.revision == 0 {
                return Err(AgentFailure::StaleContext);
            }
            if connection.disconnected {
                let observed = match calendar_connector_id(&connection) {
                    Some(connector_id) => {
                        let grants =
                            floe_vault::VaultGrantRecords::new(self.vault).grants().await?;
                        observe_calendar_binding(
                            &grants,
                            person_id,
                            connector_id,
                            &connection.connection_id,
                        )?
                    }
                    None => None,
                };
                return calendar_access_requirement(
                    Some(&connection),
                    consumer,
                    floe_context_contract::SourceAccessRequirementKind::Reconnect,
                    observed,
                );
            }
            if connection.calendars.is_empty() {
                return calendar_access_requirement(
                    Some(&connection),
                    consumer,
                    floe_context_contract::SourceAccessRequirementKind::SelectResource,
                    None,
                );
            }
            let result = async {
                if connection.provider == floe_context_contract::CalendarProvider::EventKit {
                #[cfg(target_os = "macos")]
                {
                    let connections = crate::vault_host::calendar_access::CoreCalendarConnections {
                        core: self.core,
                        person_id,
                    };
                    let grants = crate::vault_host::calendar_access::VaultNativeCalendarGrants {
                        vault: self.vault,
                    };
                    let calendar_ids = connection
                        .calendars
                        .iter()
                        .map(|calendar| calendar.calendar_id.clone())
                        .collect();
                    let source = floe_provider_adapters::sources::native_calendar::NativeCalendarReadAccess::new(
                        person_id,
                        self.device_id.to_owned(),
                        connection.provider,
                        calendar_ids,
                        connection.connection_id.clone(),
                        connection.revision,
                    );
                    let window = floe_context::RemoteCallWindow {
                        deadline,
                        cancellation: cancellation.clone(),
                    };
                    let (view, dependency) = floe_context::read_native_calendar_view(
                        &connections,
                        &source,
                        &grants,
                        &self.core.lease_registry,
                        floe_context::NativeCalendarViewRead {
                            person_id,
                            device_id: self.device_id,
                            consumer,
                            query,
                            window: &window,
                        },
                    )
                    .await?;
                    return Ok(vec![(view, dependency)]);
                }
                #[cfg(not(target_os = "macos"))]
                {
                    return Err(AgentFailure::CapabilityUnavailable);
                }
                }
            let source_client = self
                .source_client
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            if source_client.source().person_id() != person_id.to_string()
                || source_client.source().device_id() != self.device_id
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let connector_id = floe_access::hosted_calendar_connector(connection.provider)
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let person_text = person_id.to_string();
            let pairing = floe_context::RemotePairingIdentity {
                person_id: &person_text,
                client_id: source_client.source().client_id(),
                device_id: self.device_id,
            };
            let window = floe_context::RemoteCallWindow {
                deadline,
                cancellation: cancellation.clone(),
            };
            let authorized_client = floe_provider_adapters::sources::AuthorizedSourceClient::new(
                source_client,
                self.vault,
            );
            let mut reads = Vec::with_capacity(connection.calendars.len());
            for calendar in &connection.calendars {
                let (view, dependency, _) = floe_context::read_remote_calendar_view(
                    self.vault,
                    &authorized_client,
                    floe_context::RemoteCalendarViewRead {
                        person_id,
                        pairing,
                        connector_id,
                        connection_id: &connection.connection_id,
                        connection_revision: connection.revision,
                        resource: &calendar.calendar_id,
                        consumer_name: consumer,
                        query,
                        window: &window,
                        process_incarnation_id: self.core.lease_registry.process_incarnation(),
                    },
                )
                .await?;
                reads.push((view, dependency));
            }
                Ok(reads)
            }
            .await;
            // Reviewable failures classify against current grant facts; every
            // other outcome maps without touching grant state.
            if matches!(
                result,
                Err(AgentFailure::AccessReviewRequired | AgentFailure::CredentialExpired)
            ) {
                let grants = floe_vault::VaultGrantRecords::new(self.vault)
                    .grants()
                    .await?;
                let connector_id = calendar_connector_id(&connection).unwrap_or_default();
                let review = classify_calendar_review(
                    &grants,
                    person_id,
                    connector_id,
                    &connection.connection_id,
                )?;
                let reconnect = observe_calendar_binding(
                    &grants,
                    person_id,
                    connector_id,
                    &connection.connection_id,
                )?;
                return calendar_read_outcome(result, &connection, consumer, &review, reconnect);
            }
            let unused = CalendarReviewClassification {
                reason: floe_context_contract::SourceAccessRequirementKind::SelectResource,
                observed: None,
            };
            calendar_read_outcome(result, &connection, consumer, &unused, None)
        })
    }
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
                        floe_context_contract::SourceReadOutcome<(
                            AttentionView,
                            floe_context_contract::ContextDependency,
                        )>,
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
            let _ = turn_id;
            floe_context::admit_attention_outcome(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                floe_access::attention_consumer(consumer)?,
                call_id,
                deadline,
                cancellation,
            )
            .await
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
                        floe_context_contract::SourceReadOutcome<(
                            WellbeingView,
                            floe_context_contract::ContextDependency,
                        )>,
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let grant_consumer =
                floe_context_contract::GrantConsumer::builtin(consumer)
                    .map_err(|_| AgentFailure::InvalidInput)?;
            floe_context::read_wellbeing_outcome(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                consumer,
                grant_consumer,
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
                        floe_context_contract::SourceReadOutcome<(
                            PeopleView,
                            floe_context_contract::ContextDependency,
                        )>,
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let grant_consumer =
                floe_context_contract::GrantConsumer::builtin(consumer)
                    .map_err(|_| AgentFailure::InvalidInput)?;
            floe_context::read_manager_people_outcome(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                consumer,
                grant_consumer,
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
        Box::pin(floe_context::task_context_view(
            &self.core.store,
            self.person_id,
            handle,
            chrono::Utc::now(),
            16,
            8 * 1024,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::{ExpertModel, ExpertReasoner};
    use std::collections::{BTreeMap, VecDeque};

    fn calendar_connection() -> floe_day::CalendarConnection {
        floe_day::CalendarConnection {
            connection_id: "connection".into(),
            device_id: "device".into(),
            disconnected: false,
            scope: floe_context_contract::CalendarScope::Selected,
            provider: floe_context_contract::CalendarProvider::EventKit,
            calendars: vec![floe_day::CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            revision: 1,
            source_authority: floe_context_contract::SourceAuthority::new(),
            last_success_at: None,
            last_range: None,
            error: None,
            error_at: None,
            source_statuses: BTreeMap::new(),
        }
    }

    fn review_classification(
        reason: floe_context_contract::SourceAccessRequirementKind,
        observed: Option<floe_context_contract::ObservedGrant>,
    ) -> CalendarReviewClassification {
        CalendarReviewClassification { reason, observed }
    }

    fn blockers_requirement(
        outcome: floe_context_contract::SourceReadOutcome<Vec<String>>,
    ) -> floe_context_contract::SourceAccessRequirement {
        let floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("calendar review must remain actionable");
        };
        blockers.validate().unwrap();
        assert_eq!(blockers.blockers().len(), 1);
        blockers.blockers()[0].clone()
    }

    #[test]
    fn calendar_review_requirement_binds_current_connection_and_expert() {
        let connection = calendar_connection();
        let observed = floe_context_contract::ObservedGrant::try_new(
            floe_context_contract::GrantId::new(),
            floe_context_contract::GrantAuthority::new(),
        )
        .unwrap();
        let review = review_classification(
            floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource,
            Some(observed),
        );
        let outcome = calendar_read_outcome::<Vec<String>>(
            Err(AgentFailure::AccessReviewRequired),
            &connection,
            "floe.builtin.schedule",
            &review,
            None,
        )
        .unwrap();
        let requirement = blockers_requirement(outcome);
        assert_eq!(requirement.source_id(), "floe.source.calendar");
        assert_eq!(requirement.consumer().identifier(), "floe.builtin.schedule");
        assert_eq!(
            requirement.connector_id().unwrap().as_str(),
            "calendar.event_kit"
        );
        assert_eq!(requirement.connection_id().unwrap().as_str(), "connection");
        assert_eq!(requirement.resources()[0].as_str(), "primary");
        assert_eq!(
            requirement.source_authority(),
            Some(connection.source_authority)
        );
        assert_eq!(requirement.observed_grant(), Some(observed));
        assert!(requirement.inline_resolution());
    }

    #[test]
    fn calendar_unavailability_and_integrity_failure_stay_distinct() {
        let connection = calendar_connection();
        let unused = review_classification(
            floe_context_contract::SourceAccessRequirementKind::SelectResource,
            None,
        );
        assert_eq!(
            calendar_read_outcome::<Vec<String>>(
                Ok(vec![]),
                &connection,
                "floe.builtin.schedule",
                &unused,
                None,
            )
            .unwrap(),
            floe_context_contract::SourceReadOutcome::Ready(vec![])
        );
        assert_eq!(
            calendar_read_outcome::<Vec<String>>(
                Err(AgentFailure::CapabilityUnavailable),
                &connection,
                "floe.builtin.schedule",
                &unused,
                None,
            )
            .unwrap(),
            floe_context_contract::SourceReadOutcome::Unavailable(
                floe_context_contract::SourceUnavailable::TemporarilyUnavailable
            )
        );
        for failure in [
            AgentFailure::CapabilityDenied,
            AgentFailure::PolicyDenied,
            AgentFailure::StaleContext,
            AgentFailure::StorageUnavailable,
            AgentFailure::Cancelled,
        ] {
            assert_eq!(
                calendar_read_outcome::<Vec<String>>(
                    Err(failure),
                    &connection,
                    "floe.builtin.schedule",
                    &unused,
                    None,
                ),
                Err(failure)
            );
        }
    }

    #[test]
    fn unresolved_calendar_requirement_cannot_offer_inline_grant() {
        let outcome = calendar_access_requirement::<Vec<String>>(
            None,
            "floe.builtin.schedule",
            floe_context_contract::SourceAccessRequirementKind::SelectResource,
            None,
        )
        .unwrap();
        let requirement = blockers_requirement(outcome);
        assert!(requirement.connection_id().is_none());
        assert!(requirement.resources().is_empty());
        assert!(!requirement.inline_resolution());

        let mut connection = calendar_connection();
        connection.calendars[0].calendar_id = "x".repeat(257);
        let outcome = calendar_access_requirement::<Vec<String>>(
            Some(&connection),
            "floe.builtin.schedule",
            floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource,
            None,
        )
        .unwrap();
        let requirement = blockers_requirement(outcome);
        assert!(requirement.resources().is_empty());
        assert!(!requirement.inline_resolution());
    }

    fn calendar_grant_fixture(
        person_id: PersonId,
        state: floe_access::GrantState,
    ) -> floe_access::DataAccessGrant {
        let source = floe_context_contract::GrantSourceBinding::try_new(
            person_id,
            floe_context_contract::ConnectionId::try_new("connection").unwrap(),
            floe_context_contract::ConnectorId::try_new("calendar.event_kit").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("device").unwrap(),
            floe_context_contract::SourceAuthority::new(),
        )
        .unwrap();
        let scope = floe_access::GrantScope::try_new(
            vec![floe_context_contract::ResourceHandle::try_new("primary").unwrap()],
            vec![floe_context_contract::GrantDataCategory::Derived],
            vec![floe_context_contract::GrantOperation::Read],
            vec![floe_context_contract::GrantPurpose::Assistant],
            vec![floe_context_contract::GrantConsumer::builtin("floe.builtin.schedule").unwrap()],
            floe_context_contract::ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let mut grant = floe_access::DataAccessGrant::new(
            floe_context_contract::GrantId::new(),
            uuid::Uuid::new_v4(),
            source.clone(),
            scope.clone(),
        )
        .unwrap();
        if state == floe_access::GrantState::Active {
            grant
                .activate_review(grant.authority(), source, scope)
                .unwrap();
        }
        grant
    }

    #[test]
    fn calendar_review_classifies_missing_paused_and_drifted_grants() {
        let person_id = PersonId::new();
        let missing =
            classify_calendar_review(&[], person_id, "calendar.event_kit", "connection").unwrap();
        assert_eq!(
            missing.reason,
            floe_context_contract::SourceAccessRequirementKind::EnableObserve
        );
        assert_eq!(missing.observed, None);

        let paused = calendar_grant_fixture(person_id, floe_access::GrantState::Paused);
        let review = classify_calendar_review(
            std::slice::from_ref(&paused),
            person_id,
            "calendar.event_kit",
            "connection",
        )
        .unwrap();
        assert_eq!(
            review.reason,
            floe_context_contract::SourceAccessRequirementKind::EnableObserve
        );
        let observed = review.observed.unwrap();
        assert_eq!(observed.grant_id(), paused.id());
        assert_eq!(observed.authority(), paused.authority());

        let active = calendar_grant_fixture(person_id, floe_access::GrantState::Active);
        let review = classify_calendar_review(
            std::slice::from_ref(&active),
            person_id,
            "calendar.event_kit",
            "connection",
        )
        .unwrap();
        assert_eq!(
            review.reason,
            floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource
        );
        assert_eq!(review.observed.unwrap().grant_id(), active.id());
    }

    #[test]
    fn calendar_review_ignores_foreign_grants_and_fails_duplicates_closed() {
        let person_id = PersonId::new();
        let foreign = calendar_grant_fixture(PersonId::new(), floe_access::GrantState::Active);
        let review = classify_calendar_review(
            std::slice::from_ref(&foreign),
            person_id,
            "calendar.event_kit",
            "connection",
        )
        .unwrap();
        assert_eq!(
            review.reason,
            floe_context_contract::SourceAccessRequirementKind::EnableObserve
        );
        assert_eq!(review.observed, None);

        let first = calendar_grant_fixture(person_id, floe_access::GrantState::Paused);
        let second = calendar_grant_fixture(person_id, floe_access::GrantState::Paused);
        assert_eq!(
            classify_calendar_review(
                &[first, second],
                person_id,
                "calendar.event_kit",
                "connection",
            ),
            Err(AgentFailure::PolicyDenied)
        );
    }

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
            self.calls
                .lock()
                .unwrap()
                .push((request.clone(), constraint));
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
    async fn expert_can_answer_after_user_action_capability_observation() {
        let executor = FakeExecutor::new(vec![Ok(vec![ModelStep::Answer {
            text: "Calendar access is needed.".into(),
            artifacts: vec![],
        }])]);
        let scope = test_scope();
        let captured = Mutex::new(Vec::new());
        let host = ExpertModelHost {
            executor: &executor,
            scope: &scope,
            captured: &captured,
        };
        let mut step = test_step();
        let interaction_id = Uuid::new_v4();
        step.transcript.push(ExpertTranscriptEntry::Capability {
            call_id: Uuid::new_v4(),
            capability_id: "calendar.read".into(),
            input: "{}".into(),
            observation: floe_agent_contract::ExpertCapabilityObservation::NeedsUserAction {
                interaction: floe_agent_contract::UserInteractionRef {
                    interaction_id,
                    kind: floe_agent_contract::UserInteractionKind::SourceAccess,
                    status: floe_agent_contract::UserInteractionStatus::Pending,
                },
                summary: "Calendar access needs approval".into(),
            },
        });
        let outcome = ExpertReasoner::step(&host, step).await.unwrap();
        assert_eq!(
            outcome.steps,
            vec![ExpertStep::Answer {
                text: "Calendar access is needed.".into(),
            }]
        );
        let calls = executor.calls();
        let envelope = &calls[0].0.projection.envelope;
        let exchange = envelope.conversation.current_turn.last().unwrap();
        let ModelConversationEntry::ToolExchange { result, .. } = exchange else {
            panic!("expected capability observation");
        };
        assert!(result.issue.is_some());
        assert_eq!(result.coverage, DependencyCoverage::Independent);
        assert_eq!(result.artifacts.len(), 1);
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
            observation: floe_agent_contract::ExpertCapabilityObservation::Success {
                result: "no conflicts".into(),
            },
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
        undeclared
            .transcript
            .push(ExpertTranscriptEntry::Capability {
                call_id: Uuid::new_v4(),
                capability_id: "calendar.write".into(),
                input: "{}".into(),
                observation: floe_agent_contract::ExpertCapabilityObservation::Success {
                    result: "done".into(),
                },
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
