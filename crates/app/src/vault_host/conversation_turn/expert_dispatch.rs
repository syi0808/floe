//! Registering the builtin Expert endpoints and injecting the concrete readers
//! they run against.
//!
//! No Expert's judgment lives here. Each agent id is answered by the Expert
//! registered for it in `floe-experts-builtin`; this file only decides which
//! endpoints exist and which readers back the host port they use.

use super::expert_host::{
    CalendarContextReaderApi, CapturingRecorder, ConversationContextReader,
    ConversationContextReaderApi, CurrentCalendarContextReader, ExpertModelHost,
    PersonalAttentionReader, PersonalAttentionReaderApi, PersonalPeopleReader,
    PersonalPeopleReaderApi, PersonalViewSource, PersonalWellbeingReader,
    PersonalWellbeingReaderApi, ResultRecorder, StoreResultRecorder, expert_policy,
    read_context_source,
};
use super::*;
use std::sync::{Arc, Mutex};

use floe_agent_contract::{AgentEndpoint, BoxFuture, EndpointInvocation, ExpertReport};
use floe_experts_builtin::{
    BuiltinExpertHost, BuiltinExpertKind, BuiltinExpertOutput, BuiltinExpertRequest,
    StatefulExpertDraft,
};

mod stateful_settlement;
#[cfg(test)]
pub(super) use stateful_settlement::RejectStatefulSettlement;
use stateful_settlement::{StatefulExpertSettlement, VaultStatefulExpertSettlement};

/// The Experts this host serves, and the judgment registered behind each id.
///
/// Registration is static: the composition root never picks an Expert from what
/// a request appears to mean.
pub(super) fn registered_experts<'turn, 'host, 'msg>() -> floe_experts::ExpertDispatchTable<
    DelegatedMessageExperts<'turn, 'host, 'msg>,
    BuiltinExpertRequest,
    BuiltinExpertOutput,
