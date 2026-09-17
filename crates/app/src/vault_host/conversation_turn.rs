use std::{future::Future, pin::Pin};

use floe_agent_contract::{AgentFailure, DataClass, ModelPlacement, TransferConsent};
use floe_context::{AgentContext, InferencePolicyDecision, NativeContextView};
use floe_context::{
    AttentionView, CalendarContextView, FeasibilityView, PeopleView, WellbeingView,
};
use floe_conversation::{
    AgentBudget, AgentEvent, CapabilityDescriptor, CapabilityHost, CapabilityInvocation,
    ModelRequest, ModelResponse, ModelRunner, SessionStore,
};
#[cfg(test)]
use floe_conversation::{AgentCommand, AgentRuntime};
use floe_experts::{
    A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, AgentCard, InProcessAgent,
};
use floe_experts_builtin::BuiltinContextSource;
use floe_kernel::AGENT_VERSION;
// What the regressions below stand a turn up against: the expert results and
// task states they assert on, and the wire a fixture route is stated in.
use crate::{ConversationTurnRequest, RemoteTurnRoute};
#[cfg(test)]
use floe_connections::CalendarConnectionRef;
use floe_conversation::GovernedSessionStore;
#[cfg(test)]
use floe_experts::{
    A2AArtifact, A2ATaskState, BuiltinExpertSetupReceipt, EXPERT_RESULT_MEDIA_TYPE,
};
#[cfg(test)]
use floe_experts_builtin::commitments::{
    CommitmentsContextViews, CommitmentsExpertResult, run_commitments_expert_with_views,
};
#[cfg(test)]
use floe_experts_builtin::communication::{CommunicationExpertResult, run_communication_expert};
#[cfg(test)]
use floe_experts_builtin::focus_attention::{
    FocusContextViews, FocusExpertResult, run_focus_expert_with_views,
};
#[cfg(test)]
use floe_experts_builtin::life_logistics::{LifeLogisticsExpertResult, run_life_logistics_expert};
#[cfg(test)]
use floe_experts_builtin::relationships::{
    RelationshipsContextViews, RelationshipsExpertResult, run_relationships_expert_with_views,
};
#[cfg(test)]
use floe_experts_builtin::wellbeing::{
    WellbeingContextViews, WellbeingExpertResult, run_wellbeing_expert_with_views,
};
#[cfg(test)]
use floe_experts_builtin::work_context::{WorkContextExpertResult, run_work_context_expert};
#[cfg(test)]
use floe_experts_builtin::{
    BuiltinExpertKind, MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation,
};
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use crate::FloeCore;
use crate::local_context::LocalContextHost;
use floe_provider_adapters::models::{FoundationModelRunner, ServerModelRunner};
// BOUNDARY(stage-3): the conversation turn still reaches the provider adapter
// directly. Source acquisition must arrive through a Context-owned port.
use floe_provider_adapters::sources::ServerSourceClient;
use floe_provider_adapters::sources::server::CalendarContextRequest;

use super::personal_grants;
use super::remote_views;

pub(super) mod engine_ports;
pub(super) mod expert_dispatch;

const FINALIZATION_TOKENS: u64 = 1_024;
const FINALIZATION_COST_MICROS: u64 = 10_000;

struct ConversationTurnInputs<'a, Keys: VaultKeyProvider> {
    core: &'a FloeCore,
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextHost,
    person_id: PersonId,
    request: &'a ConversationTurnRequest,
    command_id: floe_agent_contract::CommandId,
    conversation_repository: &'a std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &'a std::sync::Arc<floe_conversation::RunCancellationRegistry>,
    task_coordinator: &'a floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    schedule_endpoint: &'a expert_dispatch::schedule::ScheduleEndpoint<Keys>,
    builtin_expert_endpoint: &'a expert_dispatch::BuiltinExpertEndpoint<Keys>,
    /// The Run this turn continues, as Conversation admitted it.
    mode: floe_conversation::TurnMode,
}

pub(super) async fn run<Keys: VaultKeyProvider + 'static>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextHost,
    task_coordinator: &floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    schedule_endpoint: &expert_dispatch::schedule::ScheduleEndpoint<Keys>,
    builtin_expert_endpoint: &expert_dispatch::BuiltinExpertEndpoint<Keys>,
    conversation_repository: &std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &std::sync::Arc<floe_conversation::RunCancellationRegistry>,
    person_id: PersonId,
    command_id: floe_agent_contract::CommandId,
    request: &ConversationTurnRequest,
    cancellation: floe_execution::Cancellation,
    on_admitted: impl FnMut(&floe_conversation::RunReceipt),
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_conversation::AgentSession, AgentFailure> {
    let prepared = floe_conversation::prepare_turn(
        conversation_repository.as_ref(),
        vault,
        floe_conversation::TurnPreparationRequest {
            principal: person_id.to_string(),
            person_id,
            command_id,
            session_id: request.session_id,
            expected_revision: request.expected_revision,
            text: &request.text,
            device_id: &request.device_id,
            continuation: request.continuation,
            boundary: &floe_experts_builtin::schedule::CalendarHistoryBoundary,
        },
    )
    .await?;
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        optional_context_issues: vec![],
        evidence: vec![],
    };
    let inputs = ConversationTurnInputs {
        core,
        vault,
        local_context,
        person_id,
        request,
        command_id,
        conversation_repository,
        run_cancellations,
        task_coordinator,
        schedule_endpoint,
        builtin_expert_endpoint,
        mode: prepared.mode,
    };
    Box::pin(expert_dispatch::run(
        &inputs,
        context,
        cancellation,
        on_admitted,
        emit,
    ))
    .await
}

#[cfg(test)]
async fn conversation_context(
    reader: &impl floe_knowledge::MemoryContextReader,
) -> Result<AgentContext, AgentFailure> {
    let snapshot = floe_context::acquire_memory_context(reader, chrono::Utc::now()).await?;
    Ok(AgentContext {
        projection_version: 1,
        persona: None,
        memories: snapshot.memories,
        optional_context_issues: snapshot
            .issue
            .map(|reason| floe_agent_contract::ContextIssue {
                source: floe_agent_contract::ContextSource::Memory,
                reason,
            })
            .into_iter()
            .collect(),
        evidence: vec![],
    })
}

async fn run_general_turn<Keys: VaultKeyProvider + 'static>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    context: AgentContext,
    cancellation: floe_execution::Cancellation,
    on_admitted: impl FnMut(&floe_conversation::RunReceipt),
    mut emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_conversation::AgentSession, AgentFailure> {
    let core = inputs.core;
    let vault = inputs.vault;
    let local_context = inputs.local_context;
    let person_id = inputs.person_id;
    let request = inputs.request;
    let session_id = request.session_id;
    let source_client = request
        .remote_route
        .as_ref()
        .map(|route| {
            ServerSourceClient::new(route.route.clone(), route.calendar_connections.clone())
        })
        .transpose()?;
    let model = Model::new(request.remote_route.clone())?;
    let remote_reader = match (&model, request.remote_route.as_ref()) {
        (Model::Server(_), Some(route)) => {
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            floe_access::admit_device_pairing(
                person_id,
                &request.device_id,
                floe_access::RemotePairingIdentity {
                    person_id: &pairing.person_id,
                    client_id: &pairing.client_id,
                    device_id: &pairing.device_id,
                },
            )?;
            Some(remote_views::RemoteViewReader::new(
                vault,
                source_client
                    .as_ref()
                    .ok_or(AgentFailure::CapabilityUnavailable)?,
                person_id,
                &pairing.client_id,
                &pairing.device_id,
            ))
        }
        _ => None,
    };
    let personal_liveness = personal_grants::PersonalDependencyLiveness {
        local_context,
        person_id,
        device_id: &request.device_id,
    };
    let personal_resolver = personal_grants::PersonalDependencyResolver {
        vault,
        local_context,
        person_id,
        device_id: &request.device_id,
    };
    let remote_resolver = remote_reader
        .as_ref()
        .map(|reader| remote_views::RemoteDependencyResolver { reader });
    let liveness = CompositeDependencyLiveness {
        personal: &personal_liveness,
        remote: remote_resolver
            .as_ref()
            .map(|resolver| resolver as &dyn floe_access::DependencyLiveness),
    };
    let governed_store = vault.governed_general_store_with_liveness(session_id, &liveness);
    let resolver = CompositeDependencyResolver {
        personal: &personal_resolver,
        remote: remote_resolver
            .as_ref()
            .map(|resolver| resolver as &dyn floe_access::DependencyResolver),
    };
    let attention_reader = PersonalAttentionReader {
        vault,
        local_context,
        device_id: &request.device_id,
    };
    let people_reader = PersonalPeopleReader {
        vault,
        local_context,
        device_id: &request.device_id,
    };
    let feasibility_reader = PersonalFeasibilityReader {
        vault,
        local_context,
        device_id: &request.device_id,
    };
    let wellbeing_reader = PersonalWellbeingReader {
        vault,
        local_context,
        device_id: &request.device_id,
    };
    let result_recorder = StoreResultRecorder {
        store: &governed_store,
    };
    let policy = policy(&model, request.remote_route.as_ref());
    let context_reader = ConversationContextReader {
        core,
        vault,
        person_id,
    };
    let mut expert_cards = vault.enabled_expert_cards().await?;
    if let Some(schedule_card) =
        expert_dispatch::schedule::eligible_card(vault, &request.device_id).await?
        && !expert_cards.iter().any(|card| card.id == schedule_card.id)
    {
        expert_cards.push(schedule_card);
    }
    let builtin_setup = vault
        .builtin_expert_overview()
        .await?
        .map(|overview| overview.setup);
    let capabilities = ConversationCapabilities {
        model: &model,
        policy: &policy,
        local_context,
        attention: Some(&attention_reader),
        people_reader: Some(&people_reader),
        feasibility_reader: Some(&feasibility_reader),
        wellbeing_reader: Some(&wellbeing_reader),
        recorder: Some(&result_recorder),
        remote_reader: remote_reader
            .as_ref()
            .map(|reader| reader as &dyn floe_context::SourceReader),
    };
    let schedule_runner = expert_dispatch::RegisteredScheduleTaskRunner {
        coordinator: inputs.task_coordinator,
        endpoint: inputs.schedule_endpoint,
        turn_request: request,
        context: &context,
        recorder: Some(&result_recorder),
    };
    let experts = ConversationExperts {
        model: &model,
        source_client: source_client.as_ref(),
        policy: &policy,
        context: &context,
        local_context,
        attention: Some(&attention_reader),
        people_reader: Some(&people_reader),
        feasibility_reader: Some(&feasibility_reader),
        wellbeing_reader: Some(&wellbeing_reader),
        recorder: Some(&result_recorder),
        remote_reader: remote_reader
            .as_ref()
            .map(|reader| reader as &dyn floe_context::SourceReader),
        context_reader: Some(&context_reader),
        task_views: &[],
        cards: expert_cards.clone(),
        grants: floe_experts::SourceGrants::new(builtin_setup.clone()),
        task_runners: &[(
            floe_experts_builtin::BuiltinExpertKind::Schedule.package_id(),
            &schedule_runner,
        )],
    };
    {
        let legacy_capabilities = capabilities.descriptors(person_id);
        let active_agents = experts.agent_cards(person_id);
        let catalog = floe_agent_contract::AllowedCatalog {
            cards: active_agents
                .iter()
                .map(engine_ports::contract_definition)
                .collect(),
            tools: engine_ports::contract_tools(&legacy_capabilities),
            revision: builtin_setup
                .as_ref()
                .map_or(1, |setup| setup.expected_revision.max(1)),
        };
        let budget = AgentBudget::default();
        let duration = std::time::Duration::from_millis(budget.deadline_ms);
        let deadline = tokio::time::Instant::now() + duration;
        let service = floe_conversation::ConversationService::with_run_cancellations(
            std::sync::Arc::clone(inputs.conversation_repository),
            floe_conversation::ManagerConfig {
                role_spec: floe_agent_contract::RoleSpec {
                    role_id: "manager".into(),
                    prompt: floe_conversation::prompts::manager_prompt(context.persona.as_ref())?
                        .render(),
                    output_contract: "Return one user-facing answer or one registered delegation."
                        .into(),
                },
                max_iterations: budget.max_iterations.min(64),
                max_output_bytes: budget.max_output_bytes,
                max_run_duration: duration,
                budget: floe_execution::budget::BudgetConfig::new(
                    budget.max_tokens,
                    budget.max_cost_micros,
                )
                .with_finalization_reserve(
                    FINALIZATION_TOKENS,
                    FINALIZATION_COST_MICROS.min(budget.max_cost_micros),
                ),
            },
            std::sync::Arc::clone(inputs.run_cancellations),
        )?;
        let root_model = RootModel(&model);
        let model_port = engine_ports::LegacyModelPort {
            model: &root_model,
            store: &governed_store,
            resolver: &resolver,
            policy: &policy,
            context: &context,
            person_id,
            session_id,
            capabilities: legacy_capabilities,
            active_agents,
            max_output_bytes: budget.max_output_bytes,
        };
        let tool_port = engine_ports::LegacyToolPort {
            host: &capabilities,
            store: &governed_store,
            person_id,
            session_id,
            max_output_bytes: budget.max_output_bytes,
        };
        let delegation_port = engine_ports::LegacyDelegationPort {
            task_coordinator: inputs.task_coordinator,
            schedule_endpoint: inputs.schedule_endpoint,
            builtin_expert_endpoint: inputs.builtin_expert_endpoint,
            turn_request: request,
            context: &context,
            session_id,
            max_output_bytes: budget.max_output_bytes,
        };
        let execution_profile =
            floe_vault::execution_profile(floe_inference::ModelTransport::placement(&model));
        let retry_of = request
            .retry_of
            .map(|run_id| {
                floe_agent_contract::RunId::from_uuid(run_id).ok_or(AgentFailure::InvalidInput)
            })
            .transpose()?;
        let profile = request.profile.clone();
        let receipt = service
            .run_turn_observed(
                floe_conversation::TurnRequest {
                    command_id: inputs.command_id,
                    session_id,
                    expected_session_revision: request.expected_revision,
                    principal: person_id.to_string(),
                    prompt: request.text.clone(),
                    mode: inputs.mode.clone(),
                    retry_of,
                    profile,
                    execution_profile: execution_profile.into(),
                    bounded_context: floe_agent_contract::BoundedContext {
                        text: String::new(),
                        coverage: floe_agent_contract::DependencyCoverage::Independent,
                    },
                    allowed_catalog: catalog,
                    replay: vec![],
                    deadline,
                    cancellation,
                },
                floe_conversation::ConversationPorts {
                    model: &model_port,
                    tools: &tool_port,
                    delegation: &delegation_port,
                    validator: &engine_ports::ManagerPayloadValidator,
                },
                on_admitted,
            )
            .await?;
        let session = vault.load(person_id, session_id).await?;
        emit(floe_conversation::AgentEvent {
            schema_version: AGENT_VERSION,
            session_id,
            turn_id: receipt.run_id.as_uuid(),
            event: floe_conversation::AgentEventKind::Finished {
                outcome: session
                    .last_outcome
                    .ok_or(AgentFailure::StorageUnavailable)?,
                revision: session.revision,
            },
        });
        Ok(session)
    }
}

