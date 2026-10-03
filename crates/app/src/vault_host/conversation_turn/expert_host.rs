//! Delegated Expert input translation onto the shared prepared model port.

use std::{future::Future, pin::Pin, sync::Mutex};

use floe_agent_contract::{
    AGENT_VERSION, AgentFailure, AllowedCatalog, DataClass, DependencyCoverage, ExecutionJournal,
    ExpertModelAnswer, ExpertModelCall, ExpertModelOutcome, ExpertReasoningStep, ExpertStep,
    ExpertStepOutcome, ExpertStepResult, ExpertTranscriptEntry, InferencePolicyDecision,
    InvocationKey, JournalAck, JournalEvent, ModelCapabilities, ModelConversation,
    ModelConversationEntry, ModelPlanRequest, ModelPort, ModelProjectionOutcome, ModelRequest,
    ModelResponse, ModelStep, PreparedModelPlan, SourceProjectionReview, ToolCall, ToolDescriptor,
    ToolResult,
};
use floe_context::{
    AttentionView, CalendarContextView, CalendarReviewClassification, NativeContextView,
    PeopleView, WellbeingView, classify_calendar_review,
    current_calendar_connector as calendar_connector_id, observe_calendar_binding,
};
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
pub(super) fn expert_policy(package_data_class: DataClass) -> InferencePolicyDecision {
    let mut data_classes = vec![DataClass::Personal, package_data_class];
    data_classes.sort();
    data_classes.dedup();
    InferencePolicyDecision {
        purpose: "everyday_assistance".into(),
        data_classes,
        performance_class: "interactive".into(),
        projection_version: 1,
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

/// Selected model input and source review evidence for the admitted Task.
pub(crate) struct ExpertModelHost<'a> {
    pub(crate) model: &'a dyn ModelPort,
    pub(crate) journal: &'a dyn ExecutionJournal,
    pub(crate) task_id: TaskId,
    pub(crate) device_id: &'a str,
    pub(crate) scope: &'a floe_execution::ExecutionScope,
    pub(crate) captured: &'a Mutex<Vec<floe_context_contract::ContextDependency>>,
    pub(crate) model_blocked: &'a Mutex<Option<ExpertProjectionBlocker>>,
}

#[derive(Clone)]
pub(crate) struct ExpertProjectionBlocker {
    pub(crate) plan: PreparedModelPlan,
    pub(crate) review: SourceProjectionReview,
}

struct ExpertModelInput {
    person_id: PersonId,
    invocation_id: Uuid,
    prompt: floe_agent_contract::prompts::PromptAssembly,
    context: floe_agent_contract::AgentContext,
    conversation: ModelConversation,
    catalog: AllowedCatalog,
    data_classes: Vec<DataClass>,
    max_tokens: u64,
    max_cost_micros: u64,
    max_output_bytes: usize,
    deadline: tokio::time::Instant,
}

enum ExpertGeneration {
    Answered(ModelResponse),
    NeedsSourceReview(SourceProjectionReview),
}

impl ExpertModelHost<'_> {
    async fn generate(&self, input: ExpertModelInput) -> Result<ExpertGeneration, AgentFailure> {
        let child = self.scope.child_scope(
            input.deadline,
            input.max_tokens,
            input.max_cost_micros,
            TaskId::from_uuid(input.invocation_id),
        );
        let request = ModelPlanRequest {
            principal: input.person_id.to_string(),
            device_id: self.device_id.to_owned(),
            purpose: "everyday_assistance".into(),
            consumer: floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
            required_capabilities: ModelCapabilities::chat(),
        };
        let prepared = child
            .run(self.model.prepare(request.clone(), &child))
            .await?;
        let plan = prepared.plan().clone();
        plan.validate()?;
        if plan.principal != request.principal
            || plan.device_id != request.device_id
            || plan.purpose != request.purpose
            || plan.consumer != request.consumer
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let dependencies = self
            .captured
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .clone();
        let projection =
            floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
                role: floe_context::ContextProjectionRole::Expert,
                plan: &plan,
                projection_operation_id: Uuid::new_v4(),
                purpose: &plan.purpose,
                response_contract: "One bounded Expert reasoning result.",
                correction: None,
                prompt: input.prompt,
                conversation: input.conversation,
                agent_context: &input.context,
                catalog: &input.catalog,
                expert_environment: None,
                authorized_history_dependencies: &dependencies,
                input_data_classes: input.data_classes,
                max_output_bytes: input.max_output_bytes,
            })?;
        let projection = match projection {
            ModelProjectionOutcome::Ready(projection) => projection,
            ModelProjectionOutcome::NeedsSourceReview(review) => {
                review.validate()?;
                let mut blocked = self
                    .model_blocked
                    .lock()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                if blocked.is_none() {
                    *blocked = Some(ExpertProjectionBlocker {
                        plan,
                        review: review.clone(),
                    });
                }
                return Ok(ExpertGeneration::NeedsSourceReview(review));
            }
        };
        let attempt_id = Uuid::new_v4();
        let reservation_ceiling =
            floe_execution::budget::ModelReservationCeiling::for_lease(child.budget());
        let intent = child
            .run(self.journal.record_intent(JournalEvent::ModelIntent {
                reservation_ceiling,
                parent_task_id: Some(self.task_id),
                attempt_id,
                projection_ref: projection.projection_ref,
                plan: plan.clone(),
            }))
            .await?;
        if !matches!(intent, JournalAck::Accepted { .. }) {
            return Err(AgentFailure::Conflict);
        }
        let response = child
            .run(prepared.generate(
                ModelRequest {
                    attempt_id,
                    reservation_ceiling,
                    principal: request.principal,
                    device_id: request.device_id,
                    purpose: request.purpose,
                    consumer: request.consumer,
                    projection,
                    catalog: input.catalog,
                    replay: vec![],
                },
                &child,
            ))
            .await;
        let receipt = child.budget().model_attempt_receipt(attempt_id);
        if receipt.is_none()
            && (response.is_ok() || child.budget().model_attempt_admitted(attempt_id))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let (usage, accounting) = receipt.map_or_else(
            || {
                (
                    floe_agent_contract::ModelUsage::default(),
                    floe_agent_contract::ModelAccounting::default(),
                )
            },
            |receipt| {
                (
                    floe_agent_contract::ModelUsage {
                        tokens: receipt.charged_tokens,
                        cost_micros: receipt.charged_cost_micros,
                    },
                    receipt.accounting,
                )
            },
        );
        let acknowledgment = self
            .journal
            .record_result(JournalEvent::ModelResult {
                attempt_id,
                usage,
                accounting,
            })
            .await?;
        if !matches!(acknowledgment, JournalAck::Accepted { .. }) {
            return Err(AgentFailure::Conflict);
        }
        if receipt.is_some() {
            child.budget().acknowledge_model_attempt(attempt_id)?;
        }
        let response = response?;
        if response.attempt_id != attempt_id
            || response.usage != usage
            || response.accounting != accounting
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(ExpertGeneration::Answered(response))
    }
}