> {
    let mut table = floe_experts::ExpertDispatchTable::default();
    let registrations: [(
        BuiltinExpertKind,
        floe_experts::ExpertRun<
            DelegatedMessageExperts<'turn, 'host, 'msg>,
            BuiltinExpertRequest,
            BuiltinExpertOutput,
        >,
    ); 8] = [
        (BuiltinExpertKind::Schedule, |host, request| {
            Box::pin(floe_experts_builtin::schedule::dispatch::dispatch(
                host, request,
            ))
        }),
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

/// The endpoint the delegating Run invokes for every registered builtin Expert
/// that answers in process.
///
/// The invocation is self-sufficient: session, device, AgentContext, and the
/// output bound arrive in its explicit execution context, and the
/// saved-connection store is injected at construction. No run-id staging
/// exists.
pub(crate) struct BuiltinExpertEndpoint<Keys> {
    core: Arc<FloeCore>,
    vault: Arc<EncryptedAgentVault<Keys>>,
    local_context: Arc<LocalContextHost>,
    connections: floe_provider_adapters::control::CurrentSavedConnectionStore,
}

impl<Keys> BuiltinExpertEndpoint<Keys> {
    pub(crate) fn new(
        core: Arc<FloeCore>,
        vault: Arc<EncryptedAgentVault<Keys>>,
        local_context: Arc<LocalContextHost>,
        connections: floe_provider_adapters::control::CurrentSavedConnectionStore,
    ) -> Self {
        Self {
            core,
            vault,
            local_context,
            connections,
        }
    }
}

impl<Keys: VaultKeyProvider + 'static> AgentEndpoint for BuiltinExpertEndpoint<Keys> {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        Box::pin(async move {
            let context = &invocation.request.execution_context;
            context.validate()?;
            let run_id = invocation
                .request
                .parent_run_id
                .ok_or(AgentFailure::InvalidInput)?;
            let registrations = registered_experts();
            if invocation.request.principal != self.vault.person_id().to_string()
                || !registrations.is_registered(&invocation.request.selected_agent_id)
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let person_id = self.vault.person_id();
            let source_client = ServerSourceClient::from_current_connection(
                &self.connections,
                &person_id.to_string(),
                &context.device_id,
            )?;
            let provider =
                floe_provider_adapters::models::RootModelProvider::from_current_connection_scoped(
                    &self.connections,
                    &person_id.to_string(),
                    &context.device_id,
                    floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                    floe_agent_contract::EXPERT_INFERENCE_CONSUMER,
                )?;
            let availability = floe_inference::InferenceAvailability::observe(
                &provider,
                floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                floe_agent_contract::EXPERT_INFERENCE_CONSUMER,
            )
            .await;
            let authority = floe_provider_adapters::control::SavedConnectionRecipientAuthority::new(
                self.connections.clone(),
                person_id.to_string(),
                context.device_id.clone(),
            );
            let personal_resolver = personal_grants::PersonalDependencyResolver {
                vault: &self.vault,
                local_context: &self.local_context,
                person_id,
                device_id: &context.device_id,
            };
            let remote_reader = match source_client.as_ref() {
                Some(client) => Some(remote_views::RemoteViewReader::new(
                    &self.vault,
                    client,
                    person_id,
                    client.source().client_id(),
                    client.source().device_id(),
                )),
                None => None,
            };
            let calendar_reader = CurrentCalendarContextReader {
                core: &self.core,
                vault: &self.vault,
                source_client: source_client.as_ref(),
                device_id: &context.device_id,
            };
            let remote_resolver = remote_reader
                .as_ref()
                .map(|reader| remote_views::RemoteDependencyResolver { reader });
            let calendar_resolver =
                crate::vault_host::calendar_access::NativeCalendarDependencyResolver {
                    core: &self.core,
                    vault: &self.vault,
                    person_id,
                    device_id: &context.device_id,
                };
            let resolver = CompositeDependencyResolver {
                personal: &personal_resolver,
                remote: remote_resolver
                    .as_ref()
                    .map(|resolver| resolver as &dyn floe_access::DependencyResolver),
                calendar: Some(&calendar_resolver),
            };
            let service = floe_inference::InferenceService::new(provider, resolver, authority);
            let attention_reader = PersonalAttentionReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &context.device_id,
            };
            let people_reader = PersonalPeopleReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &context.device_id,
            };
            let wellbeing_reader = PersonalWellbeingReader {
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &context.device_id,
            };
            let governed_store = self.vault.governed_general_store(context.session_id);
            let recorder = StoreResultRecorder {
                store: &governed_store,
            };
            let context_reader = ConversationContextReader {
                core: &self.core,
                vault: &self.vault,
                person_id: self.vault.person_id(),
            };
            let policy = expert_policy();
            let cards = self.vault.enabled_builtin_expert_cards().await?;
            let grants = floe_experts::SourceGrants::new(Some(
                self.vault
                    .builtin_expert_overview()
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .setup,
            ));
            let stateful_settlement = VaultStatefulExpertSettlement {
                vault: self.vault.as_ref(),
            };
            let experts = ConversationExperts {
                executor: &service,
                scope,
                availability,
                source_client: source_client.as_ref(),
                calendar_reader: Some(&calendar_reader as &dyn CalendarContextReaderApi),
                policy: &policy,
                context: &context.agent_context,
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
                stateful_settlement: &stateful_settlement,
                task_runners: &[],
            };
            let task_id = invocation.request.task_id.as_uuid();
            governed_store.record_result_independent(task_id, task_id)?;
            let task = experts
                .handle_message(A2ASendMessageRequest {
                    usage: Default::default(),
                    schema_version: AGENT_VERSION,
                    person_id: self.vault.person_id(),
                    session_id: context.session_id,
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
                    max_output_bytes: context.max_output_bytes,
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
    pub(super) executor: &'model dyn floe_inference::InferenceExecutor,
    pub(super) scope: &'model floe_execution::ExecutionScope,
    pub(super) availability: floe_inference::InferenceAvailability,
    pub(super) source_client: Option<&'model floe_provider_adapters::sources::ServerSourceClient>,
    pub(super) calendar_reader: Option<&'model dyn CalendarContextReaderApi>,
    pub(super) policy: &'model InferencePolicyDecision,
    pub(super) context: &'model AgentContext,
    pub(super) attention: Option<&'model dyn PersonalAttentionReaderApi>,
    pub(super) people_reader: Option<&'model dyn PersonalPeopleReaderApi>,
    pub(super) wellbeing_reader: Option<&'model dyn PersonalWellbeingReaderApi>,
    pub(super) recorder: Option<&'model dyn ResultRecorder>,
    pub(super) remote_reader: Option<&'model dyn floe_context::SourceReader>,
    pub(super) context_reader: Option<&'model dyn ConversationContextReaderApi>,
    pub(super) task_views: &'model [NativeContextView],
    pub(super) cards: Vec<AgentCard>,
    /// What each Expert may read, as the registry decided it.
    pub(super) grants: floe_experts::SourceGrants,
    pub(super) stateful_settlement: &'model dyn StatefulExpertSettlement,
    /// Experts that answer on the Task path, by the agent id they are registered
    /// under.
    pub(super) task_runners: &'model [(&'model str, &'model dyn ExpertTaskRunner)],
}

/// One delegated message's Expert host.
///
/// Everything an Expert may read belongs to the turn and comes straight from
/// the turn's host. What belongs to the message alone is the bounded child of
/// the Task scope its model attempts settle against, and the captured source
/// dependencies that make its dispatch coverage exact.
pub(super) struct DelegatedMessageExperts<'turn, 'model, 'msg> {
    experts: &'turn ConversationExperts<'model>,
    recorder: CapturingRecorder<'msg>,
    model: ExpertModelHost<'msg>,
}

