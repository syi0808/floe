//! Binding supplied Expert registrations to product endpoints and injecting
//! the concrete readers they run against.
//!
//! No Expert's judgment lives here. The selected registration carries its
//! runner through publication; this file supplies the invocation-scoped host.

use super::expert_host::{
    CalendarContextReaderApi, CapturingRecorder, ConversationContextReader,
    ConversationContextReaderApi, CurrentCalendarContextReader, ExpertModelHost,
    PersonalAttentionReader, PersonalAttentionReaderApi, PersonalPeopleReader,
    PersonalPeopleReaderApi, PersonalViewSource, PersonalWellbeingReader,
    PersonalWellbeingReaderApi, ResultRecorder, StoreResultRecorder, expert_policy,
};
use super::*;
use std::sync::{Arc, Mutex};

use floe_agent_contract::{AgentEndpoint, BoxFuture, EndpointInvocation, ExpertReport};
use floe_experts_builtin::{
    BuiltinExpertHost, BuiltinExpertOutput, BuiltinExpertRequest, DeclaredSourceRead,
    StatefulExpertDraft,
};

mod stateful_settlement;
#[cfg(test)]
pub(super) use stateful_settlement::RejectStatefulSettlement;
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
    connections: floe_provider_adapters::control::CurrentSavedConnectionStore,
    admission: floe_experts::ExpertAdmissionIdentity,
    selection: floe_experts::ExpertExecutionSelection,
    registration: Arc<BoundExpertRegistration>,
}