impl floe_agent_contract::ExpertModel for ExpertModelHost<'_> {
    fn answer<'a>(
        &'a self,
        call: ExpertModelCall,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertModelOutcome, AgentFailure>> {
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
            let outcome = self
                .generate(ExpertModelInput {
                    person_id: call.person_id,
                    invocation_id: call.invocation_id,
                    prompt: call.prompt,
                    context: call.context,
                    conversation: ModelConversation {
                        history: vec![],
                        current_turn: vec![ModelConversationEntry::User {
                            message_id: Uuid::new_v4(),
                            text: call.assignment,
                        }],
                    },
                    catalog: AllowedCatalog {
                        cards: vec![],
                        tools: vec![],
                        revision: 1,
                    },
                    data_classes: call.policy.data_classes,
                    max_tokens: call.max_tokens,
                    max_cost_micros: call.max_cost_micros,
                    max_output_bytes: call.max_output_bytes,
                    deadline: call.deadline,
                })
                .await?;
            let response = match outcome {
                ExpertGeneration::Answered(response) => response,
                ExpertGeneration::NeedsSourceReview(review) => {
                    return Ok(ExpertModelOutcome::Blocked(review));
                }
            };
            let [ModelStep::Answer { text, .. }] = response.steps.as_slice() else {
                return Err(AgentFailure::InvalidModelOutput);
            };
            Ok(ExpertModelOutcome::Answered(ExpertModelAnswer {
                schema_version: AGENT_VERSION,
                answer: text.clone(),
                used_tokens: response.usage.tokens,
                cost_micros: response.usage.cost_micros,
            }))
        })
    }
}

