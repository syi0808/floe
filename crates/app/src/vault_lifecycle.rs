//! The bounded host queue for encrypted Vault lifecycle only. Domain work runs
//! through ready owner handles on the retained generation runtime.

#[cfg(target_os = "android")]
use crate::android_vault_keys::AndroidVaultKeys as PlatformVaultKeys;
use crate::local_context::LocalContextHost;
use crate::owner_handles::ReadyOwners;
use crate::ready_generation::ReadyGeneration;
use crate::{
    CallerContext, FloeCore, VaultLifecycleCommandFailure, VaultLifecycleFailureProjection,
    VaultLifecycleRecovery, VaultLifecycleResult, VaultState,
};
use floe_execution::{CancelReason, Cancellation};
use floe_kernel::{
    AgentFailure, AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction,
    AgentRetryPolicy, PersonId,
};
use floe_vault::EncryptedAgentVault;
#[cfg(not(target_os = "android"))]
use floe_vault::KeyringVaultKeys as PlatformVaultKeys;
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
pub(crate) enum VaultLifecycleIntent {
    Create,
    Unlock,
    Lock,
}
impl VaultLifecycleIntent {
    fn stage(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Unlock => "unlock",
            Self::Lock => "lock",
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

pub(crate) fn project_failure(
    failure: AgentFailure,
    stage: &str,
) -> VaultLifecycleFailureProjection {
    // Lock closes admission before draining. Other failures seal the client only
    // when the lifecycle has retired the generation; recovery never replays work.
    let seal_session = stage == "lock"
        || requires_retirement(failure)
        || (stage == "unlock" && failure == AgentFailure::Conflict);
    let category = match failure {
        AgentFailure::VaultLocked | AgentFailure::NotFound | AgentFailure::ConsentRequired => {
            AgentFailureCategory::UserConfiguration
        }
        AgentFailure::IncompleteCreation
        | AgentFailure::Conflict
        | AgentFailure::StaleContext
        | AgentFailure::UnsupportedVersion => AgentFailureCategory::Integrity,
        AgentFailure::PolicyDenied | AgentFailure::CapabilityDenied => {
            AgentFailureCategory::Security
        }
        AgentFailure::VaultUnavailable
        | AgentFailure::StorageUnavailable
        | AgentFailure::Interrupted
        | AgentFailure::DeadlineExceeded
        | AgentFailure::Cancelled
        | AgentFailure::BudgetExceeded => AgentFailureCategory::Transient,
        _ => AgentFailureCategory::Internal,
    };
    VaultLifecycleFailureProjection {
        failure,
        domain: AgentFailureDomain::Vault,
        category,
        safe_actions: if seal_session {
            vec![AgentFailureSafeAction::ReopenVault]
        } else {
            vec![]
        },
        retry_policy: AgentRetryPolicy::Never,
        retryable: false,
        recovery: if seal_session {
            VaultLifecycleRecovery::ReopenVault
        } else {
            VaultLifecycleRecovery::None
        },
        reload_required: seal_session,
        seal_session,
    }
}

type Generation = ReadyGeneration<PlatformVaultKeys>;
type OpenGeneration = (CallerContext, Arc<Generation>);
#[derive(Default)]
struct Published {
    closing: bool,
    failure: Option<AgentFailure>,
    current: Option<OpenGeneration>,
}
#[derive(Default)]
struct BridgeState {
    closing: bool,
    finished: bool,
    failure: Option<AgentFailure>,
    worker: Option<Worker>,
}
pub(crate) struct VaultBridge {
    root: PathBuf,
    core: Arc<FloeCore>,
    local_context: Arc<LocalContextHost>,
    state: Mutex<BridgeState>,
    drained: Condvar,
    published: Arc<Mutex<Published>>,
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
            state: Mutex::new(BridgeState::default()),
            drained: Condvar::new(),
            published: Arc::new(Mutex::new(Published::default())),
        }
    }

