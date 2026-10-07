//! The bounded host queue for Runtime preparation. Domain work runs
//! through ready owner handles on the retained generation runtime.

use crate::local_context::LocalContextHost;
use crate::owner_handles::{ReadyOwners, execute_on_handle};
use crate::ready_generation::ReadyGeneration;
use crate::runtime_control::{
    RuntimeFailureProjection, RuntimePreparationCommandFailure, RuntimePreparationResult,
    RuntimeReadiness, RuntimeReadinessState, validate_runtime_caller, validate_runtime_id,
};
use crate::storage_profile::{ProfileVaultKeys as PlatformVaultKeys, vault_keys};
use crate::{CallerContext, FloeCore, ModelProviderFactory};
use floe_execution::{CancelReason, Cancellation};
use floe_kernel::{AgentFailure, PersonId};
use floe_vault::EncryptedAgentVault;

use std::{
    collections::HashMap,
    fs,
    os::unix::fs::DirBuilderExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use uuid::Uuid;

const MAX_RECEIPTS: usize = 64;
const MAX_PENDING: usize = 8;
const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(60);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparationIntent {
    Prepare,
}
impl PreparationIntent {
    fn stage(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
        }
    }
}

/// These failures retire the published generation before an outcome is exposed.
fn requires_retirement(failure: AgentFailure) -> bool {
    matches!(
        failure,
        AgentFailure::VaultUnavailable
            | AgentFailure::VaultLocked
            | AgentFailure::StorageUnavailable
            | AgentFailure::Interrupted
            | AgentFailure::DeadlineExceeded
    )
}

type Generation = ReadyGeneration<PlatformVaultKeys>;
type OpenGeneration = (CallerContext, Arc<Generation>);
#[derive(Default)]
struct Published {
    closing: bool,
    worker_failure: Option<AgentFailure>,
    preparation_failure: Option<AgentFailure>,
    failure_correlation: Option<Uuid>,
    current: Option<OpenGeneration>,
}
#[derive(Default)]
struct BridgeState {
    closing: bool,
    finished: bool,
    failure: Option<AgentFailure>,
    worker: Option<Worker>,
}
pub(crate) struct RuntimePreparationHost {
    root: PathBuf,
    expected: CallerContext,
    runtime_handle: tokio::runtime::Handle,
    core: Arc<FloeCore>,
    local_context: Arc<LocalContextHost>,
    model_provider_factory: Arc<dyn ModelProviderFactory>,
    state: Mutex<BridgeState>,
    drained: Condvar,
    published: Arc<Mutex<Published>>,
}
impl RuntimePreparationHost {
    pub(crate) fn new(
        database_path: &str,
        expected: CallerContext,
        runtime_handle: tokio::runtime::Handle,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
        model_provider_factory: Arc<dyn ModelProviderFactory>,
    ) -> Self {
        Self {
            root: PathBuf::from(format!("{database_path}.agent-vaults")),
            expected,
            runtime_handle,
            core,
            local_context,
            model_provider_factory,
            state: Mutex::new(BridgeState::default()),
            drained: Condvar::new(),
            published: Arc::new(Mutex::new(Published::default())),
        }
    }

    pub(crate) fn ready(&self, caller: &CallerContext) -> Result<Arc<ReadyOwners>, AgentFailure> {
        validate_runtime_caller(&self.expected, caller)?;
        let slot = self
            .published
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing {
            return Err(AgentFailure::Interrupted);
        }
        if slot.worker_failure.is_some() {
            // A terminal queue failure is reported by Runtime readiness. Feature
            // calls fail closed without advertising another impossible prepare.
            return Err(AgentFailure::Interrupted);
        }
        let Some((admitted, generation)) = slot.current.as_ref() else {
            return Err(AgentFailure::VaultUnavailable);
        };
        if admitted != caller {
            return Err(AgentFailure::PolicyDenied);
        }
        let owners = generation.owners();
        owners.check(&caller.owner_actor())?;
        if let Err(failure) = generation.check_access() {
            let gateway = self.core.product_gateway.lock();
            let close = owners.close_admission();
            gateway.and(close)?;
            return Err(failure);
        }
        Ok(owners)
    }