impl floe_agent_contract::ExpertReasoner for ExpertModelHost<'_> {
    fn step<'a>(
        &'a self,
        step: ExpertReasoningStep,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertStepResult, AgentFailure>> {
        Box::pin(async move {
            if step.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if step.deadline <= tokio::time::Instant::now() {
                return Err(AgentFailure::DeadlineExceeded);
            }
            step.prompt.validate()?;
            let catalog = expert_catalog(&step.capabilities)?;
            let conversation = expert_conversation(step.transcript, &catalog)?;
            let outcome = self
                .generate(ExpertModelInput {
                    person_id: step.person_id,
                    invocation_id: step.invocation_id,
                    prompt: step.prompt,
                    context: step.context,
                    conversation,
                    catalog,
                    data_classes: step.policy.data_classes,
                    max_tokens: step.remaining_tokens,
                    max_cost_micros: step.remaining_cost_micros,
                    max_output_bytes: step.max_output_bytes,
                    deadline: step.deadline,
                })
                .await?;
            let response = match outcome {
                ExpertGeneration::Answered(response) => response,
                ExpertGeneration::NeedsSourceReview(review) => {
                    return Ok(ExpertStepResult::Blocked(review));
                }
            };
            Ok(ExpertStepResult::Stepped(ExpertStepOutcome {
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
                        ModelStep::Delegate { .. } => Err(AgentFailure::CapabilityDenied),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                replay: None,
                used_tokens: response.usage.tokens,
                cost_micros: response.usage.cost_micros,
            }))
        })
    }
}

pub(super) struct PersonalViewSource<'a> {
    pub(super) calendar_reader: Option<&'a dyn CalendarContextReaderApi>,
    pub(super) person_id: PersonId,
    pub(super) people_reader: Option<&'a dyn PersonalPeopleReaderApi>,
    pub(super) wellbeing_reader: Option<&'a dyn PersonalWellbeingReaderApi>,
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
        selected: &floe_context_contract::SourceSelectionReference,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<floe_context_contract::SourceReadOutcome<PeopleView>, AgentFailure> {
        self.record_result_independent()?;
        let reader = self
            .people_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        match reader
            .read(
                self.person_id,
                self.consumer_name,
                selected,
                deadline,
                cancellation,
            )
            .await?
        {
            floe_context_contract::SourceReadOutcome::Ready((view, dependency)) => {
                if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
                    recorder.record(
                        self.dependency_turn_id,
                        self.dependency_result_id,
                        dependency,
                    )?;
                }
                Ok(floe_context_contract::SourceReadOutcome::Ready(view))
            }
            floe_context_contract::SourceReadOutcome::Unavailable(reason) => Ok(
                floe_context_contract::SourceReadOutcome::Unavailable(reason),
            ),
            floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => Ok(
                floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers),
            ),
        }
    }

    pub(super) async fn wellbeing_view(
        &self,
        selected: &floe_context_contract::SourceSelectionReference,
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
                selected,
                self.dependency_result_id,
                deadline,
                cancellation,
            )
            .await?
        {
            floe_context_contract::SourceReadOutcome::Ready((view, dependency)) => {
                if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
                    recorder.record(
                        self.dependency_turn_id,
                        self.dependency_result_id,
                        dependency,
                    )?;
                }
                Ok(floe_context_contract::SourceReadOutcome::Ready(view))
            }
            floe_context_contract::SourceReadOutcome::Unavailable(reason) => Ok(
                floe_context_contract::SourceReadOutcome::Unavailable(reason),
            ),
            floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => Ok(
                floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers),
            ),
        }
    }

    pub(super) async fn calendar_views(
        &self,
        source_access_id: &str,
        selected: &[floe_context_contract::SourceSelectionReference],
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
                source_access_id,
                selected,
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
        consumer: &'a str,
        selected: &'a floe_context_contract::SourceSelectionReference,
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
        selected: &'a floe_context_contract::SourceSelectionReference,
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
        selected: &'a floe_context_contract::SourceSelectionReference,
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
        source_access_id: &'a str,
        selected: &'a [floe_context_contract::SourceSelectionReference],
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

fn calendar_access_requirement<Value>(
    source_access_id: &str,
    connection: Option<&floe_connections::SourceConnection>,
    selected: &[floe_context_contract::SourceSelectionReference],
    consumer: &str,
    reason: floe_context_contract::SourceAccessRequirementKind,
    observed_grant: Option<floe_context_contract::ObservedGrant>,
) -> Result<floe_context_contract::SourceReadOutcome<Value>, AgentFailure> {
    let connector_id = connection
        .filter(|connection| floe_context::current_calendar_connector(connection).is_some())
        .map(|connection| connection.connector_id().clone());
    let connection_id = connection.map(|connection| connection.connection_id().clone());
    let resources = selected
        .iter()
        .map(|source| source.resource.clone())
        .collect::<Vec<_>>();
    let source_authority = connection
        .map(|connection| connection.source_authority())
        .filter(|authority| authority.is_valid());
    let inline_resolution = connector_id.is_some()
        && connection_id.is_some()
        && !resources.is_empty()
        && source_authority.is_some()
        && reason != floe_context_contract::SourceAccessRequirementKind::Reconnect;
    let requirement = floe_context_contract::SourceAccessRequirement::try_new(
        source_access_id,
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
    source_access_id: &str,
    connection: &floe_connections::SourceConnection,
    selected: &[floe_context_contract::SourceSelectionReference],
    consumer: &str,
    review: &floe_context::CalendarReviewClassification,
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
            source_access_id,
            Some(connection),
            selected,
            consumer,
            review.reason,
            review.observed,
        ),
        Err(AgentFailure::CredentialExpired) => calendar_access_requirement(
            source_access_id,
            Some(connection),
            selected,
            consumer,
            floe_context_contract::SourceAccessRequirementKind::Reconnect,
            reconnect_observed,
        ),
        Err(error) => Err(error),
    }
}

