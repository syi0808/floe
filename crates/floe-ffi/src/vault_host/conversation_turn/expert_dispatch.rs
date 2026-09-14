use super::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{AgentEndpoint, BoxFuture, EndpointInvocation, ExpertReport};

pub(in crate::vault_host) mod schedule;

#[derive(Clone)]
pub(in crate::vault_host) struct LegacyExpertEndpointContext {
    pub request: floe_protocol::AgentConversationTurnRequestDto,
    pub context: AgentContext,
    pub session_id: Uuid,
    pub max_output_bytes: usize,
}

pub(in crate::vault_host) struct LegacyExpertEndpoint<Keys> {
    core: Arc<FloeCore>,
    vault: Arc<EncryptedAgentVault<Keys>>,
    local_context: Arc<LocalContextStore>,
    contexts: Mutex<HashMap<Uuid, LegacyExpertEndpointContext>>,
}

impl<Keys> LegacyExpertEndpoint<Keys> {
    pub(in crate::vault_host) fn new(
        core: Arc<FloeCore>,
        vault: Arc<EncryptedAgentVault<Keys>>,
        local_context: Arc<LocalContextStore>,
    ) -> Self {
        Self {
            core,
            vault,
            local_context,
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub(in crate::vault_host) fn stage(
        &self,
        run_id: Uuid,
        context: LegacyExpertEndpointContext,
    ) -> Result<(), AgentFailure> {
        if run_id.is_nil() || context.session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut contexts = self
            .contexts
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if contexts.len() >= 4 || contexts.insert(run_id, context).is_some() {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    pub(in crate::vault_host) fn clear(&self, run_id: Uuid) -> Result<(), AgentFailure> {
        self.contexts
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .remove(&run_id);
        Ok(())
    }
}

impl<Keys: VaultKeyProvider + 'static> AgentEndpoint for LegacyExpertEndpoint<Keys> {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        Box::pin(async move {
            let run_id = invocation
                .request
                .parent_run_id
                .ok_or(AgentFailure::InvalidInput)?;
            let staged = self
                .contexts
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .remove(&run_id)
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            if invocation.request.principal != self.vault.person_id().to_string()
                || invocation.request.selected_agent_id == BuiltinExpertKind::Schedule.package_id()
                || staged.session_id != super::session_uuid(&staged.request.session_id)?
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let source_client = staged
                .request
                .remote_route
                .as_ref()
                .map(|route| ServerSourceClient::new(route.clone()))
                .transpose()?;
            let model = Model::new(staged.request.remote_route.clone())?;
            let remote_reader = match (&model, staged.request.remote_route.as_ref()) {
                (Model::Server(_), Some(route)) => {
                    let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
                    if pairing.person_id != self.vault.person_id().to_string()
                        || pairing.device_id != staged.request.device_id
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    Some(remote_views::RemoteViewReader {
                        vault: &self.vault,
                        source_client: source_client
                            .as_ref()
                            .ok_or(AgentFailure::CapabilityUnavailable)?,
                        person_id: self.vault.person_id(),
                        client_id: &pairing.client_id,
                        device_id: &pairing.device_id,
                        route,
                    })
                }
                _ => None,
            };
            let attention_reader = PersonalAttentionReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &staged.request.device_id,
            };
            let people_reader = PersonalPeopleReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &staged.request.device_id,
            };
            let feasibility_reader = PersonalFeasibilityReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &staged.request.device_id,
            };
            let wellbeing_reader = PersonalWellbeingReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &staged.request.device_id,
            };
            let governed_store = self.vault.governed_general_store(staged.session_id);
            let recorder = StoreResultRecorder {
                store: &governed_store,
            };
            let context_reader = ConversationContextReader {
                core: &self.core,
                vault: &self.vault,
                person_id: self.vault.person_id(),
            };
            let policy = super::policy(&model, staged.request.remote_route.as_ref());
            let cards = self.vault.enabled_expert_cards().await?;
            let builtin_setup = self
                .vault
                .builtin_expert_overview()
                .await?
                .ok_or(AgentFailure::VaultUnavailable)?
                .setup;
            let experts = ConversationExperts {
                model: &model,
                source_client: source_client.as_ref(),
                policy: &policy,
                context: &staged.context,
                local_context: &self.local_context,
                attention: Some(&attention_reader),
                people_reader: Some(&people_reader),
                feasibility_reader: Some(&feasibility_reader),
                wellbeing_reader: Some(&wellbeing_reader),
                recorder: Some(&recorder),
                remote_reader: remote_reader
                    .as_ref()
                    .map(|reader| reader as &dyn floe_context::SourceReader),
                context_reader: Some(&context_reader),
                task_views: &[],
                cards,
                builtin_setup: Some(builtin_setup),
                schedule_runner: None,
            };
            let task_id = invocation.request.task_id.as_uuid();
            governed_store.record_result_independent(task_id, task_id)?;
            let task = experts
                .handle_message(A2ASendMessageRequest {
                    usage: Default::default(),
                    schema_version: AGENT_VERSION,
                    person_id: self.vault.person_id(),
                    session_id: staged.session_id,
                    parent_turn_id: run_id,
                    agent_id: invocation.request.selected_agent_id.clone(),
                    message: floe_agent::A2AMessage {
                        message_id: invocation.request.invocation_key.as_uuid(),
                        context_id: run_id,
                        task_id: Some(task_id),
                        role: A2AMessageRole::User,
                        parts: vec![A2APart::Text {
                            text: invocation.request.message.clone(),
                        }],
                    },
                    max_output_bytes: staged.max_output_bytes,
                    deadline: scope.deadline(),
                    cancellation: scope.cancellation().clone(),
                })
                .await?;
            if task.id != task_id
                || task.context_id != run_id
                || task.agent_id != invocation.request.selected_agent_id
                || task.state != A2ATaskState::Completed
                || task.failure.is_some()
            {
                return Err(AgentFailure::InvalidModelOutput);
            }
            let result = task
                .data_part(EXPERT_RESULT_MEDIA_TYPE)
                .or_else(|| task.result_text().ok())
                .map(str::to_owned)
                .ok_or(AgentFailure::InvalidModelOutput)?;
            let coverage = governed_store
                .result_coverage(task_id, task_id)?
                .unwrap_or(floe_agent_contract::DependencyCoverage::Independent);
            Ok(ExpertReport {
                task_id: invocation.request.task_id,
                principal: invocation.request.principal,
                agent_id: invocation.request.selected_agent_id,
                definition_revision: invocation.request.selected_definition_revision,
                result,
                artifacts: vec![],
                coverage,
                settlement: None,
            })
        })
    }
}