    pub(crate) fn readiness(
        &self,
        caller: &CallerContext,
        correlation_id: Uuid,
    ) -> Result<RuntimeReadiness, AgentFailure> {
        validate_runtime_caller(&self.expected, caller)?;
        let state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        let worker_failure = state
            .worker
            .as_ref()
            .map(Worker::health_failure)
            .transpose()?
            .flatten();
        drop(state);
        if let Some(failure) = worker_failure {
            publish_worker_failure(&self.published, failure, correlation_id);
        }
        let mut slot = self
            .published
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing {
            return Ok(RuntimeReadiness {
                state: RuntimeReadinessState::Unavailable,
                failure: Some(RuntimeFailureProjection::project(
                    AgentFailure::Interrupted,
                    correlation_id,
                    false,
                )),
            });
        }
        if let Some(failure) = slot.worker_failure {
            return Ok(RuntimeReadiness {
                state: RuntimeReadinessState::Unavailable,
                failure: Some(RuntimeFailureProjection::project(
                    failure,
                    slot.failure_correlation.unwrap_or(correlation_id),
                    false,
                )),
            });
        }
        if let Some((admitted, generation)) = slot.current.as_ref() {
            if admitted != caller {
                return Err(AgentFailure::PolicyDenied);
            }
            let owners = generation.owners();
            let observed_failure = match owners.check(&caller.owner_actor()) {
                Ok(()) => generation.check_access().err(),
                Err(failure) => Some(failure),
            };
            if observed_failure.is_none() {
                return Ok(RuntimeReadiness {
                    state: RuntimeReadinessState::Ready,
                    failure: None,
                });
            }
            let gateway = self.core.product_gateway.lock();
            let close = owners.close_admission();
            gateway.and(close)?;
            let failure = observed_failure.unwrap_or(AgentFailure::VaultUnavailable);
            slot.preparation_failure = Some(failure);
            slot.failure_correlation = Some(correlation_id);
            return Ok(RuntimeReadiness {
                state: RuntimeReadinessState::PreparationRequired,
                failure: Some(RuntimeFailureProjection::project(
                    failure,
                    correlation_id,
                    true,
                )),
            });
        }
        if let Some(failure) = slot.preparation_failure {
            return Ok(RuntimeReadiness {
                state: RuntimeReadinessState::PreparationRequired,
                failure: Some(RuntimeFailureProjection::project(
                    failure,
                    slot.failure_correlation.unwrap_or(correlation_id),
                    true,
                )),
            });
        }
        let root = self.root.clone();
        let person = PersonId(caller.person_id());
        let stored_state = execute_on_handle(&self.runtime_handle, async move {
            stored_vault_state(&root, person)
        });
        match stored_state {
            Ok(_) => Ok(RuntimeReadiness {
                state: RuntimeReadinessState::PreparationRequired,
                failure: None,
            }),
            Err(failure) => Ok(RuntimeReadiness {
                state: RuntimeReadinessState::Unavailable,
                failure: Some(RuntimeFailureProjection::project(
                    failure,
                    correlation_id,
                    false,
                )),
            }),
        }
    }

