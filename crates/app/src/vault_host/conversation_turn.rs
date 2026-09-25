use std::{future::Future, pin::Pin};

use crate::ConversationTurnRequest;
use floe_agent_contract::{AgentFailure, DataClass};
use floe_context::{AgentContext, InferencePolicyDecision, NativeContextView};
use floe_conversation::{AgentBudget, AgentEvent, ConversationRepository, SessionStore};
use floe_experts::{
    A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, AgentCard, InProcessAgent,
};
#[cfg(test)]
use floe_experts::{A2ATaskState, EXPERT_RESULT_MEDIA_TYPE};
#[cfg(test)]
use floe_experts_builtin::{
    BuiltinExpertKind, commitments::CommitmentsExpertResult,
    life_logistics::LifeLogisticsExpertResult, work_context::WorkContextExpertResult,
};
use floe_kernel::AGENT_VERSION;
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use crate::FloeCore;
use crate::local_context::LocalContextHost;
use floe_provider_adapters::sources::ServerSourceClient;

use super::personal_grants;
use super::remote_views;

pub(super) mod engine_ports;
pub(super) mod expert_dispatch;
pub(super) mod expert_host;
pub(super) mod interaction_publication;

const FINALIZATION_TOKENS: u64 = 1_024;
const FINALIZATION_COST_MICROS: u64 = 10_000;

struct ConversationTurnInputs<'a, Keys: VaultKeyProvider> {
    core: &'a FloeCore,
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextHost,
    person_id: PersonId,
    request: &'a ConversationTurnRequest,
    connections: &'a floe_provider_adapters::control::CurrentSavedConnectionStore,
    command_id: floe_agent_contract::CommandId,
    conversation_repository: &'a std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &'a std::sync::Arc<floe_conversation::RunCancellationRegistry>,
    task_coordinator: &'a floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    /// The Run this turn continues, as Conversation admitted it.
    mode: floe_conversation::TurnMode,
    /// The data classes of the admitted Session, carried to the canonical
    /// projector. Session admission owns them; App policy does not.
    session_data_classes: Vec<DataClass>,
}

pub(super) async fn run<Keys: VaultKeyProvider + 'static>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextHost,
    task_coordinator: &floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    conversation_repository: &std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &std::sync::Arc<floe_conversation::RunCancellationRegistry>,
    connections: &floe_provider_adapters::control::CurrentSavedConnectionStore,
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
        },
    )
    .await?;
    let inputs = ConversationTurnInputs {
        core,
        vault,
        local_context,
        person_id,
        request,
        connections,
        command_id,
        conversation_repository,
        run_cancellations,
        task_coordinator,
        mode: prepared.mode,
        session_data_classes: prepared.session.data_classes.clone(),
    };
    drive_turn(&inputs, cancellation, on_admitted, emit).await
}

/// One linked-resume drive, as this host states it.
///
/// The caller names only the origin linkage and the revision to admit at:
/// the origin's own revision for an automatic resume, the current reviewed
/// Session revision for an explicit Continue. Text, profile and mode all
/// resolve from the origin's durable admission.
pub(super) struct ResumeTurnRequest {
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub device_id: String,
    pub resume: floe_conversation::InteractionResumeRef,
}

pub(super) async fn run_resume<Keys: VaultKeyProvider + 'static>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextHost,
    task_coordinator: &floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    conversation_repository: &std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &std::sync::Arc<floe_conversation::RunCancellationRegistry>,
    connections: &floe_provider_adapters::control::CurrentSavedConnectionStore,
    person_id: PersonId,
    command_id: floe_agent_contract::CommandId,
    request: &ResumeTurnRequest,
    cancellation: floe_execution::Cancellation,
    on_admitted: impl FnMut(&floe_conversation::RunReceipt),
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_conversation::AgentSession, AgentFailure> {
    let prepared = floe_conversation::prepare_resume(
        conversation_repository.as_ref(),
        vault,
        floe_conversation::ResumePreparationRequest {
            principal: person_id.to_string(),
            person_id,
            session_id: request.session_id,
            resume: request.resume,
        },
    )
    .await?;
    let derived = ConversationTurnRequest::new(
        request.session_id,
        request.expected_revision,
        prepared.text,
        request.device_id.clone(),
        prepared.origin.profile.clone(),
        false,
        None,
    );
    let inputs = ConversationTurnInputs {
        core,
        vault,
        local_context,
        person_id,
        request: &derived,
        connections,
        command_id,
        conversation_repository,
        run_cancellations,
        task_coordinator,
        mode: prepared.mode,
        session_data_classes: prepared.session.data_classes.clone(),
    };
    drive_turn(&inputs, cancellation, on_admitted, emit).await
}

async fn drive_turn<Keys: VaultKeyProvider + 'static>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    cancellation: floe_execution::Cancellation,
    on_admitted: impl FnMut(&floe_conversation::RunReceipt),
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_conversation::AgentSession, AgentFailure> {
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        optional_context_issues: vec![],
        evidence: vec![],
    };
    Box::pin(expert_dispatch::run(
        inputs,
        context,
        cancellation,
        on_admitted,
        emit,
    ))
    .await
}

/// What one automatic resume attempt decided.
///
/// Suppression is an ordinary outcome, never an error: the person's
/// decision already stands, and a suppressed automatic child never blocks
/// an explicit Continue or a fresh turn.
pub(super) enum AutoResumeOutcome {
    Admitted {
        session: floe_conversation::AgentSession,
        child: floe_conversation::RunReceipt,
    },
    Suppressed(floe_conversation::ResumeSuppression),
}

/// Admit and drive one origin's automatic child, unless suppressed.
///
/// Best-effort by design: the gate pre-reads the origin, its group and
/// the Session revision, then admission re-verifies everything atomically.
/// A lost race (a newer turn, a group flip, a concurrent claim) suppresses
/// with its honest reason instead of erroring; only storage, identity or
/// cancellation failures propagate.
#[allow(clippy::too_many_arguments)]
pub(super) async fn maybe_auto_resume<Keys: VaultKeyProvider + 'static>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextHost,
    task_coordinator: &floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    conversation_repository: &std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    run_cancellations: &std::sync::Arc<floe_conversation::RunCancellationRegistry>,
    connections: &floe_provider_adapters::control::CurrentSavedConnectionStore,
    person_id: PersonId,
    session_id: Uuid,
    origin_run_id: floe_kernel::RunId,
    device_id: &str,
    cancellation: floe_execution::Cancellation,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<AutoResumeOutcome, AgentFailure> {
    use floe_conversation::ResumeSuppression;
    if session_id.is_nil()
        || !origin_run_id.is_valid()
        || device_id.trim() != device_id
        || device_id.is_empty()
    {
        return Err(AgentFailure::InvalidInput);
    }
    let principal = person_id.to_string();
    let Some(origin) = conversation_repository.load_receipt(origin_run_id).await? else {
        return Ok(AutoResumeOutcome::Suppressed(
            ResumeSuppression::OriginNotCompleted,
        ));
    };
    if origin.principal != principal || origin.session_id != session_id {
        return Err(AgentFailure::StorageUnavailable);
    }
    let group = floe_conversation::list_run_interactions(
        conversation_repository.as_ref(),
        &principal,
        origin_run_id,
    )
    .await?;
    let link = match floe_conversation::resume_gate(&origin, &group) {
        Ok(link) => link,
        Err(suppression) => return Ok(AutoResumeOutcome::Suppressed(suppression)),
    };
    let session = vault.load(person_id, session_id).await?;
    if session.revision != origin.session_revision {
        return Ok(AutoResumeOutcome::Suppressed(ResumeSuppression::NewerTurn));
    }
    let command_id = floe_conversation::resume_command_id(origin_run_id)?;
    let mut child = None;
    let drive = run_resume(
        core,
        vault,
        local_context,
        task_coordinator,
        conversation_repository,
        run_cancellations,
        connections,
        person_id,
        command_id,
        &ResumeTurnRequest {
            session_id,
            expected_revision: origin.session_revision,
            device_id: device_id.to_owned(),
            resume: link,
        },
        cancellation,
        |receipt: &floe_conversation::RunReceipt| {
            child = Some(receipt.clone());
        },
        emit,
    )
    .await;
    match drive {
        Ok(driven) => {
            let Some(child) = child else {
                return Err(AgentFailure::StorageUnavailable);
            };
            Ok(AutoResumeOutcome::Admitted {
                session: driven,
                child,
            })
        }
        Err(AgentFailure::Conflict) => {
            // A concurrent event won between the pre-read and admission.
            // Name the honest reason with one fresh read instead of
            // erroring; a conflict with no visible cause stays an error
            // so a genuine restart race can retry.
            let fresh = vault.load(person_id, session_id).await?;
            if fresh.revision != origin.session_revision {
                return Ok(AutoResumeOutcome::Suppressed(ResumeSuppression::NewerTurn));
            }
            let group = floe_conversation::list_run_interactions(
                conversation_repository.as_ref(),
                &principal,
                origin_run_id,
            )
            .await?;
            match floe_conversation::resume_gate(&origin, &group) {
                Ok(_) => Err(AgentFailure::Conflict),
                Err(suppression) => Ok(AutoResumeOutcome::Suppressed(suppression)),
            }
        }
        Err(failure) => Err(failure),
    }
}

/// One automatic child worth attempting, without driving it.
///
/// The gate filters origin and group only. The Session revision is
/// deliberately left to admission CAS: a claimed slot rejoins before any
/// Session comparison (so a retried resolve always recovers its child),
/// while an unclaimed slot at a stale revision conflicts honestly.
/// Best-effort like the gate itself: admission re-verifies everything
/// atomically, so a lost race between this read and the claim still
/// converges (rejoin or honest conflict), never duplicates.
pub(super) struct AutoResumeClaim {
    pub link: floe_conversation::InteractionResumeRef,
    pub expected_revision: u64,
}