impl<Keys> RegisteredExpertEndpoint<Keys> {
    pub(crate) fn new(
        core: Arc<FloeCore>,
        vault: Arc<EncryptedAgentVault<Keys>>,
        local_context: Arc<LocalContextHost>,
        connections: floe_provider_adapters::control::CurrentSavedConnectionStore,
        admission: floe_experts::ExpertAdmissionIdentity,
        selection: floe_experts::ExpertExecutionSelection,
        registration: Arc<BoundExpertRegistration>,
    ) -> Self {
        Self {
            core,
            vault,
            local_context,
            connections,
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
            let registry = self.vault.expert_registry().await?.ok_or(AgentFailure::NotFound)?;
            floe_experts::AgentRegistry::restore(registry, self.vault.registry_instance_id())?
                .validate_current_execution_selection(
                    person_id,
                    &self.admission,
                    &self.selection,
                    true,
                )?;
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
                    floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                )?;
            let availability = floe_inference::InferenceAvailability::observe(
                &provider,
                floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
            )
            .await;
            let admission = floe_provider_adapters::control::SavedConnectionAdmission::new(
                self.connections.clone(),
                person_id.to_string(),
                context.device_id.clone(),
            );
            let authority = floe_access::ContextualRecipientAuthority::new(
                std::sync::Arc::clone(&self.vault),
                admission,
                floe_access::SystemConsentClock,
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
            let cards = vec![self.registration.manifest.definition.card.clone()];
            let stateful_settlement = VaultStatefulExpertSettlement {
                vault: self.vault.as_ref(),
                admission: &self.admission,
            };
            let repository = floe_vault::VaultConversationRepository::new(Arc::clone(&self.vault));
            let calendar_subject = crate::vault_host::calendar_access::DeviceCalendarSubject {
                local_context: &self.local_context,
            };
            let personal_subject =
                crate::vault_host::personal_grants::native_driver(&self.local_context);
            let snapshots = crate::vault_host::review_snapshot::HostReviewSnapshots {
                core: &self.core,
                vault: &self.vault,
                calendar_subject: &calendar_subject,
                personal_subject: &personal_subject,
                capture_deadline: scope
                    .deadline()
                    .min(tokio::time::Instant::now() + std::time::Duration::from_secs(5)),
            };
            let experts = ConversationExperts {
                executor: &service,
                scope,
                availability,
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
                stateful_settlement: &stateful_settlement,
                registrations: vec![Arc::clone(&self.registration)],
                runs: Some(&repository),
                interactions: Some(&repository),
                device_id: Some(context.device_id.as_str()),
                snapshots: Some(&snapshots),
            };
            let task_id = invocation.request.task_id.as_uuid();
            governed_store.record_result_independent(task_id, task_id)?;
            let mut output = experts
                .execute_registered(
                    &A2ASendMessageRequest {
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
            let current_registry = self.vault.expert_registry().await?.ok_or(AgentFailure::NotFound)?;
            floe_experts::AgentRegistry::restore(current_registry, self.vault.registry_instance_id())?
                .validate_current_execution_selection(
                    person_id,
                    &self.admission,
                    &self.selection,
                    true,
                )?;
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
    pub(super) stateful_settlement: &'model dyn StatefulExpertSettlement,
    pub(super) registrations: Vec<Arc<BoundExpertRegistration>>,
    /// The validated origin bindings trusted publication requires. A blocked
    /// source without them fails closed rather than completing ref-less.
    pub(super) runs: Option<&'model dyn floe_conversation::ConversationRepository>,
    pub(super) interactions: Option<&'model dyn floe_conversation::InteractionRepository>,
    pub(super) device_id: Option<&'model str>,
    /// The reviewed-snapshot capture publication binds inline cards to. A
    /// blocked source without it fails closed rather than completing
    /// ref-less.
    pub(super) snapshots:
        Option<&'model dyn crate::vault_host::review_snapshot::ReviewSnapshotSource>,
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
            match source {
                LocalExpertSource::Calendar => {
                    let calendar: floe_context_contract::CalendarViewQuery =
                        serde_json::from_value(query).map_err(|_| AgentFailure::InvalidInput)?;
                    match personal
                        .calendar_views(&calendar, deadline, cancellation)
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
                    match personal.people_view(deadline, cancellation).await? {
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
                    match personal.wellbeing_view(deadline, cancellation).await? {
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
                LocalExpertSource::ConfirmedInteractions => {
                    Ok(SourceReadOutcome::Unavailable(
                        floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
                    ))
                }
                LocalExpertSource::ConfirmedMemory => {
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
        floe_context_contract::SourceReadOutcome<DeclaredSourceRead<Self::SourceRead>>,
    > {
        Box::pin(async move {
            if self.manifest.package.id != request.agent_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            let requirements = self
                .manifest
                .source_requirements
                .iter()
                .map(|requirement| floe_context::DeclaredSourceRequirement {
                    key: &requirement.key,
                    capability: &requirement.capability,
                })
                .collect::<Vec<_>>();
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
                    floe_context_contract::SourceReadOutcome::Ready(DeclaredSourceRead::new(
                        read.payload,
                        read.dependencies,
                        read.held,
                    ))
                }
                floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
                    floe_context_contract::SourceReadOutcome::Unavailable(reason)
                }
                floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
                    floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers)
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
        // The delegation's intent lineage (Session + manager origin Run)
        // binds this message's dispatches; a non-Conversation delegation
        // carries none, and its external dispatches fail closed.
        let captured = Mutex::new(Vec::new());
        let model_blocked = Mutex::new(None);
        let lineage = floe_context_contract::RecipientLineage::try_new(
            request.session_id,
            request.parent_turn_id,
        )
        .ok();
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
                executor: self.executor,
                scope: self.scope,
                captured: &captured,
                lineage,
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
            let device_id = self.device_id.ok_or(AgentFailure::CapabilityUnavailable)?;
            let snapshots = self.snapshots.ok_or(AgentFailure::CapabilityUnavailable)?;
            let blockers = floe_context_contract::SourceAccessBlockers::try_new(captured)
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
            let origin_run_id = floe_kernel::RunId::from_uuid(request.parent_turn_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let refs = super::interaction_publication::publish_requirements(
                runs,
                interactions,
                snapshots,
                &request.person_id.to_string(),
                request.session_id,
                origin_run_id,
                floe_conversation::InteractionOrigin::Task {
                    task_id: expert_request.task_id,
                    capability_call_id: None,
                },
                request.person_id,
                device_id,
                &blockers,
                &expert_request.cancellation,
                expert_request.current_time_unix_ms,
            )
            .await?;
            output
                .artifacts
                .extend(super::interaction_publication::interaction_ref_artifacts(
                    &refs,
                )?);
        }
        let model_requirement = model_blocked
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .take();
        if let Some(requirement) = model_requirement {
            // A blocked Expert model reports its blocked-domain judgment
            // above; the trusted host publishes the captured requirement's
            // card under the Task origin and attaches the durable ref, so
            // the Manager sees exactly what to resume after Allow.
            let runs = self.runs.ok_or(AgentFailure::CapabilityUnavailable)?;
            let interactions = self
                .interactions
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let device_id = self.device_id.ok_or(AgentFailure::CapabilityUnavailable)?;
            let origin_run_id = floe_kernel::RunId::from_uuid(request.parent_turn_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let reference = super::interaction_publication::publish_model_blocker(
                runs,
                interactions,
                &request.person_id.to_string(),
                request.session_id,
                origin_run_id,
                expert_request.task_id,
                device_id,
                requirement,
                expert_request.current_time_unix_ms,
            )
            .await?;
            output
                .artifacts
                .extend(super::interaction_publication::interaction_ref_artifacts(
                    &[reference],
                )?);
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

#[cfg(test)]
mod registration_tests {
    use super::*;

    #[test]
    fn shipped_registration_set_matches_every_builtin_kind() {
        let mut actual: Vec<_> = shipped_registrations()
            .into_iter()
            .map(|registration| registration.manifest.package.id)
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

#[cfg(test)]
mod capture_tests {
    use super::expert_host::CalendarContextReaderApi;
    use super::*;
    use floe_context_contract::{
        ConnectionId, ConnectorId, GrantConsumer, GrantOperation, GrantPurpose, ResourceHandle,
        SourceAccessBlockers, SourceAccessRequirement, SourceAccessRequirementKind,
        SourceReadOutcome,
    };

    struct UnusedExecutor;

    impl floe_inference::InferenceExecutor for UnusedExecutor {
        fn execute<'a>(
            &'a self,
            _: floe_agent_contract::ModelRequest,
            _: &'a floe_execution::ExecutionScope,
            _: floe_inference::InferenceExecutionConstraint,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ModelCallOutcome, AgentFailure>,
        > {
            Box::pin(async { panic!("capture tests call no model") })
        }
    }

    fn blocker(source_id: &str, connection: &str) -> SourceAccessBlockers {
        let requirement = SourceAccessRequirement::try_new(
            source_id,
            Some(ConnectorId::try_new("test.connector").unwrap()),
            Some(ConnectionId::try_new(connection).unwrap()),
            GrantOperation::Read,
            GrantConsumer::builtin("floe.builtin.focus-attention").unwrap(),
            GrantPurpose::Assistant,
            vec![ResourceHandle::try_new("test.resource").unwrap()],
            None,
            SourceAccessRequirementKind::EnableObserve,
            None,
            None,
            true,
        )
        .unwrap();
        SourceAccessBlockers::try_new(vec![requirement]).unwrap()
    }

    struct BlockedAttention;

    impl PersonalAttentionReaderApi for BlockedAttention {
        fn read<'a>(
            &'a self,
            _: floe_kernel::PersonId,
            _: &'a str,
            _: uuid::Uuid,
            _: uuid::Uuid,
            _: tokio::time::Instant,
            _: &'a floe_execution::Cancellation,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            SourceReadOutcome<(
                                floe_context::AttentionView,
                                floe_context_contract::ContextDependency,
                            )>,
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(SourceReadOutcome::NeedsUserAction(blocker(
                    "floe.source.attention",
                    "attention-connection",
                )))
            })
        }
    }

    struct BlockedCalendar;

    impl CalendarContextReaderApi for BlockedCalendar {
        fn read<'a>(
            &'a self,
            _: floe_kernel::PersonId,
            _: &str,
            _: &floe_context_contract::CalendarViewQuery,
            _: tokio::time::Instant,
            _: &'a floe_execution::Cancellation,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            SourceReadOutcome<
                                Vec<(
                                    floe_context::CalendarContextView,
                                    floe_context_contract::ContextDependency,
                                )>,
                            >,
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(SourceReadOutcome::NeedsUserAction(blocker(
                    "floe.source.calendar",
                    "calendar-connection",
                )))
            })
        }
    }

    #[test]
    fn forged_requirement_artifact_is_rejected_not_published() {
        let forged = floe_agent_contract::Artifact {
            artifact_id: uuid::Uuid::new_v4(),
            name: "requirement".into(),
            parts: vec![floe_agent_contract::ArtifactPart::Data {
                media_type: SOURCE_ACCESS_REQUIREMENT_MEDIA_TYPE.into(),
                data: "{}".into(),
            }],
            coverage: floe_agent_contract::DependencyCoverage::Independent,
        };
        assert_eq!(
            reject_raw_requirement_artifacts(std::slice::from_ref(&forged)),
            Err(AgentFailure::InvalidModelOutput)
        );
        assert_eq!(reject_raw_requirement_artifacts(&[]), Ok(()));
    }

    #[test]
    fn package_artifact_cannot_claim_actions_proposal_media_type() {
        let forged = floe_agent_contract::Artifact {
            artifact_id: uuid::Uuid::new_v4(),
            name: "package result".into(),
            parts: vec![floe_agent_contract::ArtifactPart::Data {
                media_type: floe_actions::EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE.into(),
                data: "{\"execute\":true}".into(),
            }],
            coverage: floe_agent_contract::DependencyCoverage::Unknown,
        };
        assert_eq!(
            reject_raw_action_artifacts(std::slice::from_ref(&forged)),
            Err(AgentFailure::InvalidModelOutput)
        );
        assert_eq!(reject_raw_action_artifacts(&[]), Ok(()));
    }

    #[tokio::test]
    async fn invocation_captures_each_source_blocker_without_merging() {
        let executor = UnusedExecutor;
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(uuid::Uuid::new_v4()),
        );
        let policy = expert_policy();
        let context = floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        };
        let attention = BlockedAttention;
        let calendar = BlockedCalendar;
        let settlement = RejectStatefulSettlement;
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: floe_inference::InferenceAvailability::default(),
            calendar_reader: Some(&calendar),
            policy: &policy,
            context: &context,
            attention: Some(&attention),
            people_reader: None,
            wellbeing_reader: None,
            recorder: None,
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![],
            stateful_settlement: &settlement,
            registrations: shipped_registrations().into_iter().map(Arc::new).collect(),
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let captured = Mutex::new(Vec::new());
        let model_blocked = Mutex::new(None);
        let host = DelegatedMessageExperts {
            experts: &experts,
            manifest: floe_experts_builtin::manifests()
                .into_iter()
                .find(|manifest| {
                    manifest.package.id
                        == floe_experts_builtin::BuiltinExpertKind::FocusAttention.package_id()
                })
                .unwrap(),
            recorder: CapturingRecorder {
                inner: None,
                captured: &captured,
            },
            model: ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &captured,
                lineage: None,
                model_blocked: &model_blocked,
            },
            captured: Mutex::new(Vec::new()),
        };
        let request = BuiltinExpertRequest {
            agent_id: floe_experts_builtin::BuiltinExpertKind::FocusAttention
                .package_id()
                .into(),
            person_id: floe_kernel::PersonId::new(),
            task_id: uuid::Uuid::new_v4(),
            invocation_id: uuid::Uuid::new_v4(),
            assignment: "focus".into(),
            current_time_unix_ms: chrono::Utc::now().timestamp_millis(),
            context: context.clone(),
            staged_task_views: vec![],
            context_inputs_available: false,
            max_output_bytes: 16_384,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: floe_execution::Cancellation::default(),
        };
        let attention_outcome = BuiltinExpertHost::read_requirement(
            &host,
            &request,
            "floe.source.attention",
            serde_json::json!({"schema_version": AGENT_VERSION}),
        )
        .await
        .unwrap();
        assert!(matches!(
            attention_outcome,
            SourceReadOutcome::NeedsUserAction(_)
        ));
        let calendar_outcome = BuiltinExpertHost::read_requirement(
            &host,
            &request,
            "floe.source.calendar",
            serde_json::to_value(request.nearby_calendar_query().unwrap()).unwrap(),
        )
        .await
        .unwrap();
        assert!(matches!(
            calendar_outcome,
            SourceReadOutcome::NeedsUserAction(_)
        ));
        // Both blockers are preserved as distinct requirements: nothing is
        // merged, nothing is dropped, and the capture is deduplicated.
        let _ = BuiltinExpertHost::read_requirement(
            &host,
            &request,
            "floe.source.attention",
            serde_json::json!({"schema_version": AGENT_VERSION}),
        )
        .await
        .unwrap();
        let captured = host.take_captured().unwrap();
        assert_eq!(captured.len(), 2);
        let mut sources: Vec<_> = captured.iter().map(|blocker| blocker.source_id()).collect();
        sources.sort();
        assert_eq!(
            sources,
            vec!["floe.source.attention", "floe.source.calendar"]
        );
    }

    struct ReadyAttention;

    impl PersonalAttentionReaderApi for ReadyAttention {
        fn read<'a>(
            &'a self,
            person: floe_kernel::PersonId,
            _: &'a str,
            _: uuid::Uuid,
            _: uuid::Uuid,
            _: tokio::time::Instant,
            _: &'a floe_execution::Cancellation,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            SourceReadOutcome<(
                                floe_context::AttentionView,
                                floe_context_contract::ContextDependency,
                            )>,
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            let now = chrono::Utc::now();
            let view = floe_context::AttentionView {
                schema_version: floe_agent_contract::AGENT_VERSION,
                view_id: floe_context_contract::ATTENTION_VIEW_ID.into(),
                source_handle: "attention:test".into(),
                observed_at_unix_ms: (now - chrono::Duration::seconds(1)).timestamp_millis(),
                expires_at_unix_ms: (now + chrono::Duration::seconds(60)).timestamp_millis(),
                state: floe_context_contract::AttentionState::Focused,
                confidence_millis: 800,
                evidence_handles: vec!["att:1".into()],
            };
            let source = floe_context_contract::GrantSourceBinding::try_new(
                person,
                floe_context_contract::ConnectionId::try_new("attention-connection").unwrap(),
                floe_context_contract::ConnectorId::try_new("attention.macos").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("device").unwrap(),
                floe_context_contract::SourceAuthority::new(),
            )
            .unwrap();
            let dependency = floe_context_contract::ContextDependency::try_new(
                person,
                floe_context_contract::GrantId::new(),
                floe_context_contract::GrantAuthority::new(),
                source,
                vec![floe_context_contract::ResourceHandle::try_new("attention.coarse").unwrap()],
                vec![floe_context_contract::GrantDataCategory::Derived],
                floe_context_contract::GrantOperation::Read,
                floe_context_contract::GrantPurpose::Assistant,
                floe_context_contract::GrantConsumer::builtin("floe.builtin.focus-attention")
                    .unwrap(),
                floe_context_contract::ProcessingRestriction::LocalOnly,
                floe_context_contract::ConsumerPolicyAuthority::new(),
                uuid::Uuid::new_v4(),
                vec![7; 32],
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
                now - chrono::Duration::seconds(1),
                now + chrono::Duration::seconds(60),
            )
            .unwrap();
            Box::pin(async { Ok(SourceReadOutcome::Ready((view, dependency))) })
        }
    }

    struct ScriptedAnswer {
        text: String,
    }

    impl floe_inference::InferenceExecutor for ScriptedAnswer {
        fn execute<'a>(
            &'a self,
            _: floe_agent_contract::ModelRequest,
            _: &'a floe_execution::ExecutionScope,
            _: floe_inference::InferenceExecutionConstraint,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ModelCallOutcome, AgentFailure>,
        > {
            let text = self.text.clone();
            Box::pin(async move {
                Ok(floe_agent_contract::ModelCallOutcome::Ready(
                    floe_agent_contract::ModelResponse {
                        attempt_id: uuid::Uuid::new_v4(),
                        steps: vec![floe_agent_contract::ModelStep::Answer {
                            text,
                            artifacts: vec![],
                        }],
                        usage: floe_agent_contract::ModelUsage {
                            tokens: 10,
                            cost_micros: 10,
                        },
                    },
                ))
            })
        }
    }

    struct ProbeRecorder;

    impl ResultRecorder for ProbeRecorder {
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

    #[tokio::test]
    async fn optional_calendar_blocker_is_captured_while_judgment_uses_admitted_evidence() {
        let executor = ScriptedAnswer {
            text: r#"{"summary": "Stay focused.", "recommendation": "protect_focus", "rationale": "Deep work.", "evidence_handles": ["att:1"]}"#
                .into(),
        };
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(uuid::Uuid::new_v4()),
        );
        let policy = expert_policy();
        let context = floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        };
        let attention = ReadyAttention;
        let calendar = BlockedCalendar;
        let settlement = RejectStatefulSettlement;
        let store = ProbeRecorder;
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: floe_inference::InferenceAvailability::default(),
            calendar_reader: Some(&calendar),
            policy: &policy,
            context: &context,
            attention: Some(&attention),
            people_reader: None,
            wellbeing_reader: None,
            recorder: Some(&store),
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![],
            stateful_settlement: &settlement,
            registrations: shipped_registrations().into_iter().map(Arc::new).collect(),
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let dependencies = Mutex::new(Vec::new());
        let model_blocked = Mutex::new(None);
        let host = DelegatedMessageExperts {
            experts: &experts,
            manifest: floe_experts_builtin::manifests()
                .into_iter()
                .find(|manifest| {
                    manifest.package.id
                        == floe_experts_builtin::BuiltinExpertKind::FocusAttention.package_id()
                })
                .unwrap(),
            recorder: CapturingRecorder {
                inner: None,
                captured: &dependencies,
            },
            model: ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &dependencies,
                lineage: None,
                model_blocked: &model_blocked,
            },
            captured: Mutex::new(Vec::new()),
        };
        let expert_request = BuiltinExpertRequest {
            agent_id: floe_experts_builtin::BuiltinExpertKind::FocusAttention
                .package_id()
                .into(),
            person_id: floe_kernel::PersonId::new(),
            task_id: uuid::Uuid::new_v4(),
            invocation_id: uuid::Uuid::new_v4(),
            assignment: "focus".into(),
            current_time_unix_ms: chrono::Utc::now().timestamp_millis(),
            context: context.clone(),
            staged_task_views: vec![],
            context_inputs_available: false,
            max_output_bytes: 16_384,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: floe_execution::Cancellation::default(),
        };
        // The real dispatch through the real host: the optional calendar
        // blocker is captured for publication while the judgment runs over
        // the admitted attention evidence.
        let output = floe_experts_builtin::focus_attention::dispatch(&host, &expert_request)
            .await
            .unwrap();
        assert!(
            output
                .data_part(floe_experts_builtin::focus_attention::RESULT_MEDIA_TYPE)
                .unwrap()
                .contains("protect_focus"),
            "optional blocker must not gate the judgment: {}",
            output
                .data_part(floe_experts_builtin::focus_attention::RESULT_MEDIA_TYPE)
                .unwrap()
        );
        assert!(
            output.artifacts.len() == 1,
            "the judgment proposes no requirement of its own"
        );
        let captured = host.take_captured().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].source_id(), "floe.source.calendar");
        assert!(
            !dependencies.lock().unwrap().is_empty(),
            "admitted attention evidence stays recorded"
        );
    }

    struct BlockingExecutor {
        requirement: floe_context_contract::ProcessingRequirement,
    }

    impl floe_inference::InferenceExecutor for BlockingExecutor {
        fn execute<'a>(
            &'a self,
            _: floe_agent_contract::ModelRequest,
            _: &'a floe_execution::ExecutionScope,
            _: floe_inference::InferenceExecutionConstraint,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ModelCallOutcome, AgentFailure>,
        > {
            let requirement = self.requirement.clone();
            Box::pin(async move {
                Ok(floe_agent_contract::ModelCallOutcome::NeedsUserAction(
                    requirement,
                ))
            })
        }
    }

    fn blocked_requirement() -> floe_context_contract::ProcessingRequirement {
        floe_context_contract::ProcessingRequirement::try_new(
            "model.example",
            "server-model",
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
            vec![floe_agent_contract::DataClass::Personal],
            vec![],
            uuid::Uuid::new_v4(),
            1,
            floe_context_contract::RecipientLineage::try_new(
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn blocked_model_call_reports_blocked_domain_judgment_without_artifacts() {
        let requirement = blocked_requirement();
        let executor = BlockingExecutor {
            requirement: requirement.clone(),
        };
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(uuid::Uuid::new_v4()),
        );
        let policy = expert_policy();
        let context = floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        };
        let attention = ReadyAttention;
        let calendar = BlockedCalendar;
        let settlement = RejectStatefulSettlement;
        let store = ProbeRecorder;
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: floe_inference::InferenceAvailability::default(),
            calendar_reader: Some(&calendar),
            policy: &policy,
            context: &context,
            attention: Some(&attention),
            people_reader: None,
            wellbeing_reader: None,
            recorder: Some(&store),
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![],
            stateful_settlement: &settlement,
            registrations: shipped_registrations().into_iter().map(Arc::new).collect(),
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let dependencies = Mutex::new(Vec::new());
        let model_blocked = Mutex::new(None);
        let host = DelegatedMessageExperts {
            experts: &experts,
            manifest: floe_experts_builtin::manifests()
                .into_iter()
                .find(|manifest| {
                    manifest.package.id
                        == floe_experts_builtin::BuiltinExpertKind::FocusAttention.package_id()
                })
                .unwrap(),
            recorder: CapturingRecorder {
                inner: None,
                captured: &dependencies,
            },
            model: ExpertModelHost {
                executor: &executor,
                scope: &scope,
                captured: &dependencies,
                lineage: None,
                model_blocked: &model_blocked,
            },
            captured: Mutex::new(Vec::new()),
        };
        let expert_request = BuiltinExpertRequest {
            agent_id: floe_experts_builtin::BuiltinExpertKind::FocusAttention
                .package_id()
                .into(),
            person_id: floe_kernel::PersonId::new(),
            task_id: uuid::Uuid::new_v4(),
            invocation_id: uuid::Uuid::new_v4(),
            assignment: "focus".into(),
            current_time_unix_ms: chrono::Utc::now().timestamp_millis(),
            context: context.clone(),
            staged_task_views: vec![],
            context_inputs_available: false,
            max_output_bytes: 16_384,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: floe_execution::Cancellation::default(),
        };
        // The judgment stops at the blocked dispatch: a deterministic
        // blocked-domain report that proposes no requirement of its own,
        // while the trusted host stash holds the requirement to publish.
        let output = floe_experts_builtin::focus_attention::dispatch(&host, &expert_request)
            .await
            .unwrap();
        assert!(
            output.result.contains("Model approval"),
            "blocked judgment must name the review: {}",
            output.result
        );
        assert!(
            output
                .data_part(floe_experts_builtin::focus_attention::RESULT_MEDIA_TYPE)
                .unwrap()
                .contains("needs_user_action"),
            "blocked report carries the status: {}",
            output
                .data_part(floe_experts_builtin::focus_attention::RESULT_MEDIA_TYPE)
                .unwrap()
        );
        assert!(
            output.artifacts.len() == 1,
            "the judgment proposes no requirement of its own"
        );
        assert_eq!(*model_blocked.lock().unwrap(), Some(requirement));
    }

    #[derive(Clone, Default)]
    struct TestKeys(
        std::sync::Arc<
            Mutex<std::collections::HashMap<(floe_kernel::PersonId, uuid::Uuid), [u8; 32]>>,
        >,
    );

    impl floe_vault::VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person: floe_kernel::PersonId,
            vault: uuid::Uuid,
        ) -> Result<floe_vault::VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person, vault))
                .copied()
                .map(floe_vault::VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person: floe_kernel::PersonId,
            vault: uuid::Uuid,
            key: &floe_vault::VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person, vault), *key.as_bytes());
            Ok(())
        }
    }

    struct AvailableProvider;

    impl floe_inference::ModelProvider for AvailableProvider {
        type Prepared = ProbeTransport;

        async fn observe_profiles(
            &self,
        ) -> Vec<floe_inference::PreparedModelProfile<Self::Prepared>> {
            vec![floe_inference::PreparedModelProfile {
                profile: floe_inference::ModelProfile {
                    id: "device-model".into(),
                    purpose: floe_inference::ModelPurpose::new(
                        floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                    )
                    .unwrap(),
                    consumer: floe_inference::ModelConsumer::new(
                        floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                    )
                    .unwrap(),
                    execution_location: floe_inference::ExecutionLocation::Device,
                    data_recipient: floe_inference::DataRecipient::Device,
                    capabilities: floe_inference::ModelCapabilities(vec![]),
                    available: true,
                },
                transport: ProbeTransport,
            }]
        }
    }

    #[derive(Clone)]
    struct ProbeTransport;

    impl floe_inference::PreparedModelTransport for ProbeTransport {
        fn generate(
            &self,
            _: floe_inference::CanonicalModelRequest,
            _: floe_inference::AdmittedDispatchTarget,
        ) -> impl std::future::Future<
            Output = Result<floe_inference::CanonicalModelResponse, AgentFailure>,
        > + Send {
            async { Err(AgentFailure::ModelUnavailable) }
        }
    }

    #[tokio::test]
    async fn blocked_model_call_publishes_card_under_task_origin() {
        use floe_experts::{A2AMessageRole, A2APart, A2ATaskState};
        use std::os::unix::fs::DirBuilderExt;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let person = floe_kernel::PersonId::new();
        let run_id = floe_kernel::RunId::new();
        let task_id = uuid::Uuid::new_v4();
        let vault = std::sync::Arc::new(
            floe_vault::EncryptedAgentVault::create(&root, person, TestKeys::default())
                .await
                .unwrap(),
        );
        vault.activate_conversation_executor().await.unwrap();
        let runs = floe_vault::VaultConversationRepository::new(std::sync::Arc::clone(&vault));
        let started = floe_conversation::start_session(
            &runs,
            floe_conversation::SessionRequest {
                principal: person.to_string(),
            },
        )
        .await
        .unwrap();
        let session_id = started.session_id;
        // The blocked dispatch runs under the delegation lineage the
        // endpoint derives: this Session plus the manager origin Run.
        let requirement = floe_context_contract::ProcessingRequirement::try_new(
            "model.example",
            "server-model",
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
            vec![floe_agent_contract::DataClass::Personal],
            vec![],
            uuid::Uuid::new_v4(),
            1,
            floe_context_contract::RecipientLineage::try_new(session_id, run_id.as_uuid()).unwrap(),
        )
        .unwrap();
        let executor = BlockingExecutor {
            requirement: requirement.clone(),
        };
        let command_id = floe_agent_contract::CommandId::new();
        floe_conversation::ConversationRepository::admit_turn(
            &runs,
            floe_conversation::TurnAdmissionRequest {
                run_id,
                command_id,
                session_id,
                expected_session_revision: 0,
                principal: person.to_string(),
                request_digest: [7; 32],
                mode: floe_conversation::TurnMode::New,
                retry_of: None,
                profile: floe_conversation::ProfileSelection::Auto,
                user_message: floe_agent_contract::AgentMessage {
                    message_id: command_id.as_uuid(),
                    role: floe_agent_contract::MessageRole::User,
                    text: "Focus now".into(),
                    call_id: None,
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                },
            },
        )
        .await
        .unwrap();
        let delegation = floe_agent_contract::DelegationRequest {
            task_id: floe_agent_contract::TaskId::from_uuid(task_id)
                .expect("task id must be valid"),
            parent_run_id: Some(run_id.as_uuid()),
            principal: person.to_string(),
            invocation_key: floe_agent_contract::InvocationKey::new(),
            selected_agent_id: floe_experts_builtin::BuiltinExpertKind::FocusAttention
                .package_id()
                .into(),
            selected_definition_revision: 1,
            message: "focus".into(),
            context_refs: vec![],
            execution_context: floe_agent_contract::DelegationExecutionContext {
                session_id,
                device_id: "test-device".into(),
                agent_context: floe_agent_contract::AgentContext {
                    projection_version: 1,
                    persona: None,
                    memories: vec![],
                    optional_context_issues: vec![],
                    evidence: vec![],
                },
                max_output_bytes: 16_384,
            },
        };
        floe_conversation::ConversationRepository::journal(&runs, run_id)
            .unwrap()
            .record_intent(floe_agent_contract::JournalEvent::DelegationIntent {
                request: delegation,
            })
            .await
            .unwrap();

        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(uuid::Uuid::new_v4()),
        );
        let policy = expert_policy();
        let context = floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        };
        let attention = ReadyAttention;
        let calendar = BlockedCalendar;
        let settlement = RejectStatefulSettlement;
        let store = ProbeRecorder;
        let availability = floe_inference::InferenceAvailability::observe(
            &AvailableProvider,
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
        )
        .await;
        let device_id = "test-device";
        let snapshots = crate::vault_host::review_snapshot::NoCaptureSnapshots;
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability,
            calendar_reader: Some(&calendar),
            policy: &policy,
            context: &context,
            attention: Some(&attention),
            people_reader: None,
            wellbeing_reader: None,
            recorder: Some(&store),
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![floe_agent_contract::AgentCard {
                schema_version: floe_agent_contract::AGENT_VERSION,
                protocol_version: floe_experts::A2A_PROTOCOL_VERSION.into(),
                id: floe_experts_builtin::BuiltinExpertKind::FocusAttention
                    .package_id()
                    .into(),
                version: "1.0.0".into(),
                name: "Focus Expert".into(),
                description: "Protects the current focus".into(),
                domain_tags: vec!["focus".into()],
                skills: vec!["Protect the current focus".into()],
                supported_placements: vec![floe_agent_contract::ModelPlacement::DeviceLocal],
            }],
            stateful_settlement: &settlement,
            registrations: shipped_registrations().into_iter().map(Arc::new).collect(),
            runs: Some(&runs),
            interactions: Some(&runs),
            device_id: Some(device_id),
            snapshots: Some(&snapshots),
        };
        let request = floe_experts::A2ASendMessageRequest {
            usage: floe_inference::UsageLedger::default(),
            schema_version: floe_agent_contract::AGENT_VERSION,
            person_id: person,
            session_id,
            parent_turn_id: run_id.as_uuid(),
            agent_id: floe_experts_builtin::BuiltinExpertKind::FocusAttention
                .package_id()
                .into(),
            message: floe_experts::A2AMessage {
                message_id: uuid::Uuid::new_v4(),
                context_id: uuid::Uuid::new_v4(),
                task_id: Some(task_id),
                role: A2AMessageRole::User,
                parts: vec![A2APart::Text {
                    text: "focus".into(),
                }],
            },
            max_output_bytes: 16_384,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: floe_execution::Cancellation::default(),
        };
        let registration = shipped_registrations()
            .into_iter()
            .find(|registration| registration.manifest.package.id == request.agent_id)
            .unwrap();
        let output = experts
            .execute_registered(&request, &registration)
            .await
            .unwrap();
        let task = floe_experts::completed_expert_task(
            request,
            output.result,
            output.artifacts,
            output.settlement,
        )
        .unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(task.state, A2ATaskState::Completed);
        // The optional calendar blocker and the model blockage each
        // publish: both trusted refs attach beside the result artifact.
        let mut refs = task
            .artifacts
            .iter()
            .filter_map(|artifact| {
                artifact.parts.iter().find_map(|part| match part {
                    floe_experts::A2APart::Data { media_type, data }
                        if media_type == floe_agent_contract::USER_INTERACTION_MEDIA_TYPE =>
                    {
                        Some(
                            serde_json::from_str::<floe_agent_contract::UserInteractionRef>(data)
                                .unwrap(),
                        )
                    }
                    _ => None,
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(refs.len(), 2);
        refs.sort_by_key(|reference| reference.interaction_id);
        let kinds = refs
            .iter()
            .map(|reference| reference.kind)
            .collect::<Vec<_>>();
        assert!(kinds.contains(&floe_agent_contract::UserInteractionKind::SourceAccess));
        assert!(kinds.contains(&floe_agent_contract::UserInteractionKind::ProcessingRecipient));
        let model_ref = refs
            .iter()
            .find(|reference| {
                reference.kind == floe_agent_contract::UserInteractionKind::ProcessingRecipient
            })
            .unwrap();
        let stored = floe_conversation::InteractionRepository::get_interaction(
            &runs,
            person,
            model_ref.interaction_id,
        )
        .await
        .unwrap()
        .expect("blocked dispatch must publish a durable interaction");
        assert_eq!(stored.state, floe_conversation::InteractionState::Pending);
        assert_eq!(
            stored.origin,
            floe_conversation::InteractionOrigin::Task {
                task_id,
                capability_call_id: None,
            }
        );
        assert_eq!(
            stored.requirement.kind,
            floe_conversation::InteractionRequirementKind::ApproveProcessingRecipient
        );
        assert_eq!(stored.requirement.source_id, requirement.recipient());
        assert_eq!(stored.requirement.consumer, requirement.consumer());
        assert_eq!(stored.requirement.purpose, requirement.purpose());
    }
}
