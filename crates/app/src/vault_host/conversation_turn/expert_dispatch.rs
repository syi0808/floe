//! Binding supplied Expert registrations to product endpoints and injecting
//! the concrete readers they run against.
//!
//! No Expert's judgment lives here. The selected registration carries its
//! runner through publication; this file supplies the invocation-scoped host.

use super::expert_host::{
    CalendarContextReaderApi, CapturingRecorder, ConversationContextReader,
    ConversationContextReaderApi, ExpertModelHost, PersonalAttentionReader,
    PersonalAttentionReaderApi, PersonalPeopleReader, PersonalPeopleReaderApi, PersonalViewSource,
    PersonalWellbeingReader, PersonalWellbeingReaderApi, ResultRecorder,
    SelectedCalendarContextReader, StoreResultRecorder, expert_policy,
};
use super::*;
use std::sync::{Arc, Mutex};

use floe_agent_contract::{AgentEndpoint, BoxFuture, EndpointInvocation, ExpertReport};
use floe_experts_builtin::{
    BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, DeclaredSourceRead,
    StatefulExpertDraft,
};

mod stateful_settlement;
use stateful_settlement::{StatefulExpertSettlement, VaultStatefulExpertSettlement};

pub(crate) type SuppliedExpertRunner = for<'turn, 'model, 'msg, 'call> fn(
    &'call DelegatedMessageExperts<'turn, 'model, 'msg>,
    &'call BuiltinExpertRequest,
) -> BoxFuture<
    'call,
    Result<BuiltinExpertOutput, AgentFailure>,
>;

pub(crate) enum BoundExpertRunner {
    Shipped(floe_experts_builtin::BuiltinExpertRunner),
    Supplied(SuppliedExpertRunner),
}

impl BoundExpertRunner {
    async fn run(
        &self,
        host: &DelegatedMessageExperts<'_, '_, '_>,
        request: &BuiltinExpertRequest,
    ) -> Result<BuiltinExpertOutput, AgentFailure> {
        match self {
            Self::Shipped(kind) => kind.run(host, request).await,
            Self::Supplied(runner) => runner(host, request).await,
        }
    }
}

pub(crate) type BoundExpertRegistration = floe_experts::ExpertRegistration<BoundExpertRunner>;

pub(crate) fn shipped_registrations() -> Vec<BoundExpertRegistration> {
    floe_experts_builtin::registrations()
        .into_iter()
        .map(|registration| BoundExpertRegistration {
            manifest: registration.manifest,
            runner: BoundExpertRunner::Shipped(registration.runner),
        })
        .collect()
}

/// Raw requirement proposals are never authority: no legitimate dispatch
/// emits one, so any such artifact in an Expert output is forged.
const SOURCE_ACCESS_REQUIREMENT_MEDIA_TYPE: &str =
    "application/vnd.floe.source-access-requirement+json;version=1";

/// Reject model-produced requirement JSON outright: only host-captured
/// requirements publish, never output artifacts with this media type.
fn reject_raw_requirement_artifacts(
    artifacts: &[floe_agent_contract::Artifact],
) -> Result<(), AgentFailure> {
    for artifact in artifacts {
        for part in &artifact.parts {
            if matches!(
                part,
                floe_agent_contract::ArtifactPart::Data { media_type, .. }
                    if media_type == SOURCE_ACCESS_REQUIREMENT_MEDIA_TYPE
            ) {
                return Err(AgentFailure::InvalidModelOutput);
            }
        }
    }
    Ok(())
}

fn reject_raw_action_artifacts(
    artifacts: &[floe_agent_contract::Artifact],
) -> Result<(), AgentFailure> {
    if artifacts
        .iter()
        .flat_map(|artifact| &artifact.parts)
        .any(|part| {
            matches!(part, floe_agent_contract::ArtifactPart::Data { media_type, .. }
            if media_type == floe_actions::EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE)
        })
    {
        Err(AgentFailure::InvalidModelOutput)
    } else {
        Ok(())
    }
}

