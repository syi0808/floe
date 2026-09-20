//! Registering the builtin Expert endpoints and injecting the concrete readers
//! they run against.
//!
//! No Expert's judgment lives here. Each agent id is answered by the Expert
//! registered for it in `floe-experts-builtin`; this file only decides which
//! endpoints exist and which readers back the host port they use.

use super::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{AgentEndpoint, BoxFuture, EndpointInvocation, ExpertReport};
use floe_experts_builtin::{
    BuiltinExpertHost, BuiltinExpertKind, BuiltinExpertOutput, BuiltinExpertRequest,
};

pub(in crate::vault_host) mod schedule;

/// The Experts this host serves, and the judgment registered behind each id.
///
/// Registration is static: the composition root never picks an Expert from what
/// a request appears to mean.
pub(super) fn registered_experts<'turn, 'host>() -> floe_experts::ExpertDispatchTable<
    DelegatedMessageExperts<'turn, 'host>,
    BuiltinExpertRequest,
    BuiltinExpertOutput,
> {
    let mut table = floe_experts::ExpertDispatchTable::default();
    let registrations: [(
        BuiltinExpertKind,
        floe_experts::ExpertRun<
            DelegatedMessageExperts<'turn, 'host>,
            BuiltinExpertRequest,
            BuiltinExpertOutput,
        >,
    ); 7] = [
        (BuiltinExpertKind::Commitments, |host, request| {
            Box::pin(floe_experts_builtin::commitments::dispatch(host, request))
        }),
        (BuiltinExpertKind::Communication, |host, request| {
            Box::pin(floe_experts_builtin::communication::dispatch(host, request))
        }),
        (BuiltinExpertKind::WorkContext, |host, request| {
            Box::pin(floe_experts_builtin::work_context::dispatch(host, request))
        }),
        (BuiltinExpertKind::LifeLogistics, |host, request| {
            Box::pin(floe_experts_builtin::life_logistics::dispatch(
                host, request,
            ))
        }),
        (BuiltinExpertKind::Relationships, |host, request| {
            Box::pin(floe_experts_builtin::relationships::dispatch(host, request))
        }),
        (BuiltinExpertKind::FocusAttention, |host, request| {
            Box::pin(floe_experts_builtin::focus_attention::dispatch(
                host, request,
            ))
        }),
        (BuiltinExpertKind::Wellbeing, |host, request| {
            Box::pin(floe_experts_builtin::wellbeing::dispatch(host, request))
        }),
    ];
    for (kind, run) in registrations {
        table
            .register(kind.package_id(), run)
            .expect("each builtin Expert registers once");
    }
    table
}

#[derive(Clone)]
pub(crate) struct BuiltinExpertEndpointContext {
    pub request: crate::ConversationTurnRequest,
    pub context: AgentContext,
    pub session_id: Uuid,
    pub max_output_bytes: usize,
}

/// The endpoint the delegating Run invokes for every registered builtin Expert
/// that answers in process.
pub(crate) struct BuiltinExpertEndpoint<Keys> {
    core: Arc<FloeCore>,
    vault: Arc<EncryptedAgentVault<Keys>>,
    local_context: Arc<LocalContextHost>,
    contexts: Mutex<HashMap<Uuid, BuiltinExpertEndpointContext>>,
}