pub(super) trait ScheduleTaskRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        request: A2ASendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<A2ATask, AgentFailure>> + Send + 'a>>;
}

pub(super) struct RegisteredScheduleTaskRunner<'a, Keys: VaultKeyProvider> {
    pub coordinator: &'a floe_experts::TaskCoordinator<
        crate::vault_host::task_repository::VaultTaskRepository<Keys>,
    >,
    pub endpoint: &'a schedule::ScheduleEndpoint<Keys>,
    pub turn_request: &'a floe_protocol::AgentConversationTurnRequestDto,
    pub context: &'a AgentContext,
    pub recorder: Option<&'a dyn super::ResultRecorder>,
}

impl<Keys: VaultKeyProvider + 'static> ScheduleTaskRunner
    for RegisteredScheduleTaskRunner<'_, Keys>
{
    fn run<'a>(
        &'a self,
        request: A2ASendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<A2ATask, AgentFailure>> + Send + 'a>> {
        Box::pin(schedule::run_registered(
            self.endpoint,
            self.coordinator,
            request,
            self.turn_request,
            self.context,
            self.recorder,
        ))
    }
}

pub(super) async fn run<Keys: VaultKeyProvider + 'static>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    context: AgentContext,
    cancellation: floe_agent::Cancellation,
    on_admitted: impl FnMut(&floe_conversation::RunReceipt),
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    Box::pin(run_general_turn(
        inputs,
        context,
        cancellation,
        on_admitted,
        emit,
    ))
    .await
}