/// The endpoint the delegating Run invokes for one registered Expert.
///
/// The invocation is self-sufficient: session, device, AgentContext, and the
/// output bound arrive in its explicit execution context, and the
/// saved-connection store is injected at construction. No run-id staging
/// exists.
pub(crate) struct RegisteredExpertEndpoint<Keys> {
    core: Arc<FloeCore>,
    vault: Arc<EncryptedAgentVault<Keys>>,
    local_context: Arc<LocalContextHost>,
    connections: floe_provider_adapters::gateway::GatewayCredentialStore,
    model: Arc<dyn floe_agent_contract::ModelPort + Send + Sync>,
    actor: floe_kernel::OwnerActor,
    source_connections: Arc<floe_connections::ConnectionsService>,
    admission: floe_experts::ExpertAdmissionIdentity,
    selection: floe_experts::ExpertExecutionSelection,
    registration: Arc<BoundExpertRegistration>,
}

impl<Keys> RegisteredExpertEndpoint<Keys> {
    pub(crate) fn new(
        core: Arc<FloeCore>,
        vault: Arc<EncryptedAgentVault<Keys>>,
        local_context: Arc<LocalContextHost>,
        connections: floe_provider_adapters::gateway::GatewayCredentialStore,
        model: Arc<dyn floe_agent_contract::ModelPort + Send + Sync>,
        actor: floe_kernel::OwnerActor,
        source_connections: Arc<floe_connections::ConnectionsService>,
        admission: floe_experts::ExpertAdmissionIdentity,
        selection: floe_experts::ExpertExecutionSelection,
        registration: Arc<BoundExpertRegistration>,
    ) -> Self {
        Self {
            core,
            vault,
            local_context,
            connections,
            model,
            actor,
            source_connections,
            admission,
            selection,
            registration,
        }
    }
}

