use std::{
    cell::RefCell,
    fs,
    ops::Deref,
    os::unix::fs::DirBuilderExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "android")]
use crate::android_vault_keys::AndroidVaultKeys as PlatformVaultKeys;
use base64::Engine as _;
use floe_agent::{
    AgentEvent, AgentFailure, AgentOutcome, AgentSession, BuiltinContextSource, BuiltinExpertKind,
    BuiltinExpertSetup, BuiltinSourceBinding, BuiltinSourceState, Cancellation, ConnectionState,
    SessionStore,
};
#[cfg(not(target_os = "android"))]
use floe_core::KeyringVaultKeys as PlatformVaultKeys;
use floe_core::{
    AgentFixtureTurn, CalendarActionState, CalendarReadAccess, CalendarReadAccessRequest,
    EncryptedAgentVault, ExpertCalendarInspection, ExpertProposalReference, FloeCore,
    RemotePairingChallenge, RemoteProducerIdentity, VaultKeyProvider, recover_agent_sample,
};
use floe_domain::{
    CalendarProvider, ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer,
    GrantDataCategory, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, GrantState,
    PersonId, ProcessingRestriction, ResourceHandle,
};
use floe_experts::{Directory, TaskCoordinator};
use floe_infra::remote_authorization::{RemoteAuthorizationClient, RemotePairingClient};
use floe_knowledge::{KnowledgeActor, KnowledgeDecisionKind};
use floe_protocol::*;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{BridgeResult, agent_failure, check_version, parse_id, parse_person};
use crate::{diagnostics, local_context::LocalContextStore};

mod conversation_turn;
mod learner_worker;
mod personal_grants;
mod remote_views;
mod task_repository;

use task_repository::VaultTaskRepository;

const LEARNER_IDLE_DELAY: Duration = Duration::from_millis(750);
const LEARNER_EMPTY_DELAY: Duration = Duration::from_secs(30);
const LEARNER_ERROR_DELAY: Duration = Duration::from_secs(5);
const AGENT_VAULT_STACK_SIZE: usize = 8 * 1024 * 1024;

fn operation_name(operation: &AgentVaultOperationDto) -> &'static str {
    match operation {
        AgentVaultOperationDto::Submit { action } => action_name(action),
        AgentVaultOperationDto::Poll { .. } => "poll",
        AgentVaultOperationDto::Stop {} => "stop",
        AgentVaultOperationDto::Release {} => "release",
    }
}

fn action_name(action: &AgentVaultActionDto) -> &'static str {
    match action {
        AgentVaultActionDto::Status {} => "status",
        AgentVaultActionDto::Create {} => "create",
        AgentVaultActionDto::Unlock {} => "unlock",
        AgentVaultActionDto::Lock {} => "lock",
        AgentVaultActionDto::Session { .. } => "session",
        AgentVaultActionDto::Registry { .. } => "registry",
        AgentVaultActionDto::CalendarExperts { .. } => "calendar_experts",
        AgentVaultActionDto::CalendarAccess { .. } => "calendar_access",
        AgentVaultActionDto::PersonalAccess { .. } => "personal_access",
        AgentVaultActionDto::ContactsAccess { .. } => "contacts_access",
        AgentVaultActionDto::CalendarAction { .. } => "calendar_action",
        AgentVaultActionDto::CalendarSubjectPreview { .. } => "calendar_subject_preview",
        AgentVaultActionDto::InspectProposal { .. } => "inspect_proposal",
        AgentVaultActionDto::ConversationSession { .. } => "conversation_session",
        AgentVaultActionDto::ConversationTurn { .. } => "conversation_turn",
        AgentVaultActionDto::MemoryReview { .. } => "memory_review",
        AgentVaultActionDto::Memory {} => "memory",
        AgentVaultActionDto::Connections {} => "connections",
        AgentVaultActionDto::RemoteAuthorityInspectProducer { .. } => {
            "remote_authority_inspect_producer"
        }
        AgentVaultActionDto::RemoteAuthorityReviewAndEnroll { .. } => {
            "remote_authority_review_and_enroll"
        }
        AgentVaultActionDto::RemoteAuthorityEnrollmentStatus { .. } => {
            "remote_authority_enrollment_status"
        }
        AgentVaultActionDto::RemotePairingPrepare {} => "remote_pairing_prepare",
        AgentVaultActionDto::RemotePairingConfirm { .. } => "remote_pairing_confirm",
        AgentVaultActionDto::RemotePairingStatus { .. } => "remote_pairing_status",
        AgentVaultActionDto::RemotePairingFinalize { .. } => "remote_pairing_finalize",
        AgentVaultActionDto::RemoteCalendarGrantPreview { .. } => "remote_calendar_grant_preview",
        AgentVaultActionDto::RemoteCalendarGrantReview { .. } => "remote_calendar_grant_review",
        AgentVaultActionDto::RemoteCalendarGrantStatus { .. } => "remote_calendar_grant_status",
        AgentVaultActionDto::RemoteCalendarGrantPause { .. } => "remote_calendar_grant_pause",
        AgentVaultActionDto::RemoteViewGrantPreview { .. } => "remote_view_grant_preview",
        AgentVaultActionDto::RemoteViewGrantReview { .. } => "remote_view_grant_review",
        AgentVaultActionDto::RemoteViewGrantStatus { .. } => "remote_view_grant_status",
        AgentVaultActionDto::RemoteViewGrantPause { .. } => "remote_view_grant_pause",
    }
}

pub(crate) struct VaultBridge {
    root: PathBuf,
    core: Arc<FloeCore>,
    worker: RefCell<Option<Worker>>,
    local_context: Arc<LocalContextStore>,
}

impl VaultBridge {
    pub(crate) fn new(
        database_path: &str,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextStore>,
    ) -> Self {
        Self {
            root: PathBuf::from(format!("{database_path}.agent-vaults")),
            core,
            worker: RefCell::new(None),
            local_context,
        }
    }

    pub(crate) fn request(
        &self,
        request: AgentVaultRequestDto,
    ) -> BridgeResult<AgentVaultResultDto> {
        check_version(request.schema_version)?;
        let person = parse_person(&request.person_id)?;
        let id = parse_id(&request.request_id, "request_id", |id| id)?;
        let operation = operation_name(&request.operation);
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker = Some(
                Worker::with_core(
                    self.root.clone(),
                    PlatformVaultKeys,
                    self.core.clone(),
                    self.local_context.clone(),
                )
                .map_err(agent_failure)?,
            );
        }
        worker
            .as_ref()
            .unwrap()
            .request(person, id, request.operation)
            .and_then(VaultJobResult::into_protocol)
            .map_err(|failure| {
                let mut error = agent_failure(failure);
                error.metadata.insert("request_id".into(), id.to_string());
                error.metadata.insert("stage".into(), operation.into());
                error
            })
    }
}

struct Worker {
    sender: mpsc::SyncSender<Arc<Job>>,
    active: Mutex<Option<Arc<Job>>>,
    closing: Arc<AtomicBool>,
    learner_scheduling: floe_knowledge::LearnerScheduling,
}

struct OpenVault<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    _directory: Directory,
    _task_coordinator: TaskCoordinator<VaultTaskRepository<Keys>>,
    _recovered_tasks: Vec<floe_agent_contract::TaskReceipt>,
}

impl<Keys: VaultKeyProvider> OpenVault<Keys> {
    async fn activate(vault: EncryptedAgentVault<Keys>) -> Result<Self, AgentFailure> {
        let vault = Arc::new(vault);
        let directory = Directory::default();
        let repository = Arc::new(VaultTaskRepository::new(Arc::clone(&vault)));
        let (task_coordinator, recovered_tasks) = TaskCoordinator::activate(
            directory.clone(),
            repository,
            "everyday-assistance",
            floe_agent_contract::MAX_OUTPUT_BYTES,
        )
        .await?;
        Ok(Self {
            vault,
            _directory: directory,
            _task_coordinator: task_coordinator,
            _recovered_tasks: recovered_tasks,
        })
    }
}

impl<Keys> Deref for OpenVault<Keys> {
    type Target = EncryptedAgentVault<Keys>;

    fn deref(&self) -> &Self::Target {
        &self.vault
    }
}

struct Job {
    person: PersonId,
    id: Uuid,
    action: AgentVaultActionDto,
    cancellation: Cancellation,
    progress: Mutex<Progress>,
}