    pub(crate) fn prepare(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure> {
        validate_runtime_caller(&self.expected, caller)
            .map_err(RuntimePreparationCommandFailure::NotAdmitted)?;
        validate_runtime_id(id).map_err(RuntimePreparationCommandFailure::NotAdmitted)?;
        let mut state = self.state.lock().map_err(|_| {
            RuntimePreparationCommandFailure::NotAdmitted(AgentFailure::Interrupted)
        })?;
        if state.closing {
            return Err(RuntimePreparationCommandFailure::NotAdmitted(
                AgentFailure::Interrupted,
            ));
        }
        if let Some(worker) = state.worker.as_ref() {
            if let Some(result) = worker
                .cached_result(caller, id)
                .map_err(RuntimePreparationCommandFailure::NotAdmitted)?
            {
                if result.done {
                    return Ok(result);
                }
                if worker
                    .health_failure()
                    .map_err(RuntimePreparationCommandFailure::NotAdmitted)?
                    .is_some()
                {
                    return Err(RuntimePreparationCommandFailure::Indeterminate(
                        AgentFailure::Interrupted,
                    ));
                }
                return Ok(result);
            }
        }
        let archived = self
            .load_archived_receipt(id)
            .map_err(RuntimePreparationCommandFailure::Indeterminate)?;
        if let Some(receipt) = archived {
            if receipt.operation_id != id {
                return Err(RuntimePreparationCommandFailure::NotAdmitted(
                    AgentFailure::Conflict,
                ));
            }
            if receipt.person_id.0 != caller.person_id()
                || receipt.device_id != caller.device_id()
                || receipt.runtime_epoch != caller.runtime_epoch()
            {
                return Err(RuntimePreparationCommandFailure::NotAdmitted(
                    AgentFailure::PolicyDenied,
                ));
            }
            if receipt.intent != PreparationIntent::Prepare.stage() {
                return Err(RuntimePreparationCommandFailure::NotAdmitted(
                    AgentFailure::Conflict,
                ));
            }
            return preparation_receipt_result(caller, id, receipt)
                .map_err(RuntimePreparationCommandFailure::NotAdmitted);
        }
        if let Some(worker) = state.worker.as_ref() {
            if worker
                .health_failure()
                .map_err(|_| {
                    RuntimePreparationCommandFailure::NotAdmitted(AgentFailure::Interrupted)
                })?
                .is_some()
            {
                return Err(RuntimePreparationCommandFailure::NotAdmitted(
                    AgentFailure::Interrupted,
                ));
            }
        }
        if state.worker.is_none() {
            state.worker = Some(
                Worker::new(
                    self.root.clone(),
                    self.core.clone(),
                    self.local_context.clone(),
                    self.published.clone(),
                    self.model_provider_factory.clone(),
                )
                .map_err(RuntimePreparationCommandFailure::NotAdmitted)?,
            );
        }
        state
            .worker
            .as_ref()
            .ok_or(RuntimePreparationCommandFailure::NotAdmitted(
                AgentFailure::Interrupted,
            ))?
            .prepare(caller, id)
    }

    pub(crate) fn get_preparation(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure> {
        validate_runtime_caller(&self.expected, caller)?;
        validate_runtime_id(id)?;
        let state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.closing {
            return Err(AgentFailure::Interrupted);
        }
        if let Some(worker) = state.worker.as_ref() {
            if let Some(result) = worker.cached_result(caller, id)? {
                if !result.done && worker.health_failure()?.is_some() {
                    return Err(AgentFailure::Interrupted);
                }
                return Ok(result);
            }
        }
        drop(state);
        self.load_archived_preparation(caller, id)
    }

    pub(crate) fn acknowledge(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure> {
        validate_runtime_caller(&self.expected, caller)
            .map_err(RuntimePreparationCommandFailure::NotAdmitted)?;
        validate_runtime_id(id).map_err(RuntimePreparationCommandFailure::NotAdmitted)?;
        let state = self.state.lock().map_err(|_| {
            RuntimePreparationCommandFailure::NotAdmitted(AgentFailure::Interrupted)
        })?;
        if state.closing {
            return Err(RuntimePreparationCommandFailure::NotAdmitted(
                AgentFailure::Interrupted,
            ));
        }
        if let Some(worker) = state.worker.as_ref() {
            if let Some(result) = worker
                .acknowledge_cached(caller, id)
                .map_err(RuntimePreparationCommandFailure::NotAdmitted)?
            {
                return Ok(result);
            }
        }
        drop(state);
        let receipt = self
            .load_archived_receipt(id)
            .map_err(RuntimePreparationCommandFailure::Indeterminate)?
            .ok_or(RuntimePreparationCommandFailure::NotAdmitted(
                AgentFailure::NotFound,
            ))?;
        preparation_receipt_result(caller, id, receipt)
            .map_err(RuntimePreparationCommandFailure::NotAdmitted)
    }

    fn load_archived_preparation(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<RuntimePreparationResult, AgentFailure> {
        let archive = self
            .load_archived_receipt(id)?
            .ok_or(AgentFailure::NotFound)?;
        preparation_receipt_result(caller, id, archive)
    }

    fn load_archived_receipt(
        &self,
        id: Uuid,
    ) -> Result<Option<floe_vault::StoredVaultLifecycleReceipt>, AgentFailure> {
        let store = self.core.store.clone();
        execute_on_handle(&self.runtime_handle, async move {
            tokio::time::timeout(
                Duration::from_secs(5),
                store.load_vault_lifecycle_receipt(id),
            )
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .map_err(|_| AgentFailure::StorageUnavailable)
        })
    }

    pub(crate) fn shutdown(&self) -> Result<(), AgentFailure> {
        let (mut state, mut failure) = match self.state.lock() {
            Ok(state) => (state, None),
            Err(poisoned) => (poisoned.into_inner(), Some(AgentFailure::Interrupted)),
        };
        if state.closing {
            while !state.finished {
                state = match self.drained.wait(state) {
                    Ok(state) => state,
                    Err(poisoned) => {
                        failure.get_or_insert(AgentFailure::Interrupted);
                        poisoned.into_inner()
                    }
                };
            }
            return state.failure.or(failure).map_or(Ok(()), Err);
        }
        state.closing = true;
        let mut worker = state.worker.take();
        drop(state);
        let fence = catch_unwind(AssertUnwindSafe(|| {
            close_published(&self.published, &self.core, true)
        }))
        .unwrap_or(Err(AgentFailure::Interrupted));
        if let Err(error) = fence {
            failure.get_or_insert(error);
        }
        if let Some(worker) = worker.as_mut() {
            if let Err(error) = worker.shutdown() {
                failure.get_or_insert(error);
            }
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                failure.get_or_insert(AgentFailure::Interrupted);
                poisoned.into_inner()
            }
        };
        state.failure = failure;
        state.finished = true;
        self.drained.notify_all();
        failure.map_or(Ok(()), Err)
    }
}
impl Drop for RuntimePreparationHost {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

struct Job {
    caller: CallerContext,
    id: Uuid,
    intent: PreparationIntent,
    cancellation: Cancellation,
    result: Mutex<Option<Result<(), AgentFailure>>>,
    archived: AtomicBool,
    preparation_receipt: AtomicBool,
}
struct Worker {
    sender: Option<mpsc::SyncSender<Arc<Job>>>,
    jobs: Arc<Mutex<HashMap<Uuid, Arc<Job>>>>,
    closing: Arc<AtomicBool>,
    health: Arc<Mutex<Option<AgentFailure>>>,
    thread: Option<std::thread::JoinHandle<Result<(), AgentFailure>>>,
    failure: Option<AgentFailure>,
}
impl Worker {
    fn new(
        root: PathBuf,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
        published: Arc<Mutex<Published>>,
        model_provider_factory: Arc<dyn ModelProviderFactory>,
    ) -> Result<Self, AgentFailure> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(crate::owner_handles::EXECUTOR_STACK_BYTES)
            .enable_all()
            .build()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let (sender, receiver) = mpsc::sync_channel::<Arc<Job>>(MAX_PENDING);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        let health = Arc::new(Mutex::new(None));
        let worker_health = health.clone();
        let jobs = Arc::new(Mutex::new(HashMap::<Uuid, Arc<Job>>::new()));
        let thread = std::thread::Builder::new()
            .name("floe-runtime-preparation".into())
            .stack_size(crate::owner_handles::EXECUTOR_STACK_BYTES)
            .spawn(move || {
                let mut current = None;
                let mut fatal = None;
                let mut shutdown_failure = None;
                // This receiver blocks only the dedicated queue thread; the
                // runtime workers remain available for admitted owner work.
                while let Ok(job) = receiver.recv() {
                    // Lookup failure is not an execution outcome. Retain the
                    // queued identity and retry; never retire a live generation
                    // or overwrite an existing receipt because its read failed.
                    let saved = loop {
                        match runtime.block_on(async {
                            tokio::time::timeout(
                                Duration::from_secs(5),
                                core.store.load_vault_lifecycle_receipt(job.id),
                            )
                            .await
                        }) {
                            Ok(Ok(value)) => break value,
                            _ => std::thread::sleep(Duration::from_millis(200)),
                        }
                    };
                    let from_archive = saved.is_some();
                    let failed_before = fatal.is_some();
                    let outcome = if from_archive {
                        let receipt = saved.ok_or(AgentFailure::StorageUnavailable);
                        match receipt {
                            Ok(receipt) => {
                                job.archived.store(true, Ordering::Release);
                                let valid = receipt_is_preparation(&job, &receipt);
                                job.preparation_receipt.store(valid, Ordering::Release);
                                receipt_outcome(&job, receipt)
                            }
                            Err(failure) => Err(failure),
                        }
                    } else if worker_closing.load(Ordering::Acquire) {
                        Err(AgentFailure::Interrupted)
                    } else if let Some(failure) = fatal {
                        Err(failure)
                    } else {
                        match catch_unwind(AssertUnwindSafe(|| {
                            runtime.block_on(async {
                                tokio::time::timeout(
                                    LIFECYCLE_TIMEOUT,
                                    execute(
                                        &root,
                                        &core,
                                        &local_context,
                                        &published,
                                        &model_provider_factory,
                                        &mut current,
                                        &job,
                                    ),
                                )
                                .await
                                .map_err(|_| AgentFailure::DeadlineExceeded)?
                            })
                        })) {
                            Ok(result) => result,
                            Err(_) => {
                                fatal = Some(AgentFailure::Interrupted);
                                set_worker_failure(
                                    &worker_health,
                                    &published,
                                    AgentFailure::Interrupted,
                                    job.id,
                                );
                                Err(AgentFailure::Interrupted)
                            }
                        }
                    };
                    if !from_archive
                        && !failed_before
                        && let Some(failure) = fatal
                    {
                        let cleanup = retire(&runtime, &core, &published, &mut current, job.id);
                        if let Err(cleanup_failure) = cleanup {
                            fatal = Some(cleanup_failure);
                        }
                        set_worker_failure(
                            &worker_health,
                            &published,
                            fatal.unwrap_or(failure),
                            job.id,
                        );
                    }
                    if !from_archive
                        && !failed_before
                        && fatal.is_none()
                        && outcome
                            .as_ref()
                            .err()
                            .is_some_and(|failure| requires_retirement(*failure))
                    {
                        if let Err(failure) =
                            retire(&runtime, &core, &published, &mut current, job.id)
                        {
                            shutdown_failure.get_or_insert(failure);
                            fatal = Some(failure);
                            set_worker_failure(&worker_health, &published, failure, job.id);
                        }
                    }
                    if !from_archive && !failed_before {
                        let mut slot = published
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        match &outcome {
                            Ok(()) => {
                                slot.preparation_failure = None;
                                slot.failure_correlation = None;
                            }
                            Err(failure) => {
                                slot.preparation_failure = Some(*failure);
                                slot.failure_correlation = Some(job.id);
                            }
                        }
                    }
                    // Completion/release cannot be acknowledged before the
                    // exact outcome is durable. A failed archive retries the
                    // same receipt, never the physical lifecycle operation.
                    while !job.archived.load(Ordering::Acquire) {
                        let receipt = stored_receipt(&job, outcome);
                        if runtime
                            .block_on(async {
                                tokio::time::timeout(
                                    Duration::from_secs(5),
                                    core.store.archive_vault_lifecycle_receipt(receipt),
                                )
                                .await
                            })
                            .is_ok_and(|value| value.is_ok())
                        {
                            job.archived.store(true, Ordering::Release);
                            job.preparation_receipt.store(true, Ordering::Release);
                        } else {
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                    match job.result.lock() {
                        Ok(mut slot) => *slot = Some(outcome),
                        Err(poisoned) => {
                            *poisoned.into_inner() = Some(Err(AgentFailure::Interrupted));
                            fatal = Some(AgentFailure::Interrupted);
                            set_worker_failure(
                                &worker_health,
                                &published,
                                AgentFailure::Interrupted,
                                job.id,
                            );
                            if let Err(failure) =
                                retire(&runtime, &core, &published, &mut current, job.id)
                            {
                                shutdown_failure.get_or_insert(failure);
                                fatal = Some(failure);
                                set_worker_failure(&worker_health, &published, failure, job.id);
                            }
                        }
                    }
                }
                if let Err(failure) =
                    retire(&runtime, &core, &published, &mut current, Uuid::new_v4())
                {
                    shutdown_failure.get_or_insert(failure);
                }
                runtime.shutdown_timeout(Duration::from_secs(5));
                fatal.or(shutdown_failure).map_or(Ok(()), Err)
            })
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(Self {
            sender: Some(sender),
            jobs,
            closing,
            health,
            thread: Some(thread),
            failure: None,
        })
    }

    fn prepare(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<RuntimePreparationResult, RuntimePreparationCommandFailure> {
        let mut admitted = false;
        let result = (|| -> Result<RuntimePreparationResult, AgentFailure> {
            if self.closing.load(Ordering::Acquire) {
                return Err(AgentFailure::Interrupted);
            }
            let mut jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
            if let Some(prior) = jobs.get(&id) {
                if prior.caller != *caller || prior.intent != PreparationIntent::Prepare {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                if self.health_failure()?.is_some() {
                    // Readiness reports terminal queue health. Replay of a
                    // completed ID was checked above and remains available.
                    return Err(AgentFailure::Interrupted);
                }
                if jobs.len() >= MAX_RECEIPTS {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let pending = jobs.values().try_fold(0usize, |count, job| {
                    let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
                    Ok::<_, AgentFailure>(count + usize::from(result.is_none()))
                })?;
                if pending >= MAX_PENDING {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let job = Arc::new(Job {
                    caller: caller.clone(),
                    id,
                    intent: PreparationIntent::Prepare,
                    cancellation: Cancellation::new(),
                    result: Mutex::new(None),
                    archived: AtomicBool::new(false),
                    preparation_receipt: AtomicBool::new(false),
                });
                self.sender
                    .as_ref()
                    .ok_or(AgentFailure::Interrupted)?
                    .try_send(job.clone())
                    .map_err(|error| match error {
                        mpsc::TrySendError::Full(_) => AgentFailure::BudgetExceeded,
                        mpsc::TrySendError::Disconnected(_) => AgentFailure::Interrupted,
                    })?;
                jobs.insert(id, job);
            }
            admitted = true;
            let job = jobs.get(&id).ok_or(AgentFailure::NotFound)?;
            if job.caller != *caller {
                return Err(AgentFailure::PolicyDenied);
            }
            let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
            if result.is_none() && self.health_failure()?.is_some() {
                return Err(AgentFailure::Interrupted);
            }
            Ok(RuntimePreparationResult {
                operation_id: id,
                done: result.is_some(),
                failure: result
                    .as_ref()
                    .and_then(|value| value.as_ref().err())
                    .copied(),
            })
        })();
        result.map_err(|failure| {
            if !admitted {
                RuntimePreparationCommandFailure::NotAdmitted(failure)
            } else {
                RuntimePreparationCommandFailure::Indeterminate(failure)
            }
        })
    }

    fn cached_result(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<Option<RuntimePreparationResult>, AgentFailure> {
        let jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
        let Some(job) = jobs.get(&id) else {
            return Ok(None);
        };
        if job.caller != *caller {
            return Err(AgentFailure::PolicyDenied);
        }
        let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
        Ok(Some(RuntimePreparationResult {
            operation_id: id,
            done: result.is_some(),
            failure: result
                .as_ref()
                .and_then(|value| value.as_ref().err())
                .copied(),
        }))
    }

    fn acknowledge_cached(
        &self,
        caller: &CallerContext,
        id: Uuid,
    ) -> Result<Option<RuntimePreparationResult>, AgentFailure> {
        let mut jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
        let Some(job) = jobs.get(&id) else {
            return Ok(None);
        };
        if job.caller != *caller {
            return Err(AgentFailure::PolicyDenied);
        }
        let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
        if result.is_none()
            || !job.archived.load(Ordering::Acquire)
            || !job.preparation_receipt.load(Ordering::Acquire)
        {
            return Err(AgentFailure::Conflict);
        }
        let completed = RuntimePreparationResult {
            operation_id: id,
            done: true,
            failure: result
                .as_ref()
                .and_then(|value| value.as_ref().err())
                .copied(),
        };
        drop(result);
        jobs.remove(&id);
        Ok(Some(completed))
    }

    fn health_failure(&self) -> Result<Option<AgentFailure>, AgentFailure> {
        let finished = self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.is_finished());
        let mut health = self.health.lock().map_err(|_| AgentFailure::Interrupted)?;
        if finished && health.is_none() {
            *health = Some(AgentFailure::Interrupted);
        }
        Ok(*health)
    }

    fn shutdown(&mut self) -> Result<(), AgentFailure> {
        self.closing.store(true, Ordering::Release);
        let jobs = match self.jobs.lock() {
            Ok(jobs) => jobs,
            Err(poisoned) => {
                self.failure.get_or_insert(AgentFailure::Interrupted);
                poisoned.into_inner()
            }
        };
        for job in jobs.values() {
            job.cancellation
                .cancel_with_reason(CancelReason::OwnerDropped);
        }
        drop(jobs);
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(failure)) => {
                    self.failure.get_or_insert(failure);
                }
                Err(_) => {
                    self.failure.get_or_insert(AgentFailure::Interrupted);
                }
            }
        }
        self.failure.map_or(Ok(()), Err)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

async fn execute(
    root: &Path,
    core: &Arc<FloeCore>,
    local_context: &Arc<LocalContextHost>,
    published: &Mutex<Published>,
    model_provider_factory: &Arc<dyn ModelProviderFactory>,
    current: &mut Option<OpenGeneration>,
    job: &Job,
) -> Result<(), AgentFailure> {
    if job.cancellation.is_cancelled() {
        return Err(AgentFailure::Interrupted);
    }
    if current
        .as_ref()
        .is_some_and(|(caller, _)| *caller != job.caller)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    match job.intent {
        PreparationIntent::Prepare => {
            if let Some((_, generation)) = current.as_ref() {
                if generation.owners().check(&job.caller.owner_actor()).is_ok()
                    && generation.check_access().is_ok()
                {
                    return Ok(());
                }
                let fence = close_published(published, core, false);
                let (_, generation) = current.take().ok_or(AgentFailure::Conflict)?;
                let drain = tokio::time::timeout(DRAIN_TIMEOUT, generation.shutdown(job.id))
                    .await
                    .map_err(|_| AgentFailure::DeadlineExceeded)?;
                fence.and(drain)?;
                drop(generation);
            }
            let person = PersonId(job.caller.person_id());
            let presence = floe_vault::inspect_vault_presence(root, person)?;
            let vault = match presence {
                floe_vault::VaultPresence::Missing => {
                    match fs::DirBuilder::new().mode(0o700).create(root) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(_) => return Err(AgentFailure::VaultUnavailable),
                    }
                    EncryptedAgentVault::create(root, person, vault_keys(root)?).await?
                }
                floe_vault::VaultPresence::Existing => {
                    EncryptedAgentVault::open(root, person, vault_keys(root)?).await?
                }
            };
            let generation = Arc::new(
                ReadyGeneration::activate(
                    vault,
                    core.clone(),
                    local_context.clone(),
                    model_provider_factory.clone(),
                    job.caller.owner_actor(),
                    job.id,
                    job.cancellation.clone(),
                )
                .await?,
            );
            *current = Some((job.caller.clone(), generation.clone()));
            generation.check_access()?;
            let mut slot = published.lock().map_err(|_| AgentFailure::Interrupted)?;
            if slot.closing || job.cancellation.is_cancelled() {
                return Err(AgentFailure::Interrupted);
            }
            slot.current = current.clone();
            Ok(())
        }
    }
}

fn close_published(
    published: &Mutex<Published>,
    core: &FloeCore,
    permanent: bool,
) -> Result<(), AgentFailure> {
    let (mut slot, failure) = match published.lock() {
        Ok(slot) => (slot, None),
        Err(poisoned) => (poisoned.into_inner(), Some(AgentFailure::Interrupted)),
    };
    slot.closing |= permanent;
    let open = slot.current.take();
    drop(slot);
    let gateway = catch_unwind(AssertUnwindSafe(|| {
        if permanent {
            core.product_gateway.close()
        } else {
            core.product_gateway.lock()
        }
    }))
    .unwrap_or(Err(AgentFailure::Interrupted));
    let owners = match open {
        Some((_, open)) => open.owners().close_admission(),
        None => Ok(()),
    };
    failure.map_or(Ok(()), Err).and(gateway).and(owners)
}

fn retire(
    runtime: &tokio::runtime::Runtime,
    core: &FloeCore,
    published: &Mutex<Published>,
    current: &mut Option<OpenGeneration>,
    operation_id: Uuid,
) -> Result<(), AgentFailure> {
    let fence = catch_unwind(AssertUnwindSafe(|| close_published(published, core, false)))
        .unwrap_or(Err(AgentFailure::Interrupted));
    // Even a close-admission panic cannot skip retiring the concrete opened
    // generation. Its drop guard seals the Vault if bounded drain fails.
    let drain = match current.take() {
        Some((_, open)) => catch_unwind(AssertUnwindSafe(|| {
            runtime.block_on(async {
                tokio::time::timeout(DRAIN_TIMEOUT, open.shutdown(operation_id))
                    .await
                    .map_err(|_| AgentFailure::DeadlineExceeded)?
            })
        }))
        .unwrap_or(Err(AgentFailure::Interrupted)),
        None => Ok(()),
    };
    fence.and(drain)
}

fn stored_vault_state(root: &Path, person: PersonId) -> Result<(), AgentFailure> {
    floe_vault::inspect_vault_presence(root, person).map(|_| ())
}

fn stored_receipt(
    job: &Job,
    outcome: Result<(), AgentFailure>,
) -> floe_vault::StoredVaultLifecycleReceipt {
    floe_vault::StoredVaultLifecycleReceipt {
        operation_id: job.id,
        person_id: PersonId(job.caller.person_id()),
        device_id: job.caller.device_id().into(),
        runtime_epoch: job.caller.runtime_epoch(),
        intent: job.intent.stage().into(),
        state: outcome.is_ok().then(|| "completed".into()),
        failure: outcome.err(),
    }
}
fn receipt_outcome(
    job: &Job,
    receipt: floe_vault::StoredVaultLifecycleReceipt,
) -> Result<(), AgentFailure> {
    if !receipt_is_preparation(job, &receipt) {
        if receipt.intent == job.intent.stage()
            && (receipt.person_id.0 != job.caller.person_id()
                || receipt.device_id != job.caller.device_id()
                || receipt.runtime_epoch != job.caller.runtime_epoch())
        {
            return Err(AgentFailure::PolicyDenied);
        }
        return Err(AgentFailure::Conflict);
    }
    if let Some(failure) = receipt.failure {
        return Err(failure);
    }
    match receipt.state.as_deref() {
        Some("completed") => Ok(()),
        _ => Err(AgentFailure::StorageUnavailable),
    }
}

fn receipt_is_preparation(job: &Job, receipt: &floe_vault::StoredVaultLifecycleReceipt) -> bool {
    receipt.operation_id == job.id
        && receipt.person_id.0 == job.caller.person_id()
        && receipt.device_id == job.caller.device_id()
        && receipt.runtime_epoch == job.caller.runtime_epoch()
        && receipt.intent == job.intent.stage()
}

fn preparation_receipt_result(
    caller: &CallerContext,
    id: Uuid,
    receipt: floe_vault::StoredVaultLifecycleReceipt,
) -> Result<RuntimePreparationResult, AgentFailure> {
    if receipt.operation_id != id {
        return Err(AgentFailure::Conflict);
    }
    if receipt.person_id.0 != caller.person_id()
        || receipt.device_id != caller.device_id()
        || receipt.runtime_epoch != caller.runtime_epoch()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if receipt.intent != PreparationIntent::Prepare.stage() {
        return Err(AgentFailure::NotFound);
    }
    let failure = receipt.failure;
    if failure.is_none() && receipt.state.as_deref() != Some("completed") {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(RuntimePreparationResult {
        operation_id: id,
        done: true,
        failure,
    })
}

fn publish_worker_failure(
    published: &Mutex<Published>,
    failure: AgentFailure,
    correlation_id: Uuid,
) {
    let mut slot = published
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.worker_failure.is_none() {
        slot.worker_failure = Some(failure);
        slot.failure_correlation = Some(correlation_id);
    }
}

fn set_worker_failure(
    health: &Mutex<Option<AgentFailure>>,
    published: &Mutex<Published>,
    failure: AgentFailure,
    correlation_id: Uuid,
) {
    let mut health = health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if health.is_none() {
        *health = Some(failure);
        publish_worker_failure(published, failure, correlation_id);
    }
}