impl<Keys: VaultKeyProvider + 'static> AgentEndpoint for RegisteredExpertEndpoint<Keys> {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        Box::pin(async move {
            let context = &invocation.request.execution_context;
            context.validate()?;
            self.actor.validate()?;
            if self.actor.person_id != self.vault.person_id()
                || self.actor.device_id != context.device_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let run_id = invocation
                .request
                .parent_run_id
                .ok_or(AgentFailure::InvalidInput)?;
            self.registration.manifest.validate()?;
            if invocation.request.principal != self.vault.person_id().to_string()
                || invocation.request.selected_agent_id != self.admission.package.id
                || invocation.request.selected_definition_revision
                    != self.admission.definition_revision
                || self.registration.manifest.package != self.admission.package
                || self.registration.manifest.definition.definition_revision
                    != self.admission.definition_revision
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let person_id = self.vault.person_id();
            if self.selection.requirements.iter().any(|requirement| {
                requirement.selected.len() < usize::from(requirement.minimum_sources)
            }) {
                let repository =
                    floe_vault::VaultConversationRepository::new(Arc::clone(&self.vault));
                let origin_run_id =
                    floe_kernel::RunId::from_uuid(run_id).ok_or(AgentFailure::InvalidInput)?;
                let now_unix_ms = i64::try_from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .map_err(|_| AgentFailure::StaleContext)?
                        .as_millis(),
                )
                .map_err(|_| AgentFailure::StaleContext)?;
                let refs = floe_conversation::publish_expert_binding_blockers(
                    &repository,
                    &repository,
                    &invocation.request.principal,
                    context.session_id,
                    origin_run_id,
                    invocation.request.task_id.as_uuid(),
                    &self.admission,
                    &self.selection,
                    now_unix_ms,
                )
                .await?;
                return Ok(ExpertReport {
                    task_id: invocation.request.task_id,
                    principal: invocation.request.principal,
                    agent_id: invocation.request.selected_agent_id,
                    definition_revision: invocation.request.selected_definition_revision,
                    result: "Expert settings are required before this task can run.".into(),
                    artifacts: floe_conversation::interaction_ref_artifacts(&refs)?,
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                    settlement: None,
                });
            }
            let source_client = ServerSourceClient::from_current_connection(
                &self.connections,
                &person_id.to_string(),
                &context.device_id,
            )
            .await?;
            let signer = floe_vault::VaultAuthorizationSigner::new(
                self.vault.clone(),
                Arc::new(floe_provider_adapters::gateway::GatewayProofVerifier),
            );
            let verifier = floe_provider_adapters::gateway::GatewaySourcePreviewVerifier::new(
                self.vault.clone(),
            );
            let remote_reader = match source_client.as_ref() {
                Some(client) => Some(remote_views::RemoteViewReader::new(
                    &self.vault,
                    &self.core,
                    &signer,
                    &verifier,
                    client,
                    person_id,
                    client.source().client_id(),
                    client.source().device_id(),
                )),
                None => None,
            };
            let calendar_reader = SelectedCalendarContextReader {
                core: &self.core,
                vault: &self.vault,
                source_client: source_client.as_ref(),
                signer: &signer,
                verifier: &verifier,
                device_id: &context.device_id,
            };
            let attention_reader = PersonalAttentionReader {
                core: &self.core,
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &context.device_id,
            };
            let people_reader = PersonalPeopleReader {
                core: &self.core,
                vault: &self.vault,
                local_context: &self.local_context,
                device_id: &context.device_id,
            };
            let wellbeing_reader = PersonalWellbeingReader {
                core: &self.core,
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
            let policy = expert_policy(self.registration.manifest.data_class);
            let cards = vec![self.registration.manifest.definition.card.clone()];
            let stateful_settlement = VaultStatefulExpertSettlement {
                vault: self.vault.as_ref(),
                admission: &self.admission,
                selection: &self.selection,
            };
            let repository = floe_vault::VaultConversationRepository::new(Arc::clone(&self.vault));
            let journal = floe_conversation::ConversationRepository::journal(
                &repository,
                floe_kernel::RunId::from_uuid(run_id).ok_or(AgentFailure::InvalidInput)?,
            )?;
            let experts = ConversationExperts {
                model: self.model.as_ref(),
                journal: journal.as_ref(),
                actor: &self.actor,
                source_connections: self.source_connections.as_ref(),
                scope,
                calendar_reader: Some(&calendar_reader as &dyn CalendarContextReaderApi),
                policy: &policy,
                context: &context.agent_context,
                attention: Some(&attention_reader),
                people_reader: Some(&people_reader),
                wellbeing_reader: Some(&wellbeing_reader),
                recorder: Some(&recorder),
                remote_reader: remote_reader
                    .as_ref()
                    .map(|reader| reader as &dyn floe_context::SelectedSourceReader),
                context_reader: Some(&context_reader),
                task_views: &[],
                cards,
                stateful_settlement: &stateful_settlement,
                registrations: vec![Arc::clone(&self.registration)],
                runs: Some(&repository),
                interactions: Some(&repository),
                device_id: Some(context.device_id.as_str()),
                admitted_selection: Some(&self.selection),
            };
            let task_id = invocation.request.task_id.as_uuid();
            governed_store.record_result_independent(task_id, task_id)?;
            let mut output = experts
                .execute_registered(
                    &A2ASendMessageRequest {
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
                    },
                    &self.registration,
                )
                .await?;
            let coverage = governed_store
                .result_coverage(task_id, task_id)?
                .unwrap_or(floe_agent_contract::DependencyCoverage::Independent);
            for artifact in &mut output.artifacts {
                if artifact.coverage == floe_agent_contract::DependencyCoverage::Unknown {
                    artifact.coverage = coverage.clone();
                }
            }
            Ok(ExpertReport {
                task_id: invocation.request.task_id,
                principal: invocation.request.principal,
                agent_id: invocation.request.selected_agent_id,
                definition_revision: invocation.request.selected_definition_revision,
                result: output.result,
                artifacts: output.artifacts,
                coverage,
                settlement: output.settlement,
            })
        })
    }
}