#[derive(Default)]
struct Progress {
    events: Vec<AgentEvent>,
    done: bool,
    state: Option<AgentVaultStateDto>,
    session: Option<AgentSession>,
    registry: Option<floe_agent::RegistryOverview>,
    calendar_experts: Option<floe_agent::CalendarExpertOverview>,
    calendar_subject_preview: Option<CalendarSubjectPreviewDto>,
    proposal: Option<AgentProposalInspectionDto>,
    memory_review: Option<AgentMemoryReviewOverviewDto>,
    memory: Option<AgentMemoryOverviewDto>,
    connections: Option<Vec<floe_agent::ConnectorSnapshot>>,
    remote_producer: Option<RemoteProducerIdentityDto>,
    remote_enrollment: Option<RemoteAuthorityEnrollmentStatusDto>,
    remote_pairing: Option<RemotePairingStatusDto>,
    remote_owner: Option<RemoteOwnerPublicKeyDto>,
    remote_calendar_grant: Option<RemoteCalendarGrantOverviewDto>,
    remote_calendar_preview: Option<RemoteCalendarGrantPreviewDto>,
    remote_view_grant: Option<RemoteViewGrantOverviewDto>,
    remote_view_preview: Option<RemoteViewGrantPreviewDto>,
    personal_access: Option<PersonalAccessOverviewDto>,
    calendar_actions: Option<serde_json::Value>,
    failure: Option<AgentFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VaultJobResult {
    request_id: String,
    stage: String,
    events: Vec<AgentEvent>,
    next_sequence: usize,
    done: bool,
    state: Option<AgentVaultStateDto>,
    session: Option<AgentSession>,
    registry: Option<floe_agent::RegistryOverview>,
    calendar_experts: Option<floe_agent::CalendarExpertOverview>,
    calendar_subject_preview: Option<CalendarSubjectPreviewDto>,
    proposal: Option<AgentProposalInspectionDto>,
    memory_review: Option<AgentMemoryReviewOverviewDto>,
    memory: Option<AgentMemoryOverviewDto>,
    connections: Option<Vec<floe_agent::ConnectorSnapshot>>,
    remote_producer: Option<RemoteProducerIdentityDto>,
    remote_enrollment: Option<RemoteAuthorityEnrollmentStatusDto>,
    remote_pairing: Option<RemotePairingStatusDto>,
    remote_owner: Option<RemoteOwnerPublicKeyDto>,
    remote_calendar_grant: Option<RemoteCalendarGrantOverviewDto>,
    remote_calendar_preview: Option<RemoteCalendarGrantPreviewDto>,
    remote_view_grant: Option<RemoteViewGrantOverviewDto>,
    remote_view_preview: Option<RemoteViewGrantPreviewDto>,
    personal_access: Option<PersonalAccessOverviewDto>,
    calendar_actions: Option<serde_json::Value>,
    failure: Option<AgentFailure>,
}

impl VaultJobResult {
    fn into_protocol(self) -> Result<AgentVaultResultDto, AgentFailure> {
        let request_id = self.request_id.clone();
        let failure = self.failure.as_ref().or_else(|| {
            match self
                .session
                .as_ref()
                .and_then(|session| session.last_outcome.as_ref())
            {
                Some(AgentOutcome::Halted { reason }) => Some(reason),
                _ => None,
            }
        });
        Ok(AgentVaultResultDto {
            request_id,
            events: encode_contracts(self.events)?,
            next_sequence: self.next_sequence,
            done: self.done,
            state: self.state,
            session: self.session.as_ref().map(encode_contract).transpose()?,
            registry: self.registry.as_ref().map(encode_contract).transpose()?,
            calendar_experts: self
                .calendar_experts
                .as_ref()
                .map(encode_contract)
                .transpose()?,
            calendar_subject_preview: self.calendar_subject_preview,
            proposal: self.proposal,
            memory_review: self.memory_review,
            memory: self.memory,
            connections: self.connections.map(encode_contracts).transpose()?,
            remote_producer: self.remote_producer,
            remote_enrollment: self.remote_enrollment,
            remote_pairing: self.remote_pairing,
            remote_owner: self.remote_owner,
            remote_calendar_grant: self.remote_calendar_grant,
            remote_calendar_preview: self.remote_calendar_preview,
            remote_view_grant: self.remote_view_grant,
            remote_view_preview: self.remote_view_preview,
            personal_access: self.personal_access,
            calendar_actions: self.calendar_actions,
            failure: failure
                .map(|failure| failure_envelope(failure, &self.stage, &self.request_id)),
        })
    }
}

impl Worker {
    fn with_core<Keys: VaultKeyProvider + Clone + 'static>(
        root: PathBuf,
        keys: Keys,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextStore>,
    ) -> Result<Self, AgentFailure> {
        let (sender, receiver) = mpsc::sync_channel::<Arc<Job>>(1);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        let learner_scheduling = floe_knowledge::LearnerScheduling::default();
        let worker_learner_scheduling = learner_scheduling.clone();
        std::thread::Builder::new()
            .name("floe-agent-vault".into())
            .stack_size(AGENT_VAULT_STACK_SIZE)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let mut vault = None;
                let mut learner_delay = LEARNER_IDLE_DELAY;
                loop {
                    match receiver.recv_timeout(learner_delay) {
                        Ok(job) => {
                            learner_delay = LEARNER_IDLE_DELAY;
                            if worker_closing.load(Ordering::Acquire) {
                                break;
                            }
                            let operation = action_name(&job.action);
                            let started = Instant::now();
                            let trace_context = diagnostics::trace_context(job.id);
                            let request_id = trace_context.request_id().to_string();
                            tracing::info!(request_id, operation, "agent_job_started");
                            let result = match catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => runtime.block_on(diagnostics::instrument(
                                    execute(
                                        &root,
                                        &keys,
                                        &core,
                                        &local_context,
                                        &mut vault,
                                        &job,
                                    ),
                                    trace_context,
                                    "agent_job",
                                )),
                                Err(_) => Err(AgentFailure::VaultUnavailable),
                            })) {
                                Ok(result) => result,
                                Err(payload) => {
                                    let _ = diagnostics::panic_error(payload);
                                    Err(AgentFailure::Interrupted)
                                }
                            };
                            let elapsed_ms = started.elapsed().as_millis() as u64;
                            match &result {
                                Ok(_) => tracing::info!(
                                    request_id,
                                    operation,
                                    elapsed_ms,
                                    "agent_job_completed"
                                ),
                                Err(failure) => tracing::error!(
                                    request_id,
                                    operation,
                                    elapsed_ms,
                                    failure = ?failure,
                                    "agent_job_failed"
                                ),
                            }
                            if matches!(
                                result,
                                Err(AgentFailure::VaultUnavailable | AgentFailure::Interrupted)
                            ) {
                                vault = None;
                            }
                            if let Ok(mut progress) = job.progress.lock() {
                                match result {
                                    Ok(result) => {
                                        progress.state = Some(result.state);
                                        progress.session = result.session;
                                        progress.registry = result.registry;
                                        progress.calendar_experts = result.calendar_experts;
                                        progress.calendar_subject_preview =
                                            result.calendar_subject_preview;
                                        progress.proposal = result.proposal;
                                        progress.memory_review = result.memory_review;
                                        progress.memory = result.memory;
                                        progress.remote_producer = result.remote_producer;
                                        progress.remote_enrollment = result.remote_enrollment;
                                        progress.remote_pairing = result.remote_pairing;
                                        progress.remote_owner = result.remote_owner;
                                        progress.remote_calendar_grant =
                                            result.remote_calendar_grant;
                                        progress.remote_calendar_preview =
                                            result.remote_calendar_preview;
                                        progress.remote_view_grant = result.remote_view_grant;
                                        progress.remote_view_preview = result.remote_view_preview;
                                        progress.personal_access = result.personal_access;
                                        progress.calendar_actions = result.calendar_actions;
                                    }
                                    Err(failure) => {
                                        progress.state = Some(
                                            if matches!(
                                                failure,
                                                AgentFailure::VaultUnavailable
                                                    | AgentFailure::Interrupted
                                            ) || vault
                                                .as_ref()
                                                .is_none_or(|(person, _)| *person != job.person)
                                            {
                                                AgentVaultStateDto::Unavailable
                                            } else {
                                                AgentVaultStateDto::Ready
                                            },
                                        );
                                        progress.failure = Some(failure);
                                    }
                                }
                                let _ = worker_learner_scheduling.foreground_finished();
                                progress.done = true;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if worker_closing.load(Ordering::Acquire) {
                                break;
                            }
                            let Some((_, open_vault)) = vault.as_ref() else {
                                learner_delay = LEARNER_EMPTY_DELAY;
                                continue;
                            };
                            if matches!(worker_learner_scheduling.foreground_pending(), Ok(false)) {
                                let cleanup_result = match &runtime {
                                    Ok(runtime) => {
                                        runtime.block_on(open_vault.drain_context_cleanup(16))
                                    }
                                    Err(_) => Err(AgentFailure::VaultUnavailable),
                                };
                                match cleanup_result {
                                    Ok(_) | Err(AgentFailure::Conflict) => {}
                                    Err(AgentFailure::VaultUnavailable) => {
                                        vault = None;
                                        learner_delay = LEARNER_EMPTY_DELAY;
                                        continue;
                                    }
                                    Err(_) => {
                                        learner_delay = LEARNER_ERROR_DELAY;
                                    }
                                }
                            }
                            let learner = match worker_learner_scheduling.try_start() {
                                Ok(Some(learner)) => learner,
                                Ok(None) => continue,
                                Err(_) => {
                                    learner_delay = LEARNER_ERROR_DELAY;
                                    continue;
                                }
                            };
                            let result = catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => runtime.block_on(Box::pin(learner_worker::run(
                                    open_vault,
                                    learner.cancellation(),
                                ))),
                                Err(_) => Err(AgentFailure::VaultUnavailable),
                            }))
                            .unwrap_or(Err(AgentFailure::Interrupted));
                            drop(learner);
                            if matches!(
                                result,
                                Err(AgentFailure::VaultUnavailable | AgentFailure::Interrupted)
                            ) {
                                vault = None;
                            }
                            learner_delay = match result {
                                Ok(true) => LEARNER_IDLE_DELAY,
                                Ok(false) => LEARNER_EMPTY_DELAY,
                                Err(_) => LEARNER_ERROR_DELAY,
                            };
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    if worker_closing.load(Ordering::Acquire) {
                        break;
                    }
                }
            })
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(Self {
            sender,
            active: Mutex::new(None),
            closing,
            learner_scheduling,
        })
    }

    fn request(
        &self,
        person: PersonId,
        id: Uuid,
        operation: AgentVaultOperationDto,
    ) -> Result<VaultJobResult, AgentFailure> {
        let mut active = self.active.lock().map_err(|_| AgentFailure::Interrupted)?;
        if let AgentVaultOperationDto::Submit { ref action } = operation {
            if let Some(job) = active.as_ref() {
                if job.person != person || job.id != id || &job.action != action {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                let job = Arc::new(Job {
                    person,
                    id,
                    action: action.clone(),
                    cancellation: Cancellation::default(),
                    progress: Mutex::new(Progress::default()),
                });
                self.learner_scheduling.foreground_submitted()?;
                if self.sender.try_send(job.clone()).is_err() {
                    let _ = self.learner_scheduling.foreground_finished();
                    return Err(AgentFailure::VaultUnavailable);
                }
                *active = Some(job);
            }
        }
        let job = active.as_ref().ok_or(AgentFailure::NotFound)?;
        if job.person != person || job.id != id {
            return Err(AgentFailure::NotFound);
        }
        if matches!(operation, AgentVaultOperationDto::Stop {}) {
            job.cancellation.cancel();
        }
        let after_sequence = match operation {
            AgentVaultOperationDto::Poll { after_sequence } => after_sequence,
            _ => 0,
        };
        let progress = job.progress.lock().map_err(|_| AgentFailure::Interrupted)?;
        if after_sequence > progress.events.len() {
            return Err(AgentFailure::InvalidInput);
        }
        let response = VaultJobResult {
            request_id: id.to_string(),
            stage: action_name(&job.action).into(),
            events: progress.events[after_sequence..].to_vec(),
            next_sequence: progress.events.len(),
            done: progress.done,
            state: progress.state,
            session: progress.session.clone(),
            registry: progress.registry.clone(),
            calendar_experts: progress.calendar_experts.clone(),
            calendar_subject_preview: progress.calendar_subject_preview.clone(),
            proposal: progress.proposal.clone(),
            memory_review: progress.memory_review.clone(),
            memory: progress.memory.clone(),
            connections: progress.connections.clone(),
            remote_owner: progress.remote_owner.clone(),
            remote_producer: progress.remote_producer.clone(),
            remote_enrollment: progress.remote_enrollment.clone(),
            remote_pairing: progress.remote_pairing.clone(),
            remote_calendar_grant: progress.remote_calendar_grant.clone(),
            remote_calendar_preview: progress.remote_calendar_preview.clone(),
            remote_view_grant: progress.remote_view_grant.clone(),
            remote_view_preview: progress.remote_view_preview.clone(),
            personal_access: progress.personal_access.clone(),
            calendar_actions: progress.calendar_actions.clone(),
            failure: progress.failure,
        };
        drop(progress);
        if matches!(operation, AgentVaultOperationDto::Release {}) {
            if !response.done {
                return Err(AgentFailure::Conflict);
            }
            *active = None;
        }
        Ok(response)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        if let Ok(active) = self.active.lock()
            && let Some(job) = active.as_ref()
        {
            job.cancellation.cancel();
        }
        self.learner_scheduling.close();
    }
}

struct VaultExecutionResult {
    state: AgentVaultStateDto,
    session: Option<AgentSession>,
    registry: Option<floe_agent::RegistryOverview>,
    calendar_experts: Option<floe_agent::CalendarExpertOverview>,
    calendar_subject_preview: Option<CalendarSubjectPreviewDto>,
    proposal: Option<AgentProposalInspectionDto>,
    memory_review: Option<AgentMemoryReviewOverviewDto>,
    memory: Option<AgentMemoryOverviewDto>,
    remote_producer: Option<RemoteProducerIdentityDto>,
    remote_enrollment: Option<RemoteAuthorityEnrollmentStatusDto>,
    remote_pairing: Option<RemotePairingStatusDto>,
    remote_owner: Option<RemoteOwnerPublicKeyDto>,
    remote_calendar_grant: Option<RemoteCalendarGrantOverviewDto>,
    remote_calendar_preview: Option<RemoteCalendarGrantPreviewDto>,
    remote_view_grant: Option<RemoteViewGrantOverviewDto>,
    remote_view_preview: Option<RemoteViewGrantPreviewDto>,
    personal_access: Option<PersonalAccessOverviewDto>,
    calendar_actions: Option<serde_json::Value>,
}

fn protocol_producer_identity(
    identity: &floe_infra::remote_authorization::ProducerIdentityResponse,
) -> RemoteProducerIdentityDto {
    RemoteProducerIdentityDto {
        schema_version: identity.schema_version,
        instance_id: identity.instance_id.clone(),
        execution_owner: identity.execution_owner.clone(),
        audience: identity.audience.clone(),
        key_id: identity.key_id.clone(),
        public_key: identity.public_key.clone(),
        fingerprint: identity.fingerprint.clone(),
    }
}

fn core_producer_identity(identity: &RemoteProducerIdentityDto) -> RemoteProducerIdentity {
    RemoteProducerIdentity {
        schema_version: identity.schema_version,
        instance_id: identity.instance_id.clone(),
        execution_owner: identity.execution_owner.clone(),
        audience: identity.audience.clone(),
        key_id: identity.key_id.clone(),
        public_key: identity.public_key.clone(),
        fingerprint: identity.fingerprint.clone(),
    }
}

fn protocol_enrollment_status(
    status: floe_infra::remote_authorization::EnrollmentStatusResponse,
) -> RemoteAuthorityEnrollmentStatusDto {
    RemoteAuthorityEnrollmentStatusDto {
        enrollment_id: status.enrollment_id,
        key_id: status.key_id,
        fingerprint: status.fingerprint,
        local_confirmed: status.local_confirmed,
        admin_approved: status.admin_approved,
        active: status.active,
    }
}

fn remote_calendar_grant_overview(
    grant: &floe_domain::DataAccessGrant,
) -> Result<RemoteCalendarGrantOverviewDto, AgentFailure> {
    let resource = grant
        .scope()
        .resources()
        .first()
        .ok_or(AgentFailure::VaultUnavailable)?
        .as_str()
        .to_owned();
    let consumer = grant
        .scope()
        .consumers()
        .iter()
        .find(|candidate| candidate.identifier() == "calendar.expert")
        .ok_or(AgentFailure::VaultUnavailable)?
        .identifier()
        .to_owned();
    let recipient = match grant.scope().processing() {
        ProcessingRestriction::ApprovedRecipient { recipient, .. } => recipient.clone(),
        ProcessingRestriction::LocalOnly => "local_only".into(),
    };
    Ok(RemoteCalendarGrantOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: grant.source().person_id().to_string(),
        grant_id: grant.id(),
        grant_authority: grant.authority(),
        connector_id: grant.source().connector().as_str().to_owned(),
        connection_id: grant.source().connection_id().as_str().to_owned(),
        resource,
        source_authority: grant.source().source_authority(),
        execution_owner: grant.source().execution_owner().as_str().to_owned(),
        state: match grant.state() {
            GrantState::Paused => "paused",
            GrantState::Active => "active",
            GrantState::Revoked => "revoked",
        }
        .into(),
        review_required: grant.review_required(),
        consumer,
        purpose: "everyday_assistance".into(),
        recipient,
    })
}

fn remote_view_grant_overview(
    grant: &floe_domain::DataAccessGrant,
    connection_revision: Option<u64>,
) -> Result<RemoteViewGrantOverviewDto, AgentFailure> {
    let resource = grant
        .scope()
        .resources()
        .first()
        .ok_or(AgentFailure::VaultUnavailable)?
        .as_str()
        .to_owned();
    let (view_id, _) = resource
        .split_once(':')
        .ok_or(AgentFailure::VaultUnavailable)?;
    if !matches!(
        view_id,
        "mail.communication" | "work.context" | "life.logistics"
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    let consumer = grant
        .scope()
        .consumers()
        .first()
        .ok_or(AgentFailure::VaultUnavailable)?
        .identifier()
        .to_owned();
    let recipient = match grant.scope().processing() {
        ProcessingRestriction::ApprovedRecipient { recipient, .. } => recipient.clone(),
        ProcessingRestriction::LocalOnly => "local_only".into(),
    };
    Ok(RemoteViewGrantOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: grant.source().person_id().to_string(),
        grant_id: grant.id(),
        grant_authority: grant.authority(),
        view_id: view_id.into(),
        connector_id: grant.source().connector().as_str().into(),
        connection_id: grant.source().connection_id().as_str().into(),
        connection_revision,
        resource,
        source_authority: grant.source().source_authority(),
        execution_owner: grant.source().execution_owner().as_str().into(),
        state: match grant.state() {
            GrantState::Paused => "paused",
            GrantState::Active => "active",
            GrantState::Revoked => "revoked",
        }
        .into(),
        review_required: grant.review_required(),
        consumer,
        purpose: "everyday_assistance".into(),
        recipient,
    })
}

fn remote_view_grant_preview(
    person_id: PersonId,
    preview: &remote_views::RemoteViewGrantPreview,
) -> RemoteViewGrantPreviewDto {
    RemoteViewGrantPreviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: person_id.to_string(),
        view_id: preview.reference.view_id.clone(),
        connector_id: preview.reference.connector_id.clone(),
        connection_id: preview.reference.connection_id.clone(),
        connection_revision: preview.connection_revision,
        resource: preview.reference.resource.clone(),
        source_authority: preview.reference.source_authority,
        provider_identity: preview.reference.provider_identity.clone(),
        execution_owner: preview.reference.execution_owner.clone(),
        producer: protocol_producer_identity(&preview.producer),
        consumer: preview.consumer.clone(),
        purpose: "everyday_assistance".into(),
        recipient: preview.producer.audience.clone(),
    }
}

