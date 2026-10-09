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
    #[cfg(all(test, feature = "development-storage"))]
    archive_gate: Option<Arc<ArchiveGate>>,
    #[cfg(all(test, feature = "development-storage"))]
    generation_drain_gate: Option<Arc<GenerationDrainGate>>,
}

#[cfg(all(test, feature = "development-storage"))]
struct ArchiveGate {
    operation_id: Uuid,
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

#[cfg(all(test, feature = "development-storage"))]
struct GenerationDrainGate {
    operation_id: Uuid,
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

#[cfg(all(test, feature = "development-storage"))]
impl GenerationDrainGate {
    fn pause(&self, operation_id: Uuid) {
        if operation_id == self.operation_id {
            let _ = self.entered.send(());
            let _ = self.release.lock().expect("drain test gate lock").recv();
        }
    }
}

#[cfg(all(test, feature = "development-storage"))]
impl ArchiveGate {
    fn pause(&self, operation_id: Uuid) {
        if operation_id == self.operation_id {
            let _ = self.entered.send(());
            let _ = self.release.lock().expect("archive test gate lock").recv();
        }
    }
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
    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    expert_source_transport: Option<Arc<dyn floe_context::ExpertSourceTransport>>,
    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    calendar_operation_executor:
        Option<Arc<dyn floe_calendar_operations::CalendarOperationExecutor>>,
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
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))] expert_source_transport: Option<
            Arc<dyn floe_context::ExpertSourceTransport>,
        >,
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        calendar_operation_executor: Option<
            Arc<dyn floe_calendar_operations::CalendarOperationExecutor>,
        >,
    ) -> Self {
        Self {
            root: PathBuf::from(format!("{database_path}.agent-vaults")),
            expected,
            runtime_handle,
            core,
            local_context,
            model_provider_factory,
            #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
            expert_source_transport,
            #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
            calendar_operation_executor,
            state: Mutex::new(BridgeState::default()),
            drained: Condvar::new(),
            published: Arc::new(Mutex::new(Published::default())),
        }
    }

    #[cfg(all(test, feature = "development-storage"))]
    fn set_archive_gate(&self, gate: Arc<ArchiveGate>) {
        self.published
            .lock()
            .expect("published Runtime state lock")
            .archive_gate = Some(gate);
    }

    #[cfg(all(test, feature = "development-storage"))]
    fn set_generation_drain_gate(&self, gate: Arc<GenerationDrainGate>) {
        self.published
            .lock()
            .expect("published Runtime state lock")
            .generation_drain_gate = Some(gate);
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

    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    pub(crate) fn qa_conversation_journal(
        &self,
        caller: &CallerContext,
        run_id: floe_kernel::RunId,
    ) -> Result<Vec<floe_vault::VaultConversationJournalEntry>, AgentFailure> {
        let generation = self.qa_generation(caller)?;
        crate::owner_handles::execute_on_handle(&self.runtime_handle, async move {
            generation.qa_conversation_journal(run_id).await
        })
    }

    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    fn qa_generation(&self, caller: &CallerContext) -> Result<Arc<Generation>, AgentFailure> {
        validate_runtime_caller(&self.expected, caller)?;
        let slot = self
            .published
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing {
            return Err(AgentFailure::Interrupted);
        }
        let Some((admitted, generation)) = slot.current.as_ref() else {
            return Err(AgentFailure::VaultUnavailable);
        };
        if admitted != caller {
            return Err(AgentFailure::PolicyDenied);
        }
        generation.owners().check(&caller.owner_actor())?;
        generation.check_access()?;
        Ok(generation.clone())
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
                    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
                    self.expert_source_transport.clone(),
                    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
                    self.calendar_operation_executor.clone(),
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
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))] expert_source_transport: Option<
            Arc<dyn floe_context::ExpertSourceTransport>,
        >,
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        calendar_operation_executor: Option<
            Arc<dyn floe_calendar_operations::CalendarOperationExecutor>,
        >,
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
                                        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
                                        &expert_source_transport,
                                        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
                                        &calendar_operation_executor,
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
                    #[cfg(all(test, feature = "development-storage"))]
                    let archive_gate = published
                        .lock()
                        .ok()
                        .and_then(|slot| slot.archive_gate.clone());
                    #[cfg(all(test, feature = "development-storage"))]
                    if let Some(gate) = archive_gate {
                        gate.pause(job.id);
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
    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))] expert_source_transport: &Option<
        Arc<dyn floe_context::ExpertSourceTransport>,
    >,
    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))] calendar_operation_executor: &Option<
        Arc<dyn floe_calendar_operations::CalendarOperationExecutor>,
    >,
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
                #[cfg(all(test, feature = "development-storage"))]
                if let Some(gate) = published.lock().ok().and_then(|mut slot| {
                    slot.generation_drain_gate
                        .as_ref()
                        .is_some_and(|gate| gate.operation_id == job.id)
                        .then(|| slot.generation_drain_gate.take())
                        .flatten()
                }) {
                    gate.pause(job.id);
                }
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
                    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
                    expert_source_transport.clone(),
                    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
                    calendar_operation_executor.clone(),
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