/// The concrete readers one turn's Experts run against.
///
/// This is the injection site for the host port the builtin Experts declare:
/// every field is a reader or policy decided elsewhere and handed in here.
pub(crate) struct ConversationExperts<'model> {
    pub(super) model: &'model dyn floe_agent_contract::ModelPort,
    pub(super) journal: &'model dyn floe_agent_contract::ExecutionJournal,
    pub(super) actor: &'model floe_kernel::OwnerActor,
    pub(super) source_connections: &'model floe_connections::ConnectionsService,
    pub(super) scope: &'model floe_execution::ExecutionScope,
    pub(super) calendar_reader: Option<&'model dyn CalendarContextReaderApi>,
    pub(super) policy: &'model InferencePolicyDecision,
    pub(super) context: &'model AgentContext,
    pub(super) attention: Option<&'model dyn PersonalAttentionReaderApi>,
    pub(super) people_reader: Option<&'model dyn PersonalPeopleReaderApi>,
    pub(super) wellbeing_reader: Option<&'model dyn PersonalWellbeingReaderApi>,
    pub(super) recorder: Option<&'model dyn ResultRecorder>,
    pub(super) remote_reader: Option<&'model dyn floe_context::SelectedSourceReader>,
    pub(super) context_reader: Option<&'model dyn ConversationContextReaderApi>,
    pub(super) task_views: &'model [NativeContextView],
    pub(super) cards: Vec<AgentCard>,
    pub(super) stateful_settlement: &'model dyn StatefulExpertSettlement,
    pub(super) registrations: Vec<Arc<BoundExpertRegistration>>,
    /// The validated origin bindings trusted publication requires. A blocked
    /// source without them fails closed rather than completing ref-less.
    pub(super) runs: Option<&'model dyn floe_conversation::ConversationRepository>,
    pub(super) interactions: Option<&'model dyn floe_conversation::InteractionRepository>,
    pub(super) device_id: Option<&'model str>,
    pub(super) admitted_selection: Option<&'model floe_experts::ExpertExecutionSelection>,
}

/// One delegated message's Expert host.
///
/// Everything an Expert may read belongs to the turn and comes straight from
/// the turn's host. What belongs to the message alone is the bounded child of
/// the Task scope its model attempts settle against, the captured source
/// dependencies that make its dispatch coverage exact, and the trusted source
/// blockers this invocation observed for publication under the Task origin.
pub(crate) struct DelegatedMessageExperts<'turn, 'model, 'msg> {
    experts: &'turn ConversationExperts<'model>,
    manifest: floe_experts::ExpertManifest,
    recorder: CapturingRecorder<'msg>,
    model: ExpertModelHost<'msg>,
    captured: Mutex<Vec<floe_context_contract::SourceAccessRequirement>>,
}

impl<'turn, 'model, 'msg> DelegatedMessageExperts<'turn, 'model, 'msg> {
    /// Keep one invocation's trusted blockers, deduplicated, for the common
    /// endpoint to publish. Explicit invocation-scoped state: nothing global,
    /// nothing keyed by run id, nothing Schedule-only.
    fn capture(
        &self,
        blockers: &floe_context_contract::SourceAccessBlockers,
    ) -> Result<(), AgentFailure> {
        let mut captured = self
            .captured
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        for blocker in blockers.blockers() {
            if !captured.contains(blocker) {
                captured.push(blocker.clone());
            }
        }
        Ok(())
    }

    fn take_captured(
        &self,
    ) -> Result<Vec<floe_context_contract::SourceAccessRequirement>, AgentFailure> {
        let mut captured = self
            .captured
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        Ok(std::mem::take(&mut captured))
    }

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
            calendar_reader: self.experts.calendar_reader,
            person_id: request.person_id,
            people_reader: self.experts.people_reader,
            wellbeing_reader: self.experts.wellbeing_reader,
            recorder: Some(&self.recorder),
            dependency_turn_id: request.task_id,
            dependency_result_id: request.task_id,
            consumer_name,
        }
    }
}

struct AppLocalExpertSource<'a, 'turn, 'model, 'msg> {
    host: &'a DelegatedMessageExperts<'turn, 'model, 'msg>,
    request: &'a BuiltinExpertRequest,
}