impl<Keys> BuiltinExpertEndpoint<Keys> {
    pub(crate) fn new(
        core: Arc<FloeCore>,
        vault: Arc<EncryptedAgentVault<Keys>>,
        local_context: Arc<LocalContextHost>,
    ) -> Self {
        Self {
            core,
            vault,
            local_context,
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn stage(
        &self,
        run_id: Uuid,
        context: BuiltinExpertEndpointContext,
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

    pub(crate) fn clear(&self, run_id: Uuid) -> Result<(), AgentFailure> {
        self.contexts
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .remove(&run_id);
        Ok(())
    }
}

impl<Keys: VaultKeyProvider + 'static> AgentEndpoint for BuiltinExpertEndpoint<Keys> {
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
            let registrations = registered_experts();
            if invocation.request.principal != self.vault.person_id().to_string()
                || !registrations.is_registered(&invocation.request.selected_agent_id)
                || staged.session_id != staged.request.session_id
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let source_client = staged
                .request
                .remote_route
                .as_ref()
                .map(|route| {
                    ServerSourceClient::new(route.route.clone(), route.calendar_connections.clone())
                })
                .transpose()?;
            let model = Model::new(staged.request.remote_route.clone())?;
            let remote_reader = match (&model, staged.request.remote_route.as_ref()) {
                (Model::Server(_), Some(route)) => {
                    let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
                    if pairing.person_id != self.vault.person_id().to_string()
                        || pairing.device_id != staged.request.device_id
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    Some(remote_views::RemoteViewReader::new(
                        &self.vault,
                        source_client
                            .as_ref()
                            .ok_or(AgentFailure::CapabilityUnavailable)?,
                        self.vault.person_id(),
                        &pairing.client_id,
                        &pairing.device_id,
                    ))
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
            let grants = floe_experts::SourceGrants::new(Some(
                self.vault
                    .builtin_expert_overview()
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .setup,
            ));
            let experts = ConversationExperts {
                model: &model,
                source_client: source_client.as_ref(),
                policy: &policy,
                context: &staged.context,
                local_context: &self.local_context,
                attention: Some(&attention_reader),
                people_reader: Some(&people_reader),
                wellbeing_reader: Some(&wellbeing_reader),
                recorder: Some(&recorder),
                remote_reader: remote_reader
                    .as_ref()
                    .map(|reader| reader as &dyn floe_context::SourceReader),
                context_reader: Some(&context_reader),
                task_views: &[],
                cards,
                grants,
                task_runners: &[],
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
                    message: floe_experts::A2AMessage {
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
            let coverage = governed_store
                .result_coverage(task_id, task_id)?
                .unwrap_or(floe_agent_contract::DependencyCoverage::Independent);
            floe_experts::expert_report(invocation, &task, run_id, coverage)
        })
    }
}

/// An Expert whose work is a delegated Task of its own rather than one bounded
/// in-process call.
pub(super) trait ExpertTaskRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        request: A2ASendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<A2ATask, AgentFailure>> + Send + 'a>>;
}

pub(super) struct RegisteredScheduleTaskRunner<'a, Keys: VaultKeyProvider> {
    pub coordinator: &'a floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    pub endpoint: &'a schedule::ScheduleEndpoint<Keys>,
    pub turn_request: &'a crate::ConversationTurnRequest,
    pub context: &'a AgentContext,
    pub recorder: Option<&'a dyn floe_experts::TaskCoverageRecorder>,
}

impl<Keys: VaultKeyProvider + 'static> ExpertTaskRunner for RegisteredScheduleTaskRunner<'_, Keys> {
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
    cancellation: floe_execution::Cancellation,
    on_admitted: impl FnMut(&floe_conversation::RunReceipt),
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_conversation::AgentSession, AgentFailure> {
    Box::pin(run_general_turn(
        inputs,
        context,
        cancellation,
        on_admitted,
        emit,
    ))
    .await
}

/// The concrete readers one turn's Experts run against.
///
/// This is the injection site for the host port the builtin Experts declare:
/// every field is a reader or policy decided elsewhere and handed in here.
pub(crate) struct ConversationExperts<'model> {
    pub(super) model: &'model Model,
    pub(super) source_client: Option<&'model floe_provider_adapters::sources::ServerSourceClient>,
    pub(super) policy: &'model InferencePolicyDecision,
    pub(super) context: &'model AgentContext,
    pub(super) local_context: &'model LocalContextHost,
    pub(super) attention: Option<&'model dyn super::PersonalAttentionReaderApi>,
    pub(super) people_reader: Option<&'model dyn super::PersonalPeopleReaderApi>,
    pub(super) wellbeing_reader: Option<&'model dyn super::PersonalWellbeingReaderApi>,
    pub(super) recorder: Option<&'model dyn super::ResultRecorder>,
    pub(super) remote_reader: Option<&'model dyn floe_context::SourceReader>,
    pub(super) context_reader: Option<&'model dyn super::ConversationContextReaderApi>,
    pub(super) task_views: &'model [NativeContextView],
    pub(super) cards: Vec<AgentCard>,
    /// What each Expert may read, as the registry decided it.
    pub(super) grants: floe_experts::SourceGrants,
    /// Experts that answer on the Task path, by the agent id they are registered
    /// under.
    pub(super) task_runners: &'model [(&'model str, &'model dyn ExpertTaskRunner)],
}

impl<'model> ConversationExperts<'model> {
    /// The injected readers, bound to the consumer identity the Expert reads as.
    fn personal_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        consumer_name: &'a str,
    ) -> PersonalViewSource<'a> {
        PersonalViewSource {
            model: self.model,
            source_client: self.source_client,
            policy: self.policy,
            person_id: request.person_id,
            people_reader: self.people_reader,
            wellbeing_reader: self.wellbeing_reader,
            remote_reader: self.remote_reader,
            recorder: self.recorder,
            dependency_turn_id: request.invocation_id,
            dependency_result_id: request.invocation_id,
            consumer_name,
        }
    }
}

