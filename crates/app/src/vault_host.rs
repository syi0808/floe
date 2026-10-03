//! Host generation composition and lifecycle. Conversation and Connections
//! operations execute through typed owner handles, never this legacy S2 queue.
#[cfg(target_os = "android")]
use crate::android_vault_keys::AndroidVaultKeys as PlatformVaultKeys;
use crate::local_context::LocalContextHost;
use crate::local_operations::{LocalOperationAdmission, LocalOperationIntent, LocalOperationOwner};
use crate::{
    CalendarActionOperation, CalendarProposalInspection, FloeCore, VaultState, WorkerAction,
    WorkerResult,
};
use floe_actions::{ExpertCalendarInspection, ExpertProposalReference};
use floe_context_contract::CalendarProvider;
use floe_execution::Cancellation;
use floe_experts::{Directory, DirectoryEntry, TaskCoordinator};
use floe_kernel::{AgentFailure, PersonId};
#[cfg(not(target_os = "android"))]
use floe_vault::KeyringVaultKeys as PlatformVaultKeys;
use floe_vault::{
    EncryptedAgentVault, VaultConversationRepository, VaultKeyProvider, VaultTaskRepository,
};
use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
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
    time::Duration,
};
use uuid::Uuid;
pub(crate) mod calendar_access;
mod conversation_turn;
mod expert_binding_settings;
mod expert_setup;
mod learner_worker;
mod personal_grants;
mod product_actions;
mod remote_views;
pub(crate) mod review_snapshot;
pub(crate) mod source_resolver;
const MAX_VAULT_JOBS: usize = 64;
const MAX_IN_FLIGHT_VAULT_JOBS: usize = 8;
const LEARNER_IDLE_DELAY: Duration = Duration::from_millis(750);
const LEARNER_EMPTY_DELAY: Duration = Duration::from_secs(30);
const LEARNER_ERROR_DELAY: Duration = Duration::from_secs(5);

pub(crate) struct VaultBridge {
    root: PathBuf,
    core: Arc<FloeCore>,
    local_context: Arc<LocalContextHost>,
    worker: RefCell<Option<Worker>>,
    ready: Arc<Mutex<Option<Arc<crate::owner_handles::ReadyOwners>>>>,
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
            local_context,
            worker: RefCell::new(None),
            ready: Arc::new(Mutex::new(None)),
        }
    }
    pub(crate) fn ready(
        &self,
        caller: &crate::CallerContext,
    ) -> Result<Arc<crate::owner_handles::ReadyOwners>, AgentFailure> {
        let owners = self
            .ready
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .clone()
            .ok_or(AgentFailure::VaultUnavailable)?;
        owners.check(&caller.owner_actor())?;
        Ok(owners)
    }
    pub(crate) fn local_request(
        &self,
        caller: &crate::CallerContext,
        id: Uuid,
        intent: Option<LocalOperationIntent>,
        owner: LocalOperationOwner,
        release: bool,
    ) -> Result<WorkerResult, AgentFailure> {
        self.worker()?
            .local_request(caller, id, intent, owner, release)
    }
    fn worker(&self) -> Result<RefMut<'_, Worker>, AgentFailure> {
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker = Some(Worker::new(
                self.root.clone(),
                PlatformVaultKeys,
                self.core.clone(),
                self.local_context.clone(),
                self.ready.clone(),
            )?);
        }
        Ok(RefMut::map(worker, |worker| {
            worker.as_mut().expect("initialized")
        }))
    }
    pub(crate) fn shutdown(&self) {
        self.worker.borrow_mut().take();
    }
}
struct Worker {
    sender: Option<mpsc::SyncSender<Arc<Job>>>,
    jobs: Mutex<HashMap<Uuid, Arc<Job>>>,
    closing: Arc<AtomicBool>,
    learner_scheduling: floe_knowledge::LearnerScheduling,
    thread: Option<std::thread::JoinHandle<()>>,
}
struct Job {
    person: PersonId,
    id: Uuid,
    action: WorkerAction,
    admission: LocalOperationAdmission,
    cancellation: Cancellation,
    result: Mutex<Option<Result<VaultExecutionResult, AgentFailure>>>,
}

struct RootAgentEnvironmentAdmission {
    actor: floe_kernel::OwnerActor,
    operation_id: Uuid,
    cancellation: Cancellation,
}