/// Evaluate the automatic gate for one origin without driving.
///
/// Returns the claim when the origin and its group currently admit a
/// child; returns `None` on any origin/group suppression. Only storage,
/// identity or cancellation failures propagate.
pub(super) async fn evaluate_auto_resume<Keys: VaultKeyProvider + 'static>(
    conversation_repository: &std::sync::Arc<floe_vault::VaultConversationRepository<Keys>>,
    person_id: PersonId,
    session_id: Uuid,
    origin_run_id: floe_kernel::RunId,
) -> Result<Option<AutoResumeClaim>, AgentFailure> {
    if session_id.is_nil() || !origin_run_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let principal = person_id.to_string();
    let origin = match conversation_repository.load_receipt(origin_run_id).await? {
        Some(origin) => origin,
        None => return Ok(None),
    };
    if origin.principal != principal || origin.session_id != session_id {
        return Err(AgentFailure::StorageUnavailable);
    }
    let group = floe_conversation::list_run_interactions(
        conversation_repository.as_ref(),
        &principal,
        origin_run_id,
    )
    .await?;
    let link = match floe_conversation::resume_gate(&origin, &group) {
        Ok(link) => link,
        Err(_) => return Ok(None),
    };
    Ok(Some(AutoResumeClaim {
        link,
        expected_revision: origin.session_revision,
    }))
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
    let vault = inputs.vault;
    let local_context = inputs.local_context;
    let person_id = inputs.person_id;
    let request = inputs.request;
    let session_id = request.session_id;
    let source_client = ServerSourceClient::from_current_connection(
        inputs.connections,
        &person_id.to_string(),
        &request.device_id,
    )?;
    let remote_reader = match source_client.as_ref() {
        Some(client) => Some(remote_views::RemoteViewReader::new(
            vault,
            client,
            person_id,
            client.source().client_id(),
            client.source().device_id(),
        )),
        None => None,
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
    let calendar_resolver = crate::vault_host::calendar_access::NativeCalendarDependencyResolver {
        core: inputs.core,
        vault,
        person_id,
        device_id: &request.device_id,
    };
    let resolver = CompositeDependencyResolver {
        personal: &personal_resolver,
        remote: remote_resolver
            .as_ref()
            .map(|resolver| resolver as &dyn floe_access::DependencyResolver),
        calendar: Some(&calendar_resolver),
    };
    {
        // Canonical root catalog: Expert cards come from the Experts-owned
        // Directory admitted for this principal, with the registered
        // definition revisions; Manager tools come straight from Context's
        // descriptor definitions. Model placement never filters either.
        let directory_catalog = inputs.task_coordinator.catalog(&person_id.to_string())?;
        let expert_cards = directory_catalog.cards;
        let active_experts: Vec<floe_agent_contract::AgentCard> = expert_cards
            .iter()
            .map(|entry| entry.card.clone())
            .collect();
        let catalog = floe_agent_contract::AllowedCatalog {
            cards: expert_cards,
            tools: floe_context::manager_tool_descriptors(),
            revision: directory_catalog.revision.max(1),
        };
        let budget = AgentBudget::default();
        let duration = std::time::Duration::from_millis(budget.deadline_ms);
        let deadline = tokio::time::Instant::now() + duration;
        let service = floe_conversation::ConversationService::with_run_cancellations(
            std::sync::Arc::clone(inputs.conversation_repository),
            floe_conversation::ManagerConfig {
                role_spec: floe_agent_contract::RoleSpec {
                    role_id: "manager".into(),
                    instructions: floe_conversation::prompts::manager_prompt(
                        context.persona.as_ref(),
                    )?
                    .render(),
                    output_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT.into(),
                },
                purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
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
        let provider = floe_provider_adapters::models::RootModelProvider::from_current_connection(
            inputs.connections,
            &person_id.to_string(),
            &request.device_id,
        )?;
        let admission = floe_provider_adapters::control::SavedConnectionAdmission::new(
            inputs.connections.clone(),
            person_id.to_string(),
            request.device_id.clone(),
        );
        let authority = floe_access::ContextualRecipientAuthority::new(
            vault,
            admission,
            floe_access::SystemConsentClock,
        );
        let model_service = floe_inference::InferenceService::new(provider, resolver, authority);
        // Canonical root projection: Conversation filtering plus Context
        // assembly, reauthorizing history through the same Context dependency
        // authority the model fence uses. Committed turn coverage is the
        // evidence; no recorder side channel participates.
        let evidence_reader = floe_vault::ContextEvidenceReader::new(vault, session_id);
        let projection_port = floe_conversation::ConversationModelProjection::new(
            evidence_reader,
            resolver,
            session_id,
            context.clone(),
            inputs.session_data_classes.clone(),
            active_experts,
        )?;
        // Canonical root tools: Context owns the descriptors and the reads;
        // each successful result returns its dependency coverage directly, and
        // each recoverable blocker publishes under the admitted Tool origin.
        let tool_service = floe_context::ContextToolService::new(
            person_id,
            request.device_id.clone(),
            floe_vault::VaultGrantRecords::new(vault),
            personal_grants::native_driver(local_context),
            remote_reader.as_ref(),
        )?;
        let calendar_subject =
            crate::vault_host::calendar_access::DeviceCalendarSubject { local_context };
        let personal_subject = personal_grants::native_driver(local_context);
        let review_snapshots = crate::vault_host::review_snapshot::HostReviewSnapshots {
            core: inputs.core,
            vault,
            calendar_subject: &calendar_subject,
            personal_subject: &personal_subject,
            capture_deadline: deadline
                .min(tokio::time::Instant::now() + std::time::Duration::from_secs(5)),
        };
        let publishing_tools = interaction_publication::PublishingToolPort::new(
            &tool_service,
            inputs.conversation_repository.as_ref(),
            inputs.conversation_repository.as_ref(),
            &review_snapshots,
            person_id,
            session_id,
            request.device_id.clone(),
        )?;
        let tool_port = &publishing_tools;
        // Canonical root delegation: TaskCoordinator serves as the
        // DelegationPort directly. The Directory resolves the endpoint, and
        // the invocation carries the explicit execution context; App holds
        // no run-id endpoint authority.
        let delegation_port = inputs.task_coordinator;
        let retry_of = request.retry_of;
        let profile = request.profile.clone();
        // The explicit delegation host context, forwarded through
        // Conversation to the Engine without interpretation. Runtime-only:
        // never part of the canonical turn intent or request identity.
        let delegation_context = floe_agent_contract::DelegationExecutionContext {
            session_id,
            device_id: request.device_id.clone(),
            agent_context: context.clone(),
            max_output_bytes: budget.max_output_bytes,
        };
        let receipt = service
            .run_turn_observed(
                floe_conversation::TurnRequest {
                    command_id: inputs.command_id,
                    session_id,
                    expected_session_revision: request.expected_revision,
                    principal: person_id.to_string(),
                    device_id: request.device_id.clone(),
                    now_unix_ms: chrono::Utc::now().timestamp_millis(),
                    prompt: request.text.clone(),
                    mode: inputs.mode.clone(),
                    retry_of,
                    profile,
                    allowed_catalog: catalog,
                    replay: vec![],
                    deadline,
                    cancellation,
                    delegation_context: Some(delegation_context),
                },
                floe_conversation::ConversationPorts {
                    projection: &projection_port,
                    coverage_resolver: projection_port.coverage_resolver(),
                    model: &model_service,
                    tools: tool_port,
                    delegation: delegation_port,
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

#[cfg(test)]
async fn optional_task_views(
    core: &FloeCore,
    person_id: PersonId,
    context: &mut AgentContext,
) -> Result<Vec<NativeContextView>, AgentFailure> {
    let handle = uuid::Uuid::new_v5(&person_id.0, b"floe.tasks");
    let acquired = floe_context::acquire_optional_source(
        floe_agent_contract::ContextSource::Tasks,
        floe_context::task_context_view(
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

#[derive(Clone, Copy)]
struct CompositeDependencyResolver<'a> {
    personal: &'a dyn floe_access::DependencyResolver,
    remote: Option<&'a dyn floe_access::DependencyResolver>,
    calendar: Option<&'a dyn floe_access::DependencyResolver>,
}

impl floe_access::DependencyResolver for CompositeDependencyResolver<'_> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a floe_context_contract::ContextDependency,
        request: &'a floe_access::DependencyAuthorization,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        if matches!(
            dependency.source().connector().as_str(),
            "calendar.event_kit" | "calendar.android"
        ) {
            match self.calendar {
                Some(calendar) => calendar.authorize(dependency, request),
                None => Box::pin(async { Err(AgentFailure::PolicyDenied) }),
            }
        } else if floe_access::is_device_local_source(dependency.source().connector().as_str()) {
            self.personal.authorize(dependency, request)
        } else if let Some(remote) = self.remote {
            remote.authorize(dependency, request)
        } else {
            Box::pin(async { Err(AgentFailure::PolicyDenied) })
        }
    }
}

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
    // Canonical Expert host under test: the shared-Inference executor, policy
    // and source host the delegated endpoints prepare.
    use super::expert_dispatch::{ConversationExperts, RejectStatefulSettlement};
    use super::expert_host::{
        CalendarContextReaderApi, PersonalAttentionReader, PersonalAttentionReaderApi,
        ResultRecorder, StoreResultRecorder, expert_policy,
    };
    use super::interaction_publication::PublishingToolPort;
    use floe_agent_contract::ModelPlacement;
    use floe_agent_contract::{ModelCallOutcome, ModelRequest, ModelResponse};
    use floe_context::AttentionView;
    use floe_conversation::AgentMessage;
    use floe_conversation::{ConversationRepository, InteractionRepository};
    use floe_execution::Cancellation;
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
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            floe_context_contract::SourceReadOutcome<floe_context::SourceRead>,
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
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
                Ok(floe_context_contract::SourceReadOutcome::Ready(
                    floe_context::SourceRead::new(
                        request.source().clone(),
                        value,
                        dependency,
                        scope,
                    ),
                ))
            })
        }
    }

    impl CalendarContextReaderApi for FixtureRemoteReader<'_> {
        fn read<'a>(
            &'a self,
            person_id: PersonId,
            consumer: &'a str,
            query: &'a floe_context_contract::CalendarViewQuery,
            deadline: tokio::time::Instant,
            cancellation: &'a Cancellation,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            floe_context_contract::SourceReadOutcome<
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
            Box::pin(async move {
                if person_id != self.person_id {
                    return Err(AgentFailure::CapabilityDenied);
                }
                let connections = self
                    .source_client
                    .observe_calendar_connections(deadline, cancellation)
                    .await?;
                let mut reads = Vec::new();
                for connection in connections {
                    let view = match self
                        .source_client
                        .read_calendar_context_view(
                            floe_provider_adapters::sources::server::CalendarContextRequest {
                                connector_id: &connection.connector_id,
                                connection_id: &connection.connection_id,
                                connection_revision: connection.connection_revision,
                                range_start_unix_ms: query.range_start_unix_ms(),
                                range_end_unix_ms: query.range_end_unix_ms(),
                                cursor: query.cursor().unwrap_or(""),
                                limit: query.limit(),
                            },
                            deadline,
                            cancellation,
                        )
                        .await
                    {
                        Ok(view) => view,
                        Err(AgentFailure::CapabilityUnavailable) => {
                            return Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                                floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
                            ));
                        }
                        Err(error) => return Err(error),
                    };
                    let source = floe_context_contract::GrantSourceBinding::try_new(
                        person_id,
                        floe_context_contract::ConnectionId::try_new(&connection.connection_id)
                            .map_err(|_| AgentFailure::InvalidInput)?,
                        floe_context_contract::ConnectorId::try_new(&connection.connector_id)
                            .map_err(|_| AgentFailure::InvalidInput)?,
                        floe_context_contract::ExecutionOwnerId::try_new(
                            "00000000-0000-4000-8000-000000000098",
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?,
                        floe_context_contract::SourceAuthority::new(),
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?;
                    let resource = floe_context_contract::ResourceHandle::try_new(format!(
                        "calendar.timeline:{}",
                        connection.connection_id
                    ))
                    .map_err(|_| AgentFailure::InvalidInput)?;
                    let now = chrono::Utc::now();
                    let dependency = floe_context_contract::ContextDependency::try_new(
                        person_id,
                        floe_context_contract::GrantId::new(),
                        floe_context_contract::GrantAuthority::new(),
                        source,
                        vec![resource],
                        vec![floe_context_contract::GrantDataCategory::Derived],
                        floe_context_contract::GrantOperation::Read,
                        floe_context_contract::GrantPurpose::Assistant,
                        floe_context_contract::GrantConsumer::builtin(consumer)
                            .map_err(|_| AgentFailure::InvalidInput)?,
                        floe_context_contract::ProcessingRestriction::ApprovedRecipient {
                            recipient: "server-audience".into(),
                            categories: vec![floe_context_contract::GrantDataCategory::Derived],
                        },
                        floe_context_contract::ConsumerPolicyAuthority::new(),
                        Uuid::new_v4(),
                        serde_json::to_vec(query).map_err(|_| AgentFailure::InvalidInput)?,
                        Uuid::new_v4(),
                        Uuid::new_v4(),
                        now,
                        now + chrono::Duration::minutes(5),
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?;
                    reads.push((view, dependency));
                }
                Ok(floe_context_contract::SourceReadOutcome::Ready(reads))
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

    /// A canned canonical Inference executor: records each domain call with
    /// its execution constraint and answers the queued texts in order.
    struct CannedExpertExecutor {
        calls: Mutex<
            Vec<(
                floe_agent_contract::ModelRequest,
                floe_inference::InferenceExecutionConstraint,
            )>,
        >,
        answers: Mutex<std::collections::VecDeque<String>>,
    }

    impl CannedExpertExecutor {
        fn answering(answers: Vec<serde_json::Value>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                answers: Mutex::new(
                    answers
                        .into_iter()
                        .map(|answer| answer.to_string())
                        .collect(),
                ),
            }
        }

        fn calls(
            &self,
        ) -> Vec<(
            floe_agent_contract::ModelRequest,
            floe_inference::InferenceExecutionConstraint,
        )> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl floe_inference::InferenceExecutor for CannedExpertExecutor {
        fn execute<'a>(
            &'a self,
            request: floe_agent_contract::ModelRequest,
            _scope: &'a floe_execution::ExecutionScope,
            constraint: floe_inference::InferenceExecutionConstraint,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ModelCallOutcome, AgentFailure>,
        > {
            self.calls
                .lock()
                .unwrap()
                .push((request.clone(), constraint));
            let answer = self
                .answers
                .lock()
                .unwrap()
                .pop_front()
                .expect("canned expert answer");
            Box::pin(async move {
                Ok(floe_agent_contract::ModelCallOutcome::Ready(
                    floe_agent_contract::ModelResponse {
                        attempt_id: request.attempt_id,
                        steps: vec![floe_agent_contract::ModelStep::Answer {
                            text: answer,
                            artifacts: vec![],
                        }],
                        usage: floe_agent_contract::ModelUsage {
                            tokens: 64,
                            cost_micros: 0,
                        },
                    },
                ))
            })
        }
    }

    /// A generous Task scope for delegated Expert tests: the Expert bounds
    /// its own child, so the parent only needs to outlive the message.
    fn expert_scope() -> floe_execution::ExecutionScope {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(1_000_000, 1_000_000_000),
            Default::default(),
        );
        floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()),
        )
    }

    fn tool_scope() -> floe_execution::ExecutionScope {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(uuid::Uuid::new_v4()),
        )
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

    impl floe_agent_contract::ModelPort for OptionalSourceModel {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a floe_execution::ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
            Box::pin(async move {
                let context = &request.projection.envelope.contextual_data;
                assert!(context.memories.is_empty());
                assert_eq!(context.optional_context_issues.len(), 1);
                assert_eq!(context.optional_context_issues[0].source, self.source);
                let asks_memory = request.projection.envelope.conversation.current_turn.iter().any(|message| {
                    matches!(message, floe_agent_contract::ModelConversationEntry::User { text, .. } if text == "What do you remember about me?")
                });
                let attempt_id = request.attempt_id;
                self.requests.lock().unwrap().push(request);
                Ok(ModelCallOutcome::Ready(ModelResponse {
                    attempt_id,
                    steps: vec![floe_agent_contract::ModelStep::Answer {
                        text: if asks_memory {
                            "Saved memory is unavailable; I cannot inspect it right now."
                        } else {
                            "Hello! How can I help?"
                        }
                        .into(),
                        artifacts: vec![],
                    }],
                    usage: floe_agent_contract::ModelUsage {
                        tokens: 1,
                        cost_micros: 0,
                    },
                }))
            })
        }
    }

    struct NoDispatch;

    impl floe_agent_contract::ToolPort for NoDispatch {
        fn invoke<'a>(
            &'a self,
            _: floe_agent_contract::ToolCall,
            _: &'a floe_execution::ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<floe_agent_contract::ToolResult, AgentFailure>>
        {
            Box::pin(async { panic!("No tool is admitted in this test") })
        }
    }

    impl floe_agent_contract::DelegationPort for NoDispatch {
        fn delegate<'a>(
            &'a self,
            _: floe_agent_contract::DelegationRequest,
            _: &'a floe_execution::ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::TaskReceipt, AgentFailure>,
        > {
            Box::pin(async { panic!("No delegation is admitted in this test") })
        }
    }

    async fn run_optional_source_turn(
        vault: &Arc<EncryptedAgentVault<AttentionTestKeys>>,
        session: &floe_conversation::AgentSession,
        context: AgentContext,
        model: &OptionalSourceModel,
        text: &str,
    ) -> floe_conversation::AgentSession {
        let person_id = session.person_id;
        let local_context = LocalContextHost::default();
        let resolver = personal_grants::PersonalDependencyResolver {
            vault: vault.as_ref(),
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let projection = floe_conversation::ConversationModelProjection::new(
            floe_vault::ContextEvidenceReader::new(vault.as_ref(), session.id),
            resolver,
            session.id,
            context,
            session.data_classes.clone(),
            vec![],
        )
        .unwrap();
        let repository = Arc::new(floe_vault::VaultConversationRepository::new(Arc::clone(
            vault,
        )));
        let service = floe_conversation::ConversationService::new(
            repository,
            floe_conversation::ManagerConfig {
                role_spec: floe_agent_contract::RoleSpec {
                    role_id: "manager".into(),
                    instructions: "Answer safely.".into(),
                    output_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT.into(),
                },
                purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
                max_iterations: 4,
                max_output_bytes: 16384,
                max_run_duration: std::time::Duration::from_secs(10),
                budget: floe_execution::budget::BudgetConfig::new(16384, 1000000),
            },
        )
        .unwrap();
        let receipt = service
            .run_turn(
                floe_conversation::TurnRequest {
                    command_id: floe_agent_contract::CommandId::new(),
                    session_id: session.id,
                    expected_session_revision: session.revision,
                    principal: person_id.to_string(),
                    device_id: "test-device".into(),
                    now_unix_ms: 1_700_000_000_000,
                    prompt: text.into(),
                    mode: floe_conversation::TurnMode::New,
                    retry_of: None,
                    profile: floe_conversation::ProfileSelection::Auto,
                    allowed_catalog: Default::default(),
                    replay: vec![],
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                    cancellation: Cancellation::default(),
                    delegation_context: None,
                },
                floe_conversation::ConversationPorts {
                    projection: &projection,
                    coverage_resolver: projection.coverage_resolver(),
                    model,
                    tools: &NoDispatch,
                    delegation: &NoDispatch,
                    validator: &engine_ports::ManagerPayloadValidator,
                },
            )
            .await
            .unwrap();
        assert_eq!(receipt.state, floe_conversation::RunState::Completed);
        vault.load(person_id, session.id).await.unwrap()
    }

    #[tokio::test]
    async fn optional_memory_failure_preserves_conversation_and_integrity_fence() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault = Arc::new(
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap(),
        );
        vault.activate_conversation_executor().await.unwrap();
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
                let completed =
                    run_optional_source_turn(&vault, &session, context, &model, text).await;
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
        let vault = Arc::new(
            EncryptedAgentVault::create(&vault_root, person_id, AttentionTestKeys::default())
                .await
                .unwrap(),
        );
        vault.activate_conversation_executor().await.unwrap();
        let session = vault.create_session().await.unwrap();
        let mut context = conversation_context(vault.as_ref()).await.unwrap();
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
        let completed = run_optional_source_turn(&vault, &session, context, &model, "Hello").await;
        assert_eq!(
            completed.last_outcome,
            Some(floe_conversation::AgentOutcome::Completed)
        );
        assert_eq!(vault.load(person_id, session.id).await.unwrap(), completed);
        assert_eq!(model.requests.lock().unwrap().len(), 1);
    }

    const COMMITMENTS_AGENT_ID: &str = BuiltinExpertKind::Commitments.package_id();
    const COMMUNICATION_AGENT_ID: &str = BuiltinExpertKind::Communication.package_id();
    const WORK_CONTEXT_AGENT_ID: &str = BuiltinExpertKind::WorkContext.package_id();
    const LIFE_LOGISTICS_AGENT_ID: &str = BuiltinExpertKind::LifeLogistics.package_id();
    const RELATIONSHIPS_AGENT_ID: &str = BuiltinExpertKind::Relationships.package_id();
    const FOCUS_AGENT_ID: &str = BuiltinExpertKind::FocusAttention.package_id();
    const WELLBEING_AGENT_ID: &str = BuiltinExpertKind::Wellbeing.package_id();

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
            supported_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
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
                    settlement: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn schedule_delegation_uses_registered_task_runner() {
        let executor = CannedExpertExecutor::answering(vec![]);
        let scope = expert_scope();
        let policy = expert_policy();
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let person_id = PersonId::new();
        let runner = FixtureScheduleRunner {
            calls: AtomicUsize::new(0),
        };
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(true, true).await,
            source_client: None,
            calendar_reader: None,
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
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
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[(
                floe_experts_builtin::BuiltinExpertKind::Schedule.package_id(),
                &runner,
            )],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let task_id = Uuid::new_v4();
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_inference::UsageLedger::default(),
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

    fn saved_server_connection(
        base_url: &str,
        person_id: PersonId,
        device_id: &str,
    ) -> floe_inference::SavedServerConnection {
        floe_inference::SavedServerConnection {
            base_url: base_url.into(),
            token: "t".repeat(32),
            client_id: "test-client".into(),
            person_id: person_id.to_string(),
            device_id: device_id.into(),
        }
    }

    /// A legacy Expert Server model, admitted for a fixture caller.
    /// The caller identity only has to be self-consistent here.
    fn legacy_source_client(base_url: &str) -> ServerSourceClient {
        let person_id = PersonId::new();
        ServerSourceClient::from_current_connection(
            &floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
                saved_server_connection(base_url, person_id, "test-device"),
            )),
            &person_id.to_string(),
            "test-device",
        )
        .unwrap()
        .unwrap()
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
            .execute(
                person_id,
                LocalContextCommand::RegisterAttentionHost {
                    host_epoch: host_epoch.clone(),
                },
                None,
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
                consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
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
                consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
                change: floe_access::PersonalAccessChange::Review {
                    expected_native_subject_fingerprint: subject.clone(),
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
        let turn_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let outcome = reader
            .read(
                person_id,
                floe_access::ATTENTION_ASSISTANT_CONSUMER,
                call_id,
                turn_id,
                tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                &Cancellation::default(),
            )
            .await
            .unwrap();
        let floe_context_contract::SourceReadOutcome::Ready((attention_view, attention_dependency)) =
            outcome
        else {
            panic!("admitted attention read must stay ready");
        };
        recorder
            .record(turn_id, call_id, attention_dependency)
            .unwrap();
        let capability_output = serde_json::to_string(&attention_view).unwrap();
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
        let authorization = floe_context::DependencyAuthorization {
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        floe_context::revalidate_turn_coverage(
            store.committed_turn_coverage(turn_id).await.unwrap(),
            &resolver,
            &authorization,
        )
        .await
        .unwrap();

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
        let executor = CannedExpertExecutor::answering(vec![]);
        let scope = expert_scope();
        let policy = expert_policy();
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let person_id = PersonId::new();
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(false, true).await,
            source_client: None,
            calendar_reader: None,
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            wellbeing_reader: None,
            recorder: None,
            remote_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_inference::UsageLedger::default(),
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
    async fn expert_offering_exposes_only_bounded_context_observe_capabilities() {
        let executor = CannedExpertExecutor::answering(vec![]);
        let scope = expert_scope();
        let policy = expert_policy();
        // Canonical Manager tools come from Context, identically for the
        // device and server model choices: availability never depends on
        // model route or host consent.
        let device_catalog = floe_context::manager_tool_descriptors();
        let server_catalog = floe_context::manager_tool_descriptors();
        assert_eq!(device_catalog, server_catalog);
        assert_eq!(device_catalog.len(), 7);
        assert_eq!(
            device_catalog
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
        for descriptor in &device_catalog {
            descriptor.validate().unwrap();
            assert_eq!(
                descriptor.definition_revision,
                floe_context::MANAGER_TOOL_DEFINITION_REVISION
            );
            let schema: serde_json::Value = serde_json::from_str(&descriptor.input_schema).unwrap();
            assert_eq!(schema["additionalProperties"], false);
            assert!(descriptor.input_schema.len() < 1024);
        }
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(true, true).await,
            source_client: None,
            calendar_reader: None,
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
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
        let executor = CannedExpertExecutor::answering(vec![]);
        let scope = expert_scope();
        let policy = expert_policy();
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(true, true).await,
            source_client: None,
            calendar_reader: None,
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: vec![],
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_inference::UsageLedger::default(),
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
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let local_context = LocalContextHost::default();
        // No remote reader: IO is impossible, so an unknown `authority` field
        // must fail input validation rather than reach any transport.
        let tools = floe_context::ContextToolService::new(
            person_id,
            "test-device",
            floe_vault::VaultGrantRecords::new(&vault),
            personal_grants::native_driver(&local_context),
            None::<&remote_views::RemoteViewReader<AttentionTestKeys>>,
        )
        .unwrap();
        let result = tools
            .invoke_outcome(
                &floe_agent_contract::ToolCall {
                    call_id: uuid::Uuid::new_v4(),
                    invocation_key: floe_agent_contract::InvocationKey::new(),
                    tool_id: "mail.communication.read".into(),
                    definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
                    input: r#"{"query":"reply","authority":"send"}"#.into(),
                },
                &tool_scope(),
            )
            .await;
        assert_eq!(result.err(), Some(AgentFailure::InvalidInput));
    }

    #[tokio::test]
    async fn context_tool_service_requires_reviewed_personal_grants() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let local_context = LocalContextHost::default();
        // A fresh vault holds no reviewed grants: every personal tool fails
        // closed without consulting any model route.
        let tools = floe_context::ContextToolService::new(
            person_id,
            "test-device",
            floe_vault::VaultGrantRecords::new(&vault),
            personal_grants::native_driver(&local_context),
            None::<&remote_views::RemoteViewReader<AttentionTestKeys>>,
        )
        .unwrap();
        let scope = tool_scope();
        for tool_id in [
            "people.identity.read",
            "schedule.feasibility.read",
            "attention.coarse.read",
            "wellbeing.derived.read",
        ] {
            let outcome = tools
                .invoke_outcome(
                    &floe_agent_contract::ToolCall {
                        call_id: uuid::Uuid::new_v4(),
                        invocation_key: floe_agent_contract::InvocationKey::new(),
                        tool_id: tool_id.into(),
                        definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
                        input: "{}".into(),
                    },
                    &scope,
                )
                .await
                .unwrap();
            let floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) = outcome
            else {
                panic!("{tool_id} without grants must block, not fail");
            };
            assert_eq!(blockers.blockers().len(), 1, "{tool_id}");
            assert_eq!(
                blockers.blockers()[0].reason(),
                floe_context_contract::SourceAccessRequirementKind::EnableObserve,
                "{tool_id}"
            );
        }
        for (tool_id, input) in [
            ("mail.communication.read", r#"{"query":"x"}"#),
            ("work.context.read", "{}"),
            ("life.logistics.read", "{}"),
        ] {
            let outcome = tools
                .invoke_outcome(
                    &floe_agent_contract::ToolCall {
                        call_id: uuid::Uuid::new_v4(),
                        invocation_key: floe_agent_contract::InvocationKey::new(),
                        tool_id: tool_id.into(),
                        definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
                        input: input.into(),
                    },
                    &scope,
                )
                .await
                .unwrap();
            let floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) = outcome
            else {
                panic!("{tool_id} without a reader must block, not fail");
            };
            assert_eq!(blockers.blockers().len(), 1, "{tool_id}");
            assert_eq!(
                blockers.blockers()[0].reason(),
                floe_context_contract::SourceAccessRequirementKind::SelectResource,
                "{tool_id}"
            );
        }
    }

    #[tokio::test]
    async fn composed_blocked_tool_publishes_a_durable_interaction() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault = std::sync::Arc::new(
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap(),
        );
        vault.activate_conversation_executor().await.unwrap();
        let repository = std::sync::Arc::new(floe_vault::VaultConversationRepository::new(
            std::sync::Arc::clone(&vault),
        ));
        let started = floe_conversation::start_session(
            repository.as_ref(),
            floe_conversation::SessionRequest {
                principal: person_id.to_string(),
            },
        )
        .await
        .unwrap();
        let run_id = floe_kernel::RunId::new();
        let command_id = floe_agent_contract::CommandId::new();
        repository
            .admit_turn(floe_conversation::TurnAdmissionRequest {
                run_id,
                command_id,
                session_id: started.session_id,
                expected_session_revision: 0,
                principal: person_id.to_string(),
                request_digest: [7; 32],
                mode: floe_conversation::TurnMode::New,
                retry_of: None,
                profile: floe_conversation::ProfileSelection::Auto,
                user_message: floe_agent_contract::AgentMessage {
                    message_id: command_id.as_uuid(),
                    role: floe_agent_contract::MessageRole::User,
                    text: "summarize my day".into(),
                    call_id: None,
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                },
            })
            .await
            .unwrap();

        let call = floe_agent_contract::ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: floe_agent_contract::InvocationKey::new(),
            tool_id: "attention.coarse.read".into(),
            definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
            input: "{}".into(),
        };
        repository
            .journal(run_id)
            .unwrap()
            .record_intent(floe_agent_contract::JournalEvent::ToolIntent { call: call.clone() })
            .await
            .unwrap();

        let local_context = LocalContextHost::default();
        let tools = floe_context::ContextToolService::new(
            person_id,
            "test-device".to_owned(),
            floe_vault::VaultGrantRecords::new(vault.as_ref()),
            personal_grants::native_driver(&local_context),
            None::<&remote_views::RemoteViewReader<'_, AttentionTestKeys>>,
        )
        .unwrap();
        let core = FloeCore::open(":memory:").await.unwrap();
        let calendar_subject =
            crate::vault_host::review_snapshot::fixtures::FixtureCalendarSubject {
                fingerprint: "a".repeat(64),
            };
        let personal_subject =
            crate::vault_host::review_snapshot::fixtures::FixturePersonalInspector {
                fingerprint: "b".repeat(64),
            };
        let snapshots = crate::vault_host::review_snapshot::HostReviewSnapshots {
            core: &core,
            vault: vault.as_ref(),
            calendar_subject: &calendar_subject,
            personal_subject: &personal_subject,
            capture_deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        };
        let port = PublishingToolPort::new(
            &tools,
            repository.as_ref(),
            repository.as_ref(),
            &snapshots,
            person_id,
            started.session_id,
            "test-device".into(),
        )
        .unwrap();
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()).with_run_id(run_id),
        );
        // A fresh vault holds no grants: the real read blocks, the boundary
        // publishes, and the settled result carries the durable ref.
        let result = floe_agent_contract::ToolPort::invoke(&port, call.clone(), &scope)
            .await
            .unwrap();
        assert_eq!(result.artifacts.len(), 1);
        let data = match &result.artifacts[0].parts[0] {
            floe_agent_contract::ArtifactPart::Data { data, .. } => data.clone(),
            _ => panic!("blocked result must carry a ref part"),
        };
        let reference: floe_agent_contract::UserInteractionRef =
            serde_json::from_str(&data).unwrap();
        let stored = repository
            .get_interaction(person_id, reference.interaction_id)
            .await
            .unwrap()
            .expect("blocked call must publish a durable interaction");
        assert_eq!(stored.state, floe_conversation::InteractionState::Pending);
        assert_eq!(
            stored.origin,
            floe_conversation::InteractionOrigin::Tool {
                call_id: call.call_id
            }
        );
        assert!(matches!(
            stored.target,
            floe_conversation::ReviewedTarget::InlineObserve(_)
        ));

        // Replaying the blocked call replays the same durable interaction.
        let again = floe_agent_contract::ToolPort::invoke(&port, call.clone(), &scope)
            .await
            .unwrap();
        let again_data = match &again.artifacts[0].parts[0] {
            floe_agent_contract::ArtifactPart::Data { data, .. } => data.clone(),
            _ => panic!("blocked result must carry a ref part"),
        };
        let again_ref: floe_agent_contract::UserInteractionRef =
            serde_json::from_str(&again_data).unwrap();
        assert_eq!(again_ref.interaction_id, reference.interaction_id);

        // Finishing the run projects the durable ref as an Interaction
        // message in a usable completed turn.
        let finished = repository
            .finish_run(
                run_id,
                1,
                floe_conversation::RunTerminal {
                    state: floe_conversation::RunState::Completed,
                    output: Some("Attention access needs your review.".into()),
                    steps: vec![
                        floe_agent_contract::EngineStep::Tool(result),
                        floe_agent_contract::EngineStep::Answer {
                            text: "Attention access needs your review.".into(),
                            artifacts: vec![],
                        },
                    ],
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                    issue: None,
                    interactions: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(finished.state, floe_conversation::RunState::Completed);
        let session = vault.load(person_id, started.session_id).await.unwrap();
        assert!(
            session.messages.iter().any(|message| matches!(
                message,
                AgentMessage::Interaction {
                    interaction_id,
                    ..
                } if *interaction_id == reference.interaction_id
            )),
            "completed turn must carry the Interaction message"
        );
    }

    struct StaticSourceReader {
        payload: serde_json::Value,
    }

    impl floe_context::SourceReader for StaticSourceReader {
        fn read<'a>(
            &'a self,
            request: &'a floe_context::SourceReadRequest,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            floe_context_contract::SourceReadOutcome<floe_context::SourceRead>,
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let person_id = request.person_id();
                let source = floe_context_contract::GrantSourceBinding::try_new(
                    person_id,
                    floe_context_contract::ConnectionId::try_new("connection").unwrap(),
                    floe_context_contract::ConnectorId::try_new("connector").unwrap(),
                    floe_context_contract::ExecutionOwnerId::try_new("owner").unwrap(),
                    floe_context_contract::SourceAuthority::new(),
                )
                .unwrap();
                let now = chrono::Utc::now();
                let consumer = request.consumer().clone();
                let processing = floe_context_contract::ProcessingRestriction::ApprovedRecipient {
                    recipient: "gateway-local".into(),
                    categories: vec![floe_context_contract::GrantDataCategory::Metadata],
                };
                let dependency = floe_context_contract::ContextDependency::try_new(
                    person_id,
                    floe_context_contract::GrantId::new(),
                    floe_context_contract::GrantAuthority::new(),
                    source,
                    vec![floe_context_contract::ResourceHandle::try_new("resource").unwrap()],
                    vec![floe_context_contract::GrantDataCategory::Metadata],
                    floe_context_contract::GrantOperation::Read,
                    request.purpose(),
                    consumer.clone(),
                    processing.clone(),
                    floe_context_contract::ConsumerPolicyAuthority::new(),
                    uuid::Uuid::new_v4(),
                    request.query_fingerprint().to_vec(),
                    uuid::Uuid::new_v4(),
                    request.process_incarnation_id(),
                    now - chrono::Duration::minutes(1),
                    now + chrono::Duration::minutes(5),
                )
                .unwrap();
                let scope = floe_access::GrantScope::try_new(
                    vec![floe_context_contract::ResourceHandle::try_new("resource").unwrap()],
                    vec![floe_context_contract::GrantDataCategory::Metadata],
                    vec![floe_context_contract::GrantOperation::Read],
                    vec![request.purpose()],
                    vec![consumer],
                    processing,
                )
                .unwrap();
                Ok(floe_context_contract::SourceReadOutcome::Ready(
                    floe_context::SourceRead::new(
                        request.source().clone(),
                        self.payload.clone(),
                        dependency,
                        scope,
                    ),
                ))
            })
        }
    }

    #[tokio::test]
    async fn context_tool_service_admits_remote_observations_with_direct_coverage() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let local_context = LocalContextHost::default();
        let remote = StaticSourceReader {
            payload: serde_json::json!({"hits": ["m1"]}),
        };
        let tools = floe_context::ContextToolService::new(
            person_id,
            "test-device",
            floe_vault::VaultGrantRecords::new(&vault),
            personal_grants::native_driver(&local_context),
            Some(remote),
        )
        .unwrap();
        let scope = tool_scope();
        for (tool_id, input) in [
            ("mail.communication.read", r#"{"query":"invoice"}"#),
            ("work.context.read", "{}"),
            ("life.logistics.read", "{}"),
        ] {
            let call = floe_agent_contract::ToolCall {
                call_id: uuid::Uuid::new_v4(),
                invocation_key: floe_agent_contract::InvocationKey::new(),
                tool_id: tool_id.into(),
                definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
                input: input.into(),
            };
            let outcome = tools.invoke_outcome(&call, &scope).await.unwrap();
            let floe_context_contract::SourceReadOutcome::Ready(result) = outcome else {
                panic!("{tool_id} must stay ready");
            };
            assert_eq!(result.call_id, call.call_id);
            assert!(result.text.contains("m1"), "{tool_id}: {}", result.text);
            assert!(result.artifacts.is_empty());
            // Direct coverage from the service: no recorder side channel.
            match &result.coverage {
                floe_agent_contract::DependencyCoverage::Dependent { dependencies } => {
                    assert_eq!(dependencies.len(), 1);
                    assert_eq!(dependencies[0].person_id(), person_id);
                    assert!(matches!(
                        dependencies[0].processing(),
                        floe_context_contract::ProcessingRestriction::ApprovedRecipient { .. }
                    ));
                }
                coverage => panic!("{tool_id} must return dependent coverage: {coverage:?}"),
            }
        }
    }

    #[tokio::test]
    async fn expired_remote_dependencies_are_rejected_before_io() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let source_client = legacy_source_client("http://127.0.0.1:1");
        let reader = remote_views::RemoteViewReader::new(
            &vault,
            &source_client,
            person_id,
            "client",
            "test-device",
        );
        let resolver = remote_views::RemoteDependencyResolver { reader: &reader };
        let source = floe_context_contract::GrantSourceBinding::try_new(
            person_id,
            floe_context_contract::ConnectionId::try_new("connection").unwrap(),
            floe_context_contract::ConnectorId::try_new("gmail").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("owner").unwrap(),
            floe_context_contract::SourceAuthority::new(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        let expired = floe_context_contract::ContextDependency::try_new(
            person_id,
            floe_context_contract::GrantId::new(),
            floe_context_contract::GrantAuthority::new(),
            source,
            vec![floe_context_contract::ResourceHandle::try_new("resource").unwrap()],
            vec![floe_context_contract::GrantDataCategory::Metadata],
            floe_context_contract::GrantOperation::Read,
            floe_context_contract::GrantPurpose::Assistant,
            floe_context_contract::GrantConsumer::builtin("assistant").unwrap(),
            floe_context_contract::ProcessingRestriction::ApprovedRecipient {
                recipient: "gateway-local".into(),
                categories: vec![floe_context_contract::GrantDataCategory::Metadata],
            },
            floe_context_contract::ConsumerPolicyAuthority::new(),
            uuid::Uuid::new_v4(),
            b"fingerprint".to_vec(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            now - chrono::Duration::minutes(10),
            now - chrono::Duration::minutes(1),
        )
        .unwrap();
        let authorization = floe_access::DependencyAuthorization {
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        assert_eq!(
            floe_access::DependencyResolver::authorize(&resolver, &expired, &authorization).await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn canonical_root_projection_composes_vault_evidence_and_session_classes() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let vault =
            EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                .await
                .unwrap();
        let session = vault.create_session().await.unwrap();
        let local_context = LocalContextHost::default();
        // The exact root composition shape: vault evidence plus the composite
        // Context dependency authority the model fence uses.
        let personal_resolver = personal_grants::PersonalDependencyResolver {
            vault: &vault,
            local_context: &local_context,
            person_id,
            device_id: "test-device",
        };
        let resolver = CompositeDependencyResolver {
            personal: &personal_resolver,
            remote: None,
            calendar: None,
        };
        let projector = floe_conversation::ConversationModelProjection::new(
            floe_vault::ContextEvidenceReader::new(&vault, session.id),
            resolver,
            session.id,
            AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            vec![DataClass::Personal],
            vec![],
        )
        .unwrap();
        let projection = floe_agent_contract::ModelProjectionPort::project(
            &projector,
            floe_agent_contract::ModelProjectionRequest {
                principal: person_id.to_string(),
                role: floe_agent_contract::RoleSpec {
                    role_id: "manager".into(),
                    instructions: "Answer.".into(),
                    output_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT.into(),
                },
                conversation: floe_agent_contract::ModelConversation {
                    history: vec![],
                    current_turn: vec![floe_agent_contract::ModelConversationEntry::User {
                        message_id: uuid::Uuid::new_v4(),
                        text: "hello".into(),
                    }],
                },
                catalog: floe_agent_contract::AllowedCatalog {
                    cards: vec![],
                    tools: floe_context::manager_tool_descriptors(),
                    revision: 1,
                },
                max_output_bytes: 4096,
                correction: None,
            },
            &tool_scope(),
        )
        .await
        .unwrap();
        assert_eq!(
            projection.envelope.scoped_instructions.purpose,
            floe_inference::CANONICAL_MODEL_PURPOSE
        );
        assert_eq!(projection.input_data_classes, vec![DataClass::Personal]);
        assert_eq!(
            projection.coverage,
            floe_agent_contract::DependencyCoverage::Independent
        );
        assert_eq!(
            projection
                .envelope
                .scoped_instructions
                .available_capabilities
                .len(),
            7
        );
    }

    #[tokio::test]
    async fn commitments_delegation_reads_fresh_view_and_returns_typed_artifact() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let person_id = PersonId::new();
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
            let (catalog_request, socket) = request(socket).await;
            assert!(catalog_request.starts_with("GET /v1/connectors "));
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "person_id": person_id.to_string(),
                    "device_id": "test-device",
                    "connectors": [{
                        "id": "calendar.google",
                        "status": "connected",
                        "connection_id": "00000000-0000-4000-8000-000000000010",
                        "connection_revision": 7
                    }]
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (calendar_request, socket) = request(socket).await;
            assert_calendar_request_contract(&calendar_request);
            respond_not_found(socket).await;
        });
        let answer = serde_json::json!({
            "summary": "A reply and Friday commitment are requested.",
            "findings": [{
                "evidence_handle": "mail:request",
                "kind": "request_to_user",
                "statement": "Confirm the review by Friday.",
                "epistemic_status": "observed",
                "confidence_millis": 1000
            }]
        });
        let executor = CannedExpertExecutor::answering(vec![answer]);
        let scope = expert_scope();
        let source_client = ServerSourceClient::from_current_connection(
            &floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
                saved_server_connection(&format!("http://{address}"), person_id, "test-device"),
            )),
            &person_id.to_string(),
            "test-device",
        )
        .unwrap()
        .unwrap();
        let remote_reader = FixtureRemoteReader {
            source_client: &source_client,
            person_id,
        };
        let policy = expert_policy();
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let recorder = FixtureResultRecorder;
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(false, true).await,
            source_client: Some(&source_client),
            calendar_reader: Some(&remote_reader),
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: Some(&recorder),
            remote_reader: Some(&remote_reader),
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let task_id = uuid::Uuid::new_v4();
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_inference::UsageLedger::default(),
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
        // The Expert asked for remote execution through shared Inference with
        // the exact source coverage it read; the catalog stays empty because
        // answering Experts declare no capabilities.
        let calls = executor.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].0.purpose,
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE
        );
        assert_eq!(
            calls[0].0.consumer,
            floe_agent_contract::EXPERT_INFERENCE_CONSUMER
        );
        assert_eq!(
            calls[0].1,
            floe_inference::InferenceExecutionConstraint::RemoteOnly
        );
        assert!(matches!(
            calls[0].0.projection.coverage,
            floe_agent_contract::DependencyCoverage::Dependent { .. }
        ));
        assert!(calls[0].0.catalog.tools.is_empty());
        let envelope = serde_json::to_string(&calls[0].0.projection.envelope).unwrap();
        assert!(envelope.contains("Commitments Expert"));
        assert!(envelope.contains("Confirm by Friday"));
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
            let (catalog_request, socket) = request(socket).await;
            assert!(catalog_request.starts_with("GET /v1/connectors "));
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "person_id": person_id.to_string(),
                    "device_id": "test-device",
                    "connectors": [{
                        "id": "calendar.google",
                        "status": "connected",
                        "connection_id": "00000000-0000-4000-8000-000000000010",
                        "connection_revision": 7
                    }]
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (calendar_request, socket) = request(socket).await;
            assert_calendar_request_contract(&calendar_request);
            let calendar_query: serde_json::Value =
                serde_json::from_str(calendar_request.split_once("\r\n\r\n").unwrap().1).unwrap();
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
                        "range_start_unix_ms": calendar_query["range_start_unix_ms"],
                        "range_end_unix_ms": calendar_query["range_end_unix_ms"],
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
        });
        let answer = serde_json::json!({
            "summary": "Four bounded sources support the delivery commitment.",
            "findings": [
                {"evidence_handle":"mail:request","kind":"request_to_user","statement":"A reply was requested.","epistemic_status":"observed","confidence_millis":1000},
                {"evidence_handle":"calendar:review","kind":"user_commitment","statement":"A review is scheduled.","epistemic_status":"observed","confidence_millis":1000},
                {"evidence_handle":task_id.to_string(),"kind":"user_commitment","statement":"A task remains open.","epistemic_status":"observed","confidence_millis":1000},
                {"evidence_handle":memory_id.to_string(),"kind":"user_commitment","statement":"The delivery was confirmed.","epistemic_status":"observed","confidence_millis":1000}
            ]
        });
        let executor = CannedExpertExecutor::answering(vec![answer]);
        let scope = expert_scope();
        let source_client = ServerSourceClient::from_current_connection(
            &floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(Some(
                saved_server_connection(&format!("http://{address}"), person_id, "test-device"),
            )),
            &person_id.to_string(),
            "test-device",
        )
        .unwrap()
        .unwrap();
        let policy = expert_policy();
        let remote_reader = FixtureRemoteReader {
            source_client: &source_client,
            person_id,
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
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(false, true).await,
            source_client: Some(&source_client),
            calendar_reader: Some(&remote_reader),
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: Some(&recorder),
            remote_reader: Some(&remote_reader),
            wellbeing_reader: None,
            context_reader: None,
            task_views: &tasks,

            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_inference::UsageLedger::default(),
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
        let calls = executor.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1,
            floe_inference::InferenceExecutionConstraint::RemoteOnly
        );
        let envelope = serde_json::to_string(&calls[0].0.projection.envelope).unwrap();
        assert!(envelope.contains("mail:request"));
        assert!(envelope.contains("calendar:review"));
        assert!(envelope.contains(&task_id.to_string()));
        assert!(envelope.contains(&memory_id.to_string()));
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
        let answers: Vec<serde_json::Value> = cases
            .iter()
            .map(|(_, _, _, answer)| answer.clone())
            .collect();
        let roles: Vec<&str> = cases.iter().map(|(_, role, _, _)| *role).collect();
        let executor = CannedExpertExecutor::answering(answers);
        let scope = expert_scope();
        let server = tokio::spawn(async move {
            for (path, _, view, _) in cases {
                let (socket, _) = listener.accept().await.unwrap();
                let (view_request, socket) = request(socket).await;
                assert!(view_request.starts_with(&format!("POST {path} ")));
                respond(
                    socket,
                    serde_json::json!({"schema_version": 1, "view": view}).to_string(),
                )
                .await;
            }
        });
        let source_client = legacy_source_client(&format!("http://{address}"));
        let person_id = PersonId::new();
        let policy = expert_policy();
        let remote_reader = FixtureRemoteReader {
            source_client: &source_client,
            person_id,
        };
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let recorder = FixtureResultRecorder;
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(false, true).await,
            source_client: Some(&source_client),
            calendar_reader: Some(&remote_reader),
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: Some(&recorder),
            remote_reader: Some(&remote_reader),
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let mut results = Vec::new();
        for agent_id in [WORK_CONTEXT_AGENT_ID, LIFE_LOGISTICS_AGENT_ID] {
            let task = experts
                .handle_message(A2ASendMessageRequest {
                    usage: floe_inference::UsageLedger::default(),
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
        let calls = executor.calls();
        assert_eq!(calls.len(), 2);
        for ((request, constraint), role) in calls.iter().zip(roles) {
            assert_eq!(
                *constraint,
                floe_inference::InferenceExecutionConstraint::RemoteOnly
            );
            let instructions = request.projection.envelope.stable_instructions.render();
            assert!(instructions.contains(role));
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn personal_delegations_use_bounded_views_and_capability_free_experts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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
                            let calendar_query: serde_json::Value = serde_json::from_str(
                                optional_request.split_once("\r\n\r\n").unwrap().1,
                            )
                            .unwrap();
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
                                        "range_start_unix_ms": calendar_query["range_start_unix_ms"],
                                        "range_end_unix_ms": calendar_query["range_end_unix_ms"],
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
        let executor = CannedExpertExecutor::answering(vec![]);
        let scope = expert_scope();
        let policy = expert_policy();
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(true, true).await,
            source_client: None,
            calendar_reader: None,
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        for (agent_id, _, _, _, _, source_handle) in cases {
            let result = experts
                .handle_message(A2ASendMessageRequest {
                    usage: floe_inference::UsageLedger::default(),
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
            assert!(executor.calls().is_empty());
            let _ = (agent_id, source_handle);
            break;
        }
        server.abort();
    }

    #[tokio::test]
    async fn unavailable_personal_provider_is_typed_and_never_runs_the_expert() {
        let executor = CannedExpertExecutor::answering(vec![]);
        let scope = expert_scope();
        let policy = expert_policy();
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        let experts = ConversationExperts {
            executor: &executor,
            scope: &scope,
            availability: test_model_availability(true, true).await,
            source_client: None,
            calendar_reader: None,
            policy: &policy,
            context: &context,
            attention: None,
            people_reader: None,
            recorder: None,
            remote_reader: None,
            wellbeing_reader: None,
            context_reader: None,
            task_views: &[],
            cards: test_expert_cards(),
            stateful_settlement: &RejectStatefulSettlement,
            task_runners: &[],
            runs: None,
            interactions: None,
            device_id: None,
            snapshots: None,
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_inference::UsageLedger::default(),
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

    // 2-B.5 B2/B4: revoke through the real CompositeDependencyResolver path.
    //
    // The attention connector is device-local, so the composite routes it to
    // the vault-backed personal resolver: a grant that is reviewed, read,
    // then disabled exercises projection-time authorize, dispatch-time
    // reauthorization, and history reauthorization with no fakes.

    #[derive(Clone)]
    struct B2DeviceTransport {
        calls: Arc<AtomicUsize>,
    }

    impl floe_inference::PreparedModelTransport for B2DeviceTransport {
        async fn generate(
            &self,
            _request: floe_inference::CanonicalModelRequest,
            _target: floe_inference::AdmittedDispatchTarget,
        ) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(floe_inference::CanonicalModelResponse {
                output: vec![floe_agent_contract::ModelStep::Answer {
                    text: "done".into(),
                    artifacts: vec![],
                }],
                used_tokens: 10,
                cost_micros: 5,
            })
        }
    }

    #[tokio::test]
    async fn expert_eligibility_is_owned_and_unions_execution_classes_once() {
        use floe_agent_contract::ModelPlacement;
        let mut cards = test_expert_cards()[..3].to_vec();
        cards[0].supported_placements = vec![ModelPlacement::DeviceLocal];
        cards[1].supported_placements = vec![ModelPlacement::Remote];
        cards[2].supported_placements = vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote];
        cards.push(cards[2].clone());
        for (device, remote, indices) in [
            (false, false, vec![]),
            (true, false, vec![0, 2]),
            (false, true, vec![1, 2]),
            (true, true, vec![0, 1, 2]),
        ] {
            let available = test_model_availability(device, remote).await;
            let eligible = floe_experts::eligible_cards_for_availability(&cards, available);
            assert_eq!(
                eligible,
                indices
                    .into_iter()
                    .map(|index| cards[index].clone())
                    .collect::<Vec<_>>()
            );
        }
    }

    async fn test_model_availability(
        device: bool,
        remote: bool,
    ) -> floe_inference::InferenceAvailability {
        struct Provider {
            device: bool,
            remote: bool,
        }
        impl floe_inference::ModelProvider for Provider {
            type Prepared = B2DeviceTransport;
            async fn observe_profiles(
                &self,
            ) -> Vec<floe_inference::PreparedModelProfile<Self::Prepared>> {
                [
                    (floe_inference::ExecutionLocation::Device, self.device),
                    (floe_inference::ExecutionLocation::Gateway, self.remote),
                ]
                .into_iter()
                .map(
                    |(location, available)| floe_inference::PreparedModelProfile {
                        profile: floe_inference::ModelProfile {
                            id: format!("{location:?}"),
                            purpose: floe_inference::ModelPurpose::new(
                                floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
                            )
                            .unwrap(),
                            consumer: floe_inference::ModelConsumer::new(
                                floe_agent_contract::EXPERT_INFERENCE_CONSUMER,
                            )
                            .unwrap(),
                            execution_location: location,
                            data_recipient: floe_inference::DataRecipient::Device,
                            capabilities: Default::default(),
                            available,
                        },
                        transport: B2DeviceTransport {
                            calls: Arc::new(AtomicUsize::new(0)),
                        },
                    },
                )
                .collect()
            }
        }
        floe_inference::InferenceAvailability::observe(
            &Provider { device, remote },
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::EXPERT_INFERENCE_CONSUMER,
        )
        .await
    }

    struct B2DeviceProvider {
        transport: B2DeviceTransport,
    }

    impl floe_inference::ModelProvider for B2DeviceProvider {
        type Prepared = B2DeviceTransport;

        async fn observe_profiles(
            &self,
        ) -> Vec<floe_inference::PreparedModelProfile<Self::Prepared>> {
            vec![floe_inference::PreparedModelProfile {
                profile: floe_inference::ModelProfile {
                    id: "device".into(),
                    purpose: floe_inference::ModelPurpose::new(
                        floe_inference::CANONICAL_MODEL_PURPOSE,
                    )
                    .unwrap(),
                    consumer: floe_inference::ModelConsumer::new(
                        floe_inference::CANONICAL_MODEL_CONSUMER,
                    )
                    .unwrap(),
                    execution_location: floe_inference::ExecutionLocation::Device,
                    data_recipient: floe_inference::DataRecipient::Device,
                    capabilities: floe_inference::ModelCapabilities(vec![]),
                    available: true,
                },
                transport: self.transport.clone(),
            }]
        }
    }

    struct B2AllowAuthority;

    impl floe_access::ModelDispatchRecipientAuthority for B2AllowAuthority {
        fn check_recipient<'a>(
            &'a self,
            _request: &'a floe_access::ModelDispatchRequest,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<floe_access::RecipientCheckOutcome, AgentFailure>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move { Ok(floe_access::RecipientCheckOutcome::Granted) })
        }
    }

    struct B2Attention {
        _root: tempfile::TempDir,
        vault: EncryptedAgentVault<AttentionTestKeys>,
        local_context: Arc<LocalContextHost>,
        stop: Arc<AtomicBool>,
        person_id: PersonId,
    }

    impl B2Attention {
        async fn reviewed() -> Self {
            let root = tempfile::tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let person_id = PersonId::new();
            let vault =
                EncryptedAgentVault::create(root.path(), person_id, AttentionTestKeys::default())
                    .await
                    .unwrap();
            let local_context = Arc::new(LocalContextHost::default());
            let host_epoch = "attention-b2-host".to_owned();
            local_context
                .execute(
                    person_id,
                    LocalContextCommand::RegisterAttentionHost {
                        host_epoch: host_epoch.clone(),
                    },
                    None,
                )
                .unwrap();
            let subject = "a".repeat(64);
            let stop = Arc::new(AtomicBool::new(false));
            tokio::spawn(drive_attention_host(
                local_context.clone(),
                person_id,
                host_epoch,
                subject.clone(),
                stop.clone(),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(AtomicUsize::new(0)),
            ));
            let inspected = floe_access::apply_personal_access(
                &vault,
                &personal_grants::native_driver(&local_context),
                person_id,
                floe_access::PersonalAccessConfiguration {
                    connector: floe_access::ATTENTION_CONNECTOR.into(),
                    device_id: "test-device".into(),
                    consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
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
                    consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
                    change: floe_access::PersonalAccessChange::Review {
                        expected_native_subject_fingerprint: subject,
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
            Self {
                _root: root,
                vault,
                local_context,
                stop,
                person_id,
            }
        }

        async fn disable(&self) {
            let overview = floe_access::apply_personal_access(
                &self.vault,
                &personal_grants::native_driver(&self.local_context),
                self.person_id,
                floe_access::PersonalAccessConfiguration {
                    connector: floe_access::ATTENTION_CONNECTOR.into(),
                    device_id: "test-device".into(),
                    consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
                    change: floe_access::PersonalAccessChange::SetEnabled { enabled: false },
                },
                Cancellation::default(),
            )
            .await
            .unwrap();
            assert_eq!(overview.state, floe_access::PersonalAccessState::Paused);
        }

        /// The canonical attention read through vault grants and the device
        /// driver: the dependency it returns is currently authorized.
        async fn attention_exchange(
            &self,
        ) -> (
            floe_agent_contract::ToolCall,
            floe_agent_contract::ToolResult,
        ) {
            let tools = floe_context::ContextToolService::new(
                self.person_id,
                "test-device",
                floe_vault::VaultGrantRecords::new(&self.vault),
                personal_grants::native_driver(&self.local_context),
                None::<&remote_views::RemoteViewReader<AttentionTestKeys>>,
            )
            .unwrap();
            let call = floe_agent_contract::ToolCall {
                call_id: Uuid::new_v4(),
                invocation_key: floe_agent_contract::InvocationKey::new(),
                tool_id: "attention.coarse.read".into(),
                definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
                input: "{}".into(),
            };
            let outcome = tools.invoke_outcome(&call, &tool_scope()).await.unwrap();
            let floe_context_contract::SourceReadOutcome::Ready(result) = outcome else {
                panic!("attention read must stay ready");
            };
            assert!(
                matches!(
                    result.coverage,
                    floe_agent_contract::DependencyCoverage::Dependent { .. }
                ),
                "attention read must carry direct coverage: {:?}",
                result.coverage
            );
            (call, result)
        }

        fn shutdown(&self) {
            self.stop.store(true, Ordering::Release);
        }
    }

    fn b2_scope() -> (
        floe_execution::budget::BudgetLedger,
        floe_execution::ExecutionScope,
    ) {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100_000, 10_000_000),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()),
        );
        (ledger, scope)
    }

    async fn b2_project(
        fixture: &B2Attention,
        session_id: Uuid,
        history: Vec<floe_agent_contract::ModelConversationEntry>,
        current_turn: Vec<floe_agent_contract::ModelConversationEntry>,
    ) -> floe_agent_contract::AuthorizedModelProjection {
        let personal = personal_grants::PersonalDependencyResolver {
            vault: &fixture.vault,
            local_context: &fixture.local_context,
            person_id: fixture.person_id,
            device_id: "test-device",
        };
        let resolver = CompositeDependencyResolver {
            personal: &personal,
            remote: None,
            calendar: None,
        };
        let projector = floe_conversation::ConversationModelProjection::new(
            floe_vault::ContextEvidenceReader::new(&fixture.vault, session_id),
            resolver,
            session_id,
            floe_agent_contract::AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            vec![DataClass::Personal],
            vec![],
        )
        .unwrap();
        floe_agent_contract::ModelProjectionPort::project(
            &projector,
            floe_agent_contract::ModelProjectionRequest {
                principal: fixture.person_id.to_string(),
                role: floe_agent_contract::RoleSpec {
                    role_id: "manager".into(),
                    instructions: "Answer.".into(),
                    output_contract: floe_conversation::MANAGER_OUTPUT_CONTRACT.into(),
                },
                conversation: floe_agent_contract::ModelConversation {
                    history,
                    current_turn,
                },
                catalog: floe_agent_contract::AllowedCatalog {
                    cards: vec![],
                    tools: floe_context::manager_tool_descriptors(),
                    revision: 1,
                },
                max_output_bytes: 4096,
                correction: None,
            },
            &tool_scope(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn revoke_after_projection_denies_dispatch_before_provider_handoff() {
        let fixture = B2Attention::reviewed().await;
        let (call, result) = fixture.attention_exchange().await;
        let dependency = match result.coverage.clone() {
            floe_agent_contract::DependencyCoverage::Dependent { dependencies } => {
                assert_eq!(dependencies.len(), 1);
                dependencies.into_iter().next().unwrap()
            }
            coverage => panic!("attention read must be dependent: {coverage:?}"),
        };
        let session = fixture.vault.create_session().await.unwrap();
        // Projection succeeds with dependency D: the live exchange folds into
        // the authorized coverage.
        let projection = b2_project(
            &fixture,
            session.id,
            vec![],
            vec![
                floe_agent_contract::ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "how am I doing?".into(),
                },
                floe_agent_contract::ModelConversationEntry::ToolExchange { call, result },
            ],
        )
        .await;
        assert_eq!(
            projection.coverage,
            floe_agent_contract::DependencyCoverage::dependent(dependency.clone()).unwrap()
        );
        // The dispatch fence admits while the grant is live.
        let personal = personal_grants::PersonalDependencyResolver {
            vault: &fixture.vault,
            local_context: &fixture.local_context,
            person_id: fixture.person_id,
            device_id: "test-device",
        };
        let resolver = CompositeDependencyResolver {
            personal: &personal,
            remote: None,
            calendar: None,
        };
        let dispatch = || floe_access::ModelDispatchRequest {
            person_id: fixture.person_id,
            projection_ref: Uuid::new_v4(),
            projection_revision: 1,
            coverage: floe_agent_contract::DependencyCoverage::dependent(dependency.clone())
                .unwrap(),
            input_data_classes: vec![DataClass::Personal],
            purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
            consumer: floe_inference::CANONICAL_MODEL_CONSUMER.into(),
            profile_id: "b2-device".into(),
            target: floe_access::ModelDispatchTarget::Device,
            lineage: None,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: Cancellation::default(),
        };
        let authority = B2AllowAuthority;
        let permit = floe_access::admit_model_dispatch(dispatch(), &resolver, &authority)
            .await
            .unwrap();
        // Authority for D changes after projection but before handoff.
        fixture.disable().await;
        // The consume fence reauthorizes D and denies: the provider receives
        // no unauthorized projection.
        assert_eq!(
            floe_access::consume_model_dispatch(permit).await.err(),
            Some(floe_access::ModelDispatchDenial::Hard(
                AgentFailure::AccessReviewRequired
            ))
        );
        // End to end through InferenceService: dispatch denies, the provider
        // is never posted, and nothing is charged.
        let transport = B2DeviceTransport {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let provider = B2DeviceProvider {
            transport: transport.clone(),
        };
        let service = floe_inference::InferenceService::new(provider, resolver, authority);
        let (ledger, scope) = b2_scope();
        assert_eq!(
            floe_agent_contract::ModelPort::generate(
                &service,
                floe_agent_contract::ModelRequest {
                    attempt_id: Uuid::new_v4(),
                    principal: fixture.person_id.to_string(),
                    projection,
                    catalog: floe_agent_contract::AllowedCatalog::default(),
                    purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
                    consumer: floe_inference::CANONICAL_MODEL_CONSUMER.into(),
                    preferred_profile_id: None,
                    replay: vec![],
                    lineage: None,
                },
                &scope,
            )
            .await
            .err(),
            Some(AgentFailure::AccessReviewRequired)
        );
        assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.settled.tokens, 0);
        assert_eq!(snapshot.settled.cost_micros, 0);
        assert_eq!(snapshot.settled.attempts, 0);
        assert_eq!(snapshot.unknown_tokens, 0);
        fixture.shutdown();
    }

    /// Revokes the attention grant after the consume fence's authorization
    /// succeeds, so post-response revalidation observes revoked authority.
    struct RevokeAfterConsume<'a> {
        inner: CompositeDependencyResolver<'a>,
        vault: &'a EncryptedAgentVault<AttentionTestKeys>,
        local_context: &'a LocalContextHost,
        person_id: PersonId,
        calls: AtomicUsize,
    }

    impl floe_access::DependencyResolver for RevokeAfterConsume<'_> {
        fn authorize<'a>(
            &'a self,
            dependency: &'a floe_context_contract::ContextDependency,
            request: &'a floe_access::DependencyAuthorization,
        ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
            Box::pin(async move {
                let result =
                    floe_access::DependencyResolver::authorize(&self.inner, dependency, request)
                        .await;
                // Admit (1st) and consume (2nd) observe live authority; revoke
                // before the post-response revalidation (3rd).
                if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                    result.as_ref().unwrap();
                    floe_access::apply_personal_access(
                        self.vault,
                        &personal_grants::native_driver(self.local_context),
                        self.person_id,
                        floe_access::PersonalAccessConfiguration {
                            connector: floe_access::ATTENTION_CONNECTOR.into(),
                            device_id: "test-device".into(),
                            consumers: vec![floe_access::ATTENTION_ASSISTANT_CONSUMER.into()],
                            change: floe_access::PersonalAccessChange::SetEnabled {
                                enabled: false,
                            },
                        },
                        Cancellation::default(),
                    )
                    .await
                    .unwrap();
                }
                result
            })
        }
    }

    #[tokio::test]
    async fn revoke_after_handoff_suppresses_response_but_keeps_charge() {
        let fixture = B2Attention::reviewed().await;
        let (call, result) = fixture.attention_exchange().await;
        let session = fixture.vault.create_session().await.unwrap();
        let projection = b2_project(
            &fixture,
            session.id,
            vec![],
            vec![
                floe_agent_contract::ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "how am I doing?".into(),
                },
                floe_agent_contract::ModelConversationEntry::ToolExchange { call, result },
            ],
        )
        .await;
        let personal = personal_grants::PersonalDependencyResolver {
            vault: &fixture.vault,
            local_context: &fixture.local_context,
            person_id: fixture.person_id,
            device_id: "test-device",
        };
        let revoking = RevokeAfterConsume {
            inner: CompositeDependencyResolver {
                personal: &personal,
                remote: None,
                calendar: None,
            },
            vault: &fixture.vault,
            local_context: &fixture.local_context,
            person_id: fixture.person_id,
            calls: AtomicUsize::new(0),
        };
        let transport = B2DeviceTransport {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let provider = B2DeviceProvider {
            transport: transport.clone(),
        };
        let service = floe_inference::InferenceService::new(provider, revoking, B2AllowAuthority);
        let (ledger, scope) = b2_scope();
        // Admit and consume succeed; the grant is revoked mid-flight; the
        // post-response revalidation denies and suppresses the content.
        assert_eq!(
            floe_agent_contract::ModelPort::generate(
                &service,
                floe_agent_contract::ModelRequest {
                    attempt_id: Uuid::new_v4(),
                    principal: fixture.person_id.to_string(),
                    projection,
                    catalog: floe_agent_contract::AllowedCatalog::default(),
                    purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
                    consumer: floe_inference::CANONICAL_MODEL_CONSUMER.into(),
                    preferred_profile_id: None,
                    replay: vec![],
                    lineage: None,
                },
                &scope,
            )
            .await
            .err(),
            Some(AgentFailure::PolicyDenied)
        );
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.settled.tokens, 10);
        assert_eq!(snapshot.settled.cost_micros, 5);
        assert_eq!(snapshot.settled.attempts, 1);
        fixture.shutdown();
    }

    #[tokio::test]
    async fn revoked_history_is_dropped_from_the_next_projection() {
        let fixture = B2Attention::reviewed().await;
        let (_, result) = fixture.attention_exchange().await;
        let coverage = result.coverage.clone();
        assert!(matches!(
            coverage,
            floe_agent_contract::DependencyCoverage::Dependent { .. }
        ));
        // Complete a turn whose answer depends on source-backed coverage and
        // persist the terminal coverage under the turn id.
        fixture
            .vault
            .activate_conversation_executor()
            .await
            .unwrap();
        let session = fixture.vault.create_session().await.unwrap();
        let run_id = floe_kernel::RunId::new();
        fixture
            .vault
            .admit_conversation_turn(floe_vault::VaultConversationAdmissionRequest {
                run_id,
                command_id: floe_agent_contract::CommandId::new(),
                session_id: session.id,
                person_id: fixture.person_id,
                expected_session_revision: 0,
                request_digest: [7; 32],
                text: "how am I doing?".into(),
                continuation: None,
                retry_of: None,
                resume: None,
                profile: floe_conversation::ProfileSelection::Auto,
            })
            .await
            .unwrap();
        fixture
            .vault
            .finish_conversation_run(
                run_id,
                1,
                floe_vault::VaultConversationTerminal {
                    state: floe_vault::VaultConversationRunState::Completed,
                    output: Some("you are focused".into()),
                    coverage: coverage.clone(),
                    issue: None,
                    appended_messages: vec![AgentMessage::Assistant {
                        turn_id: run_id.as_uuid(),
                        text: "you are focused".into(),
                    }],
                },
            )
            .await
            .unwrap();
        let stored = fixture
            .vault
            .load(fixture.person_id, session.id)
            .await
            .unwrap();
        assert_eq!(stored.messages.len(), 2);
        // The terminal coverage persisted under the committed turn id.
        let reader = floe_vault::ContextEvidenceReader::new(&fixture.vault, session.id);
        assert_eq!(
            floe_context::EvidenceReader::read_turn_coverage(
                &reader,
                session.id,
                run_id.as_uuid(),
            )
                .await
                .unwrap(),
            coverage
        );
        let history = || {
            vec![
                floe_agent_contract::ModelConversationEntry::User {
                    message_id: run_id.as_uuid(),
                    text: "how am I doing?".into(),
                },
                floe_agent_contract::ModelConversationEntry::Assistant {
                    message_id: run_id.as_uuid(),
                    text: "you are focused".into(),
                },
            ]
        };
        let current_turn = || {
            vec![floe_agent_contract::ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "and now?".into(),
            }]
        };
        // Fresh authorized derived history is retained with its dependency.
        let projection = b2_project(&fixture, session.id, history(), current_turn()).await;
        assert_eq!(projection.envelope.conversation.history.len(), 2);
        assert_eq!(projection.coverage, coverage);
        // Revoke the source, then project again: the derived answer is
        // dropped, the Person's own text remains, and no dependency survives.
        fixture.disable().await;
        let projection = b2_project(&fixture, session.id, history(), current_turn()).await;
        assert_eq!(projection.envelope.conversation.history.len(), 1);
        assert!(matches!(
            projection.envelope.conversation.history[0],
            floe_agent_contract::ModelConversationEntry::User { .. }
        ));
        assert_eq!(
            projection.coverage,
            floe_agent_contract::DependencyCoverage::Independent
        );
        fixture.shutdown();
    }
}