/// One delegated message's Expert host.
///
/// Everything an Expert may read belongs to the turn and comes straight from
/// the turn's host. What belongs to the message alone is the ledger its model
/// attempts are charged to: it arrives with the message, and is bound to the
/// model here for exactly as long as that message runs.
pub(super) struct DelegatedMessageExperts<'turn, 'model> {
    experts: &'turn ConversationExperts<'model>,
    model: super::ExpertModelHost<'turn, Model>,
}

impl<'turn, 'model> BuiltinExpertHost for DelegatedMessageExperts<'turn, 'model> {
    type Model = super::ExpertModelHost<'turn, Model>;
    type SourceRead = floe_context::SourceView<serde_json::Value>;

    fn model(&self) -> &Self::Model {
        &self.model
    }

    fn server_model(&self) -> Option<&Self::Model> {
        matches!(self.experts.model, Model::Server(_)).then_some(&self.model)
    }

    fn device_model(&self) -> Option<&Self::Model> {
        matches!(self.experts.model, Model::Foundation(_)).then_some(&self.model)
    }

    fn policy(&self) -> &InferencePolicyDecision {
        self.experts.policy
    }

    /// The grant the registry recorded for this Expert and source.
    fn source_grant(
        &self,
        agent_id: &str,
        source: BuiltinContextSource,
    ) -> floe_context_contract::SourceGrant {
        self.experts.grants.grant(agent_id, source.source_id())
    }