impl<'turn, 'model, 'msg> DelegatedMessageExperts<'turn, 'model, 'msg> {
    /// The injected readers, bound to the consumer identity the Expert reads as.
    ///
    /// Reads record through the capturing recorder so the dependencies behind
    /// this message's views become the exact model-dispatch coverage.
    fn personal_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        consumer_name: &'a str,
    ) -> PersonalViewSource<'a> {
        PersonalViewSource {
            source_client: self.experts.source_client,
            calendar_reader: self.experts.calendar_reader,
            person_id: request.person_id,
            people_reader: self.experts.people_reader,
            wellbeing_reader: self.experts.wellbeing_reader,
            remote_reader: self.experts.remote_reader,
            recorder: Some(&self.recorder),
            dependency_turn_id: request.task_id,
            dependency_result_id: request.task_id,
            consumer_name,
        }
    }
}

impl<'turn, 'model, 'msg> BuiltinExpertHost for DelegatedMessageExperts<'turn, 'model, 'msg> {
    type Model = ExpertModelHost<'msg>;
    type SourceRead = floe_context::SourceView<serde_json::Value>;

    fn model(&self) -> &Self::Model {
        &self.model
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
            read_context_source(
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
        if self.experts.recorder.is_none() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        self.recorder.record(turn_id, result_id, dependency)
    }

    fn calendar_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        query: floe_context_contract::CalendarViewQuery,
    ) -> floe_experts_builtin::Acquiring<
        'a,
        floe_context_contract::SourceReadOutcome<Vec<floe_context::CalendarContextView>>,
    > {
        Box::pin(async move {
            self.personal_views(request, &request.agent_id)
                .calendar_views(&query, request.deadline, &request.cancellation)
                .await
        })
    }

    fn settle_stateful_result<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        draft: StatefulExpertDraft,
    ) -> floe_experts_builtin::Acquiring<'a, BuiltinExpertOutput> {
        let dependencies = self
            .recorder
            .captured
            .lock()
            .map(|captured| captured.clone())
            .map_err(|_| AgentFailure::StorageUnavailable);
        Box::pin(async move {
            self.experts
                .stateful_settlement
                .settle(request, draft, dependencies?)
                .await
        })
    }

    fn work_context_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, Vec<floe_context::WorkContextView>> {
        Box::pin(async move {
            self.personal_views(request, ASSISTANT_CONSUMER)
                .work_context_views(request.deadline, &request.cancellation)
                .await
        })
    }

    fn people_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, floe_context::PeopleView> {
        Box::pin(async move {
            self.personal_views(request, floe_experts_builtin::relationships::CONSUMER)
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
            self.personal_views(request, floe_experts_builtin::relationships::CONSUMER)
                .confirmed_interaction_views(people, request.deadline, &request.cancellation)
                .await
        })
    }

    fn wellbeing_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> floe_experts_builtin::Acquiring<'a, floe_context::WellbeingView> {
        Box::pin(async move {
            self.personal_views(request, ASSISTANT_CONSUMER)
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
                    request.task_id,
                    request.task_id,
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
    /// The Experts this turn may offer, for the execution classes observed.
    ///
    /// Each card states where its Expert runs; offering unions the eligible
    /// cards per observed class. Nothing here reads the agent id, and nothing
    /// selects which profile a call runs on: Inference does that per call.
    fn agent_cards(&self, _: PersonId) -> Vec<AgentCard> {
        floe_experts::eligible_cards_for_availability(&self.cards, self.availability)
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
            task_id: request
                .message
                .task_id
                .ok_or(AgentFailure::CapabilityDenied)?,
            invocation_id,
            assignment: request.message.text()?.to_owned(),
            current_time_unix_ms: i64::try_from(now.as_millis())
                .map_err(|_| AgentFailure::StaleContext)?,
            context: self.context.clone(),
            max_output_bytes: request.max_output_bytes,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        // This message's attempts settle against a bounded child of the Task
        // scope, and its source reads are captured for the exact dispatch
        // coverage. Both bindings last exactly as long as this message runs.
        let captured = Mutex::new(Vec::new());
        let host = DelegatedMessageExperts {
            experts: self,
            recorder: CapturingRecorder {
                inner: self.recorder,
                captured: &captured,
            },
            model: ExpertModelHost {
                executor: self.executor,
                scope: self.scope,
                captured: &captured,
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
            output.artifacts,
            output.settlement,
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

#[cfg(test)]
mod registration_tests {
    use super::*;

    #[test]
    fn dispatch_table_matches_every_builtin_kind() {
        let mut actual: Vec<_> = registered_experts()
            .registered_ids()
            .map(str::to_owned)
            .collect();
        let mut expected: Vec<_> = BuiltinExpertKind::ALL
            .into_iter()
            .map(|kind| kind.package_id().to_owned())
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }
}