struct OpenVault<Keys: VaultKeyProvider> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    core: Arc<FloeCore>,
    local_context: Arc<LocalContextHost>,
    sources: crate::connection_observe::SourceServices,
    model: Arc<dyn floe_agent_contract::ModelPort + Send + Sync>,
    actor: floe_kernel::OwnerActor,
    owners: Arc<crate::owner_handles::ReadyOwners>,
    available: AtomicBool,
    generation_cancellation: Cancellation,
    conversation_repository: Arc<VaultConversationRepository<Keys>>,
    _recovered_conversation_runs: Vec<floe_conversation::RunRecord>,
    task_coordinator: Arc<TaskCoordinator<VaultTaskRepository<Keys>>>,
    directory: Directory,
    registrations: Vec<Arc<conversation_turn::expert_dispatch::BoundExpertRegistration>>,
    _recovered_tasks: Vec<floe_agent_contract::TaskReceipt>,
}

fn validated_expert_registrations(
    registrations: Vec<conversation_turn::expert_dispatch::BoundExpertRegistration>,
) -> Result<Vec<Arc<conversation_turn::expert_dispatch::BoundExpertRegistration>>, AgentFailure> {
    validate_expert_registration_set(registrations.iter())?;
    Ok(registrations.into_iter().map(Arc::new).collect())
}

