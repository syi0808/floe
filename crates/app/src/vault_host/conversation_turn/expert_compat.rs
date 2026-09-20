//! Staged legacy Expert model/source host compatibility.
//!
//! Temporary isolation behind `LegacyDelegationPort`, not a new architecture.
//! The root General Conversation turn never constructs these: root projection,
//! model and tools are the canonical Conversation/Context/Engine/Inference
//! owners. Only an actually delegated legacy Expert endpoint prepares its
//! legacy model selection, policy and source fallback here, from the stored
//! server credential. 2-C removes this bridge.

use std::{future::Future, pin::Pin};

use floe_agent_contract::{AgentFailure, DataClass, ModelPlacement, TransferConsent};
use floe_context::{InferencePolicyDecision, NativeContextView};
use floe_context::{AttentionView, CalendarContextView, PeopleView, WellbeingView};
use floe_conversation::{AgentMessage, ModelRequest, ModelStep};
use floe_kernel::{AGENT_VERSION, PersonId};
use floe_provider_adapters::models::{FoundationModelRunner, ServerModelRunner};
use floe_provider_adapters::sources::ServerSourceClient;
use floe_provider_adapters::sources::server::CalendarContextRequest;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use crate::FloeCore;
use crate::local_context::LocalContextHost;

use super::super::personal_grants;

pub(super) fn policy(model: &Model) -> InferencePolicyDecision {
    InferencePolicyDecision {
        // Canonical root purpose. The root ModelRequest purpose, the envelope
        // scoped purpose and the provider wire purpose must agree on this;
        // App no longer invents a different purpose per resolved route.
        purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![floe_inference::ModelTransport::placement(model)],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: external_transfer_consent(
            floe_inference::ModelTransport::placement(model),
        ),
        bounded_sensitive_projection: false,
    }
}

/// The consent a staged legacy Expert model call stands under.
///
/// No pre-resolved recipient exists anymore, so no external transfer is ever
/// consented here: the legacy Server transport runs server-local only, and
/// the prepared route asserts that to the gateway on the wire.
pub(super) fn external_transfer_consent(placement: ModelPlacement) -> TransferConsent {
    floe_inference::external_transfer_consent(placement, None)
}

pub(crate) enum Model {
    Foundation(FoundationModelRunner),
    Server(ServerModelRunner),
}

impl Model {
    /// Legacy staged-Expert model selection from the stored server
    /// credential: a stored connection means Server reasoning, absence means
    /// on-device Foundation. Pure: the candidate route is validated locally,
    /// never discovered over the network, so it can only run server-local.
    pub(super) fn for_stored_connection(
        stored: Option<floe_inference::SavedServerConnection>,
        person_id: &str,
        device_id: &str,
    ) -> Result<Self, AgentFailure> {
        match stored {
            Some(stored) => {
                let admitted =
                    floe_inference::admit_saved_connection(stored, person_id, device_id)?;
                let candidate = floe_inference::candidate_route(&admitted)?;
                ServerModelRunner::new_model_only(candidate).map(Self::Server)
            }
            None => Ok(Self::Foundation(FoundationModelRunner::encrypted())),
        }
    }

    /// The placement Experts are offered at. A server model runs
    /// server-class judgment wherever it listens; transport placement
    /// still names the data destination, which is what consent checks.
    pub(super) fn expert_eligibility(&self) -> ModelPlacement {
        match self {
            Self::Foundation(model) => floe_inference::ModelTransport::placement(model),
            Self::Server(_) => ModelPlacement::Remote,
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
                    messages: vec![AgentMessage::User {
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
            let [ModelStep::Answer { text }] = response.output.as_slice() else {
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
                        AgentMessage::User { turn_id, text }
                    }
                    floe_agent_contract::ExpertTranscriptEntry::Preamble { text } => {
                        AgentMessage::Preamble { turn_id, text }
                    }
                    floe_agent_contract::ExpertTranscriptEntry::Capability {
                        call_id,
                        capability_id,
                        input,
                        result,
                    } => AgentMessage::Capability {
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
                        ModelStep::Preamble { text } => {
                            Ok(floe_agent_contract::ExpertStep::Preamble { text })
                        }
                        ModelStep::Answer { text } => {
                            Ok(floe_agent_contract::ExpertStep::Answer { text })
                        }
                        ModelStep::Call {
                            capability_id,
                            input,
                        } => Ok(floe_agent_contract::ExpertStep::Call {
                            capability_id,
                            input,
                        }),
                        // An Expert has no one to delegate to.
                        ModelStep::Delegate { .. } => Err(AgentFailure::CapabilityDenied),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                replay: response.replay,
                used_tokens: response.used_tokens,
                cost_micros: response.cost_micros,
            })
        })
    }
}

pub(super) struct PersonalViewSource<'a> {
    pub(super) model: &'a Model,
    pub(super) source_client: Option<&'a ServerSourceClient>,
    pub(super) policy: &'a InferencePolicyDecision,
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

    pub(super) async fn work_context_views(
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