pub(super) struct ConversationExperts<'model> {
    pub(super) model: &'model Model,
    pub(super) source_client: Option<&'model floe_infra::ServerSourceClient>,
    pub(super) policy: &'model InferencePolicyDecision,
    pub(super) context: &'model AgentContext,
    pub(super) local_context: &'model LocalContextStore,
    pub(super) attention: Option<&'model dyn super::PersonalAttentionReaderApi>,
    pub(super) people_reader: Option<&'model dyn super::PersonalPeopleReaderApi>,
    pub(super) feasibility_reader: Option<&'model dyn super::PersonalFeasibilityReaderApi>,
    pub(super) wellbeing_reader: Option<&'model dyn super::PersonalWellbeingReaderApi>,
    pub(super) recorder: Option<&'model dyn super::ResultRecorder>,
    pub(super) remote_reader: Option<&'model dyn floe_context::SourceReader>,
    pub(super) context_reader: Option<&'model dyn super::ConversationContextReaderApi>,
    pub(super) task_views: &'model [NativeContextView],
    pub(super) cards: Vec<AgentCard>,
    pub(super) builtin_setup: Option<BuiltinExpertSetupReceipt>,
    pub(super) schedule_runner: Option<&'model dyn ScheduleTaskRunner>,
}

impl ConversationExperts<'_> {
    fn source_granted(&self, agent_id: &str, source: BuiltinContextSource) -> bool {
        let _ = self.local_context;
        let Some(setup) = &self.builtin_setup else {
            return true;
        };
        setup.assignments.iter().any(|assignment| {
            assignment.expert.package_id() == agent_id
                && setup.sources.iter().any(|binding| {
                    binding.source == source
                        && assignment
                            .granted_view_handles
                            .contains(&binding.view_handle)
                })
        })
    }

    fn require_source(
        &self,
        agent_id: &str,
        source: BuiltinContextSource,
    ) -> Result<(), AgentFailure> {
        self.source_granted(agent_id, source)
            .then_some(())
            .ok_or(AgentFailure::CapabilityDenied)
    }

    async fn read_remote(
        &self,
        view_id: &str,
        consumer: &str,
        query: serde_json::Value,
        request: &A2ASendMessageRequest,
    ) -> Result<floe_context::SourceView<serde_json::Value>, AgentFailure> {
        super::read_context_source(
            self.remote_reader
                .ok_or(AgentFailure::CapabilityUnavailable)?,
            request.person_id,
            view_id,
            consumer,
            query,
            request.deadline,
            &request.cancellation,
        )
        .await
    }
}

