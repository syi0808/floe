use super::remote_views::RemoteViewReaderApi;
use super::*;

mod schedule;

pub(super) async fn run<Keys: VaultKeyProvider>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    context: AgentContext,
    cancellation: floe_agent::Cancellation,
    mut emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    if let Some(session) = Box::pin(schedule::try_run(
        inputs,
        context.clone(),
        cancellation.clone(),
        &mut emit,
    ))
    .await?
    {
        return Ok(session);
    }
    Box::pin(run_general_turn(inputs, context, cancellation, emit)).await
}

pub(super) struct ConversationExperts<'model> {
    pub(super) model: &'model Model,
    pub(super) policy: &'model InferencePolicyDecision,
    pub(super) context: &'model AgentContext,
    pub(super) local_context: &'model LocalContextStore,
    pub(super) attention: Option<&'model dyn super::PersonalAttentionReaderApi>,
    pub(super) people_reader: Option<&'model dyn super::PersonalPeopleReaderApi>,
    pub(super) feasibility_reader: Option<&'model dyn super::PersonalFeasibilityReaderApi>,
    pub(super) wellbeing_reader: Option<&'model dyn super::PersonalWellbeingReaderApi>,
    pub(super) recorder: Option<&'model dyn super::ResultRecorder>,
    pub(super) remote_reader: Option<&'model dyn RemoteViewReaderApi>,
    pub(super) task_views: &'model [NativeContextView],
    pub(super) cards: Vec<AgentCard>,
    pub(super) builtin_setup: Option<BuiltinExpertSetupReceipt>,
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
    ) -> Result<(serde_json::Value, floe_domain::ContextDependency), AgentFailure> {
        self.remote_reader
            .ok_or(AgentFailure::CapabilityUnavailable)?
            .read(
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
        let request_id = crate::diagnostics::request_id().unwrap_or_default();
        tracing::info!(
            request_id,
            expert = request.agent_id,
            invocation_id = %request.message.task_id.unwrap(),
            "expert_invocation_started"
        );
        self.require_source(&request.agent_id, expert.mandatory_source())?;
        let assignment = request.message.text()?.to_owned();
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
            BuiltinExpertKind::Schedule => return Err(AgentFailure::CapabilityDenied),
            BuiltinExpertKind::Commitments => {
                let (view, dependency) = self
                    .read_remote(
                        "mail.communication",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION, "query": "", "cursor": 0, "limit": default_communication_limit()}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(invocation_id, invocation_id, dependency)?;
                let view: floe_agent::CommunicationView = serde_json::from_value(view)
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
                            self.task_views.to_vec()
                        } else {
                            vec![]
                        },
                    },
                )
                .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::Communication => {
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let (view, dependency) = self
                    .read_remote(
                        "mail.communication",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION, "query": "", "cursor": 0, "limit": default_communication_limit()}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(invocation_id, invocation_id, dependency)?;
                let view: floe_agent::CommunicationView = serde_json::from_value(view)
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let result: CommunicationExpertResult =
                    run_communication_expert(model, self.policy, mail_invocation(view)).await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::WorkContext => {
                let (view, dependency) = self
                    .read_remote(
                        "work.context",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(invocation_id, invocation_id, dependency)?;
                let view: floe_agent::WorkContextView = serde_json::from_value(view)
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let result: WorkContextExpertResult =
                    run_work_context_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::LifeLogistics => {
                let (view, dependency) = self
                    .read_remote(
                        "life.logistics",
                        &request.agent_id,
                        serde_json::json!({"schema_version": AGENT_VERSION}),
                        &request,
                    )
                    .await?;
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(invocation_id, invocation_id, dependency)?;
                let view: floe_agent::LogisticsView = serde_json::from_value(view)
                    .map_err(|_| AgentFailure::CapabilityUnavailable)?;
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let result: LifeLogisticsExpertResult =
                    run_life_logistics_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
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
            request_id,
            expert = task.agent_id,
            invocation_id = %task.id,
            elapsed_ms = expert_started.elapsed().as_millis() as u64,
            "expert_invocation_completed"
        );
        Ok(task)
    }
}