trait ConversationContextReaderApi: Send + Sync {
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

struct ConversationContextReader<'a, Keys: VaultKeyProvider> {
    core: &'a FloeCore,
    vault: &'a EncryptedAgentVault<Keys>,
    person_id: PersonId,
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
            floe_context::application::day_context_views::task_context_view(
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
async fn optional_task_views(
    core: &FloeCore,
    person_id: PersonId,
    context: &mut AgentContext,
) -> Result<Vec<NativeContextView>, AgentFailure> {
    let handle = uuid::Uuid::new_v5(&person_id.0, b"floe.tasks");
    let acquired = floe_context::acquire_optional_source(
        floe_agent_contract::ContextSource::Tasks,
        floe_context::application::day_context_views::task_context_view(
            &core.store,
            person_id,
            handle,
            chrono::Utc::now(),
            16,
            8 * 1024,
        ),
    )
    .await?;
    floe_context::record_source_issue(
        &mut context.optional_context_issues,
        floe_agent_contract::ContextSource::Tasks,
        acquired.issue.map(|issue| issue.reason),
    );
    Ok(acquired.value.into_iter().collect())
}

pub(super) async fn recover<Keys: VaultKeyProvider + 'static>(
    vault: &EncryptedAgentVault<Keys>,
    conversation_repository: &std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    person_id: PersonId,
    session_id: Uuid,
    expected_revision: u64,
) -> Result<floe_conversation::AgentSession, AgentFailure> {
    floe_conversation::recovered_session(
        conversation_repository.as_ref(),
        vault,
        person_id,
        floe_conversation::RecoveryRequest {
            session_id,
            expected_session_revision: expected_revision,
            principal: person_id.to_string(),
        },
    )
    .await
}

fn policy(model: &Model, route: Option<&RemoteTurnRoute>) -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "everyday-assistance".into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![floe_inference::ModelTransport::placement(model)],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: external_transfer_consent(
            floe_inference::ModelTransport::placement(model),
            route,
        ),
        bounded_sensitive_projection: false,
    }
}

/// Read the route's recipient off the worker envelope and let Inference say
/// whether this run's input may reach it.
fn external_transfer_consent(
    placement: ModelPlacement,
    route: Option<&RemoteTurnRoute>,
) -> TransferConsent {
    floe_inference::external_transfer_consent(placement, route.map(RemoteTurnRoute::recipient))
}

pub(crate) enum Model {
    Foundation(FoundationModelRunner),
    Server(ServerModelRunner),
}

impl Model {
    fn new(route: Option<RemoteTurnRoute>) -> Result<Self, AgentFailure> {
        match route {
            Some(route) => ServerModelRunner::new_model_only(route.route).map(Self::Server),
            None => Ok(Self::Foundation(FoundationModelRunner::encrypted())),
        }
    }
}

impl floe_inference::ModelTransport for Model {
    fn placement(&self) -> ModelPlacement {
        match self {
            Self::Foundation(model) => model.placement(),
            Self::Server(model) => model.placement(),
        }
    }

    async fn generate(
        &self,
        request: floe_inference::ModelTransportRequest,
    ) -> Result<floe_inference::ModelTransportResponse, AgentFailure> {
        let started = std::time::Instant::now();
        let placement = match self {
            Self::Foundation(_) => "device_local",
            Self::Server(_) => "remote",
        };
        tracing::info!(placement, "model_attempt_started");
        let result = match self {
            Self::Foundation(model) => model.generate(request).await,
            Self::Server(model) => model.generate(request).await,
        };
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match &result {
            Ok(_) => tracing::info!(placement, elapsed_ms, "model_attempt_completed"),
            Err(failure) => tracing::error!(
                placement,
                elapsed_ms,
                failure = ?failure,
                "model_attempt_failed"
            ),
        }
        result
    }
}

/// The root Run's model boundary over whichever transport was chosen.
///
/// Bounding the history a source may still be shown in is the Session owner's
/// rule, so it is applied here rather than inside a transport.
pub(crate) struct RootModel<'a>(pub(crate) &'a Model);

impl ModelRunner for RootModel<'_> {
    fn placement(&self) -> ModelPlacement {
        floe_inference::ModelTransport::placement(self.0)
    }

    async fn generate(&self, mut request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        floe_conversation::project_source_history(
            &mut request,
            &floe_experts_builtin::schedule::CalendarHistoryBoundary,
        );
        floe_conversation::TransportModelRunner::new(self.0)
            .generate(request)
            .await
    }
}

/// The model an Expert reasons on, as the Expert's own contract states it.
///
/// An Expert asks one question and is owed one answer. Turning that into the
/// conversation's model request, recovering a failed attempt, and charging what
/// it spent to this turn's ledger are the model owner's work, so they happen
/// here rather than inside the Expert.
pub(crate) struct ExpertModelHost<'a, Transport = Model> {
    pub(crate) model: &'a Transport,
    pub(crate) usage: floe_conversation::UsageLedger,
}

impl<Transport: floe_inference::ModelTransport + Sync> floe_agent_contract::ExpertModel
    for ExpertModelHost<'_, Transport>
{
    fn placement(&self) -> ModelPlacement {
        floe_inference::ModelTransport::placement(self.model)
    }

    fn answer<'a>(
        &'a self,
        call: floe_agent_contract::ExpertModelCall,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_agent_contract::ExpertModelAnswer, AgentFailure>,
    > {
        Box::pin(async move {
            let turn_id = Uuid::new_v4();
            let runner = floe_conversation::TransportModelRunner::new(self.model);
            let response = floe_conversation::generate_with_recovery(
                &runner,
                ModelRequest {
                    usage: self.usage.clone(),
                    replay: vec![],
                    schema_version: floe_agent_contract::AGENT_VERSION,
                    prompt: call.prompt,
                    person_id: call.person_id,
                    session_id: call.invocation_id,
                    turn_id,
                    policy: call.policy,
                    context: call.context,
                    messages: vec![floe_conversation::AgentMessage::User {
                        turn_id,
                        text: call.assignment,
                    }],
                    capabilities: vec![],
                    active_agents: vec![],
                    remaining_tokens: call.max_tokens,
                    remaining_cost_micros: call.max_cost_micros,
                    max_output_bytes: call.max_output_bytes,
                    deadline: call.deadline,
                    cancellation: call.cancellation,
                },
            )
            .await?;
            // One question, one reply: a preamble, a capability call or a
            // delegation is not an answer to an Expert's assignment.
            let [floe_conversation::ModelStep::Answer { text }] = response.output.as_slice() else {
                return Err(AgentFailure::InvalidModelOutput);
            };
            Ok(floe_agent_contract::ExpertModelAnswer {
                schema_version: response.schema_version,
                answer: text.clone(),
                used_tokens: response.used_tokens,
                cost_micros: response.cost_micros,
            })
        })
    }
}

impl<Transport: floe_inference::ModelTransport + Sync> floe_agent_contract::ExpertReasoner
    for ExpertModelHost<'_, Transport>
{
    fn step<'a>(
        &'a self,
        step: floe_agent_contract::ExpertReasoningStep,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_agent_contract::ExpertStepOutcome, AgentFailure>,
    > {
        Box::pin(async move {
            // The Expert's transcript is its own; it becomes conversation
            // messages only for as long as the model call lasts.
            let turn_id = step.invocation_id;
            let messages = step
                .transcript
                .into_iter()
                .map(|entry| match entry {
                    floe_agent_contract::ExpertTranscriptEntry::Task { text } => {
                        floe_conversation::AgentMessage::User { turn_id, text }
                    }
                    floe_agent_contract::ExpertTranscriptEntry::Preamble { text } => {
                        floe_conversation::AgentMessage::Preamble { turn_id, text }
                    }
                    floe_agent_contract::ExpertTranscriptEntry::Capability {
                        call_id,
                        capability_id,
                        input,
                        result,
                    } => floe_conversation::AgentMessage::Capability {
                        turn_id,
                        call_id,
                        capability_id,
                        input,
                        result: Ok(result),
                    },
                })
                .collect();
            let runner = floe_conversation::TransportModelRunner::new(self.model);
            let response = floe_conversation::generate_with_recovery(
                &runner,
                ModelRequest {
                    usage: self.usage.clone(),
                    replay: step.replay,
                    schema_version: floe_agent_contract::AGENT_VERSION,
                    prompt: step.prompt,
                    person_id: step.person_id,
                    session_id: step.invocation_id,
                    turn_id,
                    policy: step.policy,
                    context: step.context,
                    messages,
                    capabilities: step.capabilities,
                    active_agents: vec![],
                    remaining_tokens: step.remaining_tokens,
                    remaining_cost_micros: step.remaining_cost_micros,
                    max_output_bytes: step.max_output_bytes,
                    deadline: step.deadline,
                    cancellation: step.cancellation,
                },
            )
            .await?;
            Ok(floe_agent_contract::ExpertStepOutcome {
                schema_version: response.schema_version,
                steps: response
                    .output
                    .into_iter()
                    .map(|step| match step {
                        floe_conversation::ModelStep::Preamble { text } => {
                            Ok(floe_agent_contract::ExpertStep::Preamble { text })
                        }
                        floe_conversation::ModelStep::Answer { text } => {
                            Ok(floe_agent_contract::ExpertStep::Answer { text })
                        }
                        floe_conversation::ModelStep::Call {
                            capability_id,
                            input,
                        } => Ok(floe_agent_contract::ExpertStep::Call {
                            capability_id,
                            input,
                        }),
                        // An Expert has no one to delegate to.
                        floe_conversation::ModelStep::Delegate { .. } => {
                            Err(AgentFailure::CapabilityDenied)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                replay: response.replay,
                used_tokens: response.used_tokens,
                cost_micros: response.cost_micros,
            })
        })
    }
}

struct GovernedModel<'a, Keys, Runner: ModelRunner + ?Sized = RootModel<'a>> {
    model: &'a Runner,
    store: &'a GovernedSessionStore<'a, EncryptedAgentVault<Keys>>,
    resolver: &'a dyn floe_access::DependencyResolver,
}

impl<Keys: VaultKeyProvider, Runner: ModelRunner + ?Sized + Sync> ModelRunner
    for GovernedModel<'_, Keys, Runner>
{
    fn history_start(
        &self,
        messages: &[floe_conversation::AgentMessage],
        current_turn: Uuid,
        max_bytes: usize,
    ) -> Result<usize, AgentFailure> {
        floe_conversation::bounded_source_history_start(
            messages,
            current_turn,
            max_bytes,
            &floe_experts_builtin::schedule::CalendarHistoryBoundary,
        )
    }

    fn placement(&self) -> ModelPlacement {
        self.model.placement()
    }

    async fn generate(&self, mut request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.store
            .project_model_request(&mut request, Some(self.resolver))
            .await?;
        let fence_request = request.clone();
        let response = self.model.generate(request).await?;
        self.store
            .revalidate_current_coverage(&fence_request, self.resolver)
            .await?;
        Ok(response)
    }
}

struct CompositeDependencyLiveness<'a> {
    personal: &'a dyn floe_access::DependencyLiveness,
    remote: Option<&'a dyn floe_access::DependencyLiveness>,
}

impl floe_access::DependencyLiveness for CompositeDependencyLiveness<'_> {
    fn validate(
        &self,
        dependency: &floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure> {
        if floe_access::is_device_local_source(dependency.source().connector().as_str()) {
            self.personal.validate(dependency)
        } else {
            self.remote
                .ok_or(AgentFailure::PolicyDenied)?
                .validate(dependency)
        }
    }
}

struct CompositeDependencyResolver<'a> {
    personal: &'a dyn floe_access::DependencyResolver,
    remote: Option<&'a dyn floe_access::DependencyResolver>,
}

impl floe_access::DependencyResolver for CompositeDependencyResolver<'_> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a floe_context_contract::ContextDependency,
        request: &'a floe_access::DependencyAuthorization,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        if floe_access::is_device_local_source(dependency.source().connector().as_str()) {
            self.personal.authorize(dependency, request)
        } else if let Some(remote) = self.remote {
            remote.authorize(dependency, request)
        } else {
            Box::pin(async { Err(AgentFailure::PolicyDenied) })
        }
    }
}