pub(super) struct SelectedCalendarContextReader<'a, Keys: VaultKeyProvider> {
    pub(super) core: &'a FloeCore,
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) source_client: Option<&'a ServerSourceClient>,
    pub(super) signer: &'a dyn floe_access::AuthorizationSigner,
    pub(super) verifier: &'a dyn floe_access::SourcePreviewVerifier,
    pub(super) device_id: &'a str,
}

impl<Keys: VaultKeyProvider> CalendarContextReaderApi for SelectedCalendarContextReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        source_access_id: &'a str,
        selected: &'a [floe_context_contract::SourceSelectionReference],
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
            let selected_source = selected.first().ok_or(AgentFailure::StaleContext)?;
            let connection = self
                .core
                .source_service()
                .load(person_id, &selected_source.connection_id)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let connection = connection.ok_or(AgentFailure::CapabilityUnavailable)?;
            let connector_id =
                calendar_connector_id(&connection).ok_or(AgentFailure::CapabilityUnavailable)?;
            let native = connector_id == "calendar.event_kit";
            let logical_resource = floe_context_contract::connection_view_resource(
                "calendar.timeline",
                connection.connection_id(),
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            if !connection.is_serving()
                || selected.len() != 1
                || selected[0].resource != logical_resource
                || selected.iter().any(|source| {
                    source.capability_id != "calendar.timeline"
                        || source.contract_version != 1
                        || source.connector_id.as_str() != connector_id
                        || source.connection_id != *connection.connection_id()
                })
            {
                return Err(AgentFailure::StaleContext);
            }
            let expected_owner = if connector_id == "calendar.event_kit" {
                self.device_id.to_owned()
            } else {
                floe_access::GatewayTrustReader::pinned_producer(self.vault)
                    .await?
                    .execution_owner
            };
            if connection.execution_owner_id().as_str() != expected_owner
                || selected
                    .iter()
                    .any(|source| source.execution_owner_id.as_str() != expected_owner)
            {
                return Err(AgentFailure::StaleContext);
            }
            if !native {
                let source_client = self
                    .source_client
                    .ok_or(AgentFailure::CapabilityUnavailable)?;
                if source_client.source().person_id() != person_id.to_string()
                    || source_client.source().device_id() != self.device_id
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
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
                let authorized_client =
                    floe_provider_adapters::sources::AuthorizedSourceClient::new(
                        source_client,
                        self.signer,
                    );
                let query_bytes =
                    serde_json::to_vec(query).map_err(|_| AgentFailure::InvalidInput)?;
                let outcome = floe_context::read_selected_remote_view(
                    self.vault,
                    self.verifier,
                    self.core.store.as_ref(),
                    self.core.store.as_ref(),
                    &authorized_client,
                    person_id,
                    pairing,
                    "calendar.timeline",
                    consumer,
                    selected,
                    serde_json::to_value(query).map_err(|_| AgentFailure::InvalidInput)?,
                    &window,
                    self.core.lease_registry.process_incarnation(),
                    &query_bytes,
                )
                .await?;
                return match outcome {
                    floe_context_contract::SourceReadOutcome::Ready((payload, bindings)) => {
                        let view: CalendarContextView = serde_json::from_value(payload)
                            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                        if bindings.len() != 1 {
                            return Err(AgentFailure::Conflict);
                        }
                        Ok(floe_context_contract::SourceReadOutcome::Ready(vec![(
                            view,
                            bindings.into_iter().next().unwrap().dependency,
                        )]))
                    }
                    floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => Ok(
                        floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers),
                    ),
                    floe_context_contract::SourceReadOutcome::Unavailable(reason) => Ok(
                        floe_context_contract::SourceReadOutcome::Unavailable(reason),
                    ),
                };
            }
            let result = async {
                if connector_id == "calendar.event_kit" {
                #[cfg(target_os = "macos")]
                {
                    let connections = crate::vault_host::calendar_access::CoreCalendarConnections {
                        core: self.core,
                        person_id,
                    };
                    let grants = crate::vault_host::calendar_access::VaultNativeCalendarGrants {
                        vault: self.vault,
                    };
                    let calendar_ids: Vec<String> = connection
                        .resources()
                        .iter()
                        .map(|calendar| calendar.handle().as_str().to_owned())
                        .collect();
                    let source = floe_provider_adapters::sources::native_calendar::NativeCalendarReadAccess::new(
                        person_id,
                        self.device_id.to_owned(),
                        floe_context_contract::CalendarProvider::EventKit,
                        calendar_ids.clone(),
                        connection.connection_id().as_str().to_owned(),
                        connection.revision(),
                        self.core.store.clone(),
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
            Err(AgentFailure::CapabilityUnavailable)
            }
            .await;
            // Reviewable failures classify against current grant facts; every
            // other outcome maps without touching grant state.
            if matches!(
                result,
                Err(AgentFailure::AccessReviewRequired | AgentFailure::CredentialExpired)
            ) {
                let source = floe_context_contract::GrantSourceBinding::try_new(
                    person_id,
                    connection.connection_id().clone(),
                    connection.connector_id().clone(),
                    connection.execution_owner_id().clone(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let grants = floe_access::GrantRepository::snapshot(self.vault, source)
                    .await?
                    .grants;
                let review = classify_calendar_review(
                    &grants,
                    person_id,
                    connector_id,
                    connection.connection_id().as_str(),
                )?;
                let reconnect = observe_calendar_binding(
                    &grants,
                    person_id,
                    connector_id,
                    connection.connection_id().as_str(),
                )?;
                return calendar_read_outcome(
                    result,
                    source_access_id,
                    &connection,
                    selected,
                    consumer,
                    &review,
                    reconnect,
                );
            }
            let unused = CalendarReviewClassification {
                reason: floe_context_contract::SourceAccessRequirementKind::SelectResource,
                observed: None,
            };
            calendar_read_outcome(
                result,
                source_access_id,
                &connection,
                selected,
                consumer,
                &unused,
                None,
            )
        })
    }
}

pub(super) struct PersonalAttentionReader<'a, Keys: VaultKeyProvider> {
    pub(super) core: &'a FloeCore,
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) local_context: &'a LocalContextHost,
    pub(super) device_id: &'a str,
}