impl floe_context::LocalExpertSourceDriver for AppLocalExpertSource<'_, '_, '_, '_> {
    fn read<'a>(
        &'a self,
        source: floe_context::LocalExpertSource,
        source_access_id: &'static str,
        selected_refs: &'a [floe_context_contract::SourceSelectionReference],
        query: serde_json::Value,
        deadline: tokio::time::Instant,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<
        'a,
        Result<
            floe_context_contract::SourceReadOutcome<(
                serde_json::Value,
                Vec<floe_context_contract::ContextDependency>,
            )>,
            AgentFailure,
        >,
    > {
        Box::pin(async move {
            use floe_context::LocalExpertSource;
            use floe_context_contract::SourceReadOutcome;
            let request = self.request;
            let personal = self.host.personal_views(request, &request.agent_id);
            let selected = || {
                if selected_refs.len() != 1 {
                    return Err(AgentFailure::CapabilityDenied);
                }
                Ok(&selected_refs[0])
            };
            match source {
                LocalExpertSource::Calendar => {
                    let calendar: floe_context_contract::CalendarViewQuery =
                        serde_json::from_value(query).map_err(|_| AgentFailure::InvalidInput)?;
                    match personal
                        .calendar_views(
                            source_access_id,
                            selected_refs,
                            &calendar,
                            deadline,
                            cancellation,
                        )
                        .await?
                    {
                        SourceReadOutcome::Ready(value) => Ok(SourceReadOutcome::Ready((
                            serde_json::to_value(value).map_err(|_| AgentFailure::InvalidInput)?,
                            vec![],
                        ))),
                        SourceReadOutcome::Unavailable(reason) => {
                            Ok(SourceReadOutcome::Unavailable(reason))
                        }
                        SourceReadOutcome::NeedsUserAction(blockers) => {
                            Ok(SourceReadOutcome::NeedsUserAction(blockers))
                        }
                    }
                }
                LocalExpertSource::People => {
                    match personal
                        .people_view(selected()?, deadline, cancellation)
                        .await?
                    {
                        SourceReadOutcome::Ready(value) => Ok(SourceReadOutcome::Ready((
                            serde_json::to_value(value).map_err(|_| AgentFailure::InvalidInput)?,
                            vec![],
                        ))),
                        SourceReadOutcome::Unavailable(reason) => {
                            Ok(SourceReadOutcome::Unavailable(reason))
                        }
                        SourceReadOutcome::NeedsUserAction(blockers) => {
                            Ok(SourceReadOutcome::NeedsUserAction(blockers))
                        }
                    }
                }
                LocalExpertSource::Wellbeing => {
                    match personal
                        .wellbeing_view(selected()?, deadline, cancellation)
                        .await?
                    {
                        SourceReadOutcome::Ready(value) => Ok(SourceReadOutcome::Ready((
                            serde_json::to_value(value).map_err(|_| AgentFailure::InvalidInput)?,
                            vec![],
                        ))),
                        SourceReadOutcome::Unavailable(reason) => {
                            Ok(SourceReadOutcome::Unavailable(reason))
                        }
                        SourceReadOutcome::NeedsUserAction(blockers) => {
                            Ok(SourceReadOutcome::NeedsUserAction(blockers))
                        }
                    }
                }
                LocalExpertSource::Attention => {
                    match self
                        .host
                        .experts
                        .attention
                        .ok_or(AgentFailure::CapabilityUnavailable)?
                        .read(
                            request.person_id,
                            &request.agent_id,
                            selected()?,
                            request.task_id,
                            request.task_id,
                            deadline,
                            cancellation,
                        )
                        .await?
                    {
                        SourceReadOutcome::Ready((value, dependency)) => {
                            Ok(SourceReadOutcome::Ready((
                                serde_json::to_value(value)
                                    .map_err(|_| AgentFailure::InvalidInput)?,
                                vec![dependency],
                            )))
                        }
                        SourceReadOutcome::Unavailable(reason) => {
                            Ok(SourceReadOutcome::Unavailable(reason))
                        }
                        SourceReadOutcome::NeedsUserAction(blockers) => {
                            Ok(SourceReadOutcome::NeedsUserAction(blockers))
                        }
                    }
                }
                LocalExpertSource::ConfirmedInteractions => Ok(SourceReadOutcome::Unavailable(
                    floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
                )),
                LocalExpertSource::ConfirmedMemory => {
                    floe_context::validate_local_source_selection(
                        selected()?,
                        self.host
                            .experts
                            .device_id
                            .ok_or(AgentFailure::CapabilityDenied)?,
                    )?;
                    let snapshot = self
                        .host
                        .experts
                        .context_reader
                        .ok_or(AgentFailure::CapabilityUnavailable)?
                        .memory()
                        .await?;
                    Ok(SourceReadOutcome::Ready((
                        serde_json::json!({"memories": snapshot.memories, "issue": snapshot.issue}),
                        vec![],
                    )))
                }
                LocalExpertSource::Tasks => {
                    floe_context::validate_local_source_selection(
                        selected()?,
                        self.host
                            .experts
                            .device_id
                            .ok_or(AgentFailure::CapabilityDenied)?,
                    )?;
                    let view = self
                        .host
                        .experts
                        .context_reader
                        .ok_or(AgentFailure::CapabilityUnavailable)?
                        .tasks()
                        .await?;
                    Ok(SourceReadOutcome::Ready((
                        serde_json::to_value(view).map_err(|_| AgentFailure::InvalidInput)?,
                        vec![],
                    )))
                }
            }
        })
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

    fn read_requirement<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        key: &'a str,
        query: serde_json::Value,
    ) -> floe_experts_builtin::Acquiring<
        'a,
        floe_experts::RequirementReadOutcome<DeclaredSourceRead<Self::SourceRead>>,
    > {
        Box::pin(async move {
            if self.manifest.package.id != request.agent_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            let selection = self
                .experts
                .admitted_selection
                .ok_or(AgentFailure::CapabilityDenied)?;
            let declared = self
                .manifest
                .source_requirements
                .iter()
                .find(|requirement| requirement.key == key)
                .ok_or(AgentFailure::CapabilityDenied)?;
            let admitted = selection
                .requirements
                .iter()
                .find(|requirement| {
                    requirement.key == declared.key
                        && requirement.capability == declared.capability
                        && requirement.contract_version == declared.contract_version
                })
                .ok_or(AgentFailure::CapabilityDenied)?;
            if admitted.selected.is_empty() {
                return Ok(floe_experts::RequirementReadOutcome::Unavailable(
                    floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
                ));
            }
            if matches!(
                declared.capability.as_str(),
                "floe.tasks" | "memory.confirmed"
            ) {
                let selected = admitted
                    .selected
                    .first()
                    .ok_or(AgentFailure::CapabilityDenied)?;
                if admitted.selected.len() != 1 {
                    return Err(AgentFailure::CapabilityDenied);
                }
                floe_context::validate_local_source_selection(
                    selected,
                    self.experts
                        .device_id
                        .ok_or(AgentFailure::CapabilityDenied)?,
                )?;
            }
            let requirements = [floe_context::DeclaredSourceRequirement {
                key: &admitted.key,
                capability: &admitted.capability,
                contract_version: admitted.contract_version,
                selected_refs: &admitted.selected,
            }];
            let local_driver = AppLocalExpertSource {
                host: self,
                request,
            };
            let outcome = floe_context::read_declared_source(
                self.experts.remote_reader,
                &local_driver,
                request.person_id,
                &request.agent_id,
                &requirements,
                key,
                query,
                request.deadline,
                &request.cancellation,
            )
            .await?;
            if let floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) = &outcome {
                self.capture(blockers)?;
            }
            Ok(match outcome {
                floe_context_contract::SourceReadOutcome::Ready(read) => {
                    floe_experts::RequirementReadOutcome::Ready(DeclaredSourceRead::new(
                        read.payload,
                        read.dependencies,
                        read.held,
                    ))
                }
                floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
                    floe_experts::RequirementReadOutcome::Unavailable(reason)
                }
                floe_context_contract::SourceReadOutcome::NeedsUserAction(_) => {
                    floe_experts::RequirementReadOutcome::NeedsUserAction
                }
            })
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
}

/// The consumer identity a general assistant read is made under.
impl InProcessAgent for ConversationExperts<'_> {
    fn agent_cards(&self, _: PersonId) -> Vec<AgentCard> {
        self.cards.clone()
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        let registration = self
            .registrations
            .iter()
            .find(|registration| registration.manifest.package.id == request.agent_id)
            .ok_or(AgentFailure::CapabilityDenied)?;
        let output = self.execute_registered(&request, registration).await?;
        floe_experts::completed_expert_task(
            request,
            output.result,
            output.artifacts,
            output.settlement,
        )
    }
}