struct PersonalViewSource<'a> {
    model: &'a Model,
    source_client: Option<&'a ServerSourceClient>,
    policy: &'a InferencePolicyDecision,
    person_id: PersonId,
    people_reader: Option<&'a dyn PersonalPeopleReaderApi>,
    feasibility_reader: Option<&'a dyn PersonalFeasibilityReaderApi>,
    wellbeing_reader: Option<&'a dyn PersonalWellbeingReaderApi>,
    remote_reader: Option<&'a dyn floe_context::SourceReader>,
    recorder: Option<&'a dyn ResultRecorder>,
    dependency_turn_id: Uuid,
    dependency_result_id: Uuid,
    consumer_name: &'a str,
}

impl PersonalViewSource<'_> {
    fn record_result_independent(&self) -> Result<(), AgentFailure> {
        if let (Some(recorder), false) = (self.recorder, self.dependency_turn_id.is_nil()) {
            recorder.record_independent(self.dependency_turn_id, self.dependency_result_id)?;
        }
        Ok(())
    }

    async fn people_view(
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

    async fn feasibility_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<FeasibilityView, AgentFailure> {
        self.record_result_independent()?;
        let reader = self
            .feasibility_reader
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

    #[cfg(test)]
    async fn attention_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<AttentionView, AgentFailure> {
        self.record_result_independent()?;
        let _ = (deadline, cancellation);
        Err(AgentFailure::CapabilityUnavailable)
    }

    async fn wellbeing_view(
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

    async fn calendar_views(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<CalendarContextView>, AgentFailure> {
        let Model::Server(_) = self.model else {
            return Ok(vec![]);
        };
        if !self.server_fallback_allowed() {
            return Ok(vec![]);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        let now = i64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?;
        let source_client = self
            .source_client
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let mut views = Vec::new();
        for connection in source_client.calendar_connections() {
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

    async fn confirmed_interaction_views(
        &self,
        people: &PeopleView,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<floe_context::ConfirmedInteractionView>, AgentFailure> {
        let Model::Server(_) = self.model else {
            return Ok(vec![]);
        };
        if !self.server_fallback_allowed() {
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

    async fn work_context_views(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<floe_context::WorkContextView>, AgentFailure> {
        let Model::Server(_model) = self.model else {
            return Ok(vec![]);
        };
        if !self.server_fallback_allowed() {
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

    fn server_fallback_allowed(&self) -> bool {
        matches!(self.model, Model::Server(_))
            && (floe_inference::ModelTransport::placement(self.model)
                == ModelPlacement::DeviceLocal
                || self.policy.external_transfer_consent == TransferConsent::Granted)
    }
}

#[cfg(test)]
struct NoCapabilities;

#[cfg(test)]
impl CapabilityHost for NoCapabilities {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        vec![]
    }

    async fn invoke(&self, _: CapabilityInvocation) -> Result<String, AgentFailure> {
        Err(AgentFailure::CapabilityDenied)
    }
}

struct ConversationCapabilities<'model> {
    model: &'model Model,
    policy: &'model InferencePolicyDecision,
    local_context: &'model LocalContextHost,
    attention: Option<&'model dyn PersonalAttentionReaderApi>,
    people_reader: Option<&'model dyn PersonalPeopleReaderApi>,
    feasibility_reader: Option<&'model dyn PersonalFeasibilityReaderApi>,
    wellbeing_reader: Option<&'model dyn PersonalWellbeingReaderApi>,
    recorder: Option<&'model dyn ResultRecorder>,
    remote_reader: Option<&'model dyn floe_context::SourceReader>,
}

async fn read_context_source(
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

trait ResultRecorder: Send + Sync {
    fn record_independent(&self, turn_id: Uuid, result_id: Uuid) -> Result<(), AgentFailure>;

    fn record(
        &self,
        turn_id: uuid::Uuid,
        result_id: uuid::Uuid,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<(), AgentFailure>;
}

struct StoreResultRecorder<'a, Keys: VaultKeyProvider> {
    store: &'a GovernedSessionStore<'a, EncryptedAgentVault<Keys>>,
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

trait PersonalAttentionReaderApi: Send + Sync {
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

trait PersonalPeopleReaderApi: Send + Sync {
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

trait PersonalFeasibilityReaderApi: Send + Sync {
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
                        (FeasibilityView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

trait PersonalWellbeingReaderApi: Send + Sync {
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

struct PersonalAttentionReader<'a, Keys: VaultKeyProvider> {
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextHost,
    device_id: &'a str,
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
                &super::personal_grants::native_driver(self.local_context),
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

struct PersonalPeopleReader<'a, Keys: VaultKeyProvider> {
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextHost,
    device_id: &'a str,
}

struct PersonalFeasibilityReader<'a, Keys: VaultKeyProvider> {
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextHost,
    device_id: &'a str,
}

struct PersonalWellbeingReader<'a, Keys: VaultKeyProvider> {
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextHost,
    device_id: &'a str,
}

impl<Keys: VaultKeyProvider> PersonalFeasibilityReaderApi for PersonalFeasibilityReader<'_, Keys> {
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
                        (FeasibilityView, floe_context_contract::ContextDependency),
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            floe_context::read_feasibility(
                &floe_vault::VaultGrantRecords::new(self.vault),
                &super::personal_grants::native_driver(self.local_context),
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
                &super::personal_grants::native_driver(self.local_context),
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
                &super::personal_grants::native_driver(self.local_context),
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

impl CapabilityHost for ConversationCapabilities<'_> {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        let _ = self.local_context;
        let mut descriptors = vec![
            read_capability("people.identity.read"),
            read_capability("schedule.feasibility.read"),
            read_capability("attention.coarse.read"),
            read_capability("wellbeing.derived.read"),
        ];
        if matches!(self.model, Model::Server(_)) {
            descriptors.extend([
                CapabilityDescriptor {
                    schema_version: AGENT_VERSION,
                    id: "mail.communication.read".into(),
                    version: "1.0.0".into(),
                    read_only: true,
                    output_data_class: DataClass::Personal,
                    input_schema: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string", "maxLength": 512},
                            "cursor": {"type": "integer", "minimum": 0, "maximum": 10000},
                            "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                        },
                        "additionalProperties": false
                    })),
                },
                read_capability("work.context.read"),
                read_capability("life.logistics.read"),
            ]);
        }
        descriptors
    }

    async fn invoke(&self, invocation: CapabilityInvocation) -> Result<String, AgentFailure> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            #[serde(default)]
            query: String,
            #[serde(default)]
            cursor: usize,
            #[serde(default = "default_communication_limit")]
            limit: usize,
        }
        if matches!(
            invocation.capability_id.as_str(),
            "mail.communication.read"
                | "work.context.read"
                | "life.logistics.read"
                | "people.identity.read"
                | "schedule.feasibility.read"
                | "attention.coarse.read"
                | "wellbeing.derived.read"
        ) {
            if let Some(recorder) = self.recorder {
                recorder.record_independent(invocation.turn_id, invocation.call_id)?;
            }
        }
        let output = match invocation.capability_id.as_str() {
            "mail.communication.read" => {
                let input: Input = serde_json::from_str(&invocation.input)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let reader = self
                    .remote_reader
                    .ok_or(AgentFailure::CapabilityUnavailable)?;
                let source_view = read_context_source(
                    reader,
                    invocation.person_id,
                    "mail.communication",
                    floe_access::ATTENTION_ASSISTANT_CONSUMER,
                    serde_json::json!({
                        "schema_version": AGENT_VERSION,
                        "query": input.query,
                        "cursor": input.cursor,
                        "limit": input.limit,
                    }),
                    invocation.deadline,
                    &invocation.cancellation,
                )
                .await?;
                let value = source_view.payload().clone();
                let dependency = source_view.dependency().clone();
                self.recorder
                    .ok_or(AgentFailure::CapabilityUnavailable)?
                    .record(invocation.turn_id, invocation.call_id, dependency)?;
                Ok(value)
            }
            "work.context.read"
            | "life.logistics.read"
            | "people.identity.read"
            | "schedule.feasibility.read"
            | "attention.coarse.read"
            | "wellbeing.derived.read" => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Empty {}
                serde_json::from_str::<Empty>(&invocation.input)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let personal = PersonalViewSource {
                    model: self.model,
                    source_client: None,
                    policy: self.policy,
                    person_id: invocation.person_id,
                    people_reader: self.people_reader,
                    feasibility_reader: self.feasibility_reader,
                    wellbeing_reader: self.wellbeing_reader,
                    remote_reader: self.remote_reader,
                    recorder: self.recorder,
                    dependency_turn_id: invocation.turn_id,
                    dependency_result_id: invocation.call_id,
                    consumer_name: "assistant",
                };
                match invocation.capability_id.as_str() {
                    "work.context.read" => {
                        let reader = self
                            .remote_reader
                            .ok_or(AgentFailure::CapabilityUnavailable)?;
                        let source_view = read_context_source(
                            reader,
                            invocation.person_id,
                            "work.context",
                            floe_access::ATTENTION_ASSISTANT_CONSUMER,
                            serde_json::json!({"schema_version": AGENT_VERSION}),
                            invocation.deadline,
                            &invocation.cancellation,
                        )
                        .await?;
                        let value = source_view.payload().clone();
                        let dependency = source_view.dependency().clone();
                        self.recorder
                            .ok_or(AgentFailure::CapabilityUnavailable)?
                            .record(invocation.turn_id, invocation.call_id, dependency)?;
                        Ok(value)
                    }
                    "life.logistics.read" => {
                        let reader = self
                            .remote_reader
                            .ok_or(AgentFailure::CapabilityUnavailable)?;
                        let source_view = read_context_source(
                            reader,
                            invocation.person_id,
                            "life.logistics",
                            floe_access::ATTENTION_ASSISTANT_CONSUMER,
                            serde_json::json!({"schema_version": AGENT_VERSION}),
                            invocation.deadline,
                            &invocation.cancellation,
                        )
                        .await?;
                        let value = source_view.payload().clone();
                        let dependency = source_view.dependency().clone();
                        self.recorder
                            .ok_or(AgentFailure::CapabilityUnavailable)?
                            .record(invocation.turn_id, invocation.call_id, dependency)?;
                        Ok(value)
                    }
                    "people.identity.read" => serde_json::to_value(
                        personal
                            .people_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    ),
                    "schedule.feasibility.read" => serde_json::to_value(
                        personal
                            .feasibility_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    ),
                    "attention.coarse.read" => {
                        if !matches!(self.model, Model::Foundation(_)) {
                            return Err(AgentFailure::CapabilityUnavailable);
                        }
                        let attention =
                            self.attention.ok_or(AgentFailure::CapabilityUnavailable)?;
                        let recorder = self.recorder.ok_or(AgentFailure::CapabilityUnavailable)?;
                        let (view, dependency) = attention
                            .read(
                                invocation.person_id,
                                floe_access::ATTENTION_ASSISTANT_CONSUMER,
                                invocation.call_id,
                                invocation.turn_id,
                                invocation.deadline,
                                &invocation.cancellation,
                            )
                            .await?;
                        recorder.record(invocation.turn_id, invocation.call_id, dependency)?;
                        serde_json::to_value(view)
                    }
                    "wellbeing.derived.read" => serde_json::to_value(
                        personal
                            .wellbeing_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    ),
                    _ => unreachable!(),
                }
            }
            _ => return Err(AgentFailure::CapabilityDenied),
        }
        .map_err(|_| AgentFailure::InvalidInput)?;
        serde_json::to_string(&output).map_err(|_| AgentFailure::InvalidInput)
    }
}

fn read_capability(id: &str) -> CapabilityDescriptor {
    CapabilityDescriptor {
        schema_version: AGENT_VERSION,
        id: id.into(),
        version: "1.0.0".into(),
        read_only: true,
        output_data_class: DataClass::Personal,
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })),
    }
}

fn default_communication_limit() -> usize {
    25
}

pub(crate) use expert_dispatch::ConversationExperts;

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use crate::LocalContextCommand;
    use floe_conversation::{AgentMessage, ModelStep};
    use floe_execution::Cancellation;
    use floe_experts_builtin::prompts::focus_expert_prompt;
    use floe_protocol::{
        LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionResultDto,
        LocalContextOperationDto, PersonalAccessChangeDto, PersonalAccessConfigurationDto,
    };
    use floe_provider_adapters::sources::native_acquisition::{
        AttentionAcquisitionMode, AttentionAcquisitionResult,
    };
    use floe_vault::{VaultKey, VaultKeyProvider};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use uuid::Uuid;

    use super::*;

    struct FixtureRemoteReader<'a> {
        source_client: &'a ServerSourceClient,
        person_id: PersonId,
    }

    impl floe_context::SourceReader for FixtureRemoteReader<'_> {
        fn read<'a>(
            &'a self,
            request: &'a floe_context::SourceReadRequest,
        ) -> Pin<Box<dyn Future<Output = Result<floe_context::SourceRead, AgentFailure>> + Send + 'a>>
        {
            Box::pin(async move {
                let view_id = request.source().as_str();
                let consumer = request.consumer().identifier();
                let query = request.query();
                let deadline = request.deadline();
                let cancellation = request.cancellation();
                let (value, category, connector) = match view_id {
                    "mail.communication" => {
                        let query = query.as_object().ok_or(AgentFailure::InvalidInput)?;
                        let text = query
                            .get("query")
                            .and_then(serde_json::Value::as_str)
                            .ok_or(AgentFailure::InvalidInput)?;
                        let cursor = query
                            .get("cursor")
                            .and_then(serde_json::Value::as_u64)
                            .ok_or(AgentFailure::InvalidInput)?;
                        let limit = query
                            .get("limit")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok())
                            .ok_or(AgentFailure::InvalidInput)?;
                        (
                            serde_json::to_value(
                                self.source_client
                                    .read_communication_view(
                                        text,
                                        cursor as usize,
                                        limit,
                                        deadline,
                                        cancellation,
                                    )
                                    .await?,
                            )
                            .map_err(|_| AgentFailure::InvalidModelOutput)?,
                            floe_context_contract::GrantDataCategory::Content,
                            "gmail",
                        )
                    }
                    "work.context" => (
                        serde_json::to_value(
                            self.source_client
                                .read_work_context_view(deadline, cancellation)
                                .await?,
                        )
                        .map_err(|_| AgentFailure::InvalidModelOutput)?,
                        floe_context_contract::GrantDataCategory::Derived,
                        "linear",
                    ),
                    "life.logistics" => (
                        serde_json::to_value(
                            self.source_client
                                .read_logistics_view(deadline, cancellation)
                                .await?,
                        )
                        .map_err(|_| AgentFailure::InvalidModelOutput)?,
                        floe_context_contract::GrantDataCategory::Derived,
                        "home",
                    ),
                    _ => return Err(AgentFailure::InvalidInput),
                };
                let connection_id = "00000000-0000-4000-8000-000000000099";
                let source = floe_context_contract::GrantSourceBinding::try_new(
                    self.person_id,
                    floe_context_contract::ConnectionId::try_new(connection_id)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    floe_context_contract::ConnectorId::try_new(connector)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    floe_context_contract::ExecutionOwnerId::try_new(
                        "00000000-0000-4000-8000-000000000098",
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?,
                    floe_context_contract::SourceAuthority::new(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let consumer = floe_context_contract::GrantConsumer::builtin(consumer)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let scope = floe_context_contract::GrantScope::try_new(
                    vec![
                        floe_context_contract::ResourceHandle::try_new(format!(
                            "{view_id}:{connection_id}"
                        ))
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    ],
                    vec![category],
                    vec![floe_context_contract::GrantOperation::Read],
                    vec![floe_context_contract::GrantPurpose::Assistant],
                    vec![consumer.clone()],
                    floe_context_contract::ProcessingRestriction::ApprovedRecipient {
                        recipient: "server-audience".into(),
                        categories: vec![category],
                    },
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let now = chrono::Utc::now();
                let dependency = floe_context_contract::ContextDependency::try_new(
                    self.person_id,
                    floe_context_contract::GrantId::new(),
                    floe_context_contract::GrantAuthority::new(),
                    source,
                    scope.resources().to_vec(),
                    scope.categories().to_vec(),
                    floe_context_contract::GrantOperation::Read,
                    floe_context_contract::GrantPurpose::Assistant,
                    consumer,
                    scope.processing().clone(),
                    floe_context_contract::ConsumerPolicyAuthority::new(),
                    Uuid::new_v4(),
                    request.query_fingerprint().to_vec(),
                    Uuid::new_v4(),
                    request.process_incarnation_id(),
                    now,
                    now + chrono::Duration::minutes(5),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                Ok(floe_context::SourceRead::new(
                    request.source().clone(),
                    value,
                    dependency,
                    scope,
                ))
            })
        }
    }

    struct FixtureResultRecorder;

    impl ResultRecorder for FixtureResultRecorder {
        fn record_independent(&self, _: Uuid, _: Uuid) -> Result<(), AgentFailure> {
            Ok(())
        }

        fn record(
            &self,
            _: Uuid,
            _: Uuid,
            _: floe_context_contract::ContextDependency,
        ) -> Result<(), AgentFailure> {
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct AttentionTestKeys(Arc<Mutex<HashMap<(PersonId, uuid::Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for AttentionTestKeys {
        fn load(
            &self,
            person_id: PersonId,
            vault_id: uuid::Uuid,
        ) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: uuid::Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    fn attention_test_view() -> AttentionView {
        let now = chrono::Utc::now().timestamp_millis();
        AttentionView {
            schema_version: 1,
            view_id: "attention.coarse".into(),
            source_handle: "attention.macos:session_idle".into(),
            observed_at_unix_ms: now.saturating_sub(1),
            expires_at_unix_ms: now.saturating_add(60_000),
            state: floe_context::AttentionState::Available,
            confidence_millis: 900,
            evidence_handles: vec!["attention:aggregate".into()],
        }
    }

    async fn drive_attention_host(
        local_context: Arc<LocalContextHost>,
        person_id: PersonId,
        host_epoch: String,
        subject: String,
        stop: Arc<AtomicBool>,
        inspect_count: Arc<AtomicUsize>,
        read_count: Arc<AtomicUsize>,
    ) {
        while !stop.load(Ordering::Acquire) {
            if let Ok(result) = local_context.execute(
                person_id,
                LocalContextCommand::PollAttentionAcquisitions {
                    host_epoch: host_epoch.clone(),
                },
                None,
            ) {
                for request in result.attention_acquisitions {
                    match request.mode {
                        AttentionAcquisitionMode::InspectSubject => {
                            inspect_count.fetch_add(1, Ordering::AcqRel);
                        }
                        AttentionAcquisitionMode::ReadProjection => {
                            read_count.fetch_add(1, Ordering::AcqRel);
                        }
                    }
                    let view = (request.mode == AttentionAcquisitionMode::ReadProjection)
                        .then(|| serde_json::to_value(attention_test_view()).unwrap());
                    let completion = AttentionAcquisitionResult {
                        request_id: request.request_id,
                        host_epoch: request.host_epoch,
                        person_id: request.person_id,
                        device_id: request.device_id,
                        mode: request.mode,
                        native_subject_fingerprint_before: subject.clone(),
                        native_subject_fingerprint_after: subject.clone(),
                        permission_class: "session_observation".into(),
                        view,
                    };
                    let _ = local_context.execute(
                        person_id,
                        LocalContextCommand::CompleteAttentionAcquisition {
                            host_epoch: host_epoch.clone(),
                            result: Box::new(completion),
                        },
                        None,
                    );
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }

    struct PositiveFakeModel;

    #[tokio::test]
    async fn bounded_history_completes_without_truncating_the_encrypted_transcript() {
        struct RecordingModel(Mutex<Vec<ModelRequest>>);

        impl ModelRunner for RecordingModel {
            fn placement(&self) -> ModelPlacement {
                ModelPlacement::DeviceLocal
            }

            async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
                self.0.lock().unwrap().push(request);
                Ok(ModelResponse {
                    replay: None,
                    schema_version: AGENT_VERSION,
                    output: vec![ModelStep::Answer {
                        text: "Hello".into(),
                    }],
                    used_tokens: 1,
                    cost_micros: 0,
                })
            }
        }

        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let store = vault.governed_general_store(session.id);
        let old_turns = [Uuid::new_v4(), Uuid::new_v4()];
        for turn_id in old_turns {
            session.messages.push(AgentMessage::User {
                turn_id,
                text: "old input".repeat(800),
            });
            session.messages.push(AgentMessage::Assistant {
                turn_id,
                text: "old answer".repeat(800),
            });
        }
        session.revision = 1;
        store.compare_and_swap(&session, 0).await.unwrap();
        let saved_messages = session.messages.clone();
        let local_context = LocalContextHost::default();
        let resolver = personal_grants::PersonalDependencyResolver {
            vault: &vault,
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let model = RecordingModel(Mutex::new(Vec::new()));
        let runner = GovernedModel {
            model: &model,
            store: &store,
            resolver: &resolver,
        };
        let policy = policy(&Model::Foundation(FoundationModelRunner::encrypted()), None);
        let budget = AgentBudget {
            max_context_bytes: 4096,
            ..AgentBudget::default()
        };
        let completed = AgentRuntime {
            store: &store,
            model: &runner,
            capabilities: &NoCapabilities,
            policy: &policy,
            budget,
        }
        .run_turn(
            AgentCommand {
                schema_version: AGENT_VERSION,
                person_id,
                session_id: session.id,
                expected_revision: session.revision,
                text: "Hello".into(),
            },
            AgentContext {
                projection_version: 1,
                persona: None,
                optional_context_issues: vec![],
                memories: vec![],
                evidence: vec![],
            },
            Cancellation::default(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            completed.last_outcome,
            Some(floe_conversation::AgentOutcome::Completed)
        );
        assert_eq!(
            &completed.messages[..saved_messages.len()],
            saved_messages.as_slice()
        );
        assert_eq!(vault.load(person_id, session.id).await.unwrap(), completed);
        let requests = model.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0]
                .messages
                .iter()
                .all(|message| !old_turns.contains(&message.turn_id()))
        );
        assert!(
            serde_json::to_vec(&requests[0].messages).unwrap().len() <= budget.max_context_bytes
        );
    }

    struct UnavailableMemoryReader(AgentFailure);

    impl floe_knowledge::MemoryContextReader for UnavailableMemoryReader {
        async fn read_memory_context(
            &self,
            _: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<floe_knowledge::ContextMemory>, AgentFailure> {
            Err(self.0)
        }
    }

    struct OptionalSourceModel {
        requests: Mutex<Vec<ModelRequest>>,
        source: floe_agent_contract::ContextSource,
    }

    impl ModelRunner for OptionalSourceModel {
        fn placement(&self) -> ModelPlacement {
            ModelPlacement::DeviceLocal
        }

        async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
            assert!(request.context.memories.is_empty());
            assert_eq!(request.context.optional_context_issues.len(), 1);
            assert_eq!(
                request.context.optional_context_issues[0].source,
                self.source
            );
            let asks_memory = request.messages.iter().any(|message| {
                matches!(
                    message,
                    AgentMessage::User { text, .. } if text == "What do you remember about me?"
                )
            });
            self.requests.lock().unwrap().push(request);
            Ok(ModelResponse {
                replay: None,
                schema_version: AGENT_VERSION,
                output: vec![ModelStep::Answer {
                    text: if asks_memory {
                        "Saved memory is unavailable; I cannot inspect it right now."
                    } else {
                        "Hello! How can I help?"
                    }
                    .into(),
                }],
                used_tokens: 1,
                cost_micros: 0,
            })
        }
    }

    #[tokio::test]
    async fn optional_memory_failure_preserves_conversation_and_integrity_fence() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let local_context = LocalContextHost::default();
        let liveness = personal_grants::PersonalDependencyLiveness {
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let policy = policy(&Model::Foundation(FoundationModelRunner::encrypted()), None);
        let model = OptionalSourceModel {
            requests: Mutex::new(Vec::new()),
            source: floe_agent_contract::ContextSource::Memory,
        };
        for failure in [
            AgentFailure::CapabilityUnavailable,
            AgentFailure::CapabilityDenied,
            AgentFailure::BudgetExceeded,
        ] {
            for text in ["Hello", "What do you remember about me?"] {
                let session = vault.create_session().await.unwrap();
                let context = conversation_context(&UnavailableMemoryReader(failure))
                    .await
                    .unwrap();
                let store = vault.governed_general_store_with_liveness(session.id, &liveness);
                let completed = AgentRuntime {
                    store: &store,
                    model: &model,
                    capabilities: &NoCapabilities,
                    policy: &policy,
                    budget: AgentBudget::default(),
                }
                .run_turn(
                    AgentCommand {
                        schema_version: AGENT_VERSION,
                        person_id,
                        session_id: session.id,
                        expected_revision: 0,
                        text: text.into(),
                    },
                    context,
                    Cancellation::default(),
                    |_| {},
                )
                .await
                .unwrap();
                assert_eq!(
                    completed.last_outcome,
                    Some(floe_conversation::AgentOutcome::Completed)
                );
                assert_eq!(vault.load(person_id, session.id).await.unwrap(), completed);
                assert!(completed.messages.iter().any(|message| matches!(
                    message,
                    AgentMessage::Assistant { text: answer, .. }
                        if if text == "Hello" { answer.contains("Hello") }
                        else { answer.contains("unavailable") }
                )));
            }
        }
        assert_eq!(model.requests.lock().unwrap().len(), 6);
        for failure in [
            AgentFailure::VaultUnavailable,
            AgentFailure::StorageUnavailable,
            AgentFailure::PolicyDenied,
            AgentFailure::Cancelled,
        ] {
            assert_eq!(
                conversation_context(&UnavailableMemoryReader(failure)).await,
                Err(failure)
            );
        }
        assert_eq!(model.requests.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn over_budget_optional_tasks_preserve_the_general_conversation() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let core = FloeCore::open(root.path().join("day.db")).await.unwrap();
        for index in 0..17 {
            core.create_task(
                person_id,
                format!("Task {index}"),
                None,
                floe_day::Priority::Normal,
                chrono::Utc::now(),
            )
            .await
            .unwrap();
        }
        let vault_root = root.path().join("vault");
        fs::create_dir(&vault_root).unwrap();
        fs::set_permissions(&vault_root, fs::Permissions::from_mode(0o700)).unwrap();
        let vault =
            EncryptedAgentVault::create(&vault_root, person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let session = vault.create_session().await.unwrap();
        let mut context = conversation_context(&vault).await.unwrap();
        let views = optional_task_views(&core, person_id, &mut context)
            .await
            .unwrap();
        assert!(views.is_empty());
        assert_eq!(
            context.optional_context_issues,
            vec![floe_agent_contract::ContextIssue {
                source: floe_agent_contract::ContextSource::Tasks,
                reason: floe_agent_contract::ContextIssueReason::BudgetExceeded,
            }]
        );
        let model = OptionalSourceModel {
            requests: Mutex::new(vec![]),
            source: floe_agent_contract::ContextSource::Tasks,
        };
        let policy = policy(&Model::Foundation(FoundationModelRunner::encrypted()), None);
        let store = vault.governed_general_store(session.id);
        let completed = AgentRuntime {
            store: &store,
            model: &model,
            capabilities: &NoCapabilities,
            policy: &policy,
            budget: AgentBudget::default(),
        }
        .run_turn(
            AgentCommand {
                schema_version: AGENT_VERSION,
                person_id,
                session_id: session.id,
                expected_revision: 0,
                text: "Hello".into(),
            },
            context,
            Cancellation::default(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            completed.last_outcome,
            Some(floe_conversation::AgentOutcome::Completed)
        );
        assert_eq!(vault.load(person_id, session.id).await.unwrap(), completed);
        assert_eq!(model.requests.lock().unwrap().len(), 1);
    }

    impl ModelRunner for PositiveFakeModel {
        fn placement(&self) -> ModelPlacement {
            ModelPlacement::DeviceLocal
        }

        async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
            Ok(ModelResponse {
                replay: None,
                schema_version: 1,
                output: vec![ModelStep::Answer {
                    text: "Attention response".into(),
                }],
                used_tokens: 1,
                cost_micros: 0,
            })
        }
    }

    struct DegradedFakeModel {
        requests: Arc<Mutex<Vec<ModelRequest>>>,
    }

    impl ModelRunner for DegradedFakeModel {
        fn placement(&self) -> ModelPlacement {
            ModelPlacement::DeviceLocal
        }

        async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
            let first = self.requests.lock().unwrap().is_empty();
            self.requests.lock().unwrap().push(request);
            Ok(ModelResponse {
                replay: None,
                schema_version: AGENT_VERSION,
                output: if first {
                    vec![ModelStep::Call {
                        capability_id: "schedule.feasibility.read".into(),
                        input: "{}".into(),
                    }]
                } else {
                    vec![ModelStep::Answer {
                        text: "The schedule source is unavailable, so I cannot assess feasibility."
                            .into(),
                    }]
                },
                used_tokens: 1,
                cost_micros: 0,
            })
        }
    }

    struct FailingFeasibilityReader;

    impl PersonalFeasibilityReaderApi for FailingFeasibilityReader {
        fn read<'a>(
            &'a self,
            _: PersonId,
            _: &'a str,
            _: Uuid,
            _: tokio::time::Instant,
            _: &'a Cancellation,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            (FeasibilityView, floe_context_contract::ContextDependency),
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(AgentFailure::AccessReviewRequired) })
        }
    }

    const COMMITMENTS_AGENT_ID: &str = BuiltinExpertKind::Commitments.package_id();
    const COMMUNICATION_AGENT_ID: &str = BuiltinExpertKind::Communication.package_id();
    const WORK_CONTEXT_AGENT_ID: &str = BuiltinExpertKind::WorkContext.package_id();
    const LIFE_LOGISTICS_AGENT_ID: &str = BuiltinExpertKind::LifeLogistics.package_id();
    const RELATIONSHIPS_AGENT_ID: &str = BuiltinExpertKind::Relationships.package_id();
    const FOCUS_AGENT_ID: &str = BuiltinExpertKind::FocusAttention.package_id();
    const WELLBEING_AGENT_ID: &str = BuiltinExpertKind::Wellbeing.package_id();

    #[tokio::test]
    async fn governed_conversation_degrades_after_feasibility_read_failure() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let session = vault.create_session().await.unwrap();
        let local_context = LocalContextHost::default();
        let model = Model::Foundation(FoundationModelRunner::encrypted());
        let policy = policy(&model, None);
        let liveness = personal_grants::PersonalDependencyLiveness {
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let store = vault.governed_general_store_with_liveness(session.id, &liveness);
        let resolver = personal_grants::PersonalDependencyResolver {
            vault: &vault,
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let governed_model = DegradedFakeModel {
            requests: Arc::new(Mutex::new(vec![])),
        };
        let governed_runner = GovernedModel {
            model: &governed_model,
            store: &store,
            resolver: &resolver,
        };
        let feasibility_reader = FailingFeasibilityReader;
        let recorder = StoreResultRecorder { store: &store };
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: Some(&feasibility_reader),
            wellbeing_reader: None,
            recorder: Some(&recorder),
            remote_reader: None,
        };
        let runtime = AgentRuntime {
            store: &store,
            model: &governed_runner,
            capabilities: &capabilities,
            policy: &policy,
            budget: AgentBudget::default(),
        };
        let command = AgentCommand {
            schema_version: AGENT_VERSION,
            person_id,
            session_id: session.id,
            expected_revision: 0,
            text: "Can I fit this into my schedule?".into(),
        };

        let completed = runtime
            .run_turn(
                command,
                AgentContext {
                    projection_version: 1,
                    persona: None,
                    optional_context_issues: vec![],
                    memories: vec![],
                    evidence: vec![],
                },
                Cancellation::default(),
                |_| {},
            )
            .await
            .unwrap();

        assert_eq!(
            completed.last_outcome,
            Some(floe_conversation::AgentOutcome::Completed)
        );
        assert!(completed.messages.iter().any(|message| matches!(
            message,
            AgentMessage::Capability {
                capability_id,
                result: Err(AgentFailure::AccessReviewRequired),
                ..
            } if capability_id == "schedule.feasibility.read"
        )));
        assert!(!completed.messages.iter().any(|message| matches!(
            message,
            AgentMessage::Capability {
                result: Err(AgentFailure::PolicyDenied),
                ..
            }
        )));
        assert!(
            completed
                .messages
                .iter()
                .any(|message| matches!(message, AgentMessage::Assistant { .. }))
        );
        let requests = governed_model.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let (_, current_turn) = requests[1].model_conversation();
        assert!(current_turn.iter().any(|message| {
            message["role"] == "tool"
                && message["status"] == "error"
                && message["failure"] == "access_review_required"
                && message.get("content").is_none()
        }));
    }

    fn test_expert_cards() -> Vec<AgentCard> {
        [
            (COMMITMENTS_AGENT_ID, "Commitments Expert"),
            (COMMUNICATION_AGENT_ID, "Communication Expert"),
            (WORK_CONTEXT_AGENT_ID, "Work Context Expert"),
            (LIFE_LOGISTICS_AGENT_ID, "Life Logistics Expert"),
            (RELATIONSHIPS_AGENT_ID, "Relationships Expert"),
            (FOCUS_AGENT_ID, "Focus & Attention Expert"),
            (WELLBEING_AGENT_ID, "Wellbeing Expert"),
        ]
        .into_iter()
        .map(|(id, name)| AgentCard {
            schema_version: AGENT_VERSION,
            protocol_version: floe_experts::A2A_PROTOCOL_VERSION.into(),
            id: id.into(),
            version: "1.0.0".into(),
            name: name.into(),
            description: format!("Bounded {name} fixture."),
            domain_tags: vec!["test".into()],
            skills: vec!["Read bounded context".into()],
        })
        .collect()
    }

    struct FixtureScheduleRunner {
        calls: AtomicUsize,
    }

    impl expert_dispatch::ExpertTaskRunner for FixtureScheduleRunner {
        fn run<'a>(
            &'a self,
            request: A2ASendMessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<A2ATask, AgentFailure>> + Send + 'a>> {
            self.calls.fetch_add(1, Ordering::AcqRel);
            Box::pin(async move {
                Ok(A2ATask {
                    id: request.message.task_id.unwrap(),
                    context_id: request.message.context_id,
                    agent_id: request.agent_id,
                    state: A2ATaskState::Completed,
                    history: vec![request.message],
                    artifacts: vec![],
                    failure: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn schedule_delegation_uses_registered_task_runner() {
        let model = Model::new(Some(RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://127.0.0.1:1".into(),
                bearer_token: "test_token_that_is_long_enough_to_validate".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let person_id = PersonId::new();
        let runner = FixtureScheduleRunner {
            calls: AtomicUsize::new(0),
        };
        let experts = ConversationExperts {
            model: &model,
            source_client: None,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            wellbeing_reader: None,
            recorder: None,
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: floe_experts::A2A_PROTOCOL_VERSION.into(),
                id: BuiltinExpertKind::Schedule.package_id().into(),
                version: "1.0.0".into(),
                name: "Schedule Expert".into(),
                description: "Reviews an authorized calendar view".into(),
                domain_tags: vec!["schedule".into()],
                skills: vec!["Review a calendar assignment".into()],
                supported_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
            }],
            grants: Default::default(),
            task_runners: &[(
                floe_experts_builtin::BuiltinExpertKind::Schedule.package_id(),
                &runner,
            )],
        };
        let task_id = Uuid::new_v4();
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                session_id: Uuid::new_v4(),
                parent_turn_id: Uuid::new_v4(),
                agent_id: BuiltinExpertKind::Schedule.package_id().into(),
                message: floe_experts::A2AMessage {
                    message_id: Uuid::new_v4(),
                    context_id: Uuid::new_v4(),
                    task_id: Some(task_id),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Review my calendar".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: Cancellation::default(),
            })
            .await
            .unwrap();

        assert_eq!(task.id, task_id);
        assert_eq!(task.state, A2ATaskState::Completed);
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);
    }

    fn test_builtin_setup(
        person_id: PersonId,
        source: BuiltinContextSource,
    ) -> BuiltinExpertSetupReceipt {
        let instance_id = uuid::Uuid::new_v4();
        let mut registry = floe_experts::AgentRegistry::new(instance_id);
        registry
            .install_builtin_experts(
                person_id,
                &floe_experts::BuiltinExpertSetup {
                    instance_id,
                    expected_revision: 0,
                    setup_id: uuid::Uuid::new_v4(),
                    sources: vec![floe_experts::BuiltinSourceBinding {
                        source,
                        view_handle: uuid::Uuid::new_v4(),
                        state: floe_experts::BuiltinSourceState::Available,
                    }],
                },
            )
            .unwrap()
    }

    async fn request(mut socket: tokio::net::TcpStream) -> (String, tokio::net::TcpStream) {
        let mut bytes = Vec::new();
        let length = loop {
            let mut chunk = [0_u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            bytes.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some(header_end) = text.find("\r\n\r\n") {
                let content_length = text[..header_end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    break header_end + 4 + content_length;
                }
            }
        };
        (String::from_utf8(bytes[..length].to_vec()).unwrap(), socket)
    }

    fn assert_calendar_request_contract(request: &str) {
        assert!(request.starts_with("POST /v1/views/calendar.timeline "));
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").expect("HTTP request body").1)
                .unwrap();
        assert_eq!(body["schema_version"], 1);
        assert_eq!(body["connector_id"], "calendar.google");
        assert_eq!(
            body["connection_id"],
            "00000000-0000-4000-8000-000000000010"
        );
        assert_eq!(body["connection_revision"], 7);
        assert!(body["range_start_unix_ms"].as_i64().unwrap() >= 0);
        assert!(
            body["range_end_unix_ms"].as_i64().unwrap()
                > body["range_start_unix_ms"].as_i64().unwrap()
        );
        assert_eq!(body["cursor"], "");
        assert_eq!(body["limit"], floe_context::MAX_CALENDAR_CONTEXT_ITEMS);
        assert_eq!(body.as_object().unwrap().len(), 8);
    }

    fn test_calendar_connections() -> Vec<floe_connections::CalendarConnectionRef> {
        vec![floe_connections::CalendarConnectionRef {
            connector_id: "calendar.google".into(),
            connection_id: "00000000-0000-4000-8000-000000000010".into(),
            connection_revision: 7,
        }]
    }

    async fn respond(mut socket: tokio::net::TcpStream, body: String) {
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }

    async fn respond_not_found(mut socket: tokio::net::TcpStream) {
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    }

    #[test]
    fn configured_daily_route_takes_priority_over_the_device_model() {
        let model = Model::new(Some(RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://127.0.0.1:8431".into(),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        }))
        .unwrap();
        assert!(matches!(model, Model::Server(_)));
    }

    #[test]
    fn missing_daily_route_uses_the_device_model() {
        assert!(matches!(Model::new(None).unwrap(), Model::Foundation(_)));
    }

    #[tokio::test]
    async fn device_model_has_no_implicit_cross_device_personal_view_route() {
        let model = Model::new(None).unwrap();
        let policy = policy(&model, None);
        let result = PersonalViewSource {
            model: &model,
            source_client: None,
            policy: &policy,
            person_id: PersonId::new(),
            people_reader: None,
            feasibility_reader: None,
            wellbeing_reader: None,
            remote_reader: None,
            recorder: None,
            dependency_turn_id: Uuid::nil(),
            dependency_result_id: Uuid::nil(),
            consumer_name: "assistant",
        }
        .attention_view(
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            &floe_execution::Cancellation::default(),
        )
        .await;
        assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
    }

    #[tokio::test]
    async fn encrypted_attention_admission_model_and_final_cas_are_fenced() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = AttentionTestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys)
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let local_context = Arc::new(LocalContextHost::default());
        let host_epoch = "attention-test-host".to_owned();
        local_context
            .request(
                person_id,
                LocalContextOperationDto::RegisterAttentionHost {
                    host_epoch: host_epoch.clone(),
                },
            )
            .unwrap();
        let subject = "a".repeat(64);
        let stop = Arc::new(AtomicBool::new(false));
        let inspect_count = Arc::new(AtomicUsize::new(0));
        let read_count = Arc::new(AtomicUsize::new(0));
        let driver = tokio::spawn(drive_attention_host(
            local_context.clone(),
            person_id,
            host_epoch,
            subject.clone(),
            stop.clone(),
            inspect_count.clone(),
            read_count.clone(),
        ));

        let inspected = floe_access::apply_personal_access(
            &vault,
            &personal_grants::native_driver(&local_context),
            person_id,
            floe_access::PersonalAccessConfiguration {
                connector: floe_access::ATTENTION_CONNECTOR.into(),
                device_id: "test-device".into(),
                change: floe_access::PersonalAccessChange::Inspect,
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
        assert_eq!(inspected.native_subject_fingerprint, Some(subject.clone()));
        let reviewed = floe_access::apply_personal_access(
            &vault,
            &personal_grants::native_driver(&local_context),
            person_id,
            floe_access::PersonalAccessConfiguration {
                connector: floe_access::ATTENTION_CONNECTOR.into(),
                device_id: "test-device".into(),
                change: floe_access::PersonalAccessChange::Review {
                    expected_native_subject_fingerprint: subject.clone(),
                    consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
                    feasibility_query: None,
                    expected_grant_id: None,
                    expected_grant_authority: None,
                },
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
        assert_eq!(reviewed.state, floe_access::PersonalAccessState::Active);
        assert_eq!(
            reviewed.consumers,
            vec![floe_access::ATTENTION_ASSISTANT_CONSUMER]
        );

        let model = Model::Foundation(FoundationModelRunner::encrypted());
        let policy = policy(&model, None);
        let liveness = personal_grants::PersonalDependencyLiveness {
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let store = vault.governed_general_store_with_liveness(session.id, &liveness);
        let reader = PersonalAttentionReader {
            vault: &vault,
            local_context: &local_context,
            device_id: "test-device",
        };
        let recorder = StoreResultRecorder { store: &store };
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
            attention: Some(&reader),
            people_reader: None,
            feasibility_reader: None,
            wellbeing_reader: None,
            recorder: Some(&recorder),
            remote_reader: None,
        };
        let turn_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let capability_output = capabilities
            .invoke(CapabilityInvocation {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                turn_id,
                call_id,
                session_id: session.id,
                capability_id: "attention.coarse.read".into(),
                input: "{}".into(),
                max_output_bytes: 8 * 1024,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: Cancellation::default(),
            })
            .await
            .unwrap();
        assert!(capability_output.contains("attention.coarse"));

        let mut admitted = session.clone();
        admitted.revision = 1;
        admitted.messages = vec![
            floe_conversation::AgentMessage::User {
                turn_id,
                text: "What is my attention state?".into(),
            },
            floe_conversation::AgentMessage::Capability {
                turn_id,
                call_id,
                capability_id: "attention.coarse.read".into(),
                input: "{}".into(),
                result: Ok(capability_output),
            },
        ];
        store.compare_and_swap(&admitted, 0).await.unwrap();

        let resolver = personal_grants::PersonalDependencyResolver {
            vault: &vault,
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let fake_model = PositiveFakeModel;
        let governed_model = GovernedModel {
            model: &fake_model,
            store: &store,
            resolver: &resolver,
        };
        let request = ModelRequest {
            usage: floe_conversation::turn::UsageLedger::default(),
            replay: vec![],
            schema_version: AGENT_VERSION,
            prompt: focus_expert_prompt(),
            person_id,
            session_id: session.id,
            turn_id,
            policy: policy.clone(),
            context: AgentContext {
                projection_version: 1,
                persona: None,
                optional_context_issues: vec![],
                memories: vec![],
                evidence: vec![],
            },
            messages: admitted.messages.clone(),
            capabilities: capabilities.descriptors(person_id),
            active_agents: vec![],
            remaining_tokens: 1_000,
            remaining_cost_micros: 1_000,
            max_output_bytes: 8 * 1024,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        store
            .revalidate_current_coverage(&request, &resolver)
            .await
            .unwrap();
        let response = governed_model.generate(request).await.unwrap();
        assert!(matches!(
            response.output.as_slice(),
            [ModelStep::Answer { .. }]
        ));

        let mut completed = admitted.clone();
        completed.revision = 2;
        completed
            .messages
            .push(floe_conversation::AgentMessage::Assistant {
                turn_id,
                text: "Attention response".into(),
            });
        store.compare_and_swap(&completed, 1).await.unwrap();
        assert_eq!(vault.load(person_id, session.id).await.unwrap().revision, 2);

        let grant = vault
            .list_data_access_grants(128)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        vault
            .revoke_data_access_grant(grant.id(), grant.authority())
            .await
            .unwrap();
        let mut rejected = completed.clone();
        rejected.revision = 3;
        rejected
            .messages
            .push(floe_conversation::AgentMessage::Assistant {
                turn_id,
                text: "Late attention response".into(),
            });
        assert_eq!(
            store.compare_and_swap(&rejected, 2).await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(vault.load(person_id, session.id).await.unwrap().revision, 2);
        assert!(inspect_count.load(Ordering::Acquire) >= 3);
        assert!(read_count.load(Ordering::Acquire) >= 1);

        let independent = vault.create_session().await.unwrap();
        let independent_store = vault.governed_general_store(independent.id);
        let independent_turn = Uuid::new_v4();
        let mut independent_saved = independent.clone();
        independent_saved.revision = 1;
        independent_saved.messages = vec![
            floe_conversation::AgentMessage::User {
                turn_id: independent_turn,
                text: "Hello".into(),
            },
            floe_conversation::AgentMessage::Assistant {
                turn_id: independent_turn,
                text: "Hi".into(),
            },
        ];
        independent_store
            .compare_and_swap(&independent_saved, 0)
            .await
            .unwrap();
        assert_eq!(
            vault
                .load(person_id, independent.id)
                .await
                .unwrap()
                .messages
                .len(),
            2
        );
        stop.store(true, Ordering::Release);
        driver.await.unwrap();
    }

    #[tokio::test]
    async fn remote_expert_requires_an_admitted_reader() {
        let model = Model::new(Some(RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://127.0.0.1:1".into(),
                bearer_token: "test_token_that_is_long_enough_to_validate".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let person_id = PersonId::new();
        let experts = ConversationExperts {
            model: &model,
            source_client: None,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            wellbeing_reader: None,
            recorder: None,
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            grants: floe_experts::SourceGrants::new(Some(test_builtin_setup(
                person_id,
                BuiltinContextSource::Mail,
            ))),
            task_runners: &[],
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: COMMITMENTS_AGENT_ID.into(),
                message: floe_experts::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "review commitments".into(),
                    }],
                },
                max_output_bytes: 16 * 1024,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
    }

    #[tokio::test]
    async fn device_model_reads_person_bound_local_context() {
        let model = Model::new(None).unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextHost::default();
        let person_id = PersonId::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        local_context
            .request(
                person_id,
                floe_protocol::LocalContextOperationDto::Publish {
                    device_id: "mac-local".into(),
                    view: serde_json::json!({
                        "schema_version": AGENT_VERSION,
                        "view_id": "attention.coarse",
                        "source_handle": "attention:macos_local",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 60_000,
                        "state": "focused",
                        "confidence_millis": 750,
                        "evidence_handles": ["activity:coarse"]
                    }),
                },
            )
            .unwrap();
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            wellbeing_reader: None,
            recorder: None,
            remote_reader: None,
        };

        let result = capabilities
            .invoke(CapabilityInvocation {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                call_id: uuid::Uuid::new_v4(),
                person_id,
                session_id: uuid::Uuid::new_v4(),
                turn_id: uuid::Uuid::new_v4(),
                capability_id: "attention.coarse.read".into(),
                input: "{}".into(),
                max_output_bytes: 65_536,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
    }

    #[test]
    fn server_route_exposes_only_bounded_context_observe_capabilities() {
        let model = Model::new(Some(RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://127.0.0.1:8431".into(),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextHost::default();
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            wellbeing_reader: None,
            recorder: None,
            remote_reader: None,
        };
        let descriptors = capabilities.descriptors(PersonId::new());
        assert_eq!(descriptors.len(), 7);
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            [
                "people.identity.read",
                "schedule.feasibility.read",
                "attention.coarse.read",
                "wellbeing.derived.read",
                "mail.communication.read",
                "work.context.read",
                "life.logistics.read"
            ]
        );
        for descriptor in &descriptors {
            assert!(descriptor.read_only);
            assert_eq!(descriptor.output_data_class, DataClass::Personal);
            let schema = descriptor.input_schema.as_ref().unwrap();
            assert_eq!(schema["additionalProperties"], false);
            assert!(schema.to_string().len() < 1024);
        }
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let experts = ConversationExperts {
            model: &model,
            source_client: None,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            grants: Default::default(),
            task_runners: &[],
        };
        let cards = experts.agent_cards(PersonId::new());
        assert_eq!(cards.len(), 7);
        assert_eq!(cards[0].id, COMMITMENTS_AGENT_ID);
        assert_eq!(cards[1].id, COMMUNICATION_AGENT_ID);
        assert_eq!(cards[2].id, WORK_CONTEXT_AGENT_ID);
        assert_eq!(cards[3].id, LIFE_LOGISTICS_AGENT_ID);
        assert_eq!(cards[4].id, RELATIONSHIPS_AGENT_ID);
        assert_eq!(cards[5].id, FOCUS_AGENT_ID);
        assert_eq!(cards[6].id, WELLBEING_AGENT_ID);
        assert!(cards.iter().all(|card| card.validate().is_ok()));
    }

    #[tokio::test]
    async fn unregistered_expert_card_cannot_be_invoked_directly() {
        let model = Model::new(None).unwrap();
        let policy = policy(&model, None);
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let experts = ConversationExperts {
            model: &model,
            source_client: None,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![],
            grants: Default::default(),
            task_runners: &[],
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: RELATIONSHIPS_AGENT_ID.into(),
                message: floe_experts::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Find a follow-up.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityDenied));
    }

    #[tokio::test]
    async fn mail_capability_rejects_authority_escalation_before_io() {
        let model = Model::new(Some(RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://127.0.0.1:1".into(),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextHost::default();
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
        };
        let result = capabilities
            .invoke(CapabilityInvocation {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                call_id: uuid::Uuid::new_v4(),
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                turn_id: uuid::Uuid::new_v4(),
                capability_id: "mail.communication.read".into(),
                input: r#"{"query":"reply","authority":"send"}"#.into(),
                max_output_bytes: 65_536,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::InvalidInput));
    }

    #[tokio::test]
    async fn commitments_delegation_reads_fresh_view_and_returns_typed_artifact() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (view_request, socket) = request(socket).await;
            assert!(view_request.starts_with("POST /v1/views/mail.communication "));
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "mail.communication",
                        "source_handle": "mail:fresh",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 299_999,
                        "coverage_complete": true,
                        "items": [{
                            "evidence_handle": "mail:request",
                            "thread_handle": "mail:thread",
                            "received_unix_ms": now - 2,
                            "from": "alex@example.com",
                            "to": "person@example.com",
                            "subject": "Confirm by Friday",
                            "snippet": "Please confirm the review by Friday.",
                            "labels": ["INBOX"]
                        }]
                    }
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (calendar_request, socket) = request(socket).await;
            assert_calendar_request_contract(&calendar_request);
            respond_not_found(socket).await;

            let (socket, _) = listener.accept().await.unwrap();
            let (model_request, socket) = request(socket).await;
            assert!(model_request.starts_with("POST /v1/agent "));
            assert!(model_request.contains("Commitments Expert"));
            assert!(model_request.contains("Confirm by Friday"));
            let answer = serde_json::json!({
                "summary": "A reply and Friday commitment are requested.",
                "findings": [{
                    "evidence_handle": "mail:request",
                    "kind": "request_to_user",
                    "statement": "Confirm the review by Friday.",
                    "epistemic_status": "observed",
                    "confidence_millis": 1000
                }]
            })
            .to_string();
            let output = serde_json::json!({
                "output": [{"kind": "answer", "text": answer}],
                "used_tokens": 64,
                "call_ids": []
            })
            .to_string();
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "purpose": "everyday_assistance",
                    "output": output,
                    "trace_id": "0123456789abcdef0123456789abcdef",
                    "routing": {
                        "placement": "server_local",
                        "external_transfer": false,
                        "replay_source": "a".repeat(64)
                    }
                })
                .to_string(),
            )
            .await;
        });
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: format!("http://{address}"),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let source_client =
            ServerSourceClient::new(route.route.clone(), route.calendar_connections.clone())
                .unwrap();
        let person_id = PersonId::new();
        let remote_reader = match &model {
            Model::Server(_) => FixtureRemoteReader {
                source_client: &source_client,
                person_id,
            },
            Model::Foundation(_) => unreachable!(),
        };
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let recorder = FixtureResultRecorder;
        let experts = ConversationExperts {
            model: &model,
            source_client: Some(&source_client),
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: Some(&recorder),
            remote_reader: Some(&remote_reader),
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            grants: Default::default(),
            task_runners: &[],
        };
        let task_id = uuid::Uuid::new_v4();
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: COMMITMENTS_AGENT_ID.into(),
                message: floe_experts::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(task_id),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Check my latest mail for commitments.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await
            .unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(task.state, A2ATaskState::Completed);
        let data = task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap();
        let result: CommitmentsExpertResult = serde_json::from_str(data).unwrap();
        assert_eq!(result.findings.len(), 1);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn commitments_artifact_preserves_mail_calendar_task_and_memory_provenance() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let person_id = PersonId::new();
        let task_id = uuid::Uuid::new_v4();
        let memory_id = uuid::Uuid::new_v4();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (mail_request, socket) = request(socket).await;
            assert!(mail_request.starts_with("POST /v1/views/mail.communication "));
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "mail.communication",
                        "source_handle": "mail:selected",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 299_999,
                        "coverage_complete": true,
                        "items": [{
                            "evidence_handle": "mail:request",
                            "thread_handle": "mail:thread",
                            "received_unix_ms": now - 2,
                            "from": "alex@example.com",
                            "to": "person@example.com",
                            "subject": "Delivery review",
                            "snippet": "Please confirm the review.",
                            "labels": ["INBOX"]
                        }]
                    }
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (calendar_request, socket) = request(socket).await;
            assert_calendar_request_contract(&calendar_request);
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "calendar.timeline",
                        "source_handle": "calendar:selected",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 240_000,
                        "range_start_unix_ms": now - 86_400_000,
                        "range_end_unix_ms": now + 86_400_000,
                        "coverage_complete": true,
                        "items": [{
                            "evidence_handle": "calendar:review",
                            "untrusted_title": "Delivery review",
                            "starts_at_unix_ms": now + 10_000,
                            "ends_at_unix_ms": now + 20_000,
                            "all_day": false
                        }]
                    }
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (model_request, socket) = request(socket).await;
            assert!(model_request.starts_with("POST /v1/agent "));
            assert!(model_request.contains("mail:request"));
            assert!(model_request.contains("calendar:review"));
            assert!(model_request.contains(&task_id.to_string()));
            assert!(model_request.contains(&memory_id.to_string()));
            let answer = serde_json::json!({
                "summary": "Four bounded sources support the delivery commitment.",
                "findings": [
                    {"evidence_handle":"mail:request","kind":"request_to_user","statement":"A reply was requested.","epistemic_status":"observed","confidence_millis":1000},
                    {"evidence_handle":"calendar:review","kind":"user_commitment","statement":"A review is scheduled.","epistemic_status":"observed","confidence_millis":1000},
                    {"evidence_handle":task_id.to_string(),"kind":"user_commitment","statement":"A task remains open.","epistemic_status":"observed","confidence_millis":1000},
                    {"evidence_handle":memory_id.to_string(),"kind":"user_commitment","statement":"The delivery was confirmed.","epistemic_status":"observed","confidence_millis":1000}
                ]
            });
            let output = serde_json::json!({
                "output": [{"kind": "answer", "text": answer.to_string()}],
                "used_tokens": 64,
                "call_ids": []
            })
            .to_string();
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "purpose": "everyday_assistance",
                    "output": output,
                    "trace_id": "0123456789abcdef0123456789abcdef",
                    "routing": {
                        "placement": "server_local",
                        "external_transfer": false,
                        "replay_source": "a".repeat(64)
                    }
                })
                .to_string(),
            )
            .await;
        });
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: format!("http://{address}"),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let source_client =
            ServerSourceClient::new(route.route.clone(), route.calendar_connections.clone())
                .unwrap();
        let policy = policy(&model, Some(&route));
        let remote_reader = match &model {
            Model::Server(_) => FixtureRemoteReader {
                source_client: &source_client,
                person_id,
            },
            Model::Foundation(_) => unreachable!(),
        };
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![floe_knowledge::ContextMemory {
                target_id: memory_id,
                revision: 2,
                kind: floe_knowledge::PersonalMemoryKind::Commitment,
                statement: "The user confirmed the delivery.".into(),
                epistemic_status: floe_knowledge::EpistemicStatus::Fact,
                confidence_millis: 1000,
                observed_at_unix_ms: now - 5_000,
                valid_from_unix_ms: None,
                valid_until_unix_ms: Some(now + 180_000),
                source_refs: vec![floe_knowledge::LearningEvidenceRef {
                    session_id: uuid::Uuid::new_v4(),
                    turn_id: uuid::Uuid::new_v4(),
                }],
            }],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let recorder = FixtureResultRecorder;
        let task_handle = uuid::Uuid::new_v5(&person_id.0, b"floe.tasks");
        let tasks = [NativeContextView {
            schema_version: AGENT_VERSION,
            handle: task_handle,
            person_id,
            view_id: "floe.tasks".into(),
            data_class: DataClass::Personal,
            source_handle: format!("floe.tasks:{task_handle}"),
            observed_at_unix_ms: now as u64,
            expires_at_unix_ms: (now + 220_000) as u64,
            coverage_complete: true,
            next_cursor: None,
            items: vec![floe_context::NativeContextItem::Task {
                evidence_handle: task_id,
                untrusted_title: "Prepare delivery".into(),
                deadline_unix_ms: Some((now + 30_000) as u64),
                priority: floe_context::TaskContextPriority::High,
            }],
        }];
        let experts = ConversationExperts {
            model: &model,
            source_client: Some(&source_client),
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: Some(&recorder),
            remote_reader: Some(&remote_reader),
            wellbeing_reader: None,
            context_reader: None,
            task_views: &tasks,
            cards: test_expert_cards(),
            grants: Default::default(),
            task_runners: &[],
        };
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: COMMITMENTS_AGENT_ID.into(),
                message: floe_experts::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Assess all commitment sources.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await
            .unwrap();
        let result: CommitmentsExpertResult =
            serde_json::from_str(task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap()).unwrap();
        assert_eq!(result.findings.len(), 4);
        assert_eq!(result.expires_at_unix_ms, now + 180_000);
        assert!(result.source_handles.contains(&"mail:selected".into()));
        assert!(result.source_handles.contains(&"calendar:selected".into()));
        assert!(
            result
                .source_handles
                .contains(&format!("floe.tasks:{task_handle}"))
        );
        assert!(
            result
                .source_handles
                .contains(&format!("memory:{memory_id}:2"))
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn portfolio_delegations_read_fresh_views_and_return_typed_artifacts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let cases = [
            (
                "/v1/views/work.context",
                "Work Context Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "work.context",
                    "source_handle": "github:fresh",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "scope_handle": "workspace:selected",
                    "items": [{
                        "evidence_handle": "github:issue",
                        "kind": "project",
                        "title": "Release readiness",
                        "status": "open",
                        "blocker": "Missing validation",
                        "observed_at_unix_ms": now - 2
                    }]
                }),
                serde_json::json!({
                    "summary": "The release is blocked on validation.",
                    "insights": [{
                        "evidence_handle": "github:issue",
                        "blocker": "Missing validation",
                        "next_action": "Attach validation evidence.",
                        "confidence_millis": 1000
                    }]
                }),
            ),
            (
                "/v1/views/life.logistics",
                "Life Logistics Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "life.logistics",
                    "source_handle": "home:fresh",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "items": [{
                        "evidence_handle": "home:sensor",
                        "kind": "home_state",
                        "summary": "Window sensor",
                        "status": "open",
                        "needs_attention": true
                    }]
                }),
                serde_json::json!({
                    "summary": "The selected window needs attention.",
                    "preparations": [{
                        "evidence_handle": "home:sensor",
                        "recommendation": "Check the window before leaving.",
                        "urgency": "soon",
                        "requires_approval": false
                    }]
                }),
            ),
        ];
        let server = tokio::spawn(async move {
            for (path, role, view, answer) in cases {
                let (socket, _) = listener.accept().await.unwrap();
                let (view_request, socket) = request(socket).await;
                assert!(view_request.starts_with(&format!("POST {path} ")));
                respond(
                    socket,
                    serde_json::json!({"schema_version": 1, "view": view}).to_string(),
                )
                .await;

                let (socket, _) = listener.accept().await.unwrap();
                let (model_request, socket) = request(socket).await;
                assert!(model_request.starts_with("POST /v1/agent "));
                assert!(model_request.contains(role));
                let output = serde_json::json!({
                    "output": [{"kind": "answer", "text": answer.to_string()}],
                    "used_tokens": 64,
                    "call_ids": []
                })
                .to_string();
                respond(
                    socket,
                    serde_json::json!({
                        "schema_version": 1,
                        "purpose": "everyday_assistance",
                        "output": output,
                        "trace_id": "0123456789abcdef0123456789abcdef",
                        "routing": {
                            "placement": "server_local",
                            "external_transfer": false,
                            "replay_source": "a".repeat(64)
                        }
                    })
                    .to_string(),
                )
                .await;
            }
        });
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: format!("http://{address}"),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let source_client =
            ServerSourceClient::new(route.route.clone(), route.calendar_connections.clone())
                .unwrap();
        let person_id = PersonId::new();
        let policy = policy(&model, Some(&route));
        let remote_reader = match &model {
            Model::Server(_) => FixtureRemoteReader {
                source_client: &source_client,
                person_id,
            },
            Model::Foundation(_) => unreachable!(),
        };
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let recorder = FixtureResultRecorder;
        let experts = ConversationExperts {
            model: &model,
            source_client: Some(&source_client),
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: Some(&recorder),
            remote_reader: Some(&remote_reader),
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            grants: Default::default(),
            task_runners: &[],
        };
        let mut results = Vec::new();
        for agent_id in [WORK_CONTEXT_AGENT_ID, LIFE_LOGISTICS_AGENT_ID] {
            let task = experts
                .handle_message(A2ASendMessageRequest {
                    usage: floe_conversation::turn::UsageLedger::default(),
                    schema_version: AGENT_VERSION,
                    person_id,
                    session_id: uuid::Uuid::new_v4(),
                    parent_turn_id: uuid::Uuid::new_v4(),
                    agent_id: agent_id.into(),
                    message: floe_experts::A2AMessage {
                        message_id: uuid::Uuid::new_v4(),
                        context_id: uuid::Uuid::new_v4(),
                        task_id: Some(uuid::Uuid::new_v4()),
                        role: A2AMessageRole::User,
                        parts: vec![A2APart::Text {
                            text: "Review the selected context.".into(),
                        }],
                    },
                    max_output_bytes: 16_384,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                    cancellation: floe_execution::Cancellation::default(),
                })
                .await
                .unwrap();
            results.push(task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap().to_owned());
        }
        let work: WorkContextExpertResult = serde_json::from_str(&results[0]).unwrap();
        let logistics: LifeLogisticsExpertResult = serde_json::from_str(&results[1]).unwrap();
        assert_eq!(work.scope_handle, "workspace:selected");
        assert_eq!(work.insights[0].evidence_handle, "github:issue");
        assert_eq!(logistics.source_handle, "home:fresh");
        assert_eq!(logistics.preparations[0].evidence_handle, "home:sensor");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn personal_delegations_use_bounded_views_and_capability_free_experts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let cases = [
            (
                RELATIONSHIPS_AGENT_ID,
                "/v1/views/people.identity",
                "Relationships Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "people.identity",
                    "source_handle": "contacts:local",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "identities": [{
                        "identity_handle": "person:alex",
                        "display_name": "Alex",
                        "aliases": ["alex@example.com"],
                        "confidence_millis": 1000,
                        "evidence_handles": ["contact:alex"]
                    }]
                }),
                serde_json::json!({
                    "summary": "No confirmed interaction supports a follow-up.",
                    "follow_ups": []
                }),
                "contacts:local",
            ),
            (
                FOCUS_AGENT_ID,
                "/v1/views/attention.coarse",
                "Focus & Attention Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "attention.coarse",
                    "source_handle": "attention:mac-local",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 119_999,
                    "state": "focused",
                    "confidence_millis": 800,
                    "evidence_handles": ["attention:aggregate"]
                }),
                serde_json::json!({
                    "summary": "Protect the current focus period before the review.",
                    "recommendation": "protect_focus",
                    "rationale": "Attention, the upcoming review, and active release work support focus protection.",
                    "evidence_handles": ["attention:aggregate", "calendar:review", "work:release"]
                }),
                "attention:mac-local",
            ),
            (
                WELLBEING_AGENT_ID,
                "/v1/views/wellbeing.derived",
                "Wellbeing Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "wellbeing.derived",
                    "source_handle": "health:derived-local",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "capacity": "reduced",
                    "recovery": "needs_recovery",
                    "confidence_millis": 750,
                    "evidence_handles": ["health:aggregate"]
                }),
                serde_json::json!({
                    "summary": "Reduce optional load and preserve recovery time.",
                    "schedule_impact": "protect_recovery",
                    "rationale": "Derived capacity is reduced and recovery is needed.",
                    "evidence_handles": ["health:aggregate"]
                }),
                "health:derived-local",
            ),
        ];
        let server_cases = cases.clone();
        let server = tokio::spawn(async move {
            for (agent_id, path, role, view, answer, _) in server_cases {
                let (socket, _) = listener.accept().await.unwrap();
                let (view_request, socket) = request(socket).await;
                assert!(view_request.starts_with(&format!("POST {path} ")));
                assert!(view_request.contains(r#"{"schema_version":1}"#));
                respond(
                    socket,
                    serde_json::json!({"schema_version": 1, "view": view}).to_string(),
                )
                .await;

                let optional_paths: &[&str] = match agent_id {
                    RELATIONSHIPS_AGENT_ID => &["/v1/views/relationships.confirmed_interactions"],
                    FOCUS_AGENT_ID => &["/v1/views/calendar.timeline", "/v1/views/work.context"],
                    WELLBEING_AGENT_ID => &["/v1/views/calendar.timeline"],
                    _ => unreachable!(),
                };
                for path in optional_paths {
                    let (socket, _) = listener.accept().await.unwrap();
                    let (optional_request, socket) = request(socket).await;
                    assert!(optional_request.starts_with(&format!("POST {path} ")));
                    if *path == "/v1/views/calendar.timeline" {
                        assert_calendar_request_contract(&optional_request);
                    }
                    match (agent_id, *path) {
                        (FOCUS_AGENT_ID, "/v1/views/calendar.timeline") => {
                            respond(
                                socket,
                                serde_json::json!({
                                    "schema_version": 1,
                                    "view": {
                                        "schema_version": 1,
                                        "view_id": "calendar.timeline",
                                        "source_handle": "calendar:selected",
                                        "observed_at_unix_ms": now - 1,
                                        "expires_at_unix_ms": now + 240_000,
                                        "range_start_unix_ms": now - 86_400_000,
                                        "range_end_unix_ms": now + 86_400_000,
                                        "coverage_complete": true,
                                        "items": [{
                                            "evidence_handle": "calendar:review",
                                            "untrusted_title": "Release review",
                                            "starts_at_unix_ms": now + 10_000,
                                            "ends_at_unix_ms": now + 20_000,
                                            "all_day": false
                                        }]
                                    }
                                })
                                .to_string(),
                            )
                            .await;
                        }
                        (FOCUS_AGENT_ID, "/v1/views/work.context") => {
                            respond(
                                socket,
                                serde_json::json!({
                                    "schema_version": 1,
                                    "view": {
                                        "schema_version": 1,
                                        "view_id": "work.context",
                                        "source_handle": "work:selected",
                                        "observed_at_unix_ms": now - 1,
                                        "expires_at_unix_ms": now + 220_000,
                                        "coverage_complete": true,
                                        "scope_handle": "workspace:selected",
                                        "items": [{
                                            "evidence_handle": "work:release",
                                            "kind": "project",
                                            "title": "Release readiness",
                                            "status": "active",
                                            "blocker": null,
                                            "observed_at_unix_ms": now - 2
                                        }]
                                    }
                                })
                                .to_string(),
                            )
                            .await;
                        }
                        _ => respond_not_found(socket).await,
                    }
                }

                let (socket, _) = listener.accept().await.unwrap();
                let (model_request, socket) = request(socket).await;
                assert!(model_request.starts_with("POST /v1/agent "));
                assert!(model_request.contains(role));
                assert!(model_request.contains(r#""tools":[]"#));
                assert!(!model_request.contains("notification.send"));
                let output = serde_json::json!({
                    "output": [{"kind": "answer", "text": answer.to_string()}],
                    "used_tokens": 64,
                    "call_ids": []
                })
                .to_string();
                respond(
                    socket,
                    serde_json::json!({
                        "schema_version": 1,
                        "purpose": "everyday_assistance",
                        "output": output,
                        "trace_id": "0123456789abcdef0123456789abcdef",
                        "routing": {
                            "placement": "server_local",
                            "external_transfer": false,
                            "replay_source": "a".repeat(64)
                        }
                    })
                    .to_string(),
                )
                .await;
            }
        });
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: format!("http://{address}"),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let experts = ConversationExperts {
            model: &model,
            source_client: None,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            grants: Default::default(),
            task_runners: &[],
        };
        for (agent_id, _, _, _, _, source_handle) in cases {
            let result = experts
                .handle_message(A2ASendMessageRequest {
                    usage: floe_conversation::turn::UsageLedger::default(),
                    schema_version: AGENT_VERSION,
                    person_id: PersonId::new(),
                    session_id: uuid::Uuid::new_v4(),
                    parent_turn_id: uuid::Uuid::new_v4(),
                    agent_id: agent_id.into(),
                    message: floe_experts::A2AMessage {
                        message_id: uuid::Uuid::new_v4(),
                        context_id: uuid::Uuid::new_v4(),
                        task_id: Some(uuid::Uuid::new_v4()),
                        role: A2AMessageRole::User,
                        parts: vec![A2APart::Text {
                            text: "Assess only the supplied personal context.".into(),
                        }],
                    },
                    max_output_bytes: 16_384,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                    cancellation: floe_execution::Cancellation::default(),
                })
                .await;
            assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
            let _ = (agent_id, source_handle);
            break;
        }
        server.abort();
    }

    #[tokio::test]
    async fn unavailable_personal_provider_is_typed_and_never_runs_the_expert() {
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://127.0.0.1:1".into(),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextHost::default();
        let experts = ConversationExperts {
            model: &model,
            source_client: None,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            attention: None,
            people_reader: None,
            feasibility_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            grants: Default::default(),
            task_runners: &[],
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_conversation::turn::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: FOCUS_AGENT_ID.into(),
                message: floe_experts::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Assess my current attention.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
    }
}