impl<Keys: VaultKeyProvider> PersonalAttentionReaderApi for PersonalAttentionReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        selected: &'a floe_context_contract::SourceSelectionReference,
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
            if selected.capability_id != "attention.coarse" {
                return Err(AgentFailure::CapabilityDenied);
            }
            floe_context::admit_selected_attention_outcome(
                &personal_grants::CorePersonalConnections { core: self.core },
                self.vault,
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                selected,
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
    pub(super) core: &'a FloeCore,
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) local_context: &'a LocalContextHost,
    pub(super) device_id: &'a str,
}

pub(super) struct PersonalWellbeingReader<'a, Keys: VaultKeyProvider> {
    pub(super) core: &'a FloeCore,
    pub(super) vault: &'a EncryptedAgentVault<Keys>,
    pub(super) local_context: &'a LocalContextHost,
    pub(super) device_id: &'a str,
}

impl<Keys: VaultKeyProvider> PersonalWellbeingReaderApi for PersonalWellbeingReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        person_id: PersonId,
        consumer: &'a str,
        selected: &'a floe_context_contract::SourceSelectionReference,
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
            floe_context::read_selected_wellbeing_outcome(
                &personal_grants::CorePersonalConnections { core: self.core },
                self.vault,
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                selected,
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
        selected: &'a floe_context_contract::SourceSelectionReference,
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
            floe_context::read_selected_people_outcome(
                &personal_grants::CorePersonalConnections { core: self.core },
                self.vault,
                &personal_grants::native_driver(self.local_context),
                person_id,
                self.device_id,
                selected,
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