    pub(crate) fn ready(&self, caller: &CallerContext) -> Result<Arc<ReadyOwners>, AgentFailure> {
        caller.owner_actor().validate()?;
        let slot = self
            .published
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing {
            return Err(AgentFailure::Interrupted);
        }
        if let Some(failure) = slot.failure {
            return Err(failure);
        }
        let Some((admitted, generation)) = slot.current.as_ref() else {
            return Err(
                match stored_vault_state(&self.root, PersonId(caller.person_id()))? {
                    VaultState::Locked => AgentFailure::VaultLocked,
                    VaultState::Missing => AgentFailure::NotFound,
                    _ => AgentFailure::VaultUnavailable,
                },
            );
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

    pub(crate) fn status(&self, caller: &CallerContext) -> Result<VaultState, AgentFailure> {
        caller.owner_actor().validate()?;
        let slot = self
            .published
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing {
            return Err(AgentFailure::Interrupted);
        }
        if let Some(failure) = slot.failure {
            return Err(failure);
        }
        match slot.current.as_ref() {
            Some((admitted, generation)) => {
                if admitted != caller {
                    return Err(AgentFailure::PolicyDenied);
                }
                let owners = generation.owners();
                if owners.check(&caller.owner_actor()).is_err()
                    || generation.check_access().is_err()
                {
                    let gateway = self.core.product_gateway.lock();
                    let close = owners.close_admission();
                    gateway.and(close)?;
                    // No usable published generation remains. Unlock owns
                    // retirement/drain on the lifecycle queue before reopening.
                    return Ok(VaultState::Locked);
                }
                Ok(VaultState::Ready)
            }
            None => stored_vault_state(&self.root, PersonId(caller.person_id())),
        }
    }

    pub(crate) fn request(
        &self,
        caller: &CallerContext,
        id: Uuid,
        intent: Option<VaultLifecycleIntent>,
        release: bool,
    ) -> Result<VaultLifecycleResult, VaultLifecycleCommandFailure> {
        let state = (|| -> Result<_, AgentFailure> {
            caller.owner_actor().validate()?;
            if id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
            if state.closing {
                return Err(AgentFailure::Interrupted);
            }
            if state.worker.is_none() {
                state.worker = Some(Worker::new(
                    self.root.clone(),
                    self.core.clone(),
                    self.local_context.clone(),
                    self.published.clone(),
                )?);
            }
            Ok(state)
        })()
        .map_err(VaultLifecycleCommandFailure::NotAdmitted)?;
        state
            .worker
            .as_ref()
            .ok_or(VaultLifecycleCommandFailure::NotAdmitted(
                AgentFailure::Interrupted,
            ))?
            .request(caller, id, intent, release)
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
impl Drop for VaultBridge {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

struct Job {
    caller: CallerContext,
    id: Uuid,
    intent: VaultLifecycleIntent,
    cancellation: Cancellation,
    result: Mutex<Option<Result<VaultState, AgentFailure>>>,
    archived: AtomicBool,
}
struct Worker {
    sender: Option<mpsc::SyncSender<Arc<Job>>>,
    jobs: Arc<Mutex<HashMap<Uuid, Arc<Job>>>>,
    closing: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(), AgentFailure>>>,
    failure: Option<AgentFailure>,
}
impl Worker {
    fn new(
        root: PathBuf,
        core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>,
        published: Arc<Mutex<Published>>,
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
        let jobs = Arc::new(Mutex::new(HashMap::<Uuid, Arc<Job>>::new()));
        let thread = std::thread::Builder::new()
            .name("floe-vault-lifecycle".into())
            .stack_size(crate::owner_handles::EXECUTOR_STACK_BYTES)
            .spawn(move || {
                let mut current = None;
                let mut fatal = None;
                let mut shutdown_failure = None;
                // This receiver blocks only the dedicated queue thread; the
                // runtime workers remain available for admitted owner work.
                'work: while let Ok(job) = receiver.recv() {
                    // Lookup failure is not an execution outcome. Retain the
                    // queued identity and retry; never retire a live generation
                    // or overwrite an existing receipt because its read failed.
                    let saved = loop {
                        if worker_closing.load(Ordering::Acquire) {
                            break 'work;
                        }
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
                    let outcome = if worker_closing.load(Ordering::Acquire) {
                        Err(AgentFailure::Interrupted)
                    } else if let Some(failure) = fatal {
                        Err(failure)
                    } else {
                        match catch_unwind(AssertUnwindSafe(|| {
                            runtime.block_on(async {
                                if let Some(receipt) = saved {
                                    job.archived.store(true, Ordering::Release);
                                    return receipt_outcome(&job, receipt);
                                }
                                tokio::time::timeout(
                                    LIFECYCLE_TIMEOUT,
                                    execute(
                                        &root,
                                        &core,
                                        &local_context,
                                        &published,
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
                                Err(AgentFailure::Interrupted)
                            }
                        }
                    };
                    if let Some(failure) = fatal {
                        let mut slot = published
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        slot.failure = Some(failure);
                    }
                    if !job.archived.load(Ordering::Acquire)
                        && outcome
                            .as_ref()
                            .err()
                            .is_some_and(|failure| requires_retirement(*failure))
                    {
                        if let Err(failure) =
                            retire(&runtime, &core, &published, &mut current, job.id)
                        {
                            shutdown_failure.get_or_insert(failure);
                        }
                    }
                    // Completion/release cannot be acknowledged before the
                    // exact outcome is durable. A failed archive retries the
                    // same receipt, never the physical lifecycle operation.
                    while !job.archived.load(Ordering::Acquire) {
                        if worker_closing.load(Ordering::Acquire) {
                            break 'work;
                        }
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
                        } else {
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                    match job.result.lock() {
                        Ok(mut slot) => *slot = Some(outcome),
                        Err(poisoned) => {
                            *poisoned.into_inner() = Some(Err(AgentFailure::Interrupted));
                            fatal = Some(AgentFailure::Interrupted);
                            published
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .failure = fatal;
                            if let Err(failure) =
                                retire(&runtime, &core, &published, &mut current, job.id)
                            {
                                shutdown_failure.get_or_insert(failure);
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
            thread: Some(thread),
            failure: None,
        })
    }

    fn request(
        &self,
        caller: &CallerContext,
        id: Uuid,
        intent: Option<VaultLifecycleIntent>,
        release: bool,
    ) -> Result<VaultLifecycleResult, VaultLifecycleCommandFailure> {
        let mut admitted = false;
        let result = (|| -> Result<VaultLifecycleResult, AgentFailure> {
            if self.closing.load(Ordering::Acquire) {
                return Err(AgentFailure::Interrupted);
            }
            let mut jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
            if let Some(intent) = intent {
                if let Some(prior) = jobs.get(&id) {
                    if prior.caller != *caller || prior.intent != intent {
                        return Err(AgentFailure::Conflict);
                    }
                } else {
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
                        intent,
                        cancellation: Cancellation::new(),
                        result: Mutex::new(None),
                        archived: AtomicBool::new(false),
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
            }
            admitted = intent.is_some();
            let job = jobs.get(&id).ok_or(AgentFailure::NotFound)?;
            if job.caller != *caller {
                return Err(AgentFailure::NotFound);
            }
            let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
            if release && result.is_none() {
                return Err(AgentFailure::Conflict);
            }
            // All completed results are already durably archived. A replay
            // receipt (including a changed-intent Conflict) never replaces it.
            if release {
                if !job.archived.load(Ordering::Acquire) {
                    return Err(AgentFailure::Conflict);
                }
                let id = job.id;
                // Clone below avoids retaining a reference into the map while evicting.
                let completed = *result;
                let stage = job.intent.stage().to_string();
                drop(result);
                jobs.remove(&id);
                return Ok(VaultLifecycleResult {
                    operation_id: id,
                    stage,
                    done: true,
                    state: completed.and_then(Result::ok),
                    failure: completed.and_then(Result::err),
                });
            }
            Ok(VaultLifecycleResult {
                operation_id: id,
                stage: job.intent.stage().into(),
                done: result.is_some(),
                state: result
                    .as_ref()
                    .and_then(|value| value.as_ref().ok())
                    .copied(),
                failure: result
                    .as_ref()
                    .and_then(|value| value.as_ref().err())
                    .copied(),
            })
        })();
        result.map_err(|failure| {
            if intent.is_some() && !admitted {
                VaultLifecycleCommandFailure::NotAdmitted(failure)
            } else {
                VaultLifecycleCommandFailure::Indeterminate(failure)
            }
        })
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
    current: &mut Option<OpenGeneration>,
    job: &Job,
) -> Result<VaultState, AgentFailure> {
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
        VaultLifecycleIntent::Create | VaultLifecycleIntent::Unlock => {
            if let Some((_, generation)) = current.as_ref() {
                if job.intent == VaultLifecycleIntent::Create {
                    return Err(AgentFailure::Conflict);
                }
                if generation.owners().check(&job.caller.owner_actor()).is_ok()
                    && generation.check_access().is_ok()
                {
                    return Ok(VaultState::Ready);
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
            let vault = if job.intent == VaultLifecycleIntent::Create {
                match fs::DirBuilder::new().mode(0o700).create(root) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(AgentFailure::VaultUnavailable),
                }
                EncryptedAgentVault::create(root, person, PlatformVaultKeys).await?
            } else {
                EncryptedAgentVault::open(root, person, PlatformVaultKeys).await?
            };
            let generation = Arc::new(
                ReadyGeneration::activate(
                    vault,
                    core.clone(),
                    local_context.clone(),
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
            Ok(VaultState::Ready)
        }
        VaultLifecycleIntent::Lock => {
            let fence = close_published(published, core, false);
            let drain = match current.take() {
                Some((_, open)) => tokio::time::timeout(DRAIN_TIMEOUT, open.shutdown(job.id))
                    .await
                    .map_err(|_| AgentFailure::DeadlineExceeded)?,
                None => Ok(()),
            };
            fence.and(drain)?;
            stored_vault_state(root, PersonId(job.caller.person_id()))
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

fn stored_vault_state(root: &Path, person: PersonId) -> Result<VaultState, AgentFailure> {
    match floe_vault::inspect_vault_presence(root, person)? {
        floe_vault::VaultPresence::Missing => Ok(VaultState::Missing),
        floe_vault::VaultPresence::Existing => Ok(VaultState::Locked),
    }
}

fn stored_receipt(
    job: &Job,
    outcome: Result<VaultState, AgentFailure>,
) -> floe_vault::StoredVaultLifecycleReceipt {
    floe_vault::StoredVaultLifecycleReceipt {
        operation_id: job.id,
        person_id: PersonId(job.caller.person_id()),
        device_id: job.caller.device_id().into(),
        runtime_epoch: job.caller.runtime_epoch(),
        intent: job.intent.stage().into(),
        state: outcome.ok().map(|state| {
            match state {
                VaultState::Missing => "missing",
                VaultState::Locked => "locked",
                VaultState::Ready => "ready",
                VaultState::Unavailable => "unavailable",
            }
            .into()
        }),
        failure: outcome.err(),
    }
}
fn receipt_outcome(
    job: &Job,
    receipt: floe_vault::StoredVaultLifecycleReceipt,
) -> Result<VaultState, AgentFailure> {
    if receipt.operation_id != job.id
        || receipt.person_id.0 != job.caller.person_id()
        || receipt.device_id != job.caller.device_id()
        || receipt.runtime_epoch != job.caller.runtime_epoch()
        || receipt.intent != job.intent.stage()
    {
        return Err(AgentFailure::Conflict);
    }
    if let Some(failure) = receipt.failure {
        return Err(failure);
    }
    match receipt.state.as_deref() {
        Some("missing") => Ok(VaultState::Missing),
        Some("locked") => Ok(VaultState::Locked),
        Some("ready") => Ok(VaultState::Ready),
        Some("unavailable") => Ok(VaultState::Unavailable),
        _ => Err(AgentFailure::StorageUnavailable),
    }
}