fn protocol_owner_key(key: floe_core::RemoteOwnerPublicKey) -> RemoteOwnerPublicKeyDto {
    let public_key = key.public_key.clone();
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(public_key.as_bytes())
        .unwrap_or_default();
    let fingerprint = Sha256::digest(decoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    RemoteOwnerPublicKeyDto {
        key_id: key.key_id,
        public_key,
        fingerprint,
    }
}

fn protocol_pairing_status(
    response: floe_infra::remote_authorization::PairingStatusResponse,
) -> Result<RemotePairingStatusDto, AgentFailure> {
    if response.schema_version != PROTOCOL_VERSION
        || response.pairing_id.is_empty()
        || response.person_id.is_empty()
        || response.device_id.is_empty()
        || response
            .client_id
            .as_deref()
            .is_some_and(|client_id| client_id != response.pairing_id)
        || response.issuer.as_ref().is_some_and(|issuer| {
            response
                .issuer_fingerprint
                .as_deref()
                .is_some_and(|fingerprint| fingerprint != issuer.fingerprint)
        })
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let issuer = response.issuer.map(protocol_pairing_issuer).transpose()?;
    Ok(RemotePairingStatusDto {
        schema_version: response.schema_version,
        pairing_id: response.pairing_id,
        status: response.status,
        person_id: response.person_id,
        device_id: response.device_id,
        producer: response.producer.as_ref().map(protocol_producer_identity),
        issuer,
        issuer_fingerprint: response.issuer_fingerprint,
        token: response.token,
    })
}

fn protocol_pairing_issuer(
    response: floe_infra::remote_authorization::PairingIssuerResponse,
) -> Result<RemoteOwnerPublicKeyDto, AgentFailure> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(response.public_key.as_bytes())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    if decoded.len() != 32
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != response.public_key
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let fingerprint = Sha256::digest(&decoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if fingerprint != response.fingerprint {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok(RemoteOwnerPublicKeyDto {
        key_id: response.key_id,
        public_key: response.public_key,
        fingerprint,
    })
}

impl VaultExecutionResult {
    fn new(state: AgentVaultStateDto) -> Self {
        Self {
            state,
            session: None,
            registry: None,
            calendar_experts: None,
            calendar_subject_preview: None,
            proposal: None,
            memory_review: None,
            memory: None,
            remote_producer: None,
            remote_enrollment: None,
            remote_pairing: None,
            remote_owner: None,
            remote_calendar_grant: None,
            remote_calendar_preview: None,
            remote_view_grant: None,
            remote_view_preview: None,
            personal_access: None,
            calendar_actions: None,
        }
    }

    fn ready() -> Self {
        Self::new(AgentVaultStateDto::Ready)
    }
}

async fn execute<Keys: VaultKeyProvider + Clone>(
    root: &std::path::Path,
    keys: &Keys,
    core: &FloeCore,
    local_context: &LocalContextStore,
    current: &mut Option<(PersonId, OpenVault<Keys>)>,
    job: &Job,
) -> Result<VaultExecutionResult, AgentFailure> {
    if current
        .as_ref()
        .is_some_and(|(person, _)| *person != job.person)
    {
        return Err(AgentFailure::NotFound);
    }
    if job.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    Box::pin(execute_action(
        root,
        keys,
        core,
        local_context,
        current,
        job,
    ))
    .await
}

async fn execute_action<Keys: VaultKeyProvider + Clone>(
    root: &std::path::Path,
    keys: &Keys,
    core: &FloeCore,
    local_context: &LocalContextStore,
    current: &mut Option<(PersonId, OpenVault<Keys>)>,
    job: &Job,
) -> Result<VaultExecutionResult, AgentFailure> {
    match &job.action {
        AgentVaultActionDto::Status {} => {
            if let Some((_, vault)) = current {
                vault.check_access()?;
                return Ok(VaultExecutionResult::ready());
            }
            let state = stored_vault_state(root, job.person);
            Ok(VaultExecutionResult::new(state))
        }
        AgentVaultActionDto::Create {} => {
            if current.is_some() {
                return Err(AgentFailure::Conflict);
            }
            match fs::DirBuilder::new().mode(0o700).create(root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(AgentFailure::VaultUnavailable),
            }
            let vault = OpenVault::activate(
                EncryptedAgentVault::create(root, job.person, keys.clone()).await?,
            )
            .await?;
            *current = Some((job.person, vault));
            Ok(VaultExecutionResult::ready())
        }
        AgentVaultActionDto::Unlock {} => {
            if current.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let vault = OpenVault::activate(
                EncryptedAgentVault::open(root, job.person, keys.clone()).await?,
            )
            .await?;
            *current = Some((job.person, vault));
            Ok(VaultExecutionResult::ready())
        }
        AgentVaultActionDto::Lock {} => {
            *current = None;
            Ok(VaultExecutionResult::new(AgentVaultStateDto::Locked))
        }
        AgentVaultActionDto::Session { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let session = match operation {
                AgentFixtureOperationDto::Start {} => vault.create_sample_session().await?,
                AgentFixtureOperationDto::Resume {} => vault.resume_sample_session().await?,
                AgentFixtureOperationDto::Get { session_id } => {
                    sample_session(&**vault, job.person, session_uuid(session_id)?).await?
                }
                AgentFixtureOperationDto::Recover {
                    session_id,
                    expected_revision,
                } => {
                    sample_session(&**vault, job.person, session_uuid(session_id)?).await?;
                    recover_agent_sample(
                        &**vault,
                        job.person,
                        session_uuid(session_id)?,
                        *expected_revision,
                    )
                    .await?
                }
                AgentFixtureOperationDto::Turn {
                    session_id,
                    expected_revision,
                    prompt,
                } => {
                    vault
                        .run_persisted_agent_sample(
                            AgentFixtureTurn {
                                person_id: job.person,
                                session_id: session_uuid(session_id)?,
                                expected_revision: *expected_revision,
                                prompt: super::agent_run::fixture_prompt(*prompt),
                            },
                            job.cancellation.clone(),
                            Duration::from_millis(500),
                            |event| {
                                if let Ok(mut progress) = job.progress.lock() {
                                    if progress.events.len() < 64 {
                                        progress.events.push(event);
                                    } else {
                                        job.cancellation.cancel();
                                    }
                                } else {
                                    job.cancellation.cancel();
                                }
                            },
                        )
                        .await?
                }
            };
            Ok(VaultExecutionResult {
                session: Some(session),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::Registry { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let registry = match change {
                Some(configuration) => Some(
                    vault
                        .configure_registry(
                            decode_contract(configuration)?,
                            job.cancellation.clone(),
                        )
                        .await?,
                ),
                None => vault.registry_overview().await?,
            };
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                registry,
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::CalendarExperts { setup } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if let Some(request) = setup {
                let mut request: floe_agent::CalendarExpertSetup = decode_contract(request)?;
                let connection_id = match Box::pin(calendar_grant_authority(
                    core,
                    local_context,
                    job.person,
                    &request,
                    job.cancellation.clone(),
                ))
                .await?
                {
                    Some((connection_id, source_authority, fingerprint)) => {
                        request.source_authority = Some(source_authority);
                        request.reviewed_native_subject_fingerprint = Some(fingerprint);
                        connection_id
                    }
                    None => request.setup_id.to_string(),
                };
                Box::pin(vault.install_calendar_expert_with_connection(
                    request,
                    connection_id,
                    job.cancellation.clone(),
                ))
                .await?;
            }
            let overview = Box::pin(vault.calendar_expert_overview()).await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                calendar_experts: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::CalendarSubjectPreview { request } => {
            let preview = Box::pin(calendar_subject_preview(
                core,
                local_context,
                job.person,
                request,
                job.cancellation.clone(),
            ))
            .await?;
            Ok(VaultExecutionResult {
                calendar_subject_preview: Some(preview),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::CalendarAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let mut change: floe_agent::CalendarAccessConfiguration = decode_contract(change)?;
            let overview = Box::pin(vault.calendar_expert_overview()).await?;
            let native_setup = overview
                .setups
                .iter()
                .find(|setup| setup.setup_id == change.setup_id)
                .and_then(|setup| {
                    overview
                        .views
                        .iter()
                        .find(|view| view.handle == setup.view_handle)
                        .filter(|view| {
                            matches!(
                                view.provider,
                                floe_domain::CalendarProvider::EventKit
                                    | floe_domain::CalendarProvider::Android
                            )
                        })
                        .map(|view| (setup, view))
                });
            if let Some((setup, view)) = native_setup {
                let source_request = match &change.change {
                    floe_agent::CalendarAccessChange::SetScope {
                        provider,
                        device_id,
                        calendar_ids,
                        connection_scope,
                        connection_revision,
                        source_authority,
                        reviewed_native_subject_fingerprint,
                    } => floe_agent::CalendarExpertSetup {
                        instance_id: change.instance_id,
                        expected_revision: change.expected_revision,
                        setup_id: setup.setup_id,
                        provider: *provider,
                        device_id: device_id.clone(),
                        calendar_ids: calendar_ids.clone(),
                        connection_scope: *connection_scope,
                        connection_revision: *connection_revision,
                        source_authority: *source_authority,
                        reviewed_native_subject_fingerprint: reviewed_native_subject_fingerprint
                            .clone(),
                    },
                    _ => floe_agent::CalendarExpertSetup {
                        instance_id: change.instance_id,
                        expected_revision: change.expected_revision,
                        setup_id: setup.setup_id,
                        provider: view.provider,
                        device_id: view.device_id.clone(),
                        calendar_ids: view.calendar_ids.clone(),
                        connection_scope: view.connection_scope,
                        connection_revision: view.connection_revision,
                        source_authority: setup.source_authority.or(view.source_authority),
                        reviewed_native_subject_fingerprint: setup
                            .reviewed_native_subject_fingerprint
                            .clone(),
                    },
                };
                let requires_live_source = matches!(
                    &change.change,
                    floe_agent::CalendarAccessChange::SetEnabled { enabled: true }
                        | floe_agent::CalendarAccessChange::SetScope { .. }
                );
                let connection_id = if !requires_live_source {
                    vault.calendar_grant_connection_id(setup.setup_id).await?
                } else {
                    Box::pin(calendar_grant_authority(
                        core,
                        local_context,
                        job.person,
                        &source_request,
                        job.cancellation.clone(),
                    ))
                    .await?
                    .ok_or(AgentFailure::AccessReviewRequired)?
                    .0
                };
                let overview = vault
                    .configure_calendar_access_with_connection(
                        change,
                        connection_id,
                        job.cancellation.clone(),
                    )
                    .await?;
                if job.cancellation.is_cancelled() {
                    return Err(AgentFailure::Cancelled);
                }
                return Ok(VaultExecutionResult {
                    calendar_experts: Some(overview),
                    ..VaultExecutionResult::ready()
                });
            }
            if let floe_agent::CalendarAccessChange::SetScope {
                provider,
                device_id,
                calendar_ids,
                connection_scope,
                connection_revision,
                source_authority,
                reviewed_native_subject_fingerprint,
            } = &mut change.change
            {
                if let Some((connection_id, authority, fingerprint)) =
                    Box::pin(calendar_grant_authority(
                        core,
                        local_context,
                        job.person,
                        &floe_agent::CalendarExpertSetup {
                            instance_id: change.instance_id,
                            expected_revision: change.expected_revision,
                            setup_id: change.setup_id,
                            provider: *provider,
                            device_id: device_id.clone(),
                            calendar_ids: calendar_ids.clone(),
                            connection_scope: *connection_scope,
                            connection_revision: *connection_revision,
                            source_authority: *source_authority,
                            reviewed_native_subject_fingerprint:
                                reviewed_native_subject_fingerprint.clone(),
                        },
                        job.cancellation.clone(),
                    ))
                    .await?
                {
                    *source_authority = Some(authority);
                    if let floe_agent::CalendarAccessChange::SetScope {
                        reviewed_native_subject_fingerprint,
                        ..
                    } = &mut change.change
                    {
                        *reviewed_native_subject_fingerprint = Some(fingerprint);
                    }
                    let overview = vault
                        .configure_calendar_access_with_connection(
                            change,
                            connection_id,
                            job.cancellation.clone(),
                        )
                        .await?;
                    if job.cancellation.is_cancelled() {
                        return Err(AgentFailure::Cancelled);
                    }
                    return Ok(VaultExecutionResult {
                        calendar_experts: Some(overview),
                        ..VaultExecutionResult::ready()
                    });
                }
            }
            let overview = vault
                .configure_calendar_access(change, job.cancellation.clone())
                .await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                calendar_experts: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::PersonalAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let overview = personal_grants::apply(
                vault,
                local_context,
                job.person,
                change.clone(),
                job.cancellation.clone(),
            )
            .await?;
            Ok(VaultExecutionResult {
                personal_access: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::ContactsAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let overview = personal_grants::apply_contacts(
                vault,
                local_context,
                job.person,
                change.clone(),
                job.cancellation.clone(),
            )
            .await?;
            Ok(VaultExecutionResult {
                personal_access: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::CalendarAction { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let result = execute_agent_calendar_action(
                core,
                vault,
                job.person,
                operation,
                &job.cancellation,
            )
            .await?;
            Ok(VaultExecutionResult {
                calendar_actions: Some(
                    serde_json::to_value(result).map_err(|_| AgentFailure::InvalidInput)?,
                ),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::ConversationSession { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if vault.builtin_expert_overview().await?.is_none() {
                ensure_builtin_experts(
                    vault,
                    core,
                    local_context,
                    job.person,
                    None,
                    job.cancellation.clone(),
                )
                .await?;
            }
            let session = match operation {
                AgentConversationSessionOperationDto::Start {} => vault.create_session().await?,
                AgentConversationSessionOperationDto::Resume {} => vault.resume_session().await?,
                AgentConversationSessionOperationDto::Get { session_id } => {
                    let session = vault.load(job.person, session_uuid(session_id)?).await?;
                    if session.scope.is_some()
                        || session.data_classes != [floe_agent::DataClass::Personal]
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    session
                }
                AgentConversationSessionOperationDto::Recover {
                    session_id,
                    expected_revision,
                } => {
                    conversation_turn::recover(vault, job.person, session_id, *expected_revision)
                        .await?
                }
            };
            Ok(VaultExecutionResult {
                session: Some(session),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::ConversationTurn { request } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if let Err(failure) = Box::pin(ensure_builtin_experts(
                vault,
                core,
                local_context,
                job.person,
                request.remote_route.as_ref(),
                job.cancellation.clone(),
            ))
            .await
            {
                tracing::error!(
                    failure = ?failure,
                    stage = "ensure_builtin_experts",
                    "conversation_turn_failed"
                );
                return Err(failure);
            }
            let session = match Box::pin(conversation_turn::run(
                core,
                vault,
                local_context,
                job.person,
                request,
                job.cancellation.clone(),
                |event| {
                    if let Ok(mut progress) = job.progress.lock() {
                        if progress.events.len() < 2048 {
                            progress.events.push(event);
                        } else {
                            job.cancellation.cancel();
                        }
                    } else {
                        job.cancellation.cancel();
                    }
                },
            ))
            .await
            {
                Ok(session) => session,
                Err(failure) => {
                    tracing::error!(
                        failure = ?failure,
                        stage = "runtime",
                        "conversation_turn_failed"
                    );
                    return Err(failure);
                }
            };
            Ok(VaultExecutionResult {
                session: Some(session),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::InspectProposal {
            session_id,
            invocation_id,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let reference = ExpertProposalReference {
                person_id: job.person,
                session_id: session_uuid(session_id)?,
                invocation_id: session_uuid(invocation_id)?,
            };
            let action = core
                .inspect_expert_calendar_action(
                    vault,
                    ExpertCalendarInspection {
                        reference: reference.clone(),
                        cancellation: job.cancellation.clone(),
                        deadline: tokio::time::Instant::now() + Duration::from_secs(30),
                    },
                )
                .await?;
            let proposal = AgentProposalInspectionDto {
                schema_version: PROTOCOL_VERSION,
                person_id: job.person.to_string(),
                session_id: reference.session_id.to_string(),
                invocation_id: reference.invocation_id.to_string(),
                action: action.map(calendar_action),
            };
            Ok(VaultExecutionResult {
                proposal: Some(proposal),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::MemoryReview { decision } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let pending = vault.memory_review_snapshot().await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let decision = match decision {
                Some(request) => {
                    let candidate_id = session_uuid(&request.candidate_id)?;
                    if !pending
                        .candidates
                        .iter()
                        .any(|candidate| candidate.id == candidate_id)
                    {
                        return Err(AgentFailure::NotFound);
                    }
                    Some(
                        vault
                            .decide_knowledge_candidate(
                                candidate_id,
                                match request.decision {
                                    AgentMemoryReviewDecisionKindDto::Approve => {
                                        KnowledgeDecisionKind::Approve
                                    }
                                    AgentMemoryReviewDecisionKindDto::Reject => {
                                        KnowledgeDecisionKind::Reject
                                    }
                                },
                                KnowledgeActor::User,
                                chrono::Utc::now(),
                            )
                            .await?,
                    )
                }
                None => None,
            };
            let snapshot = if decision.is_some() {
                vault.memory_review_snapshot().await?
            } else {
                pending
            };
            let candidates = snapshot
                .candidates
                .into_iter()
                .map(|candidate| encode_contract(&candidate))
                .collect::<Result<Vec<_>, _>>()?;
            let decision = decision.as_ref().map(encode_contract).transpose()?;
            Ok(VaultExecutionResult {
                memory_review: Some(AgentMemoryReviewOverviewDto {
                    schema_version: PROTOCOL_VERSION,
                    person_id: snapshot.person_id.to_string(),
                    candidates,
                    decision,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::Memory {} => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let snapshot = vault.memory_overview_snapshot(100).await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let memories = snapshot
                .memories
                .into_iter()
                .map(|memory| {
                    Ok(AgentMemorySummaryDto {
                        target_id: memory.target_id.to_string(),
                        revision: memory.revision,
                        statement: memory.statement,
                        memory_kind: encode_contract(&memory.memory_kind)?,
                        epistemic_status: encode_contract(&memory.epistemic_status)?,
                        confidence_millis: memory.confidence_millis,
                        source_count: memory.source_count,
                        origin: match memory.origin {
                            floe_knowledge::MemoryOrigin::UserProvided => {
                                AgentMemoryOriginDto::UserProvided
                            }
                            floe_knowledge::MemoryOrigin::Learned => AgentMemoryOriginDto::Learned,
                        },
                        created_at: memory.created_at,
                        valid_from: memory.valid_from,
                        valid_until: memory.valid_until,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(VaultExecutionResult {
                memory: Some(AgentMemoryOverviewDto {
                    schema_version: PROTOCOL_VERSION,
                    person_id: snapshot.person_id.to_string(),
                    saved_count: snapshot.saved_count,
                    pending_count: snapshot.pending_count,
                    memories,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::Connections {} => {
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let device_id = format!("local-{}", std::env::consts::OS);
            let connections = core
                .calendar_connector_snapshot(job.person, &device_id, chrono::Utc::now())
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .into_iter()
                .collect();
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            job.progress
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?
                .connections = Some(connections);
            Ok(VaultExecutionResult::new(current.as_ref().map_or_else(
                || stored_vault_state(root, job.person),
                |_| AgentVaultStateDto::Ready,
            )))
        }
        AgentVaultActionDto::RemotePairingPrepare {} => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            Ok(VaultExecutionResult {
                remote_owner: Some(protocol_owner_key(
                    Box::pin(vault.remote_owner_public_key()).await?,
                )),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemotePairingConfirm {
            route,
            challenge,
            polling_proof,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
            if pairing.person_id != job.person.to_string()
                || pairing.client_id != challenge.pairing_id
                || pairing.device_id.is_empty()
                || challenge.issuer.key_id.is_empty()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let core_challenge = RemotePairingChallenge {
                pairing_id: challenge.pairing_id.clone(),
                challenge_id: challenge.challenge_id.clone(),
                challenge_b64url: challenge.challenge_b64url.clone(),
                producer_signature: challenge.producer_signature.clone(),
                producer: core_producer_identity(&challenge.producer),
                issuer: floe_core::RemoteOwnerPublicKey {
                    key_id: challenge.issuer.key_id.clone(),
                    public_key: challenge.issuer.public_key.clone(),
                },
                expires_at_unix_ms: challenge.expires_at_unix_ms,
            };
            let owner_signature = Box::pin(vault.remote_sign_pairing(
                &core_challenge,
                &pairing.person_id,
                &pairing.client_id,
                &pairing.device_id,
            ))
            .await?;
            let client = RemotePairingClient::new(&route.base_url)?;
            let response = client
                .confirm(
                    &challenge.pairing_id,
                    polling_proof,
                    &owner_signature,
                    &challenge.challenge_id,
                    tokio::time::Instant::now() + Duration::from_secs(10),
                    &job.cancellation,
                )
                .await?;
            let status = RemotePairingStatusDto {
                schema_version: response.schema_version,
                pairing_id: response.pairing_id,
                status: response.status,
                person_id: pairing.person_id.clone(),
                device_id: pairing.device_id.clone(),
                producer: Some(challenge.producer.clone()),
                issuer: Some(challenge.issuer.clone()),
                issuer_fingerprint: Some(challenge.issuer.fingerprint.clone()),
                token: None,
            };
            Ok(VaultExecutionResult {
                remote_pairing: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemotePairingStatus {
            route,
            pairing_id,
            polling_proof,
        } => {
            let client = RemotePairingClient::new(&route.base_url)?;
            let response = client
                .status(
                    pairing_id,
                    polling_proof,
                    tokio::time::Instant::now() + Duration::from_secs(10),
                    &job.cancellation,
                )
                .await?;
            let status = protocol_pairing_status(response)?;
            let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
            if status.person_id != job.person.to_string()
                || pairing.person_id != status.person_id
                || pairing.device_id != status.device_id
                || pairing.client_id != status.pairing_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(VaultExecutionResult {
                remote_pairing: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemotePairingFinalize {
            route,
            pairing_id,
            polling_proof,
            challenge,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let client = RemotePairingClient::new(&route.base_url)?;
            let response = client
                .status(
                    pairing_id,
                    polling_proof,
                    tokio::time::Instant::now() + Duration::from_secs(10),
                    &job.cancellation,
                )
                .await?;
            let status = protocol_pairing_status(response)?;
            let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
            let owner = Box::pin(vault.remote_owner_public_key()).await?;
            if status.person_id != job.person.to_string()
                || status.pairing_id != challenge.pairing_id
                || challenge.pairing_id != *pairing_id
                || pairing.person_id != status.person_id
                || pairing.device_id != status.device_id
                || pairing.client_id != status.pairing_id
                || challenge.issuer != protocol_owner_key(owner)
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let producer = core_producer_identity(&challenge.producer);
            Box::pin(vault.finalize_remote_pairing(
                pairing_id,
                &RemotePairingChallenge {
                    pairing_id: challenge.pairing_id.clone(),
                    challenge_id: challenge.challenge_id.clone(),
                    challenge_b64url: challenge.challenge_b64url.clone(),
                    producer_signature: challenge.producer_signature.clone(),
                    producer: producer.clone(),
                    issuer: floe_core::RemoteOwnerPublicKey {
                        key_id: challenge.issuer.key_id.clone(),
                        public_key: challenge.issuer.public_key.clone(),
                    },
                    expires_at_unix_ms: challenge.expires_at_unix_ms,
                },
                status.status == "approved",
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_pairing: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteAuthorityInspectProducer { route } => {
            let client = RemoteAuthorizationClient::new(route)?;
            let producer = Box::pin(client.producer_identity(
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            let remote_owner = if let Some((_, vault)) = current.as_ref() {
                Some(protocol_owner_key(
                    Box::pin(vault.remote_owner_public_key()).await?,
                ))
            } else {
                None
            };
            Ok(VaultExecutionResult {
                remote_producer: Some(protocol_producer_identity(&producer)),
                remote_owner,
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteAuthorityReviewAndEnroll { route, producer } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
            if pairing.person_id != job.person.to_string()
                || pairing.client_id.trim().is_empty()
                || pairing.device_id.trim().is_empty()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let client = RemoteAuthorizationClient::new(route)?;
            let pinned = core_producer_identity(producer);
            let observed = Box::pin(client.producer_identity(
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            if core_producer_identity(producer)
                != (RemoteProducerIdentity {
                    schema_version: observed.schema_version,
                    instance_id: observed.instance_id.clone(),
                    execution_owner: observed.execution_owner.clone(),
                    audience: observed.audience.clone(),
                    key_id: observed.key_id.clone(),
                    public_key: observed.public_key.clone(),
                    fingerprint: observed.fingerprint.clone(),
                })
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Box::pin(vault.remote_pin_producer(pinned.clone())).await?;
            let status = Box::pin(client.enroll(
                vault,
                &pairing.client_id,
                &pairing.device_id,
                &pinned,
                tokio::time::Instant::now() + Duration::from_secs(30),
                &job.cancellation,
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_producer: Some(producer.clone()),
                remote_enrollment: Some(protocol_enrollment_status(status)),
                remote_owner: Some(protocol_owner_key(
                    Box::pin(vault.remote_owner_public_key()).await?,
                )),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteAuthorityEnrollmentStatus {
            route,
            enrollment_id,
        } => {
            let client = RemoteAuthorizationClient::new(route)?;
            let status = Box::pin(client.enrollment_status(
                enrollment_id,
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_enrollment: Some(protocol_enrollment_status(status)),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteCalendarGrantPreview {
            route,
            connector_id,
            connection_id,
            resource,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
            if pairing.person_id != job.person.to_string()
                || pairing.client_id.trim().is_empty()
                || pairing.device_id.trim().is_empty()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let client = RemoteAuthorizationClient::new(route)?;
            let producer = Box::pin(client.producer_identity(
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            let source_preview = Box::pin(client.calendar_source_preview(
                connector_id,
                connection_id,
                resource,
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            let connection = core
                .calendar_connection(job.person)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .ok_or(AgentFailure::AccessReviewRequired)?;
            if connection.disconnected
                || connection.connection_id != *connection_id
                || connector_id
                    != match connection.provider {
                        CalendarProvider::Google => "calendar.google",
                        CalendarProvider::Microsoft => "calendar.microsoft",
                        _ => return Err(AgentFailure::PolicyDenied),
                    }
                || !connection
                    .calendars
                    .iter()
                    .any(|calendar| calendar.calendar_id == *resource)
            {
                return Err(AgentFailure::StaleContext);
            }
            let pinned = RemoteProducerIdentity {
                schema_version: producer.schema_version,
                instance_id: producer.instance_id.clone(),
                execution_owner: producer.execution_owner.clone(),
                audience: producer.audience.clone(),
                key_id: producer.key_id.clone(),
                public_key: producer.public_key.clone(),
                fingerprint: producer.fingerprint.clone(),
            };
            if vault.remote_pinned_producer().await? != pinned {
                return Err(AgentFailure::PolicyDenied);
            }
            let source = vault
                .verify_remote_calendar_source_preview(
                    &source_preview.descriptor_b64url,
                    &source_preview.producer_signature,
                    &pairing.person_id,
                    &pairing.client_id,
                    &pairing.device_id,
                    connector_id,
                    connection_id,
                    resource,
                )
                .await?;
            if source.audience != producer.audience
                || source.execution_owner != producer.execution_owner
                || source.provider_identity.is_empty()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(VaultExecutionResult {
                remote_calendar_preview: Some(RemoteCalendarGrantPreviewDto {
                    schema_version: PROTOCOL_VERSION,
                    person_id: job.person.to_string(),
                    connector_id: connector_id.clone(),
                    connection_id: connection_id.clone(),
                    resource: resource.clone(),
                    source_authority: source.source_authority,
                    provider_identity: source.provider_identity,
                    execution_owner: source.execution_owner,
                    producer: protocol_producer_identity(&producer),
                    consumer: "calendar.expert".into(),
                    purpose: "everyday_assistance".into(),
                    recipient: "local_only".into(),
                }),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteCalendarGrantReview {
            route,
            connector_id,
            connection_id,
            resource,
            expected_producer_fingerprint,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
            if pairing.person_id != job.person.to_string()
                || pairing.client_id.trim().is_empty()
                || pairing.device_id.trim().is_empty()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let client = RemoteAuthorizationClient::new(route)?;
            let producer = Box::pin(client.producer_identity(
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            let source_preview = Box::pin(client.calendar_source_preview(
                connector_id,
                connection_id,
                resource,
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            ))
            .await?;
            if producer.fingerprint != *expected_producer_fingerprint
                || vault.remote_pinned_producer().await?.fingerprint != producer.fingerprint
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let connection = core
                .calendar_connection(job.person)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .ok_or(AgentFailure::AccessReviewRequired)?;
            let expected_connector = match connection.provider {
                CalendarProvider::Google => "calendar.google",
                CalendarProvider::Microsoft => "calendar.microsoft",
                _ => return Err(AgentFailure::PolicyDenied),
            };
            if connector_id != expected_connector
                || connection.connection_id != *connection_id
                || !connection
                    .calendars
                    .iter()
                    .any(|calendar| calendar.calendar_id == *resource)
            {
                return Err(AgentFailure::StaleContext);
            }
            let source = vault
                .verify_remote_calendar_source_preview(
                    &source_preview.descriptor_b64url,
                    &source_preview.producer_signature,
                    &pairing.person_id,
                    &pairing.client_id,
                    &pairing.device_id,
                    connector_id,
                    connection_id,
                    resource,
                )
                .await?;
            if source.audience != producer.audience
                || source.execution_owner != producer.execution_owner
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let source = GrantSourceBinding::try_new(
                job.person,
                ConnectionId::try_new(connection.connection_id.clone())
                    .map_err(|_| AgentFailure::InvalidInput)?,
                ConnectorId::try_new(connector_id.clone())
                    .map_err(|_| AgentFailure::InvalidInput)?,
                ExecutionOwnerId::try_new(producer.execution_owner.clone())
                    .map_err(|_| AgentFailure::InvalidInput)?,
                source.source_authority,
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            let consumer = GrantConsumer::builtin("calendar.expert")
                .map_err(|_| AgentFailure::InvalidInput)?;
            let scope = GrantScope::try_new(
                vec![
                    ResourceHandle::try_new(resource.clone())
                        .map_err(|_| AgentFailure::InvalidInput)?,
                ],
                vec![GrantDataCategory::Content],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Assistant],
                vec![consumer],
                ProcessingRestriction::LocalOnly,
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            let grant = vault
                .review_and_activate_remote_calendar_grant(
                    floe_domain::GrantId::new(),
                    None,
                    source,
                    scope,
                    None,
                )
                .await?;
            Ok(VaultExecutionResult {
                remote_calendar_grant: Some(remote_calendar_grant_overview(&grant)?),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteCalendarGrantStatus { grant_id } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = vault.get_data_access_grant(*grant_id).await?;
            Ok(VaultExecutionResult {
                remote_calendar_grant: Some(remote_calendar_grant_overview(&grant)?),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteCalendarGrantPause {
            grant_id,
            expected_authority,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = vault
                .pause_remote_calendar_grant(*grant_id, *expected_authority)
                .await?;
            Ok(VaultExecutionResult {
                remote_calendar_grant: Some(remote_calendar_grant_overview(&grant)?),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteViewGrantPreview {
            route,
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let preview = remote_views::preview_remote_view_grant(
                vault,
                route,
                job.person,
                view_id,
                connector_id,
                connection_id,
                resource,
                consumer,
                tokio::time::Instant::now() + Duration::from_secs(10),
                &job.cancellation,
            )
            .await?;
            Ok(VaultExecutionResult {
                remote_view_preview: Some(remote_view_grant_preview(job.person, &preview)),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteViewGrantReview {
            route,
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
            expected_producer_fingerprint,
            expected_source_authority,
            expected_connection_revision,
            expected_provider_identity,
            expected_recipient,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = remote_views::review_and_activate_remote_view_grant(
                vault,
                route,
                job.person,
                view_id,
                connector_id,
                connection_id,
                resource,
                consumer,
                expected_producer_fingerprint,
                *expected_source_authority,
                *expected_connection_revision,
                expected_provider_identity,
                expected_recipient,
                tokio::time::Instant::now() + Duration::from_secs(30),
                &job.cancellation,
            )
            .await?;
            Ok(VaultExecutionResult {
                remote_view_grant: Some(remote_view_grant_overview(
                    &grant,
                    Some(*expected_connection_revision),
                )?),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteViewGrantStatus { grant_id } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = vault.get_remote_view_grant(*grant_id).await?;
            Ok(VaultExecutionResult {
                remote_view_grant: Some(remote_view_grant_overview(&grant, None)?),
                ..VaultExecutionResult::ready()
            })
        }
        AgentVaultActionDto::RemoteViewGrantPause {
            grant_id,
            expected_authority,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = vault
                .pause_remote_view_grant(*grant_id, *expected_authority)
                .await?;
            Ok(VaultExecutionResult {
                remote_view_grant: Some(remote_view_grant_overview(&grant, None)?),
                ..VaultExecutionResult::ready()
            })
        }
    }
}

async fn execute_agent_calendar_action<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    operation: &CalendarActionOperationDto,
    cancellation: &Cancellation,
) -> Result<crate::CalendarActionsResult, AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let mode = match operation {
        CalendarActionOperationDto::GetAuthority {} => Some(vault.agent_action_policy().await?),
        CalendarActionOperationDto::SetAuthority { calendar_create } => {
            let mode = match calendar_create {
                ActionAuthorityModeDto::Allow => floe_core::ActionAuthorityMode::Allow,
                ActionAuthorityModeDto::Ask => floe_core::ActionAuthorityMode::Ask,
                ActionAuthorityModeDto::Deny => floe_core::ActionAuthorityMode::Deny,
            };
            let mode = vault.set_agent_action_policy(mode).await?;
            core.set_action_authority(person_id, mode)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            Some(mode)
        }
        _ => None,
    };
    if let Some(calendar_create) = mode {
        return Ok(crate::CalendarActionsResult {
            actions: vec![],
            writes_enabled: None,
            authority: Some(floe_core::ActionAuthority {
                person_id,
                calendar_create,
            }),
        });
    }
    let action_id = match operation {
        CalendarActionOperationDto::Get { action_id }
        | CalendarActionOperationDto::Decide { action_id, .. }
        | CalendarActionOperationDto::Execute { action_id }
        | CalendarActionOperationDto::Recover { action_id } => session_uuid(action_id)?,
        _ => return Err(AgentFailure::InvalidInput),
    };
    let stored = vault.agent_calendar_action(action_id).await?;
    if stored.person_id != person_id || stored.agent_origin.is_none() || stored.direct {
        return Err(AgentFailure::PolicyDenied);
    }
    let action = match operation {
        CalendarActionOperationDto::Get { .. } => stored,
        CalendarActionOperationDto::Decide { decision, .. } => {
            core.decide_expert_calendar_action(
                vault,
                person_id,
                action_id,
                *decision == CalendarActionDecisionDto::Approve,
                chrono::Utc::now(),
            )
            .await?
        }
        CalendarActionOperationDto::Execute { .. } | CalendarActionOperationDto::Recover { .. } => {
            if person_id.to_string() != crate::native_calendar::LOCAL_PERSON
                || stored.provider != CalendarProvider::EventKit
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let provider =
                crate::native_calendar::NativeCalendar::new(vec![stored.calendar_id.clone()]);
            if matches!(operation, CalendarActionOperationDto::Recover { .. }) {
                core.recover_expert_calendar_action(vault, person_id, action_id, &provider)
                    .await?
            } else {
                let policy = floe_core::CalendarActionPolicy {
                    person_id,
                    provider: stored.provider,
                    allowed_calendar_ids: vec![stored.calendar_id.clone()],
                    allow_create: crate::native_calendar::NativeCalendar::enabled(),
                };
                core.execute_expert_calendar_action_with_cancellation(
                    vault,
                    person_id,
                    action_id,
                    &policy,
                    &provider,
                    chrono::Utc::now,
                    cancellation.clone(),
                )
                .await?
            }
        }
        _ => return Err(AgentFailure::InvalidInput),
    };
    Ok(crate::CalendarActionsResult {
        actions: vec![action],
        writes_enabled: None,
        authority: None,
    })
}

async fn calendar_grant_authority(
    core: &FloeCore,
    _local_context: &LocalContextStore,
    person_id: PersonId,
    request: &floe_agent::CalendarExpertSetup,
    cancellation: Cancellation,
) -> Result<Option<(String, floe_domain::SourceAuthority, String)>, AgentFailure> {
    if !matches!(
        request.provider,
        floe_domain::CalendarProvider::EventKit | floe_domain::CalendarProvider::Android
    ) {
        return Ok(None);
    }
    let connection = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if connection.disconnected
        || connection.device_id != request.device_id
        || connection.provider != request.provider
        || connection.scope != request.connection_scope
        || !request.calendar_ids.iter().all(|identifier| {
            connection
                .calendars
                .iter()
                .any(|calendar| &calendar.calendar_id == identifier)
        })
    {
        return Err(AgentFailure::Conflict);
    }
    if !connection.source_authority.is_valid() {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let reviewed_authority = request
        .source_authority
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if reviewed_authority != connection.source_authority {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let expected_fingerprint = request
        .reviewed_native_subject_fingerprint
        .as_deref()
        .ok_or(AgentFailure::AccessReviewRequired)?;
    let mut calendar_ids = request.calendar_ids.clone();
    calendar_ids.sort();
    let fingerprint = match request.provider {
        #[cfg(target_os = "macos")]
        floe_domain::CalendarProvider::EventKit => {
            let access = floe_infra::native_calendar::NativeCalendarReadAccess::new(
                person_id,
                request.device_id.clone(),
                request.provider,
                calendar_ids,
                connection.connection_id.clone(),
                connection.revision,
            );
            access
                .check(CalendarReadAccessRequest {
                    person_id,
                    device_id: request.device_id.clone(),
                    provider: request.provider,
                    calendar_ids: request.calendar_ids.clone(),
                    expected_native_subject_fingerprint: None,
                    deadline: tokio::time::Instant::now() + Duration::from_secs(30),
                    cancellation,
                })
                .await?
                .native_subject_fingerprint
        }
        #[cfg(not(target_os = "macos"))]
        floe_domain::CalendarProvider::EventKit | floe_domain::CalendarProvider::Android => {
            let host_epoch = _local_context.acquisition_host_epoch(person_id)?;
            let start = chrono::Utc::now().timestamp_millis();
            let result = _local_context
                .inspect_calendar_subject(
                    LocalContextAcquisitionRequestDto {
                        request_id: Uuid::new_v4().to_string(),
                        host_epoch,
                        person_id: person_id.to_string(),
                        device_id: request.device_id.clone(),
                        connection_id: connection.connection_id.clone(),
                        connection_revision: connection.revision,
                        provider: crate::conversion::calendar_provider_to_dto(request.provider),
                        mode: LocalContextAcquisitionModeDto::InspectSubject,
                        calendar_ids,
                        range_start_unix_ms: start,
                        range_end_unix_ms: start + 86_400_000,
                        deadline_unix_ms: start + 30_000,
                        expected_native_subject_fingerprint: None,
                    },
                    cancellation,
                )
                .await?;
            result.native_subject_fingerprint_before
        }
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    if fingerprint != expected_fingerprint {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let refreshed = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if !calendar_connection_matches(
        &refreshed,
        &connection.connection_id,
        request.provider,
        &request.device_id,
        connection.scope,
        &request.calendar_ids,
        reviewed_authority,
    ) {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(Some((
        refreshed.connection_id,
        reviewed_authority,
        fingerprint,
    )))
}

async fn calendar_subject_preview(
    core: &FloeCore,
    _local_context: &LocalContextStore,
    person_id: PersonId,
    request: &CalendarSubjectPreviewRequestDto,
    cancellation: Cancellation,
) -> Result<CalendarSubjectPreviewDto, AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let mut calendar_ids = request.calendar_ids.clone();
    let provider = crate::conversion::calendar_provider_from_dto(request.provider);
    let scope = crate::conversion::calendar_scope_from_dto(request.connection_scope);
    calendar_ids.sort();
    if calendar_ids.is_empty()
        || calendar_ids.len() > 4
        || calendar_ids.windows(2).any(|pair| pair[0] == pair[1])
        || calendar_ids
            .iter()
            .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let connection = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if connection.disconnected
        || connection.connection_id != request.connection_id
        || connection.device_id != request.device_id
        || connection.provider != provider
        || connection.scope != scope
        || connection.source_authority != request.source_authority
        || !connection.source_authority.is_valid()
        || calendar_ids.iter().any(|identifier| {
            !connection
                .calendars
                .iter()
                .any(|calendar| &calendar.calendar_id == identifier)
        })
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let fingerprint = match provider {
        #[cfg(target_os = "macos")]
        floe_domain::CalendarProvider::EventKit => {
            let access = floe_infra::native_calendar::NativeCalendarReadAccess::new(
                person_id,
                request.device_id.clone(),
                provider,
                calendar_ids.clone(),
                connection.connection_id.clone(),
                connection.revision,
            );
            access
                .check(CalendarReadAccessRequest {
                    person_id,
                    device_id: request.device_id.clone(),
                    provider,
                    calendar_ids: calendar_ids.clone(),
                    expected_native_subject_fingerprint: None,
                    deadline: tokio::time::Instant::now() + Duration::from_secs(30),
                    cancellation,
                })
                .await?
                .native_subject_fingerprint
        }
        #[cfg(not(target_os = "macos"))]
        floe_domain::CalendarProvider::EventKit | floe_domain::CalendarProvider::Android => {
            let host_epoch = _local_context.acquisition_host_epoch(person_id)?;
            let range_start = chrono::Utc::now().timestamp_millis();
            let result = _local_context
                .inspect_calendar_subject(
                    LocalContextAcquisitionRequestDto {
                        request_id: Uuid::new_v4().to_string(),
                        host_epoch,
                        person_id: person_id.to_string(),
                        device_id: request.device_id.clone(),
                        connection_id: connection.connection_id.clone(),
                        connection_revision: connection.revision,
                        provider: crate::conversion::calendar_provider_to_dto(provider),
                        mode: LocalContextAcquisitionModeDto::InspectSubject,
                        calendar_ids: calendar_ids.clone(),
                        range_start_unix_ms: range_start,
                        range_end_unix_ms: range_start + 86_400_000,
                        deadline_unix_ms: range_start + 30_000,
                        expected_native_subject_fingerprint: None,
                    },
                    cancellation,
                )
                .await?;
            if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after {
                return Err(AgentFailure::StaleContext);
            }
            result.native_subject_fingerprint_before
        }
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    let refreshed = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if !calendar_connection_matches(
        &refreshed,
        &connection.connection_id,
        provider,
        &request.device_id,
        connection.scope,
        &request.calendar_ids,
        request.source_authority,
    ) {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(CalendarSubjectPreviewDto {
        provider: crate::conversion::calendar_provider_to_dto(provider),
        device_id: request.device_id.clone(),
        calendar_ids,
        connection_scope: crate::conversion::calendar_scope_to_dto(refreshed.scope),
        connection_id: refreshed.connection_id,
        connection_revision: refreshed.revision,
        source_authority: refreshed.source_authority,
        native_subject_fingerprint: fingerprint,
    })
}

fn calendar_connection_matches(
    connection: &floe_domain::CalendarConnection,
    expected_connection_id: &str,
    provider: floe_domain::CalendarProvider,
    device_id: &str,
    scope: floe_domain::CalendarScope,
    calendar_ids: &[String],
    source_authority: floe_domain::SourceAuthority,
) -> bool {
    !connection.disconnected
        && connection.connection_id == expected_connection_id
        && connection.provider == provider
        && connection.device_id == device_id
        && connection.scope == scope
        && connection.source_authority == source_authority
        && calendar_ids.iter().all(|identifier| {
            connection
                .calendars
                .iter()
                .any(|calendar| &calendar.calendar_id == identifier)
        })
}

async fn ensure_builtin_experts<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    core: &FloeCore,
    local_context: &LocalContextStore,
    person_id: PersonId,
    remote_route: Option<&AgentRemoteRouteDto>,
    cancellation: Cancellation,
) -> Result<(), AgentFailure> {
    let _ = local_context;
    let remote_available =
        remote_route.is_some_and(|route| !route.external || route.allow_external);
    let calendar = core
        .calendar_connector_snapshot(
            person_id,
            &format!("local-{}", std::env::consts::OS),
            chrono::Utc::now(),
        )
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let calendar_state = match calendar.as_ref().map(|snapshot| snapshot.connection.state) {
        Some(ConnectionState::Ready | ConnectionState::Degraded) => BuiltinSourceState::Available,
        Some(ConnectionState::Disconnected | ConnectionState::Revoked) => {
            BuiltinSourceState::Disabled
        }
        _ => BuiltinSourceState::Unavailable,
    };
    let state = |source| match source {
        BuiltinContextSource::Calendar => calendar_state,
        BuiltinContextSource::Tasks | BuiltinContextSource::ConfirmedMemory => {
            BuiltinSourceState::Available
        }
        BuiltinContextSource::Contacts
        | BuiltinContextSource::Attention
        | BuiltinContextSource::Wellbeing => BuiltinSourceState::Unavailable,
        BuiltinContextSource::Mail
        | BuiltinContextSource::ConfirmedInteractions
        | BuiltinContextSource::WorkContext
        | BuiltinContextSource::Logistics => {
            if remote_available {
                BuiltinSourceState::Available
            } else {
                BuiltinSourceState::Unavailable
            }
        }
    };
    let sources = [
        BuiltinContextSource::Calendar,
        BuiltinContextSource::Mail,
        BuiltinContextSource::Tasks,
        BuiltinContextSource::ConfirmedMemory,
        BuiltinContextSource::Contacts,
        BuiltinContextSource::ConfirmedInteractions,
        BuiltinContextSource::Attention,
        BuiltinContextSource::WorkContext,
        BuiltinContextSource::Wellbeing,
        BuiltinContextSource::Logistics,
    ]
    .into_iter()
    .map(|source| BuiltinSourceBinding {
        source,
        view_handle: builtin_source_handle(person_id, source),
        state: state(source),
    })
    .collect::<Vec<_>>();
    let existing = vault.builtin_expert_overview().await?;
    let ensured = if let Some(existing) = existing {
        if existing.setup.sources == sources {
            Ok(existing)
        } else {
            vault
                .refresh_builtin_expert_sources(
                    existing.registry.revision,
                    sources.clone(),
                    cancellation.clone(),
                )
                .await
        }
    } else {
        let revision = vault
            .registry_overview()
            .await?
            .map_or(0, |registry| registry.revision);
        vault
            .install_builtin_experts_enabled(
                BuiltinExpertSetup {
                    instance_id: vault.registry_instance_id(),
                    expected_revision: revision,
                    setup_id: Uuid::new_v5(
                        &vault.registry_instance_id(),
                        b"floe.builtin.experts.v1",
                    ),
                    sources: sources.clone(),
                },
                cancellation.clone(),
            )
            .await
    };
    let result = match ensured {
        Ok(result) => result,
        Err(AgentFailure::Conflict) => {
            let latest = vault
                .builtin_expert_overview()
                .await?
                .ok_or(AgentFailure::Conflict)?;
            if latest.setup.sources == sources {
                latest
            } else {
                vault
                    .refresh_builtin_expert_sources(
                        latest.registry.revision,
                        sources,
                        cancellation.clone(),
                    )
                    .await?
            }
        }
        Err(failure) => return Err(failure),
    };
    if result.setup.assignments.len() != BuiltinExpertKind::BUILTIN_SETUP.len() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

fn builtin_source_handle(person_id: PersonId, source: BuiltinContextSource) -> Uuid {
    let name = match source {
        BuiltinContextSource::Calendar => "calendar",
        BuiltinContextSource::Mail => "mail",
        BuiltinContextSource::Tasks => "tasks",
        BuiltinContextSource::ConfirmedMemory => "confirmed-memory",
        BuiltinContextSource::Contacts => "contacts",
        BuiltinContextSource::ConfirmedInteractions => "confirmed-interactions",
        BuiltinContextSource::Attention => "attention",
        BuiltinContextSource::WorkContext => "work-context",
        BuiltinContextSource::Wellbeing => "wellbeing",
        BuiltinContextSource::Logistics => "logistics",
    };
    Uuid::new_v5(
        &person_id.0,
        format!("floe.builtin.source.v1:{name}").as_bytes(),
    )
}

fn stored_vault_state(root: &std::path::Path, person: PersonId) -> AgentVaultStateDto {
    match fs::symlink_metadata(root.join(person.to_string())) {
        Ok(metadata) if metadata.is_dir() => AgentVaultStateDto::Locked,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AgentVaultStateDto::Missing,
        _ => AgentVaultStateDto::Unavailable,
    }
}

fn calendar_action(action: floe_core::CalendarAction) -> AgentProposalActionDto {
    AgentProposalActionDto {
        action_id: action.id.to_string(),
        execution_id: action.execution_id.to_string(),
        expires_at: action.expires_at,
        status: match action.state {
            CalendarActionState::Pending => AgentProposalStatusDto::Pending,
            CalendarActionState::Approved => AgentProposalStatusDto::Approved,
            CalendarActionState::Rejected => AgentProposalStatusDto::Rejected,
            CalendarActionState::Executing => AgentProposalStatusDto::Executing,
            CalendarActionState::Blocked { .. } => AgentProposalStatusDto::Blocked,
            CalendarActionState::Unknown { .. } => AgentProposalStatusDto::Unknown,
            CalendarActionState::Succeeded { .. } => AgentProposalStatusDto::Succeeded,
        },
    }
}

fn session_uuid(value: &str) -> Result<Uuid, AgentFailure> {
    Uuid::parse_str(value).map_err(|_| AgentFailure::InvalidInput)
}

fn decode_contract<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, AgentFailure> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|_| AgentFailure::InvalidInput)
}

fn encode_contracts<Input: Serialize, Output: DeserializeOwned>(
    values: Vec<Input>,
) -> Result<Vec<Output>, AgentFailure> {
    values
        .iter()
        .map(decode_contract)
        .collect::<Result<Vec<_>, _>>()
}

fn encode_contract<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, AgentFailure> {
    decode_contract(value)
}

fn failure_envelope(failure: &AgentFailure, stage: &str, request_id: &str) -> AgentVaultFailureDto {
    let kind = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let classification = classify_failure(failure, stage);
    AgentVaultFailureDto {
        schema_version: PROTOCOL_VERSION,
        domain: classification.domain,
        category: classification.category,
        reason_code: classification.reason_code,
        kind: kind.clone(),
        stage: stage.into(),
        safe_actions: classification.safe_actions,
        affected_refs: vec![],
        incident_id: request_id.into(),
        retry_policy: classification.retry_policy,
        retryable: classification.retryable,
        recovery_action: recovery_action(failure, stage),
        correlation_request_id: request_id.into(),
    }
}

struct FailureClassification {
    domain: AgentFailureDomain,
    category: AgentFailureCategory,
    reason_code: String,
    safe_actions: Vec<AgentFailureSafeAction>,
    retry_policy: AgentRetryPolicy,
    retryable: bool,
}

fn classify_failure(failure: &AgentFailure, stage: &str) -> FailureClassification {
    let reason_code = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let source_stage = matches!(
        stage,
        "calendar_access"
            | "calendar_experts"
            | "calendar_subject_preview"
            | "calendar_action"
            | "personal_access"
            | "contacts_access"
            | "remote_authority_inspect_producer"
            | "remote_authority_review_and_enroll"
            | "remote_authority_enrollment_status"
            | "remote_pairing_prepare"
            | "remote_pairing_confirm"
            | "remote_pairing_status"
            | "remote_pairing_finalize"
            | "remote_calendar_grant_preview"
            | "remote_calendar_grant_review"
            | "remote_calendar_grant_status"
            | "remote_calendar_grant_pause"
            | "remote_view_grant_preview"
            | "remote_view_grant_review"
            | "remote_view_grant_status"
            | "remote_view_grant_pause"
    );
    let (domain, category, reason_code) = match failure {
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => (
            AgentFailureDomain::Vault,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        AgentFailure::PolicyDenied if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            "session_integrity".into(),
        ),
        AgentFailure::PolicyDenied if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Security,
            "data_release_or_policy_block".into(),
        ),
        AgentFailure::PolicyDenied if source_stage => (
            AgentFailureDomain::Source,
            AgentFailureCategory::Security,
            "source_access_denied".into(),
        ),
        AgentFailure::PolicyDenied => (
            AgentFailureDomain::App,
            AgentFailureCategory::Internal,
            "internal_policy_invariant".into(),
        ),
        AgentFailure::CapabilityDenied => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::Security,
            "capability_access_denied".into(),
        ),
        AgentFailure::AccessReviewRequired => (
            AgentFailureDomain::Source,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::ConsentRequired => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::CapabilityUnavailable => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if source_stage => (
            AgentFailureDomain::Source,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::ServerModelRequestRejected
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => (
            AgentFailureDomain::Capability,
            if matches!(
                failure,
                AgentFailure::InvalidModelOutput
                    | AgentFailure::LocalModelInvalidOutput
                    | AgentFailure::ServerModelInvalidOutput
            ) {
                AgentFailureCategory::Integrity
            } else {
                AgentFailureCategory::Transient
            },
            reason_code.clone(),
        ),
        AgentFailure::CredentialExpired | AgentFailure::QuotaExceeded => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::Interrupted | AgentFailure::DeadlineExceeded | AgentFailure::Stalled => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        _ if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        _ if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        _ => (
            AgentFailureDomain::App,
            AgentFailureCategory::Internal,
            reason_code,
        ),
    };

    let mut safe_actions = match failure {
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => {
            vec![AgentFailureSafeAction::ReopenVault]
        }
        _ if stage == "conversation_session" => {
            vec![AgentFailureSafeAction::StartNewSession]
        }
        AgentFailure::PolicyDenied if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ExportDiagnostics,
        ],
        AgentFailure::PolicyDenied if source_stage => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ReviewSource,
        ],
        AgentFailure::AccessReviewRequired => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ReviewSource,
        ],
        AgentFailure::ConsentRequired => vec![],
        AgentFailure::Conflict if stage == "conversation_session" => vec![
            AgentFailureSafeAction::StartNewSession,
            AgentFailureSafeAction::RefreshSession,
        ],
        AgentFailure::Conflict if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::RefreshSession,
            AgentFailureSafeAction::StartNewSession,
        ],
        AgentFailure::StaleContext if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::RefreshSession,
            AgentFailureSafeAction::StartNewSession,
        ],
        AgentFailure::StaleContext => vec![AgentFailureSafeAction::ReviewSource],
        AgentFailure::CredentialExpired => vec![AgentFailureSafeAction::RefreshSession],
        AgentFailure::CapabilityUnavailable if source_stage => {
            vec![AgentFailureSafeAction::ContinueWithoutSource]
        }
        AgentFailure::Cancelled => vec![],
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => vec![AgentFailureSafeAction::Retry],
        _ if stage == "conversation_turn" && !matches!(failure, AgentFailure::Cancelled) => {
            vec![AgentFailureSafeAction::StartNewSession]
        }
        _ => vec![],
    };
    if !matches!(failure, AgentFailure::Cancelled)
        && matches!(
            category,
            AgentFailureCategory::Internal | AgentFailureCategory::Security
        )
        && !safe_actions.contains(&AgentFailureSafeAction::ExportDiagnostics)
    {
        safe_actions.push(AgentFailureSafeAction::ExportDiagnostics);
    }
    let retry_policy = if safe_actions.contains(&AgentFailureSafeAction::Retry) {
        if matches!(
            failure,
            AgentFailure::ServerModelTimeout
                | AgentFailure::Interrupted
                | AgentFailure::DeadlineExceeded
                | AgentFailure::Stalled
        ) {
            AgentRetryPolicy::Backoff
        } else {
            AgentRetryPolicy::Immediate
        }
    } else {
        AgentRetryPolicy::Never
    };
    let retryable = !matches!(retry_policy, AgentRetryPolicy::Never);
    FailureClassification {
        domain,
        category,
        reason_code,
        safe_actions,
        retry_policy,
        retryable,
    }
}

fn recovery_action(failure: &AgentFailure, stage: &str) -> AgentVaultRecoveryActionDto {
    match failure {
        AgentFailure::Conflict | AgentFailure::DeadlineExceeded | AgentFailure::Interrupted
            if stage == "calendar_action" =>
        {
            AgentVaultRecoveryActionDto::Reconcile
        }
        AgentFailure::Conflict if matches!(stage, "conversation_session" | "conversation_turn") => {
            AgentVaultRecoveryActionDto::RefreshSession
        }
        AgentFailure::Conflict | AgentFailure::StaleContext => {
            AgentVaultRecoveryActionDto::RefreshContext
        }
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => AgentVaultRecoveryActionDto::RetryRead,
        AgentFailure::AccessReviewRequired
            if matches!(
                stage,
                "calendar_access"
                    | "calendar_experts"
                    | "calendar_subject_preview"
                    | "calendar_action"
                    | "personal_access"
                    | "contacts_access"
                    | "conversation_turn"
            ) =>
        {
            AgentVaultRecoveryActionDto::ReviewSource
        }
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => {
            AgentVaultRecoveryActionDto::ReopenVault
        }
        _ => AgentVaultRecoveryActionDto::None,
    }
}

async fn sample_session(
    store: &impl SessionStore,
    person: PersonId,
    id: Uuid,
) -> Result<AgentSession, AgentFailure> {
    let session = store.load(person, id).await?;
    if session.scope.is_some() || session.data_classes != [floe_agent::DataClass::Synthetic] {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use floe_core::VaultKey;
    use ring::signature::{self, Ed25519KeyPair, KeyPair};
    use serde_json::json;
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::Condvar,
        thread,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    mod calendar_experts;
    mod learner_worker;
    mod memory_review;
    mod proposals;

    impl Worker {
        fn new(root: PathBuf, keys: Keys) -> Result<Self, AgentFailure> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let core = runtime.block_on(FloeCore::open(":memory:")).unwrap();
            Self::with_core(
                root,
                keys,
                Arc::new(core),
                Arc::new(LocalContextStore::default()),
            )
        }
    }

    #[derive(Clone, Default)]
    struct Keys(Arc<KeyState>);
    #[derive(Default)]
    struct KeyState {
        values: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
        paused: Mutex<bool>,
        wake: Condvar,
        entered: AtomicBool,
        unavailable: AtomicBool,
    }
    impl VaultKeyProvider for Keys {
        fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
            let mut paused = self.0.paused.lock().unwrap();
            self.0.entered.store(true, Ordering::Release);
            while *paused {
                paused = self.0.wake.wait(paused).unwrap();
            }
            if self.0.unavailable.load(Ordering::Acquire) {
                return Err(AgentFailure::VaultUnavailable);
            }
            self.0
                .values
                .lock()
                .unwrap()
                .get(&(person, vault))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }
        fn insert(
            &self,
            person: PersonId,
            vault: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .values
                .lock()
                .unwrap()
                .insert((person, vault), *key.as_bytes());
            Ok(())
        }
    }

    fn wait(worker: &Worker, person: PersonId, id: Uuid) -> VaultJobResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = worker
                .request(
                    person,
                    id,
                    AgentVaultOperationDto::Poll { after_sequence: 0 },
                )
                .unwrap();
            if result.done {
                return result;
            }
            assert!(Instant::now() < deadline, "Vault job did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn perform(worker: &Worker, person: PersonId, action: AgentVaultActionDto) -> VaultJobResult {
        let id = Uuid::new_v4();
        worker
            .request(person, id, AgentVaultOperationDto::Submit { action })
            .unwrap();
        let result = wait(worker, person, id);
        worker
            .request(person, id, AgentVaultOperationDto::Release {})
            .unwrap();
        result
    }

    #[test]
    fn failure_recovery_is_stage_aware_and_conservative() {
        let conversation =
            failure_envelope(&AgentFailure::Conflict, "conversation_turn", "request");
        assert_eq!(
            conversation.recovery_action,
            AgentVaultRecoveryActionDto::RefreshSession
        );
        assert!(!conversation.retryable);
        assert_eq!(conversation.domain, AgentFailureDomain::Turn);
        assert_eq!(conversation.category, AgentFailureCategory::Integrity);
        assert_eq!(conversation.reason_code, "conflict");
        assert!(
            conversation
                .safe_actions
                .contains(&AgentFailureSafeAction::StartNewSession)
        );

        let session_policy = failure_envelope(
            &AgentFailure::PolicyDenied,
            "conversation_session",
            "request",
        );
        assert_eq!(session_policy.domain, AgentFailureDomain::Session);
        assert_eq!(session_policy.category, AgentFailureCategory::Integrity);
        assert_eq!(session_policy.reason_code, "session_integrity");
        assert_eq!(
            session_policy.safe_actions,
            vec![AgentFailureSafeAction::StartNewSession]
        );

        let release_block =
            failure_envelope(&AgentFailure::PolicyDenied, "conversation_turn", "request");
        assert_eq!(release_block.domain, AgentFailureDomain::Turn);
        assert_eq!(release_block.category, AgentFailureCategory::Security);
        assert_eq!(release_block.reason_code, "data_release_or_policy_block");
        assert!(
            release_block
                .safe_actions
                .contains(&AgentFailureSafeAction::ContinueWithoutSource)
        );
        assert!(
            release_block
                .safe_actions
                .contains(&AgentFailureSafeAction::ExportDiagnostics)
        );

        let model_refusal = failure_envelope(
            &AgentFailure::CapabilityDenied,
            "conversation_turn",
            "request",
        );
        assert_eq!(model_refusal.reason_code, "capability_access_denied");
        assert_eq!(model_refusal.category, AgentFailureCategory::Security);
        assert!(
            model_refusal
                .safe_actions
                .contains(&AgentFailureSafeAction::ExportDiagnostics)
        );

        let model_retry = failure_envelope(
            &AgentFailure::ServerModelTimeout,
            "conversation_turn",
            "request",
        );
        assert_eq!(model_retry.retry_policy, AgentRetryPolicy::Backoff);
        assert!(model_retry.retryable);
        assert!(
            model_retry
                .safe_actions
                .contains(&AgentFailureSafeAction::Retry)
        );

        let setup = failure_envelope(&AgentFailure::Conflict, "calendar_access", "request");
        assert_eq!(
            setup.recovery_action,
            AgentVaultRecoveryActionDto::RefreshContext
        );

        let review = failure_envelope(
            &AgentFailure::AccessReviewRequired,
            "calendar_access",
            "request",
        );
        assert_eq!(
            review.recovery_action,
            AgentVaultRecoveryActionDto::ReviewSource
        );
        let turn_review = failure_envelope(
            &AgentFailure::AccessReviewRequired,
            "conversation_turn",
            "request",
        );
        assert_eq!(
            turn_review.recovery_action,
            AgentVaultRecoveryActionDto::ReviewSource
        );
        let model_consent = failure_envelope(
            &AgentFailure::ConsentRequired,
            "conversation_turn",
            "request",
        );
        assert_eq!(model_consent.domain, AgentFailureDomain::Capability);
        assert_eq!(
            model_consent.category,
            AgentFailureCategory::UserConfiguration
        );
        assert!(model_consent.safe_actions.is_empty());
        assert_eq!(
            model_consent.recovery_action,
            AgentVaultRecoveryActionDto::None
        );

        for failure in [
            AgentFailure::CapabilityUnavailable,
            AgentFailure::Cancelled,
            AgentFailure::Interrupted,
            AgentFailure::DeadlineExceeded,
        ] {
            let envelope = failure_envelope(&failure, "calendar_experts", "request");
            assert_eq!(envelope.recovery_action, AgentVaultRecoveryActionDto::None);
            assert!(!envelope.retryable);
        }
    }

    #[test]
    fn remote_enrollment_requires_saved_pairing_before_network_access() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let person = PersonId::new();
        let created = perform(&worker, person, AgentVaultActionDto::Create {});
        assert_eq!(
            created.failure, None,
            "create failed: {:?}",
            created.failure
        );
        let route = AgentRemoteRouteDto {
            base_url: "http://not-loopback.invalid".into(),
            bearer_token: "not-a-real-token".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            recipient: None,
            calendar_connections: vec![],
            pairing: None,
        };
        let producer = RemoteProducerIdentityDto {
            schema_version: 1,
            instance_id: Uuid::new_v4().to_string(),
            execution_owner: Uuid::new_v4().to_string(),
            audience: "invalid".into(),
            key_id: Uuid::new_v4().to_string(),
            public_key: "invalid".into(),
            fingerprint: "invalid".into(),
        };
        let result = perform(
            &worker,
            person,
            AgentVaultActionDto::RemoteAuthorityReviewAndEnroll { route, producer },
        );
        assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
    }

    #[test]
    fn remote_enrollment_rejects_pairing_for_another_person_before_network_access() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let person = PersonId::new();
        let created = perform(&worker, person, AgentVaultActionDto::Create {});
        assert_eq!(
            created.failure, None,
            "create failed: {:?}",
            created.failure
        );
        let route = AgentRemoteRouteDto {
            base_url: "http://not-loopback.invalid".into(),
            bearer_token: "not-a-real-token".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            recipient: None,
            calendar_connections: vec![],
            pairing: Some(AgentRemotePairingDto {
                client_id: "saved-client".into(),
                person_id: PersonId::new().to_string(),
                device_id: "saved-device".into(),
            }),
        };
        let producer = RemoteProducerIdentityDto {
            schema_version: 1,
            instance_id: Uuid::new_v4().to_string(),
            execution_owner: Uuid::new_v4().to_string(),
            audience: "invalid".into(),
            key_id: Uuid::new_v4().to_string(),
            public_key: "invalid".into(),
            fingerprint: "invalid".into(),
        };
        let result = perform(
            &worker,
            person,
            AgentVaultActionDto::RemoteAuthorityReviewAndEnroll { route, producer },
        );
        assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
    }

    #[test]
    fn remote_enrollment_uses_signed_challenge_and_stays_pending_admin() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let created = perform(&worker, person, AgentVaultActionDto::Create {});
        assert_eq!(
            created.failure, None,
            "create failed: {:?}",
            created.failure
        );

        let producer_seed = [2_u8; 32];
        let producer_key = Ed25519KeyPair::from_seed_unchecked(&producer_seed).unwrap();
        let producer_public_key = URL_SAFE_NO_PAD.encode(producer_key.public_key().as_ref());
        let producer_fingerprint = Sha256::digest(producer_key.public_key().as_ref())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let producer_instance = "00000000-0000-4000-8000-000000000001";
        let producer_execution_owner = "00000000-0000-4000-8000-000000000002";
        let producer_key_id = "00000000-0000-4000-8000-000000000003";
        let producer_audience = format!("floe.server:{producer_instance}");
        let producer = RemoteProducerIdentityDto {
            schema_version: 1,
            instance_id: producer_instance.into(),
            execution_owner: producer_execution_owner.into(),
            audience: producer_audience.clone(),
            key_id: producer_key_id.into(),
            public_key: producer_public_key.clone(),
            fingerprint: producer_fingerprint,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_producer = producer.clone();
        let server = thread::spawn(move || {
            serve_signed_enrollment(listener, producer_key, server_producer, person)
        });
        let route = AgentRemoteRouteDto {
            base_url: format!("http://127.0.0.1:{}", address.port()),
            bearer_token: "secret_token_value_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            recipient: None,
            calendar_connections: vec![],
            pairing: Some(AgentRemotePairingDto {
                client_id: "client-1".into(),
                person_id: person.to_string(),
                device_id: "device-1".into(),
            }),
        };
        let result = perform(
            &worker,
            person,
            AgentVaultActionDto::RemoteAuthorityReviewAndEnroll { route, producer },
        );
        let server_result = server.join().unwrap();
        assert!(
            server_result.is_ok(),
            "fake producer failed: {server_result:?}"
        );
        assert_eq!(
            result.failure, None,
            "enrollment failed: {:?}",
            result.failure
        );
        let status = result.remote_enrollment.expect("enrollment status");
        assert!(status.local_confirmed);
        assert!(!status.admin_approved);
        assert!(!status.active);
    }

    fn serve_signed_enrollment(
        listener: TcpListener,
        producer_key: Ed25519KeyPair,
        producer: RemoteProducerIdentityDto,
        person: PersonId,
    ) -> Result<(), String> {
        let mut owner_public_key = None;
        let mut challenge_bytes = None;
        for request_number in 0..5 {
            let (mut stream, _) = listener.accept().map_err(|error| error.to_string())?;
            let (path, body) = read_http_request(&mut stream)?;
            let response = match (request_number, path.as_str()) {
                (0, "/v1/authority/producer") | (1, "/v1/authority/producer") => {
                    serde_json::to_vec(&producer).map_err(|error| error.to_string())?
                }
                (2, "/v1/authority/enrollment/begin") => {
                    let begin: serde_json::Value =
                        serde_json::from_slice(&body).map_err(|error| error.to_string())?;
                    let key_id = begin
                        .get("key_id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| "missing owner key id".to_owned())?;
                    let owner_key = begin
                        .get("public_key")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| "missing owner public key".to_owned())?;
                    owner_public_key = Some(
                        URL_SAFE_NO_PAD
                            .decode(owner_key)
                            .map_err(|error| error.to_string())?,
                    );
                    let challenge_id = "00000000-0000-4000-8000-000000000010";
                    let nonce = URL_SAFE_NO_PAD.encode([7_u8; 32]);
                    let issued_at = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|error| error.to_string())?
                        .as_millis() as i64;
                    let challenge = serde_json::to_vec(&json!({
                        "v": 1,
                        "operation": "enrollment",
                        "challenge_id": challenge_id,
                        "nonce": nonce,
                        "key_id": key_id,
                        "person_id": person.to_string(),
                        "client_id": "client-1",
                        "device_id": "device-1",
                        "audience": producer.audience.clone(),
                        "purpose": "owner_enrollment",
                        "consumer": "owner",
                        "issued_at_unix_ms": issued_at,
                        "expires_at_unix_ms": issued_at + 30_000,
                    }))
                    .map_err(|error| error.to_string())?;
                    challenge_bytes = Some(challenge.clone());
                    let mut signed = b"floe.remote.producer.v1\0".to_vec();
                    signed.extend_from_slice(&challenge);
                    let signature = producer_key.sign(&signed);
                    serde_json::to_vec(&json!({
                        "schema_version": 1,
                        "instance_id": producer.instance_id.clone(),
                        "execution_owner": producer.execution_owner.clone(),
                        "audience": producer.audience.clone(),
                        "producer_key_id": producer.key_id.clone(),
                        "producer_public_key": producer.public_key.clone(),
                        "producer_fingerprint": producer.fingerprint.clone(),
                        "enrollment_id": "00000000-0000-4000-8000-000000000011",
                        "challenge_id": challenge_id,
                        "key_id": key_id,
                        "fingerprint": "owner-fingerprint",
                        "challenge_b64url": URL_SAFE_NO_PAD.encode(challenge),
                        "producer_signature": URL_SAFE_NO_PAD.encode(signature.as_ref()),
                        "expires": null,
                    }))
                    .map_err(|error| error.to_string())?
                }
                (3, "/v1/authority/enrollment/complete") => {
                    let complete: serde_json::Value =
                        serde_json::from_slice(&body).map_err(|error| error.to_string())?;
                    let owner_signature = complete
                        .get("signature")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| "missing owner signature".to_owned())?;
                    let owner_signature = URL_SAFE_NO_PAD
                        .decode(owner_signature)
                        .map_err(|error| error.to_string())?;
                    let mut owner_message = b"floe.remote.authorization.v1\0".to_vec();
                    owner_message.extend_from_slice(
                        challenge_bytes
                            .as_deref()
                            .ok_or_else(|| "missing challenge".to_owned())?,
                    );
                    signature::UnparsedPublicKey::new(
                        &signature::ED25519,
                        owner_public_key
                            .as_deref()
                            .ok_or_else(|| "missing owner key".to_owned())?,
                    )
                    .verify(&owner_message, &owner_signature)
                    .map_err(|_| "owner signature did not verify".to_owned())?;
                    br#"{}"#.to_vec()
                }
                (4, "/v1/authority/enrollment/00000000-0000-4000-8000-000000000011") => {
                    serde_json::to_vec(&json!({
                        "enrollment_id": "00000000-0000-4000-8000-000000000011",
                        "key_id": "00000000-0000-4000-8000-000000000012",
                        "fingerprint": "owner-fingerprint",
                        "local_confirmed": true,
                        "admin_approved": false,
                        "active": false,
                    }))
                    .map_err(|error| error.to_string())?
                }
                _ => return Err(format!("unexpected request {request_number}: {path}")),
            };
            write_http_response(&mut stream, &response)?;
        }
        Ok(())
    }

    fn read_http_request(stream: &mut TcpStream) -> Result<(String, Vec<u8>), String> {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        let header_end = loop {
            let read = stream
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("request ended before headers".into());
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break end + 4;
            }
            if bytes.len() > 16 * 1024 {
                return Err("request headers too large".into());
            }
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then_some(value)
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let read = stream
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("request ended before body".into());
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        let request_line = headers
            .lines()
            .next()
            .ok_or_else(|| "missing request line".to_owned())?;
        let path = request_line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| "missing request path".to_owned())?
            .to_owned();
        Ok((
            path,
            bytes[header_end..header_end + content_length].to_vec(),
        ))
    }

    fn write_http_response(stream: &mut TcpStream, body: &[u8]) -> Result<(), String> {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(header.as_bytes())
            .and_then(|_| stream.write_all(body))
            .map_err(|error| error.to_string())
    }

    #[test]
    fn connections_are_inspectable_without_initializing_a_vault() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();

        let result = perform(&worker, person, AgentVaultActionDto::Connections {});

        assert!(result.failure.is_none());
        assert_eq!(result.connections, Some(vec![]));
        assert!(!directory.path().join("vaults").exists());
    }

    #[test]
    fn calendar_action_policy_round_trips_through_the_unlocked_vault_worker() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        perform(&worker, person, AgentVaultActionDto::Create {});
        let changed = perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarAction {
                operation: CalendarActionOperationDto::SetAuthority {
                    calendar_create: ActionAuthorityModeDto::Deny,
                },
            },
        );
        assert_eq!(changed.failure, None);
        assert_eq!(
            changed.calendar_actions.unwrap()["authority"]["calendar_create"],
            "deny"
        );
        perform(&worker, person, AgentVaultActionDto::Lock {});
        let locked = perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarAction {
                operation: CalendarActionOperationDto::GetAuthority {},
            },
        );
        assert_eq!(locked.failure, Some(AgentFailure::VaultUnavailable));
        perform(&worker, person, AgentVaultActionDto::Unlock {});
        let restored = perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarAction {
                operation: CalendarActionOperationDto::GetAuthority {},
            },
        );
        assert_eq!(restored.failure, None);
        assert_eq!(
            restored.calendar_actions.unwrap()["authority"]["calendar_create"],
            "deny"
        );
    }

    #[test]
    fn accepted_foreground_work_preempts_the_active_learner() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let lease = worker.learner_scheduling.try_start().unwrap().unwrap();
        let learner = lease.cancellation();
        let id = Uuid::new_v4();

        worker
            .request(
                person,
                id,
                AgentVaultOperationDto::Submit {
                    action: AgentVaultActionDto::Status {},
                },
            )
            .unwrap();

        assert!(learner.is_cancelled());
        wait(&worker, person, id);
        worker
            .request(person, id, AgentVaultOperationDto::Release {})
            .unwrap();
    }

    #[test]
    fn general_conversations_are_encrypted_personal_sessions_not_samples() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        perform(&worker, person, AgentVaultActionDto::Create {});
        let created = perform(
            &worker,
            person,
            AgentVaultActionDto::ConversationSession {
                operation: AgentConversationSessionOperationDto::Start {},
            },
        )
        .session
        .unwrap();
        assert!(created.scope.is_none());
        assert_eq!(created.data_classes, [floe_agent::DataClass::Personal]);
        let installed_revision = perform(
            &worker,
            person,
            AgentVaultActionDto::Registry { change: None },
        )
        .registry
        .unwrap()
        .revision;
        let resumed = perform(
            &worker,
            person,
            AgentVaultActionDto::ConversationSession {
                operation: AgentConversationSessionOperationDto::Resume {},
            },
        )
        .session
        .unwrap();
        assert_eq!(resumed.id, created.id);
        assert_eq!(
            perform(
                &worker,
                person,
                AgentVaultActionDto::Registry { change: None },
            )
            .registry
            .unwrap()
            .revision,
            installed_revision,
        );
        let sample = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Resume {},
            },
        )
        .session
        .unwrap();
        assert_ne!(sample.id, created.id);
        assert_eq!(sample.data_classes, [floe_agent::DataClass::Synthetic]);
    }

    #[test]
    fn registry_jobs_are_read_only_until_explicit_change_and_reconcile_duplicate_submits() {
        use floe_agent::{RegistryConfiguration, RegistryConfigurationTarget};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        let person = PersonId::new();
        let worker = Worker::new(root.clone(), Keys::default()).unwrap();
        assert_eq!(
            perform(
                &worker,
                person,
                AgentVaultActionDto::Registry { change: None }
            )
            .failure,
            Some(AgentFailure::VaultUnavailable)
        );
        assert!(!root.exists());
        perform(&worker, person, AgentVaultActionDto::Create {});
        let empty = perform(
            &worker,
            person,
            AgentVaultActionDto::Registry { change: None },
        );
        assert_eq!(empty.state, Some(AgentVaultStateDto::Ready));
        assert!(empty.registry.is_none());
        let session = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Start {},
            },
        )
        .session
        .unwrap();
        perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Turn {
                    session_id: session.id.to_string(),
                    expected_revision: 0,
                    prompt: AgentFixturePromptDto::Today,
                },
            },
        );
        let before = perform(
            &worker,
            person,
            AgentVaultActionDto::Registry { change: None },
        )
        .registry
        .unwrap();
        let assignment = before
            .assignments
            .iter()
            .find(|assignment| assignment.granted_tool_count == 1)
            .unwrap();
        let action = AgentVaultActionDto::Registry {
            change: Some(
                encode_contract(&RegistryConfiguration {
                    instance_id: before.instance_id,
                    expected_revision: before.revision,
                    target: RegistryConfigurationTarget::Assignment {
                        id: assignment.id,
                        enabled: false,
                    },
                })
                .unwrap(),
            ),
        };
        let id = Uuid::new_v4();
        worker
            .request(
                person,
                id,
                AgentVaultOperationDto::Submit {
                    action: action.clone(),
                },
            )
            .unwrap();
        let done = wait(&worker, person, id);
        assert_eq!(
            worker
                .request(
                    person,
                    id,
                    AgentVaultOperationDto::Submit {
                        action: action.clone()
                    }
                )
                .unwrap(),
            done
        );
        assert_eq!(
            worker.request(
                PersonId::new(),
                id,
                AgentVaultOperationDto::Poll { after_sequence: 0 }
            ),
            Err(AgentFailure::NotFound)
        );
        assert!(done.session.is_none() && done.events.is_empty());
        let after = done.registry.as_ref().unwrap();
        assert_eq!(after.revision, before.revision + 1);
        assert!(
            !after
                .assignments
                .iter()
                .find(|entry| entry.id == assignment.id)
                .unwrap()
                .enabled
        );
        worker
            .request(person, id, AgentVaultOperationDto::Release {})
            .unwrap();
        assert_eq!(
            perform(&worker, person, action).failure,
            Some(AgentFailure::Conflict)
        );
        perform(&worker, person, AgentVaultActionDto::Lock {});
        perform(&worker, person, AgentVaultActionDto::Unlock {});
        assert_eq!(
            perform(
                &worker,
                person,
                AgentVaultActionDto::Registry { change: None }
            )
            .registry
            .as_ref(),
            Some(after)
        );
        let session = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Start {},
            },
        )
        .session
        .unwrap();
        let denied = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Turn {
                    session_id: session.id.to_string(),
                    expected_revision: 0,
                    prompt: AgentFixturePromptDto::Today,
                },
            },
        )
        .session
        .unwrap();
        assert_eq!(denied.messages.len(), 1);
        assert_eq!(
            denied.last_outcome,
            Some(floe_agent::AgentOutcome::Halted {
                reason: AgentFailure::InvalidModelOutput,
            })
        );
        let current = perform(
            &worker,
            person,
            AgentVaultActionDto::Registry { change: None },
        )
        .registry
        .unwrap();
        assert_eq!(current, *after);
        perform(&worker, person, AgentVaultActionDto::Lock {});
    }

    #[test]
    fn sample_transport_cannot_read_or_recover_personal_sessions() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let keys = Keys::default();
        let person = PersonId::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let session = runtime.block_on(async {
            let vault = EncryptedAgentVault::create(&root, person, keys.clone())
                .await
                .unwrap();
            let mut session = vault.create_session().await.unwrap();
            session.active_turn = Some(Uuid::new_v4());
            session.revision = 1;
            vault.compare_and_swap(&session, 0).await.unwrap();
            session
        });
        let worker = Worker::new(root, keys).unwrap();
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Unlock {}).state,
            Some(AgentVaultStateDto::Ready)
        );
        for operation in [
            AgentFixtureOperationDto::Get {
                session_id: session.id.to_string(),
            },
            AgentFixtureOperationDto::Recover {
                session_id: session.id.to_string(),
                expected_revision: session.revision,
            },
        ] {
            let result = perform(&worker, person, AgentVaultActionDto::Session { operation });
            assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
            assert_eq!(result.state, Some(AgentVaultStateDto::Ready));
            assert!(result.session.is_none());
            assert!(result.events.is_empty());
        }
        let sample = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Resume {},
            },
        )
        .session
        .unwrap();
        assert_ne!(sample.id, session.id);
        assert_eq!(sample.data_classes, [floe_agent::DataClass::Synthetic]);
    }

    #[test]
    fn worker_keeps_expert_assignment_and_state_across_host_restart_and_new_chat() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        let keys = Keys::default();
        let person = PersonId::new();
        let worker = Worker::new(root.clone(), keys.clone()).unwrap();
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Create {}).state,
            Some(AgentVaultStateDto::Ready)
        );
        fn run(worker: &Worker, person: PersonId) -> floe_agent::ExpertResult {
            let session = perform(
                worker,
                person,
                AgentVaultActionDto::Session {
                    operation: AgentFixtureOperationDto::Start {},
                },
            )
            .session
            .unwrap();
            let completed = perform(
                worker,
                person,
                AgentVaultActionDto::Session {
                    operation: AgentFixtureOperationDto::Turn {
                        session_id: session.id.to_string(),
                        expected_revision: 0,
                        prompt: AgentFixturePromptDto::Today,
                    },
                },
            )
            .session
            .unwrap();
            assert_eq!(
                completed.last_outcome,
                Some(floe_agent::AgentOutcome::Completed)
            );
            let floe_agent::AgentMessage::Delegation { task, .. } = &completed.messages[1] else {
                panic!("expected Expert result");
            };
            serde_json::from_str(
                task.data_part(floe_agent::EXPERT_RESULT_MEDIA_TYPE)
                    .unwrap(),
            )
            .unwrap()
        }
        let first = run(&worker, person);
        assert_eq!(first.state_revision, 1);
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Lock {}).state,
            Some(AgentVaultStateDto::Locked)
        );
        drop(worker);
        let worker = Worker::new(root, keys).unwrap();
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Unlock {}).state,
            Some(AgentVaultStateDto::Ready)
        );
        let second = run(&worker, person);
        assert_eq!(second.state_revision, 2);
        assert_eq!(second.assignment_id, first.assignment_id);
        assert_eq!(second.instance_id, first.instance_id);
        assert_eq!(second.view_handle, first.view_handle);
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Lock {}).state,
            Some(AgentVaultStateDto::Locked)
        );
    }

    #[test]
    fn worker_provisions_resumes_locks_and_fails_closed_without_plaintext_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        let keys = Keys::default();
        let person = PersonId::new();
        let worker = Worker::new(root.clone(), keys.clone()).unwrap();
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Status {}).state,
            Some(AgentVaultStateDto::Missing)
        );
        assert!(!root.exists());
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Create {}).state,
            Some(AgentVaultStateDto::Ready)
        );
        let session = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Resume {},
            },
        )
        .session
        .unwrap();
        assert_eq!(session.data_classes, [floe_agent::DataClass::Synthetic]);
        assert_eq!(
            perform(&worker, PersonId::new(), AgentVaultActionDto::Lock {}).failure,
            Some(AgentFailure::NotFound)
        );
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Lock {}).state,
            Some(AgentVaultStateDto::Locked)
        );
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Unlock {}).state,
            Some(AgentVaultStateDto::Ready)
        );
        assert_eq!(
            perform(
                &worker,
                person,
                AgentVaultActionDto::Session {
                    operation: AgentFixtureOperationDto::Resume {}
                }
            )
            .session
            .unwrap(),
            session
        );
        keys.0.unavailable.store(true, Ordering::Release);
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Status {}).failure,
            Some(AgentFailure::VaultUnavailable)
        );
        assert_eq!(
            perform(
                &worker,
                person,
                AgentVaultActionDto::Session {
                    operation: AgentFixtureOperationDto::Resume {}
                }
            )
            .failure,
            Some(AgentFailure::VaultUnavailable)
        );
        keys.0.unavailable.store(false, Ordering::Release);
        assert_eq!(
            perform(&worker, person, AgentVaultActionDto::Unlock {}).state,
            Some(AgentVaultStateDto::Ready)
        );
        assert_eq!(keys.0.values.lock().unwrap().len(), 1);
    }

    #[test]
    fn worker_streams_cancels_replays_and_blocks_recovery_of_a_live_run() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        perform(&worker, person, AgentVaultActionDto::Create {});
        let session = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Start {},
            },
        )
        .session
        .unwrap();
        let id = Uuid::new_v4();
        let operation = AgentVaultOperationDto::Submit {
            action: AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Turn {
                    session_id: session.id.to_string(),
                    expected_revision: session.revision,
                    prompt: AgentFixturePromptDto::Today,
                },
            },
        };
        worker.request(person, id, operation.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = worker.request(person, id, operation.clone()).unwrap();
            if result
                .events
                .iter()
                .any(|event| matches!(event.event, floe_agent::AgentEventKind::ModelStarted { .. }))
            {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            worker.request(
                person,
                Uuid::new_v4(),
                AgentVaultOperationDto::Submit {
                    action: AgentVaultActionDto::Session {
                        operation: AgentFixtureOperationDto::Recover {
                            session_id: session.id.to_string(),
                            expected_revision: 1
                        }
                    }
                }
            ),
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            worker.request(PersonId::new(), id, AgentVaultOperationDto::Stop {}),
            Err(AgentFailure::NotFound)
        );
        worker
            .request(person, id, AgentVaultOperationDto::Stop {})
            .unwrap();
        let result = wait(&worker, person, id);
        assert_eq!(
            result.session.as_ref().unwrap().last_outcome,
            Some(floe_agent::AgentOutcome::Halted {
                reason: AgentFailure::Cancelled
            })
        );
        assert_eq!(wait(&worker, person, id), result);
        assert_eq!(
            worker.request(
                person,
                id,
                AgentVaultOperationDto::Poll {
                    after_sequence: result.next_sequence + 1
                }
            ),
            Err(AgentFailure::InvalidInput)
        );
        worker
            .request(person, id, AgentVaultOperationDto::Release {})
            .unwrap();
        let resumed = perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Resume {},
            },
        );
        assert_eq!(resumed.session, result.session);
    }

    #[test]
    fn a_blocked_key_store_does_not_block_poll_stop_or_handle_close() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        let keys = Keys::default();
        let person = PersonId::new();
        let worker = Worker::new(root.clone(), keys.clone()).unwrap();
        perform(&worker, person, AgentVaultActionDto::Create {});
        keys.0.entered.store(false, Ordering::Release);
        *keys.0.paused.lock().unwrap() = true;
        let id = Uuid::new_v4();
        worker
            .request(
                person,
                id,
                AgentVaultOperationDto::Submit {
                    action: AgentVaultActionDto::Status {},
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !keys.0.entered.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let start = Instant::now();
        assert!(
            !worker
                .request(
                    person,
                    id,
                    AgentVaultOperationDto::Poll { after_sequence: 0 }
                )
                .unwrap()
                .done
        );
        assert!(
            !worker
                .request(person, id, AgentVaultOperationDto::Stop {})
                .unwrap()
                .done
        );
        drop(worker);
        assert!(start.elapsed() < Duration::from_millis(250));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(matches!(
            runtime.block_on(EncryptedAgentVault::open(&root, person, keys.clone())),
            Err(AgentFailure::Conflict)
        ));
        *keys.0.paused.lock().unwrap() = false;
        keys.0.wake.notify_all();
        loop {
            match runtime.block_on(EncryptedAgentVault::open(&root, person, keys.clone())) {
                Ok(_) => break,
                Err(AgentFailure::Conflict) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("Vault did not release safely: {error:?}"),
            }
        }
    }
}