    fn read_source_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        view_id: &'a str,
        query: serde_json::Value,
    ) -> floe_experts_builtin::Acquiring<'a, floe_context::SourceView<serde_json::Value>> {
        Box::pin(async move {
            super::read_context_source(
                self.experts
                    .remote_reader
                    .ok_or(AgentFailure::CapabilityUnavailable)?,
                request.person_id,
                view_id,
                &request.agent_id,
                query,
                request.deadline,
                &request.cancellation,
            )
            .await
        })
    }

    fn record_dependency(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure> {
        self.experts
            .recorder
            .ok_or(AgentFailure::CapabilityUnavailable)?
            .record(turn_id, result_id, dependency)
    }

    fn calendar_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, Vec<floe_context::CalendarContextView>> {
        Box::pin(async move {
            self.experts
                .personal_views(request, ASSISTANT_CONSUMER)
                .calendar_views(request.deadline, &request.cancellation)
                .await
        })
    }

    fn work_context_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, Vec<floe_context::WorkContextView>> {
        Box::pin(async move {
            self.experts
                .personal_views(request, ASSISTANT_CONSUMER)
                .work_context_views(request.deadline, &request.cancellation)
                .await
        })
    }

    fn people_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, floe_context::PeopleView> {
        Box::pin(async move {
            self.experts
                .personal_views(request, floe_experts_builtin::relationships::CONSUMER)
                .people_view(request.deadline, &request.cancellation)
                .await
        })
    }

    fn confirmed_interaction_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        people: &'a floe_context::PeopleView,
    ) -> floe_experts_builtin::Acquiring<'a, Vec<floe_context::ConfirmedInteractionView>> {
        Box::pin(async move {
            self.experts
                .personal_views(request, floe_experts_builtin::relationships::CONSUMER)
                .confirmed_interaction_views(people, request.deadline, &request.cancellation)
                .await
        })
    }

    fn wellbeing_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, floe_context::WellbeingView> {
        Box::pin(async move {
            self.experts
                .personal_views(request, ASSISTANT_CONSUMER)
                .wellbeing_view(request.deadline, &request.cancellation)
                .await
        })
    }

    fn attention_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<
        'a,
        (
            floe_context::AttentionView,
            floe_context_contract::ContextDependency,
        ),
    > {
        Box::pin(async move {
            self.experts
                .attention
                .ok_or(AgentFailure::CapabilityUnavailable)?
                .read(
                    request.person_id,
                    floe_access::ATTENTION_EXPERT_CONSUMER,
                    request.invocation_id,
                    request.invocation_id,
                    request.deadline,
                    &request.cancellation,
                )
                .await
        })
    }

    fn conversation_context_available(&self) -> bool {
        self.experts.context_reader.is_some()
    }

    fn memory_context<'a>(
        &'a self,
    ) -> floe_experts_builtin::Acquiring<'a, floe_knowledge::MemoryContextSnapshot> {
        Box::pin(async move {
            self.experts
                .context_reader
                .ok_or(AgentFailure::CapabilityUnavailable)?
                .memory()
                .await
        })
    }

    fn task_view<'a>(&'a self) -> floe_experts_builtin::Acquiring<'a, NativeContextView> {
        Box::pin(async move {
            self.experts
                .context_reader
                .ok_or(AgentFailure::CapabilityUnavailable)?
                .tasks()
                .await
        })
    }

    fn staged_task_views(&self) -> &[NativeContextView] {
        self.experts.task_views
    }
}

/// The consumer identity a general assistant read is made under.
const ASSISTANT_CONSUMER: &str = "assistant";

impl InProcessAgent for ConversationExperts<'_> {
    /// The Experts this turn may offer, for the model it is running on.
    ///
    /// Each card states where its Expert runs; nothing here reads the agent id.
    fn agent_cards(&self, _: PersonId) -> Vec<AgentCard> {
        floe_experts::eligible_cards(&self.cards, self.model.expert_eligibility())
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        let cards = self.agent_cards(request.person_id);
        let invocation_id = floe_experts::admit_expert_message(&request, &cards)?;
        let expert_started = std::time::Instant::now();
        tracing::info!(
            expert = request.agent_id,
            invocation_id = %invocation_id,
            "expert_invocation_started"
        );
        if let Some((_, runner)) = self
            .task_runners
            .iter()
            .find(|(agent_id, _)| *agent_id == request.agent_id)
        {
            return runner.run(request).await;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        let expert_request = BuiltinExpertRequest {
            agent_id: request.agent_id.clone(),
            person_id: request.person_id,
            invocation_id,
            assignment: request.message.text()?.to_owned(),
            current_time_unix_ms: i64::try_from(now.as_millis())
                .map_err(|_| AgentFailure::StaleContext)?,
            context: self.context.clone(),
            max_output_bytes: request.max_output_bytes,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        // This message's attempts are charged to the ledger it carries.
        let host = DelegatedMessageExperts {
            experts: self,
            model: super::ExpertModelHost {
                model: self.model,
                usage: request.usage.clone(),
            },
        };
        let output = registered_experts()
            .run(&request.agent_id, &host, &expert_request)
            .await?;
        let task = floe_experts::completed_expert_task(
            request,
            &output.artifact_name,
            output.summary,
            output.data,
        )?;
        tracing::info!(
            expert = task.agent_id,
            invocation_id = %task.id,
            elapsed_ms = expert_started.elapsed().as_millis() as u64,
            "expert_invocation_completed"
        );
        Ok(task)
    }
}
