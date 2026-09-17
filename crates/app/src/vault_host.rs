use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    fs,
    ops::Deref,
    os::unix::fs::DirBuilderExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "android")]
use crate::android_vault_keys::AndroidVaultKeys as PlatformVaultKeys;
use floe_agent_contract::AgentFailure;
use floe_conversation::{AgentEvent, AgentSession, SessionStore};
// What the regressions below read off a finished turn.
use crate::{AgentFixtureTurn, recover_agent_sample, run_persisted_agent_sample};
use crate::{
    CalendarActionOperation, CalendarProposalInspection, CalendarSubjectPreview,
    ConversationSessionOperation, ConversationTurnRequest, FixtureOperation, RemoteGrantOverview,
    RemoteTurnRoute, VaultState, WorkerAction, WorkerOperation, WorkerResult,
};
use floe_actions::{ExpertCalendarInspection, ExpertProposalReference};
use floe_context_contract::CalendarProvider;
#[cfg(test)]
use floe_conversation::AgentOutcome;
use floe_execution::Cancellation;
use floe_experts::{BuiltinSourceBinding, BuiltinSourceEvidence};
use floe_experts::{Directory, DirectoryEntry, TaskCoordinator};
use floe_experts_builtin::{BuiltinContextSource, BuiltinExpertKind, BuiltinSourceRequirement};
use floe_kernel::PersonId;
use floe_provider_adapters::control::authorization::RemoteAuthorityEndpoint;
#[cfg(not(target_os = "macos"))]
use floe_provider_adapters::sources::{CalendarAcquisitionMode, CalendarAcquisitionRequest};
#[cfg(not(target_os = "android"))]
use floe_vault::KeyringVaultKeys as PlatformVaultKeys;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use crate::local_context::LocalContextHost;
use crate::{FloeCore, diagnostics};

mod calendar_access;
mod conversation_turn;
mod expert_setup;
mod learner_worker;
mod personal_grants;
mod remote_authority;
mod remote_views;

use floe_vault::{VaultConversationRepository, VaultTaskRepository};

const LEARNER_IDLE_DELAY: Duration = Duration::from_millis(750);
const LEARNER_EMPTY_DELAY: Duration = Duration::from_secs(30);
const LEARNER_ERROR_DELAY: Duration = Duration::from_secs(5);
const AGENT_VAULT_STACK_SIZE: usize = 8 * 1024 * 1024;
const MAX_VAULT_JOBS: usize = 64;
const MAX_IN_FLIGHT_VAULT_JOBS: usize = 8;

pub struct VaultBridge {
    root: PathBuf,
    core: Arc<FloeCore>,
    worker: RefCell<Option<Worker>>,
    local_context: Arc<LocalContextHost>,
    app_events: Arc<crate::events::AppEventBuffer>,
}

/// A vault request that could not complete, and where it stopped.
///
/// Which stage a request reached is what a caller needs to report it; turning
/// that into an error on a wire is the caller's own business.
pub struct VaultRequestFailure {
    pub failure: AgentFailure,
    pub request_id: Uuid,
    pub stage: &'static str,
}

impl VaultBridge {
    pub(crate) fn new(
        database_path: &str,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
    ) -> Self {
        Self {
            root: PathBuf::from(format!("{database_path}.agent-vaults")),
            core,
            worker: RefCell::new(None),
            local_context,
            app_events: Arc::new(crate::events::AppEventBuffer::default()),
        }
    }

    /// Run one request against this Person's vault worker.
    ///
    /// The operation is still the worker's own envelope, which R003 07
    /// replaces; what this no longer does is read it off the wire or decide
    /// what a failure looks like on one.
    pub fn request(
        &self,
        person: PersonId,
        request_id: Uuid,
        operation: WorkerOperation,
    ) -> Result<WorkerResult, VaultRequestFailure> {
        let stage = operation.name();
        self.worker()
            .and_then(|worker| worker.request(person, request_id, operation))
            .map_err(|failure| VaultRequestFailure {
                failure,
                request_id,
                stage,
            })
    }

    pub fn conversation_query(
        &self,
        person: PersonId,
        query: ConversationQuery,
    ) -> Result<Option<floe_conversation::RunReceipt>, AgentFailure> {
        self.worker()?.conversation_query(person, query)
    }

    pub fn app_events(&self) -> &crate::events::AppEventBuffer {
        self.app_events.as_ref()
    }

    pub(crate) fn precheck_turn(
        &self,
        person: PersonId,
        request: floe_conversation::TurnPrecheckRequest,
    ) -> Result<floe_conversation::TurnPrecheck, AgentFailure> {
        self.worker()?.precheck_turn(person, request)
    }

    pub(crate) fn start_conversation(
        &self,
        person: PersonId,
        command_id: floe_kernel::CommandId,
        request: ConversationTurnRequest,
    ) -> Result<floe_conversation::RunReceipt, AgentFailure> {
        self.worker()?
            .start_conversation(person, command_id, request)
    }

    pub(crate) fn cancel_conversation(
        &self,
        person: PersonId,
        command_id: floe_kernel::CommandId,
        run_id: floe_kernel::RunId,
    ) -> Result<floe_conversation::CancelRunStatus, AgentFailure> {
        self.worker()?
            .cancel_conversation(person, command_id, run_id)
    }

    fn worker(&self) -> Result<RefMut<'_, Worker>, AgentFailure> {
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker = Some(Worker::with_core(
                self.root.clone(),
                PlatformVaultKeys,
                self.core.clone(),
                self.local_context.clone(),
                Arc::clone(&self.app_events),
            )?);
        }
        Ok(RefMut::map(worker, |worker| {
            worker.as_mut().expect("worker was initialized")
        }))
    }

    pub(crate) fn shutdown(&self) {
        self.worker.borrow_mut().take();
    }
}

struct Worker {
    sender: mpsc::SyncSender<WorkerMessage>,
    jobs: Mutex<HashMap<Uuid, Arc<Job>>>,
    run_cancellations: Arc<floe_conversation::RunCancellationRegistry>,
    closing: Arc<AtomicBool>,
    learner_scheduling: floe_knowledge::LearnerScheduling,
    app_events: Arc<crate::events::AppEventBuffer>,
}

struct OpenVault<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    available: AtomicBool,
    conversation_repository: Arc<VaultConversationRepository<Keys>>,
    _recovered_conversation_runs: Vec<floe_vault::VaultConversationRunRecord>,
    task_coordinator: TaskCoordinator<VaultTaskRepository<Keys>>,
    directory: Directory,
    schedule_endpoint: Arc<conversation_turn::expert_dispatch::schedule::ScheduleEndpoint<Keys>>,
    builtin_expert_endpoint: Arc<conversation_turn::expert_dispatch::BuiltinExpertEndpoint<Keys>>,
    _recovered_tasks: Vec<floe_agent_contract::TaskReceipt>,
}