impl InProcessAgent for ConversationExperts<'_> {
    fn agent_cards(&self, _: PersonId) -> Vec<AgentCard> {
        self.cards
            .iter()
            .filter(|card| {
                matches!(self.model, Model::Server(_))
                    || BuiltinExpertKind::from_package_id(&card.id)
                        .is_some_and(BuiltinExpertKind::supports_device_model)
            })
            .cloned()
            .collect()
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        if request.schema_version != AGENT_VERSION
            || request.message.role != A2AMessageRole::User
            || request.message.task_id.is_none()
            || !self
                .agent_cards(request.person_id)
                .iter()
                .any(|card| card.id == request.agent_id)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let expert = BuiltinExpertKind::from_package_id(&request.agent_id)
            .ok_or(AgentFailure::CapabilityDenied)?;
        let expert_started = std::time::Instant::now();
        tracing::info!(
            expert = request.agent_id,
            invocation_id = %request.message.task_id.unwrap(),
            "expert_invocation_started"
        );
        let assignment = request.message.text()?.to_owned();
        if expert == BuiltinExpertKind::Schedule {
            return self
                .schedule_runner
                .ok_or(AgentFailure::CapabilityUnavailable)?
                .run(request)
                .await;
        }
        self.require_source(&request.agent_id, expert.mandatory_source())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        let current_time_unix_ms =
            i64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?;
        let invocation_id = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
        let mut expert_context = self.context.clone();
        if !self.source_granted(&request.agent_id, BuiltinContextSource::ConfirmedMemory) {
            expert_context.memories.clear();
        }
        let mut task_views = self.task_views.to_vec();
        if expert == BuiltinExpertKind::Commitments
            && let Some(reader) = self.context_reader
        {
            if self.source_granted(&request.agent_id, BuiltinContextSource::ConfirmedMemory) {
                let snapshot = reader.memory().await?;
                expert_context.memories = snapshot.memories;
                floe_context::record_source_issue(
                    &mut expert_context.optional_context_issues,
                    floe_agent::ContextSource::Memory,
                    snapshot.issue,
                );
            }
            if self.source_granted(&request.agent_id, BuiltinContextSource::Tasks) {
                let acquired = floe_context::acquire_optional_source(
                    floe_agent::ContextSource::Tasks,
                    reader.tasks(),
                )
                .await?;
                floe_context::record_source_issue(
                    &mut expert_context.optional_context_issues,
                    floe_agent::ContextSource::Tasks,
                    acquired.issue.map(|issue| issue.reason),
                );
                task_views = acquired.value.into_iter().collect();
            }
        }
        let mail_invocation = |view| MailExpertInvocation {
            usage: request.usage.clone(),
            person_id: request.person_id,
            invocation_id,
            assignment: assignment.clone(),
            current_time_unix_ms,
            context: expert_context.clone(),
            view,
            max_output_bytes: request.max_output_bytes,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let portfolio_invocation = || PortfolioExpertInvocation {
            usage: request.usage.clone(),
            person_id: request.person_id,
            invocation_id,
            assignment: assignment.clone(),
            current_time_unix_ms,
            context: expert_context.clone(),
            max_output_bytes: request.max_output_bytes,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let personal_invocation = || PersonalExpertInvocation {
            usage: request.usage.clone(),
            person_id: request.person_id,
            invocation_id,
            assignment: assignment.clone(),
            current_time_unix_ms,
            context: expert_context.clone(),
            max_output_bytes: request.max_output_bytes,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let personal_views = PersonalViewSource {
            model: self.model,
            source_client: self.source_client,
            policy: self.policy,
            person_id: request.person_id,
            people_reader: self.people_reader,
            feasibility_reader: self.feasibility_reader,
            wellbeing_reader: self.wellbeing_reader,
            remote_reader: self.remote_reader,
            recorder: self.recorder,
            dependency_turn_id: invocation_id,
            dependency_result_id: invocation_id,
            consumer_name: if expert == BuiltinExpertKind::Relationships {
                "contacts.expert"
            } else {
                "assistant"
            },
        };
        let (summary, data) = match expert {
            BuiltinExpertKind::Schedule => unreachable!("schedule uses the registered task runner"),
            BuiltinExpertKind::Commitments => {
                let source_view = self
                    .read_remote(
                        "mail.communication",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION, "query": "", "cursor": 0, "limit": default_communication_limit()}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(
                        invocation_id,
                        invocation_id,
                        source_view.dependency().clone(),
                    )?;
                let view: floe_agent::CommunicationView =
                    serde_json::from_value(source_view.payload().clone())
                        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let calendars =
                    if self.source_granted(&request.agent_id, BuiltinContextSource::Calendar) {
                        personal_views
                            .calendar_views(request.deadline, &request.cancellation)
                            .await?
                    } else {
                        vec![]
                    };
                let result: CommitmentsExpertResult = run_commitments_expert_with_views(
                    model,
                    self.policy,
                    mail_invocation(view),
                    CommitmentsContextViews {
                        calendars,
                        tasks: if self
                            .source_granted(&request.agent_id, BuiltinContextSource::Tasks)
                        {
                            task_views
                        } else {
                            vec![]
                        },
                    },
                )
                .await?;
                drop(source_view);
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::Communication => {
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let source_view = self
                    .read_remote(
                        "mail.communication",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION, "query": "", "cursor": 0, "limit": default_communication_limit()}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(
                        invocation_id,
                        invocation_id,
                        source_view.dependency().clone(),
                    )?;
                let view: floe_agent::CommunicationView =
                    serde_json::from_value(source_view.payload().clone())
                        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let result: CommunicationExpertResult =
                    run_communication_expert(model, self.policy, mail_invocation(view)).await?;
                drop(source_view);
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::WorkContext => {
                let source_view = self
                    .read_remote(
                        "work.context",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(
                        invocation_id,
                        invocation_id,
                        source_view.dependency().clone(),
                    )?;
                let view: floe_agent::WorkContextView =
                    serde_json::from_value(source_view.payload().clone())
                        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let result: WorkContextExpertResult =
                    run_work_context_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
                drop(source_view);
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::LifeLogistics => {
                let source_view = self
                    .read_remote(
                        "life.logistics",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(
                        invocation_id,
                        invocation_id,
                        source_view.dependency().clone(),
                    )?;
                let view: floe_agent::LogisticsView =
                    serde_json::from_value(source_view.payload().clone())
                        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let result: LifeLogisticsExpertResult =
                    run_life_logistics_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
                drop(source_view);
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::Relationships => {
                let people = personal_views
                    .people_view(request.deadline, &request.cancellation)
                    .await?;
                let confirmed_interactions = if self.source_granted(
                    &request.agent_id,
                    BuiltinContextSource::ConfirmedInteractions,
                ) {
                    personal_views
                        .confirmed_interaction_views(
                            &people,
                            request.deadline,
                            &request.cancellation,
                        )
                        .await?
                } else {
                    vec![]
                };
                let result: RelationshipsExpertResult = run_relationships_expert_with_views(
                    self.model,
                    self.policy,
                    personal_invocation(),
                    RelationshipsContextViews {
                        people,
                        confirmed_interactions,
                    },
                )
                .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::FocusAttention => {
                let (attention, dependency) = {
                    if !matches!(self.model, Model::Foundation(_)) {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    self.attention
                        .ok_or(AgentFailure::CapabilityUnavailable)?
                        .read(
                            request.person_id,
                            personal_grants::ATTENTION_EXPERT_CONSUMER,
                            invocation_id,
                            invocation_id,
                            request.deadline,
                            &request.cancellation,
                        )
                        .await?
                };
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(invocation_id, invocation_id, dependency)?;
                let calendars =
                    if self.source_granted(&request.agent_id, BuiltinContextSource::Calendar) {
                        personal_views
                            .calendar_views(request.deadline, &request.cancellation)
                            .await?
                    } else {
                        vec![]
                    };
                let active_work =
                    if self.source_granted(&request.agent_id, BuiltinContextSource::WorkContext) {
                        personal_views
                            .work_context_views(request.deadline, &request.cancellation)
                            .await?
                    } else {
                        vec![]
                    };
                let result: FocusExpertResult = run_focus_expert_with_views(
                    self.model,
                    self.policy,
                    personal_invocation(),
                    FocusContextViews {
                        attention,
                        calendars,
                        active_work,
                    },
                )
                .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::Wellbeing => {
                let wellbeing = personal_views
                    .wellbeing_view(request.deadline, &request.cancellation)
                    .await?;
                let calendars =
                    if self.source_granted(&request.agent_id, BuiltinContextSource::Calendar) {
                        personal_views
                            .calendar_views(request.deadline, &request.cancellation)
                            .await?
                    } else {
                        vec![]
                    };
                let result: WellbeingExpertResult = run_wellbeing_expert_with_views(
                    self.model,
                    self.policy,
                    personal_invocation(),
                    WellbeingContextViews {
                        wellbeing,
                        calendars,
                    },
                )
                .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
        };
        let task = A2ATask {
            id: request.message.task_id.ok_or(AgentFailure::InvalidInput)?,
            context_id: request.message.context_id,
            agent_id: request.agent_id,
            state: A2ATaskState::Completed,
            history: vec![request.message],
            artifacts: vec![A2AArtifact {
                artifact_id: uuid::Uuid::new_v4(),
                name: expert.result_artifact_name().into(),
                parts: vec![
                    A2APart::Text { text: summary },
                    A2APart::Data {
                        media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                        data,
                    },
                ],
            }],
            failure: None,
        };
        tracing::info!(
            expert = task.agent_id,
            invocation_id = %task.id,
            elapsed_ms = expert_started.elapsed().as_millis() as u64,
            "expert_invocation_completed"
        );
        Ok(task)
    }
}