fn validate_expert_registration_set<'a>(
    registrations: impl IntoIterator<
        Item = &'a conversation_turn::expert_dispatch::BoundExpertRegistration,
    >,
) -> Result<(), AgentFailure> {
    let registrations: Vec<_> = registrations.into_iter().collect();
    if registrations.is_empty() || registrations.len() > 64 {
        return Err(AgentFailure::InvalidInput);
    }
    let mut package_ids = std::collections::HashSet::new();
    for registration in &registrations {
        registration.manifest.validate()?;
        if let conversation_turn::expert_dispatch::BoundExpertRunner::Shipped(runner) =
            &registration.runner
        {
            if runner.manifest() != registration.manifest {
                return Err(AgentFailure::Conflict);
            }
        }
        if !package_ids.insert(registration.manifest.package.id.as_str()) {
            return Err(AgentFailure::Conflict);
        }
    }
    floe_experts::manifest_set_digest(
        &registrations
            .iter()
            .map(|registration| registration.manifest.clone())
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

impl<Keys: VaultKeyProvider + 'static> OpenVault<Keys> {
    async fn activate(
        vault: EncryptedAgentVault<Keys>,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
        registrations: Vec<conversation_turn::expert_dispatch::BoundExpertRegistration>,
        admission: RootAgentEnvironmentAdmission,
    ) -> Result<Self, AgentFailure> {
        let registrations = validated_expert_registrations(registrations)?;
        let vault = Arc::new(vault);
        let conversation_activation = vault.activate_conversation_executor().await?;
        let conversation_repository =
            Arc::new(VaultConversationRepository::new(Arc::clone(&vault)));
        let directory = Directory::default();
        let repository = Arc::new(VaultTaskRepository::new(Arc::clone(&vault)));
        let (task_coordinator, recovered_tasks) = TaskCoordinator::activate(
            directory.clone(),
            repository,
            "everyday-assistance",
            floe_agent_contract::MAX_OUTPUT_BYTES,
        )
        .await?;
        let task_coordinator = Arc::new(task_coordinator);
        let sources = crate::connection_observe::SourceServices::new(
            vault.clone(),
            core.clone(),
            local_context.clone(),
            admission.actor.clone(),
        )?;
        let resolver = sources.dependency_resolver.clone();
        let model: Arc<dyn floe_agent_contract::ModelPort + Send + Sync> =
            Arc::new(floe_inference::InferenceService::new(
                floe_provider_adapters::gateway::CompositeModelProvider::new(
                    sources.gateway_credentials.as_ref().clone(),
                ),
                resolver.clone(),
                sources.gateway_credentials.clone(),
            ));
        let budget = floe_conversation::AgentBudget::default();
        let conversation: Arc<dyn floe_conversation::ConversationOwner> =
            Arc::new(floe_conversation::ConversationService::new(
                floe_conversation::ConversationDependencies {
                    repository: conversation_repository.clone(),
                    sessions: vault.clone(),
                    experts: task_coordinator.clone(),
                    model: model.clone(),
                    evidence: sources.evidence_reader.clone(),
                    resolver,
                    connections: sources.connections.clone(),
                    runtime_epoch: admission.actor.runtime_epoch,
                },
                floe_conversation::ManagerConfig {
                    role_spec: floe_conversation::prompts::manager_role_spec(),
                    purpose: floe_inference::CANONICAL_MODEL_PURPOSE.into(),
                    max_iterations: budget.max_iterations.min(64),
                    max_output_bytes: budget.max_output_bytes,
                    max_run_duration: Duration::from_millis(budget.deadline_ms),
                    budget: floe_execution::budget::BudgetConfig::new(
                        budget.max_tokens,
                        budget.max_cost_micros,
                    )
                    .with_finalization_reserve(1_024, 10_000.min(budget.max_cost_micros)),
                },
            )?);
        let owners = Arc::new(crate::owner_handles::ReadyOwners::new(
            admission.actor.clone(),
            sources.connections.clone(),
            conversation,
        ));
        let open = Self {
            vault,
            core,
            local_context,
            sources,
            model,
            actor: admission.actor.clone(),
            owners,
            available: AtomicBool::new(true),
            generation_cancellation: Cancellation::default(),
            conversation_repository,
            _recovered_conversation_runs: conversation_activation.interrupted,
            task_coordinator,
            directory,
            registrations,
            _recovered_tasks: recovered_tasks,
        };
        open.prepare_root_agent_environment(&admission).await?;
        let scope = crate::owner_handles::host_scope(
            admission.operation_id,
            admission.cancellation.clone(),
            Duration::from_secs(30),
        );
        open.sources
            .connections
            .activate(&admission.actor, &scope)
            .await?;
        open.owners
            .conversation
            .activate(&admission.actor, &scope)
            .await?;
        Ok(open)
    }

    async fn prepare_root_agent_environment(
        &self,
        admission: &RootAgentEnvironmentAdmission,
    ) -> Result<(), AgentFailure> {
        if admission.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let first_install = self.vault.expert_registry().await?.is_none();
        ensure_expert_bundle(self, admission.cancellation.clone()).await?;
        if first_install {
            expert_binding_settings::bind_initial_defaults(
                self,
                self.vault.person_id(),
                &admission.actor.device_id,
                admission.operation_id,
                &admission.cancellation,
            )
            .await?;
        }
        if admission.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        self.publish_expert_directory(&self.registrations).await?;
        if admission.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        Ok(())
    }

    async fn publish_expert_directory(
        &self,
        registrations: &[Arc<conversation_turn::expert_dispatch::BoundExpertRegistration>],
    ) -> Result<(), AgentFailure> {
        validate_expert_registration_set(registrations.iter().map(Arc::as_ref))?;
        let mut entries = Vec::new();
        let snapshot = self
            .vault
            .expert_registry()
            .await?
            .ok_or(AgentFailure::NotFound)?;
        let registry =
            floe_experts::AgentRegistry::restore(snapshot, self.vault.registry_instance_id())?;
        for (card, admission) in registry.enabled_expert_admissions(self.vault.person_id())? {
            let Some(registration) = registrations
                .iter()
                .find(|registration| registration.manifest.package == admission.package)
            else {
                continue;
            };
            let manifest = &registration.manifest;
            manifest.validate()?;
            let installed = registry.resolve_admitted(self.vault.person_id(), &admission)?;
            let selection = registry.execution_selection(self.vault.person_id(), &admission)?;
            if card != manifest.definition.card || installed.manifest != *manifest {
                return Err(AgentFailure::Conflict);
            }
            entries.push((
                DirectoryEntry {
                    definition: manifest.definition.clone(),
                    admission: admission.clone(),
                    selection: selection.clone(),
                    reviewed: true,
                    enabled: true,
                    admitted_principals: vec![self.vault.person_id().to_string()],
                    purposes: vec!["everyday-assistance".into()],
                },
                Arc::new(
                    conversation_turn::expert_dispatch::RegisteredExpertEndpoint::new(
                        Arc::clone(&self.core),
                        Arc::clone(&self.vault),
                        Arc::clone(&self.local_context),
                        self.sources.gateway_credentials.as_ref().clone(),
                        self.model.clone(),
                        self.actor.clone(),
                        self.sources.connections.clone(),
                        admission.clone(),
                        selection,
                        Arc::clone(registration),
                    ),
                ) as Arc<dyn floe_agent_contract::AgentEndpoint>,
            ));
        }
        self.directory.publish("product.experts", entries)?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    fn mark_unavailable(&self) {
        self.available.store(false, Ordering::Release);
        self.generation_cancellation
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        self.owners.close_admission();
        self.vault.seal();
    }

    async fn shutdown(&self, operation_id: Uuid) -> Result<(), AgentFailure> {
        self.owners.close_admission();
        self.generation_cancellation
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        let scope = crate::owner_handles::host_scope(
            operation_id,
            Cancellation::default(),
            Duration::from_secs(35),
        );
        let (conversation, sources) = tokio::join!(
            self.owners.conversation.shutdown(&scope),
            self.sources.connections.shutdown_and_drain(&scope),
        );
        self.mark_unavailable();
        conversation.and(sources)
    }

    async fn run_learner(&self, scheduling: Cancellation) -> Result<bool, AgentFailure> {
        let cancellation = self.generation_cancellation.child_scope();
        let work = learner_worker::run(
            &self.vault,
            self.model.as_ref(),
            &self.actor.device_id,
            cancellation.clone(),
        );
        tokio::pin!(work);
        tokio::select! {
            biased;
            _ = scheduling.cancelled() => {
                cancellation.cancel_with_reason(scheduling.reason().unwrap_or(floe_execution::CancelReason::OwnerDropped));
                work.await
            }
            result = &mut work => result,
        }
    }
}

impl<Keys: VaultKeyProvider> Deref for OpenVault<Keys> {
    type Target = EncryptedAgentVault<Keys>;

    fn deref(&self) -> &Self::Target {
        &self.vault
    }
}

impl<Keys: VaultKeyProvider> Drop for OpenVault<Keys> {
    fn drop(&mut self) {
        self.available.store(false, Ordering::Release);
        self.generation_cancellation
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
        self.owners.close_admission();
        self.vault.seal();
    }
}

impl Worker {
    fn new<Keys: VaultKeyProvider + Clone + 'static>(
        root: PathBuf,
        keys: Keys,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
        ready: Arc<Mutex<Option<Arc<crate::owner_handles::ReadyOwners>>>>,
    ) -> Result<Self, AgentFailure> {
        let (sender, receiver) = mpsc::sync_channel::<Arc<Job>>(MAX_IN_FLIGHT_VAULT_JOBS);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        let learner_scheduling = floe_knowledge::LearnerScheduling::default();
        let scheduling = learner_scheduling.clone();
        let thread = std::thread::Builder::new()
            .name("floe-vault-lifecycle".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build();
                let mut current: Option<(PersonId, Arc<OpenVault<Keys>>)> = None;
                let mut delay = LEARNER_IDLE_DELAY;
                while !worker_closing.load(Ordering::Acquire) {
                    match receiver.recv_timeout(delay) {
                        Ok(job) => {
                            let result = catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => runtime.block_on(execute_action(
                                    &root,
                                    &keys,
                                    &core,
                                    &local_context,
                                    &mut current,
                                    &job,
                                )),
                                Err(_) => Err(AgentFailure::VaultUnavailable),
                            }))
                            .unwrap_or(Err(AgentFailure::Interrupted));
                            if matches!(
                                result,
                                Err(AgentFailure::VaultUnavailable | AgentFailure::Interrupted)
                            ) {
                                if let Some((_, open)) = current.take() {
                                    if let Ok(runtime) = &runtime {
                                        let _ = catch_unwind(AssertUnwindSafe(|| {
                                            runtime.block_on(open.shutdown(job.id))
                                        }));
                                    }
                                    open.mark_unavailable();
                                }
                            }
                            if let Ok(mut slot) = ready.lock() {
                                *slot = current
                                    .as_ref()
                                    .filter(|(_, open)| open.is_available())
                                    .map(|(_, open)| open.owners.clone());
                            }
                            if let Ok(mut slot) = job.result.lock() {
                                *slot = Some(result);
                            }
                            let _ = scheduling.foreground_finished();
                            delay = LEARNER_IDLE_DELAY;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let Some((_, open)) = current.as_ref() else {
                                delay = LEARNER_EMPTY_DELAY;
                                continue;
                            };
                            if !open.is_available() {
                                current = None;
                                if let Ok(mut slot) = ready.lock() {
                                    *slot = None;
                                }
                                continue;
                            }
                            let lease = match scheduling.try_start() {
                                Ok(Some(value)) => value,
                                Ok(None) => continue,
                                Err(_) => {
                                    delay = LEARNER_ERROR_DELAY;
                                    continue;
                                }
                            };
                            let result = catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => {
                                    runtime.block_on(open.run_learner(lease.cancellation()))
                                }
                                Err(_) => Err(AgentFailure::VaultUnavailable),
                            }))
                            .unwrap_or(Err(AgentFailure::Interrupted));
                            if matches!(
                                result,
                                Err(AgentFailure::VaultUnavailable | AgentFailure::Interrupted)
                            ) {
                                if let Some((_, open)) = current.take() {
                                    if let Ok(runtime) = &runtime {
                                        let _ = catch_unwind(AssertUnwindSafe(|| {
                                            runtime.block_on(open.shutdown(Uuid::new_v4()))
                                        }));
                                    }
                                    open.mark_unavailable();
                                }
                                if let Ok(mut slot) = ready.lock() {
                                    *slot = None;
                                }
                            }
                            delay = match result {
                                Ok(true) => LEARNER_IDLE_DELAY,
                                Ok(false) => LEARNER_EMPTY_DELAY,
                                Err(_) => LEARNER_ERROR_DELAY,
                            };
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                if let Ok(mut slot) = ready.lock() {
                    *slot = None;
                }
                if let Some((_, open)) = current.take() {
                    if let Ok(runtime) = &runtime {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            runtime.block_on(open.shutdown(Uuid::new_v4()))
                        }));
                    }
                    open.mark_unavailable();
                }
            })
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(Self {
            sender: Some(sender),
            jobs: Mutex::new(HashMap::new()),
            closing,
            learner_scheduling,
            thread: Some(thread),
        })
    }
    fn local_request(
        &self,
        caller: &crate::CallerContext,
        id: Uuid,
        intent: Option<LocalOperationIntent>,
        owner: LocalOperationOwner,
        release: bool,
    ) -> Result<WorkerResult, AgentFailure> {
        if id.is_nil() || self.closing.load(Ordering::Acquire) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
        if let Some(intent) = intent {
            if intent.owner() != owner {
                return Err(AgentFailure::PolicyDenied);
            }
            let admission = LocalOperationAdmission {
                caller: caller.clone(),
                intent,
            };
            if let Some(prior) = jobs.get(&id) {
                if prior.admission != admission {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                if jobs.len() >= MAX_VAULT_JOBS {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let job = Arc::new(Job {
                    person: PersonId(caller.person_id()),
                    id,
                    action: admission.intent.action(caller),
                    admission,
                    cancellation: Cancellation::default(),
                    result: Mutex::new(None),
                });
                self.learner_scheduling.foreground_submitted()?;
                if self
                    .sender
                    .as_ref()
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .try_send(job.clone())
                    .is_err()
                {
                    let _ = self.learner_scheduling.foreground_finished();
                    return Err(AgentFailure::BudgetExceeded);
                }
                jobs.insert(id, job);
            }
        }
        let job = jobs.get(&id).cloned().ok_or(AgentFailure::NotFound)?;
        if job.admission.caller != *caller || job.admission.intent.owner() != owner {
            return Err(AgentFailure::NotFound);
        }
        let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
        let value = result.as_ref().and_then(|r| r.as_ref().ok());
        let response = WorkerResult {
            request_id: id,
            person_id: job.person,
            stage: job.action.name().into(),
            done: result.is_some(),
            state: value.map(|v| v.state),
            registry: value.and_then(|v| v.registry.clone()),
            expert_candidates: value.and_then(|v| v.expert_candidates.clone()),
            proposal: value.and_then(|v| v.proposal.clone()),
            memory_review: value.and_then(|v| v.memory_review.clone()),
            memory: value.and_then(|v| v.memory.clone()),
            calendar_actions: value.and_then(|v| v.calendar_actions.clone()),
            failure: result.as_ref().and_then(|r| r.as_ref().err()).copied(),
        };
        if release {
            if !response.done {
                return Err(AgentFailure::Conflict);
            }
            jobs.remove(&id);
        }
        Ok(response)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        self.learner_scheduling.close();
        if let Ok(jobs) = self.jobs.lock() {
            for job in jobs.values() {
                job.cancellation.cancel();
            }
        }
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[derive(Default)]
struct VaultExecutionResult {
    state: VaultState,
    registry: Option<floe_experts::RegistryOverview>,
    expert_candidates: Option<crate::ExpertCandidateCatalog>,
    proposal: Option<CalendarProposalInspection>,
    memory_review: Option<floe_knowledge::MemoryReviewResult>,
    memory: Option<floe_knowledge::MemoryOverviewSnapshot>,
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
async fn execute_action<Keys: VaultKeyProvider + Clone + 'static>(
    root: &std::path::Path,
    keys: &Keys,
    core: &Arc<FloeCore>,
    local_context: &Arc<LocalContextHost>,
    current: &mut Option<(PersonId, Arc<OpenVault<Keys>>)>,
    job: &Job,
) -> Result<VaultExecutionResult, AgentFailure> {
    if job.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if current
        .as_ref()
        .is_some_and(|(person, _)| *person != job.person)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    match &job.action {
        WorkerAction::Status => {
            if let Some((_, vault)) = current {
                vault.check_access()?;
                return Ok(VaultExecutionResult::ready());
            }
            let state = stored_vault_state(root, job.person)?;
            Ok(VaultExecutionResult::new(state))
        }
        WorkerAction::Create => {
            let admission = &job.admission;
            if admission.intent
                != LocalOperationIntent::VaultCommand(crate::VaultLifecycleCommand::Create)
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let environment_admission = RootAgentEnvironmentAdmission {
                actor: admission.caller.owner_actor(),
                operation_id: job.id,
                cancellation: job.cancellation.clone(),
            };
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
                conversation_turn::expert_dispatch::shipped_registrations(),
                environment_admission,
            )
            .await?;
            *current = Some((job.person, Arc::new(vault)));
            Ok(VaultExecutionResult::ready())
        }
        WorkerAction::Unlock => {
            let admission = &job.admission;
            if admission.intent
                != LocalOperationIntent::VaultCommand(crate::VaultLifecycleCommand::Unlock)
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let environment_admission = RootAgentEnvironmentAdmission {
                actor: admission.caller.owner_actor(),
                operation_id: job.id,
                cancellation: job.cancellation.clone(),
            };
            if current.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let vault = OpenVault::activate(
                EncryptedAgentVault::open(root, job.person, keys.clone()).await?,
                Arc::clone(core),
                Arc::clone(local_context),
                conversation_turn::expert_dispatch::shipped_registrations(),
                environment_admission,
            )
            .await?;
            *current = Some((job.person, Arc::new(vault)));
            Ok(VaultExecutionResult::ready())
        }
        WorkerAction::Lock => {
            if let Some((_, open)) = current.take() {
                open.shutdown(job.id).await?;
            }
            Ok(VaultExecutionResult::new(VaultState::Locked))
        }
        WorkerAction::Registry { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let registry = match change {
                Some(configuration) => {
                    let registry = vault
                        .configure_registry(configuration.clone(), job.cancellation.clone())
                        .await?;
                    vault.publish_expert_directory(&vault.registrations).await?;
                    Some(registry)
                }
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
        WorkerAction::ExpertCandidates {
            assignment_id,
            requirement_key,
            device_id,
        } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let candidates = expert_binding_settings::inspect(
                vault,
                job.person,
                device_id,
                *assignment_id,
                requirement_key,
                &job.cancellation,
            )
            .await?;
            Ok(VaultExecutionResult {
                expert_candidates: Some(candidates),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::ExpertReplaceBinding { change, device_id } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let candidates = expert_binding_settings::replace(
                vault,
                job.person,
                device_id,
                job.id,
                change,
                &job.cancellation,
            )
            .await?;
            Ok(VaultExecutionResult {
                expert_candidates: Some(candidates),
                ..VaultExecutionResult::ready()
            })
        }
        WorkerAction::CalendarAction { operation } => {
            let result = product_actions::execute(
                core,
                current.as_ref().map(|(_, vault)| vault.vault.as_ref()),
                job.person,
                operation,
                &job.cancellation,
            )
            .await?;
            Ok(VaultExecutionResult {
                calendar_actions: Some(result),
                ..VaultExecutionResult::new(if current.is_some() {
                    VaultState::Ready
                } else {
                    stored_vault_state(root, job.person)?
                })
            })
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

async fn ensure_expert_bundle<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    cancellation: Cancellation,
) -> Result<(), AgentFailure> {
    floe_experts::ensure_expert_bundle(&expert_setup::VaultExpertBundle {
        vault,
        manifests: floe_experts_builtin::registrations()
            .into_iter()
            .map(|registration| registration.manifest)
            .collect(),
        cancellation,
    })
    .await
}

/// The Person's vault, as this device's own storage shows it.
fn stored_vault_state(
    root: &std::path::Path,
    person: PersonId,
) -> Result<VaultState, AgentFailure> {
    match fs::symlink_metadata(root.join(person.to_string()).join("vault.id")) {
        Ok(metadata) if metadata.is_file() => Ok(VaultState::Locked),
        Ok(_) => Err(AgentFailure::VaultUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(VaultState::Missing),
        Err(_) => Err(AgentFailure::VaultUnavailable),
    }
}