#[cfg(all(test, feature = "development-storage"))]
mod tests {
    use super::*;
    use crate::{AppComposition, AppHost, AppOpenOptions, HostError, RuntimeReadinessState};
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::OpenOptionsExt,
        path::PathBuf,
        sync::{Arc, mpsc},
        time::Instant,
    };
    use tempfile::TempDir;

    const TEST_TIMEOUT: Duration = Duration::from_secs(30);

    struct TestGateRelease(Option<mpsc::Sender<()>>);

    impl TestGateRelease {
        fn release(&mut self) {
            if let Some(release) = self.0.take() {
                release.send(()).expect("resume gated worker");
            }
        }
    }

    impl Drop for TestGateRelease {
        fn drop(&mut self) {
            if let Some(release) = self.0.take() {
                let _ = release.send(());
            }
        }
    }

    fn open_profile(profile: &TempDir) -> AppHost<AppComposition> {
        crate::open_default_with_options(
            profile.path().to_str().expect("UTF-8 test path"),
            AppOpenOptions::default(),
        )
        .expect("open isolated Runtime profile")
    }

    fn completed_prepare(
        host: &AppHost<AppComposition>,
    ) -> (crate::CallerContext, Uuid, RuntimePreparationResult) {
        let request = host.request(Uuid::new_v4()).expect("admit request");
        let services = request.services();
        let caller = request.caller().clone();
        let operation_id = Uuid::new_v4();
        services
            .prepare_runtime(&caller, operation_id)
            .expect("admit Runtime prepare");
        let deadline = Instant::now() + TEST_TIMEOUT;
        let result = loop {
            let result = services
                .get_runtime_preparation(&caller, operation_id)
                .expect("read Runtime preparation");
            if result.done {
                break result;
            }
            assert!(
                Instant::now() < deadline,
                "Runtime prepare exceeded timeout"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        services
            .acknowledge_runtime_preparation(&caller, operation_id)
            .expect("archive-before-ACK");
        (caller, operation_id, result)
    }

    fn failed_prepare(
        host: &AppHost<AppComposition>,
        caller: &crate::CallerContext,
    ) -> RuntimePreparationResult {
        let request = host.request(Uuid::new_v4()).expect("admit request");
        let services = request.services();
        let operation_id = Uuid::new_v4();
        services
            .prepare_runtime(caller, operation_id)
            .expect("admit failed Runtime prepare");
        let deadline = Instant::now() + TEST_TIMEOUT;
        loop {
            let result = services
                .get_runtime_preparation(caller, operation_id)
                .expect("read failed Runtime preparation");
            if result.done {
                services
                    .acknowledge_runtime_preparation(caller, operation_id)
                    .expect("acknowledge archived failure");
                return result;
            }
            assert!(Instant::now() < deadline, "failed prepare exceeded timeout");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn vault_paths(
        bridge: &RuntimePreparationHost,
        caller: &crate::CallerContext,
    ) -> (PathBuf, PathBuf, PathBuf) {
        let directory = bridge.root.join(caller.person_id().to_string());
        let vault_id = fs::read_to_string(directory.join("vault.id")).expect("read vault id");
        let key = bridge
            .root
            .join(".development-keys")
            .join(format!("{}-{vault_id}.key", caller.person_id()));
        (directory.clone(), key, directory.join("sessions.db"))
    }

    #[test]
    fn injected_terminal_health_replays_completed_receipt_but_rejects_new_work() {
        let profile = TempDir::new().expect("isolated profile directory");
        let host = open_profile(&profile);
        let (caller, operation_id, completed) = completed_prepare(&host);
        assert_eq!(completed.failure, None);

        {
            let request = host.request(Uuid::new_v4()).expect("admit test request");
            let services = request.services();
            let state = services
                .runtime_preparation
                .state
                .lock()
                .expect("Runtime bridge state");
            let worker = state.worker.as_ref().expect("preparation worker");
            // This simulates an already-published terminal health flag. It does
            // not panic, poison, or crash the worker thread.
            *worker.health.lock().expect("worker health") = Some(AgentFailure::Interrupted);
        }

        let request = host.request(Uuid::new_v4()).expect("admit observation");
        let services = request.services();
        let readiness = services
            .runtime_readiness(&caller, Uuid::new_v4())
            .expect("observe terminal worker");
        assert_eq!(readiness.state, RuntimeReadinessState::Unavailable);
        let failure = readiness.failure.expect("terminal worker failure");
        assert!(failure.safe_actions.is_empty());
        assert_eq!(failure.recovery, crate::RuntimeRecovery::None);

        let replayed = services
            .prepare_runtime(&caller, operation_id)
            .expect("completed archived operation remains replayable");
        assert_eq!(replayed, completed);
        let new_operation = services.prepare_runtime(&caller, Uuid::new_v4());
        assert!(matches!(
            new_operation,
            Err(RuntimePreparationCommandFailure::NotAdmitted(
                AgentFailure::Interrupted
            ))
        ));
        drop(request);
        host.shutdown().expect("retire terminal worker");
    }

    #[test]
    fn missing_key_prepare_failure_preserves_the_existing_encrypted_data() {
        let profile = TempDir::new().expect("isolated profile directory");
        let host = open_profile(&profile);
        let (caller, _, result) = completed_prepare(&host);
        assert_eq!(result.failure, None);
        let (root, key, database) = {
            let request = host.request(Uuid::new_v4()).expect("admit inspection");
            let bridge = &request.services().runtime_preparation;
            let (directory, key, database) = vault_paths(bridge, &caller);
            (directory, key, database)
        };
        assert!(key.is_file(), "successful creation stored its key");
        host.shutdown().expect("close first App generation");
        let database_before = fs::read(&database).expect("read encrypted database");
        fs::remove_file(&key).expect("simulate missing custody key");

        let reopened = open_profile(&profile);
        let request = reopened
            .request(Uuid::new_v4())
            .expect("admit reopened request");
        let caller_after_open = request.caller().clone();
        assert_eq!(caller_after_open.person_id(), caller.person_id());
        assert_eq!(caller_after_open.device_id(), caller.device_id());
        assert_ne!(caller_after_open.runtime_epoch(), caller.runtime_epoch());
        let failure = failed_prepare(&reopened, &caller_after_open);
        assert_eq!(failure.failure, Some(AgentFailure::VaultUnavailable));
        assert!(!key.exists(), "failed open did not recreate a missing key");
        assert_eq!(
            fs::read(&database).expect("read preserved database"),
            database_before
        );
        assert!(root.join("vault.id").is_file(), "vault identity remains");
        drop(request);
        reopened.shutdown().expect("close failed preparation host");
    }

    #[test]
    fn incomplete_creation_prepare_failure_preserves_key_database_and_marker() {
        let profile = TempDir::new().expect("isolated profile directory");
        let host = open_profile(&profile);
        let (caller, _, result) = completed_prepare(&host);
        assert_eq!(result.failure, None);
        let (directory, key, database) = {
            let request = host.request(Uuid::new_v4()).expect("admit inspection");
            vault_paths(&request.services().runtime_preparation, &caller)
        };
        let vault_id = fs::read_to_string(directory.join("vault.id")).expect("read vault id");
        let person = caller.person_id();
        let pending = serde_json::json!({
            "marker_version": 1,
            "person_id": person,
            "vault_id": vault_id,
            "encrypted_layout_version": 3
        });
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("creation.pending"))
            .expect("write incomplete-creation evidence");
        marker
            .write_all(pending.to_string().as_bytes())
            .expect("persist marker fixture");
        marker.sync_all().expect("sync marker fixture");
        host.shutdown().expect("close first App generation");
        let key_before = fs::read(&key).expect("read existing key");
        let database_before = fs::read(&database).expect("read encrypted database");

        let reopened = open_profile(&profile);
        let request = reopened
            .request(Uuid::new_v4())
            .expect("admit reopened request");
        let caller_after_open = request.caller().clone();
        let failure = failed_prepare(&reopened, &caller_after_open);
        assert_eq!(failure.failure, Some(AgentFailure::IncompleteCreation));
        assert_eq!(fs::read(&key).expect("preserved key"), key_before);
        assert_eq!(
            fs::read(&database).expect("preserved database"),
            database_before
        );
        assert!(directory.join("creation.pending").is_file());
        drop(request);
        reopened.shutdown().expect("close incomplete-creation host");
    }

    #[test]
    fn shutdown_budget_timeout_retains_admitted_receipt_until_archive_and_cleanup_finish() {
        let profile = TempDir::new().expect("isolated profile directory");
        let host = open_profile(&profile);
        let operation_id = Uuid::new_v4();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let mut release = TestGateRelease(Some(release_tx));
        let gate = Arc::new(ArchiveGate {
            operation_id,
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let (caller, job, store) = {
            let request = host.request(Uuid::new_v4()).expect("admit request");
            let services = request.services();
            let bridge = &services.runtime_preparation;
            bridge.set_archive_gate(gate);
            let caller = request.caller().clone();
            let store = bridge.core.store.clone();
            services
                .prepare_runtime(&caller, operation_id)
                .expect("admit immutable operation");
            let state = bridge.state.lock().expect("Runtime bridge state");
            let worker = state.worker.as_ref().expect("preparation worker");
            let job = worker
                .jobs
                .lock()
                .expect("worker jobs")
                .get(&operation_id)
                .expect("admitted operation is retained")
                .clone();
            (caller, job, store)
        };
        entered_rx
            .recv_timeout(TEST_TIMEOUT)
            .expect("worker reached the archive boundary");
        assert!(!job.archived.load(Ordering::Acquire));
        assert!(job.result.lock().expect("job result").is_none());

        let timed_out = host.shutdown_with_budget_for_test(Duration::from_millis(100));
        assert_eq!(timed_out, Err(HostError::Shutdown));
        assert!(!host.retirement_complete_for_test());
        assert!(!job.archived.load(Ordering::Acquire));
        assert!(job.result.lock().expect("job result").is_none());

        release.release();
        let deadline = Instant::now() + TEST_TIMEOUT;
        loop {
            let archived = job.archived.load(Ordering::Acquire);
            let result = job.result.lock().expect("job result");
            if archived && result.is_some() {
                assert_eq!(*result, Some(Ok(())));
                break;
            }
            drop(result);
            assert!(Instant::now() < deadline, "archive did not resume");
            std::thread::sleep(Duration::from_millis(10));
        }
        let retirement_deadline = Instant::now() + TEST_TIMEOUT;
        while !host.retirement_complete_for_test() {
            assert!(
                Instant::now() < retirement_deadline,
                "cleanup did not finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        let verify_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("verification runtime");
        let receipt = verify_runtime
            .block_on(store.load_vault_lifecycle_receipt(operation_id))
            .expect("read durable operation receipt")
            .expect("admitted ID was archived");
        assert_eq!(receipt.operation_id, operation_id);
        assert_eq!(receipt.person_id.0, caller.person_id());
        assert_eq!(receipt.intent, "prepare");
        assert_eq!(receipt.failure, None);
        assert_eq!(receipt.state.as_deref(), Some("completed"));
    }

    #[test]
    fn sealed_published_generation_is_fenced_and_drained_before_same_host_reprepare() {
        let profile = TempDir::new().expect("isolated profile directory");
        let host = open_profile(&profile);
        let (caller, _, result) = completed_prepare(&host);
        assert_eq!(result.failure, None);
        let request = host
            .request(Uuid::new_v4())
            .expect("admit same-host Runtime request");
        assert_eq!(request.caller(), &caller, "caller epoch remains unchanged");
        let services = request.services();
        let bridge = &services.runtime_preparation;

        let (old_generation, old_owners, old_drain_complete) = {
            let slot = bridge.published.lock().expect("published Runtime state");
            let (admitted, generation) =
                slot.current.as_ref().expect("published current generation");
            assert_eq!(admitted, &caller);
            let owners = generation.owners();
            assert!(owners.check(&caller.owner_actor()).is_ok());
            let drain_complete = generation.shutdown_completion_probe_for_test();
            assert!(!drain_complete.load(Ordering::Acquire));

            // Only seal the published generation. Its owner admission and the
            // worker's current pointer remain untouched for execute() to fence.
            generation.seal_vault_for_test();
            assert!(generation.check_access().is_err());
            assert!(owners.check(&caller.owner_actor()).is_ok());
            assert!(
                slot.current
                    .as_ref()
                    .is_some_and(|(_, published)| { Arc::ptr_eq(published, generation) })
            );
            (Arc::downgrade(generation), owners, drain_complete)
        };

        let operation_id = Uuid::new_v4();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let mut release = TestGateRelease(Some(release_tx));
        bridge.set_generation_drain_gate(Arc::new(GenerationDrainGate {
            operation_id,
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }));

        services
            .prepare_runtime(&caller, operation_id)
            .expect("admit a new prepare ID in the same caller epoch");
        entered_rx
            .recv_timeout(TEST_TIMEOUT)
            .expect("execute fenced old admission before starting its drain");
        assert!(old_owners.check(&caller.owner_actor()).is_err());
        assert!(!old_drain_complete.load(Ordering::Acquire));
        assert!(
            bridge
                .published
                .lock()
                .expect("published Runtime state")
                .current
                .is_none(),
            "old generation is unpublished while its drain is paused"
        );
        // Owner handles retain the product-store lease, so release the test's
        // references before execute() drains and activates the replacement.
        drop(old_owners);
        release.release();

        let deadline = Instant::now() + TEST_TIMEOUT;
        let result = loop {
            let result = services
                .get_runtime_preparation(&caller, operation_id)
                .expect("read same-host prepare result");
            if result.done {
                break result;
            }
            assert!(
                Instant::now() < deadline,
                "same-host prepare exceeded timeout"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(result.failure, None);
        assert!(
            old_drain_complete.load(Ordering::Acquire),
            "execute awaited successful retirement of the old generation"
        );
        assert!(
            old_generation.upgrade().is_none(),
            "old generation was released after its drain"
        );
        services
            .acknowledge_runtime_preparation(&caller, operation_id)
            .expect("archive-before-ACK for replacement prepare");

        let readiness = services
            .runtime_readiness(&caller, Uuid::new_v4())
            .expect("observe replacement generation readiness");
        assert_eq!(readiness.state, RuntimeReadinessState::Ready);
        let new_owners = services
            .ready_owners(&caller)
            .expect("replacement generation is ready");
        assert!(new_owners.check(&caller.owner_actor()).is_ok());
        drop(request);
        host.shutdown().expect("close same App host");
    }
}