impl<Keys: VaultKeyProvider + 'static> OpenVault<Keys> {
    async fn activate(
        vault: EncryptedAgentVault<Keys>,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
    ) -> Result<Self, AgentFailure> {
        let vault = Arc::new(vault);
        let conversation_activation = vault.activate_conversation_executor().await?;
        let conversation_repository =
            Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
        let directory = Directory::default();
        let schedule_endpoint = Arc::new(
            conversation_turn::expert_dispatch::schedule::ScheduleEndpoint::new(
                Arc::clone(&core),
                Arc::clone(&vault),
                Arc::clone(&local_context),
            ),
        );
        let builtin_expert_endpoint = Arc::new(
            conversation_turn::expert_dispatch::BuiltinExpertEndpoint::new(
                core,
                Arc::clone(&vault),
                local_context,
            ),
        );
        let definition = conversation_turn::expert_dispatch::schedule::schedule_definition();
        directory.register(
            DirectoryEntry {
                definition,
                reviewed: true,
                enabled: true,
                admitted_principals: vec![vault.person_id().to_string()],
                purposes: vec!["everyday-assistance".into()],
            },
            schedule_endpoint.clone(),
        )?;
        let repository = Arc::new(VaultTaskRepository::new(
            Arc::clone(&vault),
            conversation_turn::expert_dispatch::schedule::CALENDAR_EXPERT_SETTLEMENT_OWNER,
        ));
        let (task_coordinator, recovered_tasks) = TaskCoordinator::activate(
            directory.clone(),
            repository,
            "everyday-assistance",
            floe_agent_contract::MAX_OUTPUT_BYTES,
        )
        .await?;
        Ok(Self {
            vault,
            available: AtomicBool::new(true),
            conversation_repository,
            _recovered_conversation_runs: conversation_activation.interrupted,
            task_coordinator,
            directory,
            schedule_endpoint,
            builtin_expert_endpoint,
            _recovered_tasks: recovered_tasks,
        })
    }

    async fn sync_expert_directory(&self) -> Result<(), AgentFailure> {
        for kind in BuiltinExpertKind::BUILTIN_SETUP {
            match self.directory.unregister(kind.package_id()) {
                Ok(_) | Err(AgentFailure::NotFound) => {}
                Err(failure) => return Err(failure),
            }
        }
        for card in self.vault.enabled_expert_cards().await? {
            if !BuiltinExpertKind::BUILTIN_SETUP
                .iter()
                .any(|kind| card.id == kind.package_id())
            {
                continue;
            }
            self.directory.register(
                DirectoryEntry {
                    definition: conversation_turn::engine_ports::contract_definition(&card),
                    reviewed: true,
                    enabled: true,
                    admitted_principals: vec![self.vault.person_id().to_string()],
                    purposes: vec!["everyday-assistance".into()],
                },
                self.builtin_expert_endpoint.clone(),
            )?;
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    fn mark_unavailable(&self) {
        self.available.store(false, Ordering::Release);
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
    action: Box<WorkerAction>,
    cancellation: Cancellation,
    run_cancellations: Arc<floe_conversation::RunCancellationRegistry>,
    admission: Mutex<Option<Result<floe_conversation::RunReceipt, AgentFailure>>>,
    admission_ready: Condvar,
    progress: Mutex<Progress>,
    app_events: Arc<crate::events::AppEventBuffer>,
}

impl Job {
    fn publish_admission(&self, result: Result<floe_conversation::RunReceipt, AgentFailure>) {
        if let Ok(mut admission) = self.admission.lock()
            && admission.is_none()
        {
            if let Ok(receipt) = &result {
                self.app_events.publish_command(receipt);
            }
            *admission = Some(result);
            self.admission_ready.notify_all();
        }
    }

    fn wait_for_admission(&self) -> Result<floe_conversation::RunReceipt, AgentFailure> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        loop {
            if let Some(result) = admission.as_ref() {
                return result.clone();
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AgentFailure::VaultUnavailable);
            }
            let (next, timeout) = self
                .admission_ready
                .wait_timeout(admission, remaining)
                .map_err(|_| AgentFailure::Interrupted)?;
            admission = next;
            if timeout.timed_out() && admission.is_none() {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
    }
}

enum WorkerMessage {
    Job(Arc<Job>),
    ConversationQuery(ConversationQueryJob),
    ConversationPrecheck(ConversationPrecheckJob),
    ConversationCancel(ConversationCancelJob),
}

pub enum ConversationQuery {
    Command(floe_kernel::CommandId),
    Run(floe_kernel::RunId),
    Message(floe_kernel::RunId),
}

struct ConversationQueryJob {
    person: PersonId,
    query: ConversationQuery,
    reply: mpsc::SyncSender<Result<Option<floe_conversation::RunReceipt>, AgentFailure>>,
}

struct ConversationPrecheckJob {
    person: PersonId,
    request: floe_conversation::TurnPrecheckRequest,
    reply: mpsc::SyncSender<Result<floe_conversation::TurnPrecheck, AgentFailure>>,
}

struct ConversationCancelJob {
    person: PersonId,
    command_id: floe_kernel::CommandId,
    run_id: floe_kernel::RunId,
    reply: mpsc::SyncSender<Result<floe_conversation::CancelRunStatus, AgentFailure>>,
}

/// How far one command has got, as the worker records it.
///
/// Every slot holds the owner's own value; nothing here is a wire shape.
#[derive(Default)]
struct Progress {
    events: Vec<AgentEvent>,
    done: bool,
    state: Option<VaultState>,
    session: Option<AgentSession>,
    registry: Option<floe_experts::RegistryOverview>,
    calendar_experts: Option<floe_experts::CalendarExpertOverview>,
    calendar_subject_preview: Option<CalendarSubjectPreview>,
    proposal: Option<CalendarProposalInspection>,
    memory_review: Option<floe_knowledge::MemoryReviewResult>,
    memory: Option<floe_knowledge::MemoryOverviewSnapshot>,
    connections: Option<Vec<floe_connections::ConnectorSnapshot>>,
    remote_producer: Option<floe_access::RemoteProducerIdentity>,
    remote_enrollment: Option<floe_access::RemoteEnrollmentStatus>,
    remote_pairing: Option<floe_connections::PairingStatus>,
    remote_owner: Option<floe_access::RemoteOwnerPublicKey>,
    remote_calendar_grant: Option<RemoteGrantOverview>,
    remote_calendar_preview: Option<crate::RemoteCalendarGrantPreview>,
    remote_view_grant: Option<RemoteGrantOverview>,
    remote_view_preview: Option<floe_access::RemoteViewGrantPreview>,
    personal_access: Option<floe_access::PersonalAccessOverview>,
    calendar_actions: Option<crate::CalendarActionsResult>,
    failure: Option<AgentFailure>,
}

impl Worker {
    fn with_core<Keys: VaultKeyProvider + Clone + 'static>(
        root: PathBuf,
        keys: Keys,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
        app_events: Arc<crate::events::AppEventBuffer>,
    ) -> Result<Self, AgentFailure> {
        let (sender, receiver) = mpsc::sync_channel::<WorkerMessage>(MAX_IN_FLIGHT_VAULT_JOBS);
        let closing = Arc::new(AtomicBool::new(false));
        let run_cancellations = Arc::new(floe_conversation::RunCancellationRegistry::default());
        let worker_run_cancellations = Arc::clone(&run_cancellations);
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
                let conversation_runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("floe-conversation-runtime")
                    .enable_all()
                    .build();
                let mut vault: Option<(PersonId, Arc<OpenVault<Keys>>)> = None;
                let mut learner_delay = LEARNER_IDLE_DELAY;
                loop {
                    match receiver.recv_timeout(learner_delay) {
                        Ok(message) => {
                            learner_delay = LEARNER_IDLE_DELAY;
                            if worker_closing.load(Ordering::Acquire) {
                                break;
                            }
                            if vault
                                .as_ref()
                                .is_some_and(|(_, open_vault)| !open_vault.is_available())
                            {
                                vault = None;
                            }
                            let job = match message {
                                WorkerMessage::Job(job) => job,
                                WorkerMessage::ConversationQuery(query) => {
                                    let result = match (&runtime, vault.as_ref()) {
                                        (Ok(runtime), Some((person, open_vault)))
                                            if *person == query.person =>
                                        {
                                            runtime.block_on(async {
                                                match query.query {
                                                    ConversationQuery::Command(command_id) => {
                                                        floe_conversation::get_command(
                                                            open_vault
                                                                .conversation_repository
                                                                .as_ref(),
                                                            floe_conversation::CommandQuery {
                                                                principal: query.person.to_string(),
                                                                command_id,
                                                            },
                                                        )
                                                        .await
                                                    }
                                                    ConversationQuery::Run(run_id)
                                                    | ConversationQuery::Message(run_id) => {
                                                        floe_conversation::get_run(
                                                            open_vault
                                                                .conversation_repository
                                                                .as_ref(),
                                                            floe_conversation::RunQuery {
                                                                principal: query.person.to_string(),
                                                                run_id,
                                                            },
                                                        )
                                                        .await
                                                    }
                                                }
                                            })
                                        }
                                        (Ok(_), Some(_)) => Err(AgentFailure::NotFound),
                                        (Ok(_), None) | (Err(_), _) => {
                                            Err(AgentFailure::VaultUnavailable)
                                        }
                                    };
                                    let _ = query.reply.send(result);
                                    let _ = worker_learner_scheduling.foreground_finished();
                                    continue;
                                }
                                WorkerMessage::ConversationPrecheck(precheck) => {
                                    let result = match (&runtime, vault.as_ref()) {
                                        (Ok(runtime), Some((person, open_vault)))
                                            if *person == precheck.person =>
                                        {
                                            runtime.block_on(floe_conversation::precheck_turn(
                                                open_vault.conversation_repository.as_ref(),
                                                precheck.request,
                                            ))
                                        }
                                        (Ok(_), Some(_)) => Err(AgentFailure::NotFound),
                                        (Ok(_), None) | (Err(_), _) => {
                                            Err(AgentFailure::VaultUnavailable)
                                        }
                                    };
                                    let _ = precheck.reply.send(result);
                                    let _ = worker_learner_scheduling.foreground_finished();
                                    continue;
                                }
                                WorkerMessage::ConversationCancel(cancel) => {
                                    let result = match (&runtime, vault.as_ref()) {
                                        (Ok(runtime), Some((person, open_vault)))
                                            if *person == cancel.person =>
                                        {
                                            runtime.block_on(floe_conversation::cancel_run_command(
                                                open_vault.conversation_repository.as_ref(),
                                                &worker_run_cancellations,
                                                floe_conversation::CancelRunCommand {
                                                    command_id: cancel.command_id,
                                                    run_id: cancel.run_id,
                                                    principal: cancel.person.to_string(),
                                                },
                                            ))
                                        }
                                        (Ok(_), Some(_)) => Err(AgentFailure::NotFound),
                                        (Ok(_), None) | (Err(_), _) => {
                                            Err(AgentFailure::VaultUnavailable)
                                        }
                                    };
                                    let _ = cancel.reply.send(result);
                                    let _ = worker_learner_scheduling.foreground_finished();
                                    continue;
                                }
                            };
                            let operation = job.action.name();
                            let started = Instant::now();
                            let trace_context = diagnostics::trace_context(job.id);
                            let request_id = trace_context.request_id().to_string();
                            tracing::info!(request_id, operation, "agent_job_started");
                            if matches!(*job.action, WorkerAction::ConversationTurn { .. }) {
                                let failure = match (&conversation_runtime, vault.as_ref()) {
                                    (Ok(runtime), Some((person, open_vault)))
                                        if *person == job.person =>
                                    {
                                        let core = Arc::clone(&core);
                                        let local_context = Arc::clone(&local_context);
                                        let open_vault = Arc::clone(open_vault);
                                        let health_vault = Arc::clone(&open_vault);
                                        let task_job = Arc::clone(&job);
                                        let task_scheduling = worker_learner_scheduling.clone();
                                        runtime.spawn(async move {
                                            let execution_job = Arc::clone(&task_job);
                                            let execution = tokio::spawn(diagnostics::instrument(
                                                async move {
                                                    let WorkerAction::ConversationTurn { request } =
                                                        &*execution_job.action
                                                    else {
                                                        return Err(AgentFailure::InvalidInput);
                                                    };
                                                    execute_conversation_turn_action(
                                                        &core,
                                                        &open_vault,
                                                        &local_context,
                                                        &execution_job,
                                                        request,
                                                    )
                                                    .await
                                                },
                                                trace_context,
                                                "agent_job",
                                            ));
                                            let result = match execution.await {
                                                Ok(result) => result,
                                                Err(error) => {
                                                    if error.is_panic() {
                                                        let _ = diagnostics::panic_error(
                                                            error.into_panic(),
                                                        );
                                                    }
                                                    Err(AgentFailure::Interrupted)
                                                }
                                            };
                                            if matches!(
                                                &result,
                                                Err(AgentFailure::VaultUnavailable
                                                    | AgentFailure::Interrupted)
                                            ) {
                                                health_vault.mark_unavailable();
                                            }
                                            trace_job_result(
                                                &request_id,
                                                operation,
                                                started,
                                                &result,
                                            );
                                            finish_job(
                                                &task_job,
                                                result,
                                                health_vault.is_available(),
                                                &task_scheduling,
                                            );
                                        });
                                        continue;
                                    }
                                    (Ok(_), Some(_)) => AgentFailure::NotFound,
                                    (Ok(_), None) | (Err(_), _) => AgentFailure::VaultUnavailable,
                                };
                                let result = Err(failure);
                                trace_job_result(&request_id, operation, started, &result);
                                finish_job(
                                    &job,
                                    result,
                                    vault.is_some(),
                                    &worker_learner_scheduling,
                                );
                                continue;
                            }
                            let result = match catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => runtime.block_on(diagnostics::instrument(
                                    execute(&root, &keys, &core, &local_context, &mut vault, &job),
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
                            trace_job_result(&request_id, operation, started, &result);
                            if matches!(
                                result,
                                Err(AgentFailure::VaultUnavailable | AgentFailure::Interrupted)
                            ) {
                                vault = None;
                            }
                            let vault_available = vault
                                .as_ref()
                                .is_some_and(|(person, _)| *person == job.person);
                            finish_job(&job, result, vault_available, &worker_learner_scheduling);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if worker_closing.load(Ordering::Acquire) {
                                break;
                            }
                            if vault
                                .as_ref()
                                .is_some_and(|(_, open_vault)| !open_vault.is_available())
                            {
                                vault = None;
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
            jobs: Mutex::new(HashMap::new()),
            run_cancellations,
            closing,
            learner_scheduling,
            app_events,
        })
    }

    fn request(
        &self,
        person: PersonId,
        id: Uuid,
        operation: WorkerOperation,
    ) -> Result<WorkerResult, AgentFailure> {
        let person_id = person;
        if let WorkerOperation::Submit { action } = operation {
            self.submit_job(person, id, action)?;
            return self.poll_job(person_id, id, 0, false, false);
        }
        let (after_sequence, stop, release) = match operation {
            WorkerOperation::Poll { after_sequence } => (after_sequence, false, false),
            WorkerOperation::Stop => (0, true, false),
            WorkerOperation::Release => (0, false, true),
            WorkerOperation::Submit { .. } => unreachable!("submitted above"),
        };
        self.poll_job(person_id, id, after_sequence, stop, release)
    }

    /// Read how far one job got, after any stop or release the caller asked for.
    fn poll_job(
        &self,
        person: PersonId,
        id: Uuid,
        after_sequence: usize,
        stop: bool,
        release: bool,
    ) -> Result<WorkerResult, AgentFailure> {
        let jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
        let job = jobs.get(&id).cloned().ok_or(AgentFailure::NotFound)?;
        if job.person != person {
            return Err(AgentFailure::NotFound);
        }
        drop(jobs);
        if stop {
            // A turn is cancelled through its Run, so a cancel that the
            // conversation owner has never heard of still stops the job.
            if matches!(*job.action, WorkerAction::ConversationTurn { .. }) {
                let command_id = floe_agent_contract::CommandId::from_uuid(job.id)
                    .ok_or(AgentFailure::InvalidInput)?;
                if matches!(
                    job.run_cancellations.cancel_command(
                        floe_conversation::CancelCommandRequest {
                            command_id,
                            principal: person.to_string(),
                        },
                    )?,
                    floe_conversation::CancelRunStatus::Unknown
                ) && !job
                    .progress
                    .lock()
                    .map_err(|_| AgentFailure::Interrupted)?
                    .done
                {
                    job.cancellation.cancel();
                }
            } else {
                job.cancellation.cancel();
            }
        }
        let progress = job.progress.lock().map_err(|_| AgentFailure::Interrupted)?;
        if after_sequence > progress.events.len() {
            return Err(AgentFailure::InvalidInput);
        }
        let response = WorkerResult {
            request_id: id,
            person_id: person,
            stage: job.action.name().into(),
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
        if release {
            if !response.done {
                return Err(AgentFailure::Conflict);
            }
            self.jobs
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?
                .remove(&id);
        }
        Ok(response)
    }

    fn start_conversation(
        &self,
        person: PersonId,
        command_id: floe_kernel::CommandId,
        request: ConversationTurnRequest,
    ) -> Result<floe_conversation::RunReceipt, AgentFailure> {
        let job = self.submit_job(
            person,
            command_id.as_uuid(),
            Box::new(WorkerAction::ConversationTurn {
                request: Box::new(request),
            }),
        )?;
        job.wait_for_admission()
    }

    fn submit_job(
        &self,
        person: PersonId,
        id: Uuid,
        action: Box<WorkerAction>,
    ) -> Result<Arc<Job>, AgentFailure> {
        let mut jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
        if let Some(job) = jobs.get(&id) {
            if job.person != person {
                return Err(AgentFailure::NotFound);
            }
            // One request id means one command; a second one under the same id
            // is a different request, whatever it asks for.
            if job.action.name() != action.name() {
                return Err(AgentFailure::Conflict);
            }
            return Ok(Arc::clone(job));
        }
        let mut in_flight = 0_usize;
        let mut exclusive_in_flight = false;
        let mut incompatible_in_flight = false;
        let mut completed = Vec::new();
        for (request_id, job) in jobs.iter() {
            if job
                .progress
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?
                .done
            {
                completed.push(*request_id);
            } else {
                in_flight += 1;
                exclusive_in_flight |= job.action.is_exclusive_host();
                incompatible_in_flight |=
                    !matches!(*job.action, WorkerAction::ConversationTurn { .. });
            }
        }
        if in_flight >= MAX_IN_FLIGHT_VAULT_JOBS
            || (in_flight > 0 && action.is_exclusive_host())
            || exclusive_in_flight
            || (in_flight > 0 && (!action.is_concurrent_host() || incompatible_in_flight))
        {
            return Err(AgentFailure::Conflict);
        }
        if jobs.len() >= MAX_VAULT_JOBS {
            for request_id in completed {
                jobs.remove(&request_id);
                if jobs.len() < MAX_VAULT_JOBS {
                    break;
                }
            }
        }
        if jobs.len() >= MAX_VAULT_JOBS {
            return Err(AgentFailure::BudgetExceeded);
        }
        let job = Arc::new(Job {
            person,
            id,
            action,
            cancellation: Cancellation::default(),
            run_cancellations: Arc::clone(&self.run_cancellations),
            admission: Mutex::new(None),
            admission_ready: Condvar::new(),
            progress: Mutex::new(Progress::default()),
            app_events: Arc::clone(&self.app_events),
        });
        self.learner_scheduling.foreground_submitted()?;
        if self
            .sender
            .try_send(WorkerMessage::Job(job.clone()))
            .is_err()
        {
            let _ = self.learner_scheduling.foreground_finished();
            return Err(AgentFailure::VaultUnavailable);
        }
        jobs.insert(id, Arc::clone(&job));
        Ok(job)
    }

    fn conversation_query(
        &self,
        person: PersonId,
        query: ConversationQuery,
    ) -> Result<Option<floe_conversation::RunReceipt>, AgentFailure> {
        let (reply, response) = mpsc::sync_channel(1);
        self.learner_scheduling.foreground_submitted()?;
        if self
            .sender
            .try_send(WorkerMessage::ConversationQuery(ConversationQueryJob {
                person,
                query,
                reply,
            }))
            .is_err()
        {
            let _ = self.learner_scheduling.foreground_finished();
            return Err(AgentFailure::VaultUnavailable);
        }
        response
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(Err(AgentFailure::VaultUnavailable))
    }

    fn precheck_turn(
        &self,
        person: PersonId,
        request: floe_conversation::TurnPrecheckRequest,
    ) -> Result<floe_conversation::TurnPrecheck, AgentFailure> {
        let (reply, response) = mpsc::sync_channel(1);
        self.learner_scheduling.foreground_submitted()?;
        if self
            .sender
            .try_send(WorkerMessage::ConversationPrecheck(
                ConversationPrecheckJob {
                    person,
                    request,
                    reply,
                },
            ))
            .is_err()
        {
            let _ = self.learner_scheduling.foreground_finished();
            return Err(AgentFailure::VaultUnavailable);
        }
        response
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(Err(AgentFailure::VaultUnavailable))
    }

    fn cancel_conversation(
        &self,
        person: PersonId,
        command_id: floe_kernel::CommandId,
        run_id: floe_kernel::RunId,
    ) -> Result<floe_conversation::CancelRunStatus, AgentFailure> {
        let (reply, response) = mpsc::sync_channel(1);
        self.learner_scheduling.foreground_submitted()?;
        if self
            .sender
            .try_send(WorkerMessage::ConversationCancel(ConversationCancelJob {
                person,
                command_id,
                run_id,
                reply,
            }))
            .is_err()
        {
            let _ = self.learner_scheduling.foreground_finished();
            return Err(AgentFailure::VaultUnavailable);
        }
        response
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(Err(AgentFailure::VaultUnavailable))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        if let Ok(jobs) = self.jobs.lock() {
            for job in jobs.values() {
                job.cancellation.cancel();
            }
        }
        self.learner_scheduling.close();
    }
}

/// What one command produced, before anything says it on a wire.
#[derive(Default)]
struct VaultExecutionResult {
    state: VaultState,
    session: Option<AgentSession>,
    registry: Option<floe_experts::RegistryOverview>,
    calendar_experts: Option<floe_experts::CalendarExpertOverview>,
    calendar_subject_preview: Option<CalendarSubjectPreview>,
    proposal: Option<CalendarProposalInspection>,
    memory_review: Option<floe_knowledge::MemoryReviewResult>,
    memory: Option<floe_knowledge::MemoryOverviewSnapshot>,
    remote_producer: Option<floe_access::RemoteProducerIdentity>,
    remote_enrollment: Option<floe_access::RemoteEnrollmentStatus>,
    remote_pairing: Option<floe_connections::PairingStatus>,
    remote_owner: Option<floe_access::RemoteOwnerPublicKey>,
    remote_calendar_grant: Option<RemoteGrantOverview>,
    remote_calendar_preview: Option<crate::RemoteCalendarGrantPreview>,
    remote_view_grant: Option<RemoteGrantOverview>,
    remote_view_preview: Option<floe_access::RemoteViewGrantPreview>,
    personal_access: Option<floe_access::PersonalAccessOverview>,
    calendar_actions: Option<crate::CalendarActionsResult>,
}

impl VaultExecutionResult {
    fn new(state: VaultState) -> Self {
        Self {
            state,
            ..Self::default()
        }
    }

    fn ready() -> Self {
        Self::new(VaultState::Ready)
    }
}

fn trace_job_result(
    request_id: &str,
    operation: &str,
    started: Instant,
    result: &Result<VaultExecutionResult, AgentFailure>,
) {
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(_) => tracing::info!(request_id, operation, elapsed_ms, "agent_job_completed"),
        Err(failure) => tracing::error!(
            request_id,
            operation,
            elapsed_ms,
            failure = ?failure,
            "agent_job_failed"
        ),
    }
}

fn finish_job(
    job: &Job,
    result: Result<VaultExecutionResult, AgentFailure>,
    vault_available: bool,
    learner_scheduling: &floe_knowledge::LearnerScheduling,
) {
    if let Err(failure) = &result {
        job.publish_admission(Err(*failure));
    }
    if let Ok(mut progress) = job.progress.lock() {
        match result {
            Ok(result) => {
                progress.state = Some(result.state);
                progress.session = result.session;
                progress.registry = result.registry;
                progress.calendar_experts = result.calendar_experts;
                progress.calendar_subject_preview = result.calendar_subject_preview;
                progress.proposal = result.proposal;
                progress.memory_review = result.memory_review;
                progress.memory = result.memory;
                progress.remote_producer = result.remote_producer;
                progress.remote_enrollment = result.remote_enrollment;
                progress.remote_pairing = result.remote_pairing;
                progress.remote_owner = result.remote_owner;
                progress.remote_calendar_grant = result.remote_calendar_grant;
                progress.remote_calendar_preview = result.remote_calendar_preview;
                progress.remote_view_grant = result.remote_view_grant;
                progress.remote_view_preview = result.remote_view_preview;
                progress.personal_access = result.personal_access;
                progress.calendar_actions = result.calendar_actions;
            }
            Err(failure) => {
                progress.state = Some(
                    if matches!(
                        failure,
                        AgentFailure::VaultUnavailable | AgentFailure::Interrupted
                    ) || !vault_available
                    {
                        VaultState::Unavailable
                    } else {
                        VaultState::Ready
                    },
                );
                progress.failure = Some(failure);
            }
        }
        let _ = learner_scheduling.foreground_finished();
        progress.done = true;
    }
}

async fn execute<Keys: VaultKeyProvider + Clone + 'static>(
    root: &std::path::Path,
    keys: &Keys,
    core: &Arc<FloeCore>,
    local_context: &Arc<LocalContextHost>,
    current: &mut Option<(PersonId, Arc<OpenVault<Keys>>)>,
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

async fn execute_conversation_turn_action<Keys: VaultKeyProvider + 'static>(
    core: &FloeCore,
    vault: &OpenVault<Keys>,
    local_context: &LocalContextHost,
    job: &Job,
    request: &ConversationTurnRequest,
) -> Result<VaultExecutionResult, AgentFailure> {
    let refreshed = Box::pin(ensure_builtin_experts(
        vault,
        core,
        local_context,
        job.person,
        request.remote_route.as_ref(),
        job.cancellation.clone(),
        floe_experts::BuiltinExpertRefresh::ExistingOnly,
    ))
    .await;
    match floe_experts::expert_refresh_outcome(refreshed) {
        floe_experts::ExpertRefreshOutcome::Ready => {}
        floe_experts::ExpertRefreshOutcome::Degraded(failure) => tracing::warn!(
            failure = ?failure,
            stage = "refresh_builtin_experts",
            "conversation_turn_degraded"
        ),
        floe_experts::ExpertRefreshOutcome::Fatal(failure) => return Err(failure),
    }
    vault.sync_expert_directory().await?;
    let session = match Box::pin(conversation_turn::run(
        core,
        vault,
        local_context,
        &vault.task_coordinator,
        &vault.schedule_endpoint,
        &vault.builtin_expert_endpoint,
        &vault.conversation_repository,
        &job.run_cancellations,
        job.person,
        floe_agent_contract::CommandId::from_uuid(job.id).ok_or(AgentFailure::InvalidInput)?,
        request,
        job.cancellation.clone(),
        |receipt| job.publish_admission(Ok(receipt.clone())),
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
    let admitted = job
        .admission
        .lock()
        .ok()
        .and_then(|admission| match admission.as_ref() {
            Some(Ok(receipt)) => Some(receipt.clone()),
            Some(Err(_)) | None => None,
        });
    if let Some(admitted) = admitted
        && let Ok(Some(receipt)) = floe_conversation::get_run(
            vault.conversation_repository.as_ref(),
            floe_conversation::RunQuery {
                principal: job.person.to_string(),
                run_id: admitted.run_id,
            },
        )
        .await
    {
        job.app_events.publish_run(&receipt);
    }
    Ok(VaultExecutionResult {
        session: Some(session),
        ..VaultExecutionResult::ready()
    })
}

async fn execute_action<Keys: VaultKeyProvider + Clone + 'static>(
    root: &std::path::Path,
    keys: &Keys,
    core: &Arc<FloeCore>,
    local_context: &Arc<LocalContextHost>,
    current: &mut Option<(PersonId, Arc<OpenVault<Keys>>)>,
    job: &Job,
) -> Result<VaultExecutionResult, AgentFailure> {
    match job.action.as_ref() {
        WorkerAction::Status => {
            if let Some((_, vault)) = current {
                vault.check_access()?;
                return Ok(VaultExecutionResult::ready());
            }
            let state = stored_vault_state(root, job.person);
            Ok(VaultExecutionResult::new(state))
        }
        WorkerAction::Create => {
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
                Arc::clone(core),
                Arc::clone(local_context),
            )
            .await?;
            *current = Some((job.person, Arc::new(vault)));
            Ok(VaultExecutionResult::ready())
        }
        WorkerAction::Unlock => {
            if current.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let vault = OpenVault::activate(
                EncryptedAgentVault::open(root, job.person, keys.clone()).await?,
                Arc::clone(core),
                Arc::clone(local_context),
            )
            .await?;
            *current = Some((job.person, Arc::new(vault)));
            Ok(VaultExecutionResult::ready())
        }
        WorkerAction::Lock => {
            *current = None;
            Ok(VaultExecutionResult::new(VaultState::Locked))
        }
        WorkerAction::Session { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let session = match operation {
                FixtureOperation::Start => vault.create_sample_session().await?,
                FixtureOperation::Resume => vault.resume_sample_session().await?,
                FixtureOperation::Get { session_id } => {
                    sample_session(&***vault, job.person, *session_id).await?
                }
                FixtureOperation::Recover {
                    session_id,
                    expected_revision,
                } => {
                    sample_session(&***vault, job.person, *session_id).await?;
                    recover_agent_sample(&***vault, job.person, *session_id, *expected_revision)
                        .await?
                }
                FixtureOperation::Turn {
                    session_id,
                    expected_revision,
                    prompt,
                } => {
                    run_persisted_agent_sample(
                        vault,
                        AgentFixtureTurn {
                            person_id: job.person,
                            session_id: *session_id,
                            expected_revision: *expected_revision,
                            prompt: *prompt,
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
        WorkerAction::Registry { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let registry = match change {
                Some(configuration) => Some(
                    vault
                        .configure_registry(configuration.clone(), job.cancellation.clone())
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
        WorkerAction::CalendarExperts { setup } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let store = calendar_access::VaultCalendarSetups {
                vault: vault.vault.as_ref(),
                packaging: schedule_packaging(),
                cancellation: job.cancellation.clone(),
            };
            let overview = match setup {
                Some(request) => {
                    let admission = calendar_access::DeviceCalendarAdmission::new(
                        core,
                        local_context,
                        job.person,
                        job.cancellation.clone(),
                    );
                    Box::pin(floe_experts::install_calendar_expert(
                        &store,
                        &admission,
                        (**request).clone(),
                    ))
                    .await?
                }
                None => Box::pin(floe_experts::CalendarSetupStore::overview(&store)).await?,
            };
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                calendar_experts: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::CalendarSubjectPreview { request } => {
            let admission = calendar_access::DeviceCalendarAdmission::new(
                core,
                local_context,
                job.person,
                job.cancellation.clone(),
            );
            let subject = Box::pin(floe_context::preview_native_calendar_subject(
                &admission.connections,
                &admission.device,
                &floe_context::NativeCalendarSourceRequest {
                    person_id: job.person,
                    provider: request.provider,
                    device_id: request.device_id.clone(),
                    calendar_ids: request.calendar_ids.clone(),
                    connection_scope: request.connection_scope,
                    source_authority: Some(request.source_authority),
                    reviewed_native_subject_fingerprint: None,
                    connection_id: Some(request.connection_id.clone()),
                },
                &admission.window,
            ))
            .await?;
            Ok(VaultExecutionResult {
                calendar_subject_preview: Some(CalendarSubjectPreview {
                    provider: subject.provider,
                    device_id: subject.device_id,
                    calendar_ids: subject.calendar_ids,
                    connection_scope: subject.connection_scope,
                    connection_id: subject.connection_id,
                    connection_revision: subject.connection_revision,
                    source_authority: subject.source_authority,
                    native_subject_fingerprint: subject.native_subject_fingerprint,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::CalendarAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let store = calendar_access::VaultCalendarSetups {
                vault: vault.vault.as_ref(),
                packaging: schedule_packaging(),
                cancellation: job.cancellation.clone(),
            };
            let admission = calendar_access::DeviceCalendarAdmission::new(
                core,
                local_context,
                job.person,
                job.cancellation.clone(),
            );
            let overview = Box::pin(floe_experts::apply_calendar_access(
                &store,
                &admission,
                (**change).clone(),
            ))
            .await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                calendar_experts: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::PersonalAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let overview = floe_access::apply_personal_access(
                vault.vault.as_ref(),
                &personal_grants::native_driver(local_context),
                job.person,
                (**change).clone(),
                job.cancellation.clone(),
            )
            .await?;
            Ok(VaultExecutionResult {
                personal_access: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::ContactsAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let overview = floe_access::apply_contacts(
                vault.vault.as_ref(),
                &personal_grants::native_driver(local_context),
                job.person,
                (**change).clone(),
                job.cancellation.clone(),
            )
            .await?;
            Ok(VaultExecutionResult {
                personal_access: Some(overview),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::CalendarAction { operation } => {
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
                calendar_actions: Some(result),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::ConversationSession { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let session = match operation {
                ConversationSessionOperation::Start => {
                    ensure_builtin_experts(
                        vault,
                        core,
                        local_context,
                        job.person,
                        None,
                        job.cancellation.clone(),
                        floe_experts::BuiltinExpertRefresh::InstallIfAbsent,
                    )
                    .await?;
                    let receipt = floe_conversation::start_session(
                        vault.conversation_repository.as_ref(),
                        floe_conversation::SessionRequest {
                            principal: job.person.to_string(),
                        },
                    )
                    .await?;
                    floe_conversation::admitted_session(&***vault, job.person, receipt).await?
                }
                ConversationSessionOperation::Resume => {
                    let receipt = floe_conversation::resume_session(
                        vault.conversation_repository.as_ref(),
                        floe_conversation::SessionRequest {
                            principal: job.person.to_string(),
                        },
                    )
                    .await?;
                    floe_conversation::admitted_session(&***vault, job.person, receipt).await?
                }
                ConversationSessionOperation::Get { session_id } => {
                    let receipt = floe_conversation::get_session(
                        vault.conversation_repository.as_ref(),
                        floe_conversation::SessionReadRequest {
                            principal: job.person.to_string(),
                            session_id: *session_id,
                        },
                    )
                    .await?;
                    floe_conversation::admitted_session(&***vault, job.person, receipt).await?
                }
                ConversationSessionOperation::Recover {
                    session_id,
                    expected_revision,
                } => {
                    conversation_turn::recover(
                        vault,
                        &vault.conversation_repository,
                        job.person,
                        *session_id,
                        *expected_revision,
                    )
                    .await?
                }
            };
            Ok(VaultExecutionResult {
                session: Some(session),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::ConversationTurn { request } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            execute_conversation_turn_action(core, vault, local_context, job, request).await
        }
        WorkerAction::InspectProposal {
            session_id,
            invocation_id,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let reference = ExpertProposalReference {
                person_id: job.person,
                session_id: *session_id,
                invocation_id: *invocation_id,
            };
            let action = core
                .inspect_expert_calendar_action(
                    vault.vault.as_ref(),
                    ExpertCalendarInspection {
                        reference: reference.clone(),
                        cancellation: job.cancellation.clone(),
                        deadline: tokio::time::Instant::now() + Duration::from_secs(30),
                    },
                )
                .await?;
            let proposal = CalendarProposalInspection {
                person_id: job.person,
                session_id: reference.session_id,
                invocation_id: reference.invocation_id,
                action,
            };
            Ok(VaultExecutionResult {
                proposal: Some(proposal),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::MemoryReview { decision } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let review =
                floe_knowledge::review_memory(vault.vault.as_ref(), *decision, chrono::Utc::now())
                    .await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                memory_review: Some(review),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::Memory => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let snapshot = vault
                .memory_overview_snapshot(floe_knowledge::MAX_MEMORY_OVERVIEW_ITEMS)
                .await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok(VaultExecutionResult {
                memory: Some(snapshot),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::Connections => {
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
                |_| VaultState::Ready,
            )))
        }
        WorkerAction::RemotePairingPrepare => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            Ok(VaultExecutionResult {
                remote_owner: Some(Box::pin(vault.remote_owner_public_key()).await?),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemotePairingConfirm {
            route,
            challenge,
            polling_proof,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            let service = floe_connections::PairingService::new(
                floe_provider_adapters::control::authorization::HttpRemoteControl::new(
                    &route.route.base_url,
                )?,
            );
            let status = Box::pin(floe_connections::confirm_pairing(
                &service,
                &remote_authority::VaultPairingKeys {
                    vault: vault.vault.as_ref(),
                },
                &job.person.to_string(),
                pairing_identity(pairing),
                challenge,
                polling_proof,
                tokio::time::Instant::now() + remote_authority::PAIRING_DEADLINE,
                &job.cancellation,
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_pairing: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemotePairingStatus {
            route,
            pairing_id,
            polling_proof,
        } => {
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            let service = floe_connections::PairingService::new(
                floe_provider_adapters::control::authorization::HttpRemoteControl::new(
                    &route.route.base_url,
                )?,
            );
            let status = Box::pin(floe_connections::read_pairing_status(
                &service,
                &job.person.to_string(),
                pairing_identity(pairing),
                pairing_id,
                polling_proof,
                tokio::time::Instant::now() + remote_authority::PAIRING_DEADLINE,
                &job.cancellation,
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_pairing: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemotePairingFinalize {
            route,
            pairing_id,
            polling_proof,
            challenge,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            let service = floe_connections::PairingService::new(
                floe_provider_adapters::control::authorization::HttpRemoteControl::new(
                    &route.route.base_url,
                )?,
            );
            let status = Box::pin(floe_connections::finalize_pairing(
                &service,
                &remote_authority::VaultPairingKeys {
                    vault: vault.vault.as_ref(),
                },
                &job.person.to_string(),
                pairing_identity(pairing),
                pairing_id,
                polling_proof,
                challenge,
                tokio::time::Instant::now() + remote_authority::PAIRING_DEADLINE,
                &job.cancellation,
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_pairing: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteAuthorityInspectProducer { route } => {
            let vault = current.as_ref().map(|(_, vault)| vault.vault.as_ref());
            let transport = RemoteAuthorityEndpoint::new(&route.route, vault)?;
            let store = vault.map(|vault| remote_authority::VaultRemoteAuthority { vault });
            let inspection = Box::pin(floe_access::inspect_remote_authority(
                &transport,
                store
                    .as_ref()
                    .map(|store| store as &dyn floe_access::RemoteAuthorityStore),
                &remote_authority::authority_window(job.cancellation.clone()),
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_producer: Some(inspection.producer),
                remote_owner: inspection.owner,
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteAuthorityReviewAndEnroll { route, producer } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            let vault = vault.vault.as_ref();
            let transport = RemoteAuthorityEndpoint::new(&route.route, Some(vault))?;
            let enrolled = Box::pin(floe_access::review_and_enroll_remote_authority(
                &transport,
                &remote_authority::VaultRemoteAuthority { vault },
                job.person,
                access_pairing_identity(pairing),
                (**producer).clone(),
                &remote_authority::authority_window(job.cancellation.clone()),
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_producer: Some(enrolled.producer),
                remote_enrollment: Some(enrolled.enrollment),
                remote_owner: Some(enrolled.owner),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteAuthorityEnrollmentStatus {
            route,
            enrollment_id,
        } => {
            let vault = current.as_ref().map(|(_, vault)| vault.vault.as_ref());
            let transport = RemoteAuthorityEndpoint::new(&route.route, vault)?;
            let status = Box::pin(floe_access::remote_enrollment_status(
                &transport,
                enrollment_id,
                &remote_authority::authority_window(job.cancellation.clone()),
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_enrollment: Some(status),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteCalendarGrantPreview {
            route,
            connector_id,
            connection_id,
            resource,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            let vault = vault.vault.as_ref();
            let transport = RemoteAuthorityEndpoint::new(&route.route, Some(vault))?;
            let evidence = calendar_access::remote_calendar_evidence(core, job.person).await?;
            let preview = Box::pin(floe_access::preview_remote_calendar_grant(
                vault,
                &transport,
                floe_access::RemoteCalendarGrantRequest {
                    person_id: job.person,
                    pairing: access_pairing_identity(pairing),
                    connector_id,
                    connection_id,
                    resource,
                },
                evidence.as_access(),
                &remote_authority::authority_window(job.cancellation.clone()),
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_calendar_preview: Some(crate::RemoteCalendarGrantPreview {
                    person_id: job.person,
                    connector_id: preview.reference.connector_id,
                    connection_id: preview.reference.connection_id,
                    resource: preview.reference.resource,
                    source_authority: preview.reference.source_authority,
                    provider_identity: preview.reference.provider_identity,
                    execution_owner: preview.reference.execution_owner,
                    producer: preview.producer,
                    consumer: preview.consumer,
                    recipient: preview.recipient,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteCalendarGrantReview {
            route,
            connector_id,
            connection_id,
            resource,
            expected_producer_fingerprint,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?;
            let vault = vault.vault.as_ref();
            let transport = RemoteAuthorityEndpoint::new(&route.route, Some(vault))?;
            let evidence = calendar_access::remote_calendar_evidence(core, job.person).await?;
            let grant = Box::pin(floe_access::review_and_activate_remote_calendar_grant(
                vault,
                &transport,
                floe_access::RemoteCalendarGrantRequest {
                    person_id: job.person,
                    pairing: access_pairing_identity(pairing),
                    connector_id,
                    connection_id,
                    resource,
                },
                evidence.as_access(),
                expected_producer_fingerprint,
                &remote_authority::authority_window(job.cancellation.clone()),
            ))
            .await?;
            Ok(VaultExecutionResult {
                remote_calendar_grant: Some(RemoteGrantOverview {
                    grant,
                    connection_revision: None,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteCalendarGrantStatus { grant_id } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = floe_access::remote_calendar_grant(vault.vault.as_ref(), *grant_id).await?;
            Ok(VaultExecutionResult {
                remote_calendar_grant: Some(RemoteGrantOverview {
                    grant,
                    connection_revision: None,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteCalendarGrantPause {
            grant_id,
            expected_authority,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = floe_access::pause_remote_calendar_grant(
                vault.vault.as_ref(),
                *grant_id,
                *expected_authority,
            )
            .await?;
            Ok(VaultExecutionResult {
                remote_calendar_grant: Some(RemoteGrantOverview {
                    grant,
                    connection_revision: None,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteViewGrantPreview {
            route,
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let person_text = job.person.to_string();
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?.clone();
            let source_client = floe_provider_adapters::sources::ServerSourceClient::new(
                route.route.clone(),
                route.calendar_connections.clone(),
            )?;
            let transport = floe_provider_adapters::sources::AuthorizedSourceClient::new(
                &source_client,
                vault.vault.as_ref(),
            );
            let preview = floe_access::preview_remote_view_grant(
                vault.vault.as_ref(),
                &transport,
                floe_access::RemoteViewGrantRequest {
                    person_id: job.person,
                    pairing: floe_access::RemotePairingIdentity {
                        person_id: &person_text,
                        client_id: &pairing.client_id,
                        device_id: &pairing.device_id,
                    },
                    view_id,
                    connector_id,
                    connection_id,
                    resource,
                    consumer_name: consumer,
                    data_category: floe_context::remote_view_data_category(view_id),
                },
                floe_context::remote_view_resource(view_id, connection_id) == *resource,
                floe_context::is_remote_view(view_id),
                &floe_access::RemoteCallWindow {
                    deadline: tokio::time::Instant::now() + Duration::from_secs(10),
                    cancellation: job.cancellation.clone(),
                },
            )
            .await?;
            Ok(VaultExecutionResult {
                remote_view_preview: Some(preview),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteViewGrantReview {
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
            let person_text = job.person.to_string();
            let pairing = route.pairing().ok_or(AgentFailure::PolicyDenied)?.clone();
            let source_client = floe_provider_adapters::sources::ServerSourceClient::new(
                route.route.clone(),
                route.calendar_connections.clone(),
            )?;
            let transport = floe_provider_adapters::sources::AuthorizedSourceClient::new(
                &source_client,
                vault.vault.as_ref(),
            );
            let grant = floe_access::review_and_activate_remote_view_grant(
                vault.vault.as_ref(),
                &transport,
                floe_access::RemoteViewGrantRequest {
                    person_id: job.person,
                    pairing: floe_access::RemotePairingIdentity {
                        person_id: &person_text,
                        client_id: &pairing.client_id,
                        device_id: &pairing.device_id,
                    },
                    view_id,
                    connector_id,
                    connection_id,
                    resource,
                    consumer_name: consumer,
                    data_category: floe_context::remote_view_data_category(view_id),
                },
                floe_access::RemoteViewGrantExpectation {
                    producer_fingerprint: expected_producer_fingerprint,
                    source_authority: *expected_source_authority,
                    connection_revision: *expected_connection_revision,
                    provider_identity: expected_provider_identity,
                    recipient: expected_recipient,
                },
                floe_context::remote_view_resource(view_id, connection_id) == *resource,
                floe_context::is_remote_view(view_id),
                &floe_access::RemoteCallWindow {
                    deadline: tokio::time::Instant::now() + Duration::from_secs(30),
                    cancellation: job.cancellation.clone(),
                },
            )
            .await?;
            Ok(VaultExecutionResult {
                remote_view_grant: Some(RemoteGrantOverview {
                    grant,
                    connection_revision: Some(*expected_connection_revision),
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteViewGrantStatus { grant_id } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = vault.get_remote_view_grant(*grant_id).await?;
            Ok(VaultExecutionResult {
                remote_view_grant: Some(RemoteGrantOverview {
                    grant,
                    connection_revision: None,
                }),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::RemoteViewGrantPause {
            grant_id,
            expected_authority,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let grant = vault
                .pause_remote_view_grant(*grant_id, *expected_authority)
                .await?;
            Ok(VaultExecutionResult {
                remote_view_grant: Some(RemoteGrantOverview {
                    grant,
                    connection_revision: None,
                }),
                ..VaultExecutionResult::ready()
            })
        }
    }
}

async fn execute_agent_calendar_action<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    operation: &CalendarActionOperation,
    cancellation: &Cancellation,
) -> Result<crate::services::CalendarActionsResult, AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let mode = match operation {
        CalendarActionOperation::GetAuthority => Some(vault.agent_action_policy().await?),
        CalendarActionOperation::SetAuthority { calendar_create } => {
            let mode = vault.set_agent_action_policy(*calendar_create).await?;
            core.set_action_authority(person_id, mode)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            Some(mode)
        }
        _ => None,
    };
    if let Some(calendar_create) = mode {
        return Ok(crate::services::CalendarActionsResult {
            actions: vec![],
            writes_enabled: None,
            authority: Some(floe_actions::ActionAuthority {
                person_id,
                calendar_create,
            }),
        });
    }
    let action_id = match operation {
        CalendarActionOperation::Get { action_id }
        | CalendarActionOperation::Decide { action_id, .. }
        | CalendarActionOperation::Execute { action_id }
        | CalendarActionOperation::Recover { action_id } => *action_id,
        _ => return Err(AgentFailure::InvalidInput),
    };
    let stored = vault.agent_calendar_action(action_id).await?;
    if stored.person_id != person_id || stored.agent_origin.is_none() || stored.direct {
        return Err(AgentFailure::PolicyDenied);
    }
    let action = match operation {
        CalendarActionOperation::Get { .. } => stored,
        CalendarActionOperation::Decide { approve, .. } => {
            core.decide_expert_calendar_action(
                vault,
                person_id,
                action_id,
                *approve,
                chrono::Utc::now(),
            )
            .await?
        }
        CalendarActionOperation::Execute { .. } | CalendarActionOperation::Recover { .. } => {
            if person_id.to_string()
                != floe_provider_adapters::sources::native_calendar::LOCAL_PERSON
                || stored.provider != CalendarProvider::EventKit
            {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            let provider =
                floe_provider_adapters::sources::native_calendar::NativeCalendar::new(vec![
                    stored.calendar_id.clone(),
                ]);
            if matches!(operation, CalendarActionOperation::Recover { .. }) {
                core.recover_expert_calendar_action(vault, person_id, action_id, &provider)
                    .await?
            } else {
                let policy = stored.expert_proposal_policy(
                    floe_provider_adapters::sources::native_calendar::NativeCalendar::enabled(),
                );
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
    Ok(crate::services::CalendarActionsResult {
        actions: vec![action],
        writes_enabled: None,
        authority: None,
    })
}

async fn ensure_builtin_experts<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    core: &FloeCore,
    local_context: &LocalContextHost,
    person_id: PersonId,
    remote_route: Option<&RemoteTurnRoute>,
    cancellation: Cancellation,
    when: floe_experts::BuiltinExpertRefresh,
) -> Result<(), AgentFailure> {
    let _ = local_context;
    floe_experts::ensure_builtin_experts(
        &expert_setup::VaultBuiltinExperts {
            vault,
            specs: builtin_setup_specs(),
            cancellation,
        },
        builtin_source_bindings(core, person_id, remote_route).await,
        BuiltinExpertKind::BUILTIN_SETUP.len(),
        when,
    )
    .await
}

/// What each builtin source is bound to, and how well it is being served.
///
/// Which source needs what is the builtin Experts' own declaration and what a
/// connection state means is Connections'; this reads the two together into the
/// bindings the registry records.
async fn builtin_source_bindings(
    core: &FloeCore,
    person_id: PersonId,
    remote_route: Option<&RemoteTurnRoute>,
) -> Vec<BuiltinSourceBinding> {
    let paired_server =
        if remote_route.is_some_and(|route| !route.route.external || route.route.allow_external) {
            BuiltinSourceEvidence::Serving
        } else {
            BuiltinSourceEvidence::Absent
        };
    let calendar = core
        .calendar_connector_snapshot(
            person_id,
            &format!("local-{}", std::env::consts::OS),
            chrono::Utc::now(),
        )
        .await
        .ok()
        .flatten()
        .map(|snapshot| snapshot.connection.state);
    let device_connection = match calendar {
        Some(state) if state.is_serving() => BuiltinSourceEvidence::Serving,
        Some(state) if state.is_withheld() => BuiltinSourceEvidence::Withheld,
        _ => BuiltinSourceEvidence::Absent,
    };
    [
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
        source: source_id(source),
        view_handle: builtin_source_handle(person_id, source),
        state: match source.requirement() {
            BuiltinSourceRequirement::DeviceConnection => device_connection,
            BuiltinSourceRequirement::Device => BuiltinSourceEvidence::Serving,
            BuiltinSourceRequirement::PairedServer => paired_server,
            BuiltinSourceRequirement::Unserved => BuiltinSourceEvidence::Absent,
        }
        .state(),
    })
    .collect()
}

/// The registry identity of one builtin source.
fn source_id(source: BuiltinContextSource) -> floe_experts::AgentId {
    floe_experts::AgentId::try_new(source.source_id()).expect("builtin source ids are valid")
}

/// What the builtin Experts declare about themselves, in the shape the registry
/// installs. The declarations are the Experts'; the packaging is the registry's.
fn builtin_setup_specs() -> Vec<floe_experts::ExpertSetupSpec> {
    floe_experts_builtin::builtin_setup_declarations()
        .into_iter()
        .map(expert_setup_spec)
        .collect()
}

fn expert_setup_spec(
    declaration: floe_experts_builtin::BuiltinExpertDeclaration,
) -> floe_experts::ExpertSetupSpec {
    let expert = floe_experts::AgentId::try_new(declaration.expert_id)
        .expect("builtin expert ids are valid");
    let packaging = expert_packaging(&declaration, expert.clone());
    floe_experts::ExpertSetupSpec {
        packages: packaging.packages(declaration.data_class),
        expert,
        required_sources: declaration
            .required_sources
            .iter()
            .map(|source| {
                floe_experts::AgentId::try_new(*source).expect("builtin source ids are valid")
            })
            .collect(),
        mandatory_source: floe_experts::AgentId::try_new(declaration.mandatory_source)
            .expect("builtin source ids are valid"),
    }
}

fn expert_packaging(
    declaration: &floe_experts_builtin::BuiltinExpertDeclaration,
    expert: floe_experts::AgentId,
) -> floe_experts::ExpertPackaging {
    floe_experts::ExpertPackaging {
        expert,
        tool_id: declaration.tool_id.clone(),
        version: declaration.version.to_owned(),
        publisher: declaration.publisher.to_owned(),
        metadata: floe_experts::ExpertMetadata {
            name: declaration.name.to_owned(),
            description: declaration.description.to_owned(),
            domain_tags: declaration.domain_tags.clone(),
            skills: declaration.skills.clone(),
            supported_placements: declaration.supported_placements.clone(),
        },
        state_schema_version: floe_experts_builtin::BUILTIN_EXPERT_STATE_SCHEMA_VERSION,
    }
}

/// How the Schedule Expert is packaged when its calendar setup is installed.
fn schedule_packaging() -> floe_experts::ExpertPackaging {
    let declaration = floe_experts_builtin::BuiltinExpertKind::Schedule.declaration();
    let expert = floe_experts::AgentId::try_new(declaration.expert_id)
        .expect("builtin expert ids are valid");
    expert_packaging(&declaration, expert)
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

/// Whether the client must reload the session before continuing.
///
/// Recoveries that keep the current session usable do not force a reload.

/// Whether the client must stop applying results for the current session.

async fn sample_session(
    store: &impl SessionStore,
    person: PersonId,
    id: Uuid,
) -> Result<AgentSession, AgentFailure> {
    let session = store.load(person, id).await?;
    if session.scope.is_some()
        || session.data_classes != [floe_agent_contract::DataClass::Synthetic]
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    // These regressions drive the worker the way the binding does, so the
    // fixtures they stand up are stated on the binding's wire.
    use crate::AgentFixturePrompt;
    use floe_protocol::*;
    use floe_vault::VaultKey;
    use ring::signature::{self, Ed25519KeyPair, KeyPair};
    use serde_json::json;
    use sha2::{Digest, Sha256};
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
    mod vault_registry;

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
                Arc::new(LocalContextHost::default()),
                Arc::new(crate::events::AppEventBuffer::default()),
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

    fn wait(worker: &Worker, person: PersonId, id: Uuid) -> WorkerResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = worker
                .request(person, id, WorkerOperation::Poll { after_sequence: 0 })
                .unwrap();
            if result.done {
                return result;
            }
            assert!(Instant::now() < deadline, "Vault job did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn perform(worker: &Worker, person: PersonId, action: WorkerAction) -> WorkerResult {
        let id = Uuid::new_v4();
        worker
            .request(
                person,
                id,
                WorkerOperation::Submit {
                    action: Box::new(action),
                },
            )
            .unwrap();
        let result = wait(worker, person, id);
        worker
            .request(person, id, WorkerOperation::Release)
            .unwrap();
        result
    }

    #[test]
    fn remote_enrollment_requires_saved_pairing_before_network_access() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let person = PersonId::new();
        let created = perform(&worker, person, WorkerAction::Create {});
        assert_eq!(
            created.failure, None,
            "create failed: {:?}",
            created.failure
        );
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://not-loopback.invalid".into(),
                bearer_token: "not-a-real-token".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: None,
            },
            calendar_connections: vec![],
        };
        let producer = floe_access::RemoteProducerIdentity {
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
            WorkerAction::RemoteAuthorityReviewAndEnroll {
                route: Box::new(route),
                producer: Box::new(producer),
            },
        );
        assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
    }

    #[test]
    fn remote_enrollment_rejects_pairing_for_another_person_before_network_access() {
        let directory = tempfile::tempdir().unwrap();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let person = PersonId::new();
        let created = perform(&worker, person, WorkerAction::Create {});
        assert_eq!(
            created.failure, None,
            "create failed: {:?}",
            created.failure
        );
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: "http://not-loopback.invalid".into(),
                bearer_token: "not-a-real-token".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: Some(floe_inference::RoutePairing {
                    client_id: "saved-client".into(),
                    person_id: PersonId::new().to_string(),
                    device_id: "saved-device".into(),
                }),
            },
            calendar_connections: vec![],
        };
        let producer = floe_access::RemoteProducerIdentity {
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
            WorkerAction::RemoteAuthorityReviewAndEnroll {
                route: Box::new(route),
                producer: Box::new(producer),
            },
        );
        assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
    }

    #[test]
    fn remote_enrollment_uses_signed_challenge_and_stays_pending_admin() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let created = perform(&worker, person, WorkerAction::Create {});
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
        let producer = floe_access::RemoteProducerIdentity {
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
        let route = RemoteTurnRoute {
            route: floe_inference::RemoteRoute {
                base_url: format!("http://127.0.0.1:{}", address.port()),
                bearer_token: "secret_token_value_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
                recipient: None,
                pairing: Some(floe_inference::RoutePairing {
                    client_id: "client-1".into(),
                    person_id: person.to_string(),
                    device_id: "device-1".into(),
                }),
            },
            calendar_connections: vec![],
        };
        let result = perform(
            &worker,
            person,
            WorkerAction::RemoteAuthorityReviewAndEnroll {
                route: Box::new(route),
                producer: Box::new(producer),
            },
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
        producer: floe_access::RemoteProducerIdentity,
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

        let result = perform(&worker, person, WorkerAction::Connections {});

        assert!(result.failure.is_none());
        assert_eq!(result.connections, Some(vec![]));
        assert!(!directory.path().join("vaults").exists());
    }

    #[test]
    fn calendar_action_policy_round_trips_through_the_unlocked_vault_worker() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        perform(&worker, person, WorkerAction::Create {});
        let changed = perform(
            &worker,
            person,
            WorkerAction::CalendarAction {
                operation: CalendarActionOperation::SetAuthority {
                    calendar_create: floe_actions::ActionAuthorityMode::Deny,
                },
            },
        );
        assert_eq!(changed.failure, None);
        assert_eq!(
            changed
                .calendar_actions
                .unwrap()
                .authority
                .unwrap()
                .calendar_create,
            floe_actions::ActionAuthorityMode::Deny
        );
        perform(&worker, person, WorkerAction::Lock {});
        let locked = perform(
            &worker,
            person,
            WorkerAction::CalendarAction {
                operation: CalendarActionOperation::GetAuthority {},
            },
        );
        assert_eq!(locked.failure, Some(AgentFailure::VaultUnavailable));
        perform(&worker, person, WorkerAction::Unlock {});
        let restored = perform(
            &worker,
            person,
            WorkerAction::CalendarAction {
                operation: CalendarActionOperation::GetAuthority {},
            },
        );
        assert_eq!(restored.failure, None);
        assert_eq!(
            restored
                .calendar_actions
                .unwrap()
                .authority
                .unwrap()
                .calendar_create,
            floe_actions::ActionAuthorityMode::Deny
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
                WorkerOperation::Submit {
                    action: Box::new(WorkerAction::Status {}),
                },
            )
            .unwrap();

        assert!(learner.is_cancelled());
        wait(&worker, person, id);
        worker
            .request(person, id, WorkerOperation::Release)
            .unwrap();
    }

    #[test]
    fn general_conversations_are_encrypted_personal_sessions_not_samples() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        perform(&worker, person, WorkerAction::Create {});
        let created = perform(
            &worker,
            person,
            WorkerAction::ConversationSession {
                operation: ConversationSessionOperation::Start,
            },
        )
        .session
        .unwrap();
        assert!(created.scope.is_none());
        assert_eq!(
            created.data_classes,
            [floe_agent_contract::DataClass::Personal]
        );
        assert_eq!(
            perform(
                &worker,
                person,
                WorkerAction::ConversationSession {
                    operation: ConversationSessionOperation::Recover {
                        session_id: created.id,
                        expected_revision: created.revision + 1,
                    },
                },
            )
            .failure,
            Some(AgentFailure::Conflict)
        );
        assert_eq!(
            perform(
                &worker,
                person,
                WorkerAction::ConversationSession {
                    operation: ConversationSessionOperation::Recover {
                        session_id: created.id,
                        expected_revision: created.revision,
                    },
                },
            )
            .session,
            Some(created.clone())
        );
        let installed_revision = perform(&worker, person, WorkerAction::Registry { change: None })
            .registry
            .unwrap()
            .revision;
        let resumed = perform(
            &worker,
            person,
            WorkerAction::ConversationSession {
                operation: ConversationSessionOperation::Resume,
            },
        )
        .session
        .unwrap();
        assert_eq!(resumed.id, created.id);
        assert_eq!(
            perform(&worker, person, WorkerAction::Registry { change: None },)
                .registry
                .unwrap()
                .revision,
            installed_revision,
        );
        let sample = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Resume,
            },
        )
        .session
        .unwrap();
        assert_ne!(sample.id, created.id);
        assert_eq!(
            sample.data_classes,
            [floe_agent_contract::DataClass::Synthetic]
        );
    }

    #[test]
    fn registry_jobs_are_read_only_until_explicit_change_and_reconcile_duplicate_submits() {
        use floe_experts::{RegistryConfiguration, RegistryConfigurationTarget};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        let person = PersonId::new();
        let worker = Worker::new(root.clone(), Keys::default()).unwrap();
        assert_eq!(
            perform(&worker, person, WorkerAction::Registry { change: None }).failure,
            Some(AgentFailure::VaultUnavailable)
        );
        assert!(!root.exists());
        perform(&worker, person, WorkerAction::Create {});
        let empty = perform(&worker, person, WorkerAction::Registry { change: None });
        assert_eq!(empty.state, Some(VaultState::Ready));
        assert!(empty.registry.is_none());
        let session = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Start,
            },
        )
        .session
        .unwrap();
        perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Turn {
                    session_id: session.id,
                    expected_revision: 0,
                    prompt: AgentFixturePrompt::Today,
                },
            },
        );
        let before = perform(&worker, person, WorkerAction::Registry { change: None })
            .registry
            .unwrap();
        let assignment = before
            .assignments
            .iter()
            .find(|assignment| assignment.granted_tool_count == 1)
            .unwrap();
        let action = || WorkerAction::Registry {
            change: Some(RegistryConfiguration {
                instance_id: before.instance_id,
                expected_revision: before.revision,
                target: RegistryConfigurationTarget::Assignment {
                    id: assignment.id,
                    enabled: false,
                },
            }),
        };
        let id = Uuid::new_v4();
        worker
            .request(
                person,
                id,
                WorkerOperation::Submit {
                    action: Box::new(action()),
                },
            )
            .unwrap();
        let done = wait(&worker, person, id);
        let replayed = worker
            .request(
                person,
                id,
                WorkerOperation::Submit {
                    action: Box::new(action()),
                },
            )
            .unwrap();
        assert_eq!(
            (replayed.request_id, replayed.stage, replayed.done),
            (done.request_id, done.stage.clone(), done.done)
        );
        assert!(matches!(
            worker.request(
                PersonId::new(),
                id,
                WorkerOperation::Poll { after_sequence: 0 }
            ),
            Err(AgentFailure::NotFound)
        ));
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
            .request(person, id, WorkerOperation::Release)
            .unwrap();
        assert_eq!(
            perform(&worker, person, action()).failure,
            Some(AgentFailure::Conflict)
        );
        perform(&worker, person, WorkerAction::Lock {});
        perform(&worker, person, WorkerAction::Unlock {});
        assert_eq!(
            perform(&worker, person, WorkerAction::Registry { change: None })
                .registry
                .as_ref(),
            Some(after)
        );
        let session = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Start,
            },
        )
        .session
        .unwrap();
        let denied = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Turn {
                    session_id: session.id,
                    expected_revision: 0,
                    prompt: AgentFixturePrompt::Today,
                },
            },
        )
        .session
        .unwrap();
        assert_eq!(denied.messages.len(), 1);
        assert_eq!(
            denied.last_outcome,
            Some(floe_conversation::AgentOutcome::Halted {
                reason: AgentFailure::InvalidModelOutput,
            })
        );
        let current = perform(&worker, person, WorkerAction::Registry { change: None })
            .registry
            .unwrap();
        assert_eq!(current, *after);
        perform(&worker, person, WorkerAction::Lock {});
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
            perform(&worker, person, WorkerAction::Unlock {}).state,
            Some(VaultState::Ready)
        );
        for operation in [
            FixtureOperation::Get {
                session_id: session.id,
            },
            FixtureOperation::Recover {
                session_id: session.id,
                expected_revision: session.revision,
            },
        ] {
            let result = perform(&worker, person, WorkerAction::Session { operation });
            assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
            assert_eq!(result.state, Some(VaultState::Ready));
            assert!(result.session.is_none());
            assert!(result.events.is_empty());
        }
        let sample = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Resume,
            },
        )
        .session
        .unwrap();
        assert_ne!(sample.id, session.id);
        assert_eq!(
            sample.data_classes,
            [floe_agent_contract::DataClass::Synthetic]
        );
    }

    #[test]
    fn worker_keeps_expert_assignment_and_state_across_host_restart_and_new_chat() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vaults");
        let keys = Keys::default();
        let person = PersonId::new();
        let worker = Worker::new(root.clone(), keys.clone()).unwrap();
        assert_eq!(
            perform(&worker, person, WorkerAction::Create {}).state,
            Some(VaultState::Ready)
        );
        fn run(worker: &Worker, person: PersonId) -> floe_experts::ExpertResult {
            let session = perform(
                worker,
                person,
                WorkerAction::Session {
                    operation: FixtureOperation::Start,
                },
            )
            .session
            .unwrap();
            let completed = perform(
                worker,
                person,
                WorkerAction::Session {
                    operation: FixtureOperation::Turn {
                        session_id: session.id,
                        expected_revision: 0,
                        prompt: AgentFixturePrompt::Today,
                    },
                },
            )
            .session
            .unwrap();
            assert_eq!(
                completed.last_outcome,
                Some(floe_conversation::AgentOutcome::Completed)
            );
            let floe_conversation::AgentMessage::Delegation { task, .. } = &completed.messages[1]
            else {
                panic!("expected Expert result");
            };
            serde_json::from_str(
                task.data_part(floe_experts::EXPERT_RESULT_MEDIA_TYPE)
                    .unwrap(),
            )
            .unwrap()
        }
        let first = run(&worker, person);
        assert_eq!(first.state_revision, 1);
        assert_eq!(
            perform(&worker, person, WorkerAction::Lock {}).state,
            Some(VaultState::Locked)
        );
        drop(worker);
        let worker = Worker::new(root, keys).unwrap();
        assert_eq!(
            perform(&worker, person, WorkerAction::Unlock {}).state,
            Some(VaultState::Ready)
        );
        let second = run(&worker, person);
        assert_eq!(second.state_revision, 2);
        assert_eq!(second.assignment_id, first.assignment_id);
        assert_eq!(second.instance_id, first.instance_id);
        assert_eq!(second.view_handle, first.view_handle);
        assert_eq!(
            perform(&worker, person, WorkerAction::Lock {}).state,
            Some(VaultState::Locked)
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
            perform(&worker, person, WorkerAction::Status {}).state,
            Some(VaultState::Missing)
        );
        assert!(!root.exists());
        assert_eq!(
            perform(&worker, person, WorkerAction::Create {}).state,
            Some(VaultState::Ready)
        );
        let session = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Resume,
            },
        )
        .session
        .unwrap();
        assert_eq!(
            session.data_classes,
            [floe_agent_contract::DataClass::Synthetic]
        );
        assert_eq!(
            perform(&worker, PersonId::new(), WorkerAction::Lock {}).failure,
            Some(AgentFailure::NotFound)
        );
        assert_eq!(
            perform(&worker, person, WorkerAction::Lock {}).state,
            Some(VaultState::Locked)
        );
        assert_eq!(
            perform(&worker, person, WorkerAction::Unlock {}).state,
            Some(VaultState::Ready)
        );
        assert_eq!(
            perform(
                &worker,
                person,
                WorkerAction::Session {
                    operation: FixtureOperation::Resume
                }
            )
            .session
            .unwrap(),
            session
        );
        keys.0.unavailable.store(true, Ordering::Release);
        assert_eq!(
            perform(&worker, person, WorkerAction::Status {}).failure,
            Some(AgentFailure::VaultUnavailable)
        );
        assert_eq!(
            perform(
                &worker,
                person,
                WorkerAction::Session {
                    operation: FixtureOperation::Resume
                }
            )
            .failure,
            Some(AgentFailure::VaultUnavailable)
        );
        keys.0.unavailable.store(false, Ordering::Release);
        assert_eq!(
            perform(&worker, person, WorkerAction::Unlock {}).state,
            Some(VaultState::Ready)
        );
        assert_eq!(keys.0.values.lock().unwrap().len(), 1);
    }

    #[test]
    fn worker_streams_cancels_replays_and_blocks_recovery_of_a_live_run() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        perform(&worker, person, WorkerAction::Create {});
        let session = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Start,
            },
        )
        .session
        .unwrap();
        let id = Uuid::new_v4();
        let operation = || WorkerOperation::Submit {
            action: Box::new(WorkerAction::Session {
                operation: FixtureOperation::Turn {
                    session_id: session.id,
                    expected_revision: session.revision,
                    prompt: AgentFixturePrompt::Today,
                },
            }),
        };
        worker.request(person, id, operation()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = worker.request(person, id, operation()).unwrap();
            if result.events.iter().any(|event| {
                matches!(
                    event.event,
                    floe_conversation::AgentEventKind::ModelStarted { .. }
                )
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(matches!(
            worker.request(
                person,
                Uuid::new_v4(),
                WorkerOperation::Submit {
                    action: Box::new(WorkerAction::Session {
                        operation: FixtureOperation::Recover {
                            session_id: session.id,
                            expected_revision: 1
                        }
                    })
                }
            ),
            Err(AgentFailure::Conflict)
        ));
        assert!(matches!(
            worker.request(PersonId::new(), id, WorkerOperation::Stop),
            Err(AgentFailure::NotFound)
        ));
        worker.request(person, id, WorkerOperation::Stop).unwrap();
        let result = wait(&worker, person, id);
        assert_eq!(
            result.session.as_ref().unwrap().last_outcome,
            Some(floe_conversation::AgentOutcome::Halted {
                reason: AgentFailure::Cancelled
            })
        );
        let awaited = wait(&worker, person, id);
        assert_eq!(
            (awaited.request_id, awaited.next_sequence, awaited.done),
            (result.request_id, result.next_sequence, result.done)
        );
        assert!(matches!(
            worker.request(
                person,
                id,
                WorkerOperation::Poll {
                    after_sequence: result.next_sequence + 1
                }
            ),
            Err(AgentFailure::InvalidInput)
        ));
        worker
            .request(person, id, WorkerOperation::Release)
            .unwrap();
        let resumed = perform(
            &worker,
            person,
            WorkerAction::Session {
                operation: FixtureOperation::Resume,
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
        perform(&worker, person, WorkerAction::Create {});
        keys.0.entered.store(false, Ordering::Release);
        *keys.0.paused.lock().unwrap() = true;
        let id = Uuid::new_v4();
        worker
            .request(
                person,
                id,
                WorkerOperation::Submit {
                    action: Box::new(WorkerAction::Status {}),
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
                .request(person, id, WorkerOperation::Poll { after_sequence: 0 })
                .unwrap()
                .done
        );
        assert!(
            !worker
                .request(person, id, WorkerOperation::Stop)
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

/// The Person's vault, as this device's own storage shows it.
/// The pairing a route names, as Connections states one.
fn pairing_identity(
    pairing: &floe_inference::RoutePairing,
) -> floe_connections::PairingIdentity<'_> {
    floe_connections::PairingIdentity {
        person_id: &pairing.person_id,
        client_id: &pairing.client_id,
        device_id: &pairing.device_id,
    }
}

/// The pairing a route names, as Access states one.
fn access_pairing_identity(
    pairing: &floe_inference::RoutePairing,
) -> floe_access::RemotePairingIdentity<'_> {
    floe_access::RemotePairingIdentity {
        person_id: &pairing.person_id,
        client_id: &pairing.client_id,
        device_id: &pairing.device_id,
    }
}

fn stored_vault_state(root: &std::path::Path, person: PersonId) -> VaultState {
    if root.join(person.to_string()).join("vault.id").exists() {
        VaultState::Locked
    } else {
        VaultState::Missing
    }
}
