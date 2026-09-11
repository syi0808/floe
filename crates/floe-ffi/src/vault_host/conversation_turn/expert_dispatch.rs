use super::*;

mod schedule;

pub(super) async fn run<Keys: VaultKeyProvider>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    context: AgentContext,
    cancellation: floe_agent::Cancellation,
    mut emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    if let Some(session) =
        schedule::try_run(inputs, context.clone(), cancellation.clone(), &mut emit).await?
    {
        return Ok(session);
    }
    run_general_turn(inputs, context, cancellation, emit).await
}

pub(super) struct ConversationExperts<'model> {
    pub(super) model: &'model Model,
    pub(super) policy: &'model InferencePolicyDecision,
    pub(super) context: &'model AgentContext,
    pub(super) local_context: &'model LocalContextStore,
    pub(super) task_views: &'model [NativeContextView],
    pub(super) cards: Vec<AgentCard>,
    pub(super) builtin_setup: Option<BuiltinExpertSetupReceipt>,
}

impl ConversationExperts<'_> {
    fn source_granted(&self, agent_id: &str, source: BuiltinContextSource) -> bool {
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
            local_context: self.local_context,
            person_id: request.person_id,
        };
        let (summary, data) = match expert {
            BuiltinExpertKind::Schedule => return Err(AgentFailure::CapabilityDenied),
            BuiltinExpertKind::Commitments => {
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let view = model
                    .read_communication_view(
                        "",
                        0,
                        default_communication_limit(),
                        request.deadline,
                        &request.cancellation,
                    )
                    .await?;
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
                let view = model
                    .read_communication_view(
                        "",
                        0,
                        default_communication_limit(),
                        request.deadline,
                        &request.cancellation,
                    )
                    .await?;
                let result: CommunicationExpertResult =
                    run_communication_expert(model, self.policy, mail_invocation(view)).await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::WorkContext => {
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let view = model
                    .read_work_context_view(request.deadline, &request.cancellation)
                    .await?;
                let result: WorkContextExpertResult =
                    run_work_context_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                )
            }
            BuiltinExpertKind::LifeLogistics => {
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let view = model
                    .read_logistics_view(request.deadline, &request.cancellation)
                    .await?;
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
                let attention = personal_views
                    .attention_view(request.deadline, &request.cancellation)
                    .await?;
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
        Ok(A2ATask {
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
        })
    }
}