impl ConversationExperts<'_> {
    async fn execute_registered(
        &self,
        request: &A2ASendMessageRequest,
        registration: &BoundExpertRegistration,
    ) -> Result<BuiltinExpertOutput, AgentFailure> {
        let manifest = &registration.manifest;
        let cards = self.agent_cards(request.person_id);
        let invocation_id = floe_experts::admit_expert_message(&request, &cards)?;
        let expert_started = std::time::Instant::now();
        tracing::info!(
            expert = request.agent_id,
            invocation_id = %invocation_id,
            "expert_invocation_started"
        );
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
            staged_task_views: self.task_views.to_vec(),
            context_inputs_available: self.context_reader.is_some(),
            max_output_bytes: request.max_output_bytes,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        // This message's attempts settle against a bounded child of the Task
        // scope, and its source reads are captured for the exact dispatch
        // coverage. Both bindings last exactly as long as this message runs.
        let captured = Mutex::new(Vec::new());
        let model_blocked = Mutex::new(None);
        let origin_run_id = floe_kernel::RunId::from_uuid(request.parent_turn_id)
            .ok_or(AgentFailure::InvalidInput)?;
        if let Some(receipt) = match self.runs {
            Some(runs) => runs.load_receipt(origin_run_id).await?,
            None => None,
        } {
            if receipt.principal != request.person_id.to_string()
                || receipt.session_id != request.session_id
            {
                return Err(AgentFailure::CapabilityDenied);
            }
        }
        manifest.validate()?;
        if manifest.package.id != request.agent_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let host = DelegatedMessageExperts {
            experts: self,
            manifest: manifest.clone(),
            recorder: CapturingRecorder {
                inner: self.recorder,
                captured: &captured,
            },
            model: ExpertModelHost {
                model: self.model,
                journal: self.journal,
                task_id: floe_kernel::TaskId::from_uuid(expert_request.task_id)
                    .ok_or(AgentFailure::InvalidInput)?,
                device_id: self.device_id.ok_or(AgentFailure::CapabilityDenied)?,
                scope: self.scope,
                captured: &captured,
                model_blocked: &model_blocked,
            },
            captured: Mutex::new(Vec::new()),
        };
        let mut output = registration.runner.run(&host, &expert_request).await?;
        reject_raw_requirement_artifacts(&output.artifacts)?;
        if output.settlement.is_none() {
            reject_raw_action_artifacts(&output.artifacts)?;
        }
        let captured = host.take_captured()?;
        if !captured.is_empty() {
            let runs = self.runs.ok_or(AgentFailure::CapabilityUnavailable)?;
            let interactions = self
                .interactions
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let blockers = floe_context_contract::SourceAccessBlockers::try_new(captured)
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
            let origin_run_id = floe_kernel::RunId::from_uuid(request.parent_turn_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let refs = floe_conversation::publish_task_source_review(
                runs,
                interactions,
                self.source_connections,
                floe_conversation::PublishTaskSourceReview {
                    actor: self.actor.clone(),
                    session_id: request.session_id,
                    origin_run_id,
                    task_id: expert_request.task_id,
                    capability_call_id: None,
                    blockers,
                    now_unix_ms: expert_request.current_time_unix_ms,
                },
                self.scope,
            )
            .await?;
            output
                .artifacts
                .extend(floe_conversation::interaction_ref_artifacts(&refs)?);
        }
        let model_requirement = model_blocked
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .take();
        if let Some(blocked) = model_requirement {
            let runs = self.runs.ok_or(AgentFailure::CapabilityUnavailable)?;
            let interactions = self
                .interactions
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let origin_run_id = floe_kernel::RunId::from_uuid(request.parent_turn_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let references = floe_conversation::publish_task_projection_review(
                runs,
                interactions,
                self.source_connections,
                floe_conversation::PublishTaskProjectionReview {
                    actor: self.actor.clone(),
                    session_id: request.session_id,
                    origin_run_id,
                    task_id: expert_request.task_id,
                    capability_call_id: None,
                    plan: blocked.plan,
                    review: blocked.review,
                    now_unix_ms: expert_request.current_time_unix_ms,
                },
                self.scope,
            )
            .await?;
            output
                .artifacts
                .extend(floe_conversation::interaction_ref_artifacts(&references)?);
        }
        tracing::info!(
            expert = request.agent_id,
            invocation_id = %expert_request.task_id,
            elapsed_ms = expert_started.elapsed().as_millis() as u64,
            "expert_invocation_completed"
        );
        Ok(output)
    }
}
