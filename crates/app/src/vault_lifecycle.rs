//! The bounded host queue for encrypted Vault lifecycle only. Domain work runs
//! through ready owner handles on the retained generation runtime.

#[cfg(target_os = "android")]
use crate::android_vault_keys::AndroidVaultKeys as PlatformVaultKeys;
#[cfg(not(target_os = "android"))]
use floe_vault::KeyringVaultKeys as PlatformVaultKeys;
use crate::{CallerContext, FloeCore, VaultLifecycleResult, VaultState};
use crate::local_context::LocalContextHost;
use crate::owner_handles::ReadyOwners;
use crate::ready_generation::ReadyGeneration;
use floe_execution::{CancelReason, Cancellation};
use floe_kernel::{AgentFailure, PersonId};
use floe_vault::EncryptedAgentVault;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    os::unix::fs::DirBuilderExt,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc, atomic::{AtomicBool, Ordering}},
    time::Duration,
};
use uuid::Uuid;

const MAX_RECEIPTS: usize = 64;
const MAX_PENDING: usize = 8;
const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(60);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VaultLifecycleIntent { Create, Unlock, Lock }
impl VaultLifecycleIntent {
    fn stage(self) -> &'static str {
        match self { Self::Create => "create",
            Self::Unlock => "unlock", Self::Lock => "lock" }
    }
}

type Generation = ReadyGeneration<PlatformVaultKeys>;
type OpenGeneration = (CallerContext, Arc<Generation>);
#[derive(Default)]
struct Published { closing: bool, failure: Option<AgentFailure>, current: Option<OpenGeneration> }
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
    pub(crate) fn new(database_path: &str, core: Arc<FloeCore>, local_context: Arc<LocalContextHost>) -> Self {
        Self { root: PathBuf::from(format!("{database_path}.agent-vaults")), core, local_context,
            state: Mutex::new(BridgeState::default()), drained: Condvar::new(),
            published: Arc::new(Mutex::new(Published::default())) }
    }

    pub(crate) fn ready(&self, caller: &CallerContext) -> Result<Arc<ReadyOwners>, AgentFailure> {
        caller.owner_actor().validate()?;
        let slot = self.published.lock().map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing { return Err(AgentFailure::Interrupted); }
        if let Some(failure) = slot.failure { return Err(failure); }
        let Some((admitted, generation)) = slot.current.as_ref() else {
            return Err(match stored_vault_state(&self.root, PersonId(caller.person_id()))? {
                VaultState::Locked => AgentFailure::VaultLocked,
                VaultState::Missing => AgentFailure::NotFound,
                _ => AgentFailure::VaultUnavailable,
            });
        };
        if admitted != caller { return Err(AgentFailure::PolicyDenied); }
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
        let slot = self.published.lock().map_err(|_| AgentFailure::Interrupted)?;
        if slot.closing { return Err(AgentFailure::Interrupted); }
        if let Some(failure) = slot.failure { return Err(failure); }
        match slot.current.as_ref() {
            Some((admitted, generation)) => {
                if admitted != caller { return Err(AgentFailure::PolicyDenied); }
                let owners = generation.owners();
                owners.check(&caller.owner_actor())?;
                if let Err(failure) = generation.check_access() {
                    let gateway = self.core.product_gateway.lock();
                    let close = owners.close_admission();
                    gateway.and(close)?;
                    return Err(failure);
                }
                Ok(VaultState::Ready)
            }
            None => stored_vault_state(&self.root, PersonId(caller.person_id())),
        }
    }

    pub(crate) fn request(&self, caller: &CallerContext, id: Uuid,
        intent: Option<VaultLifecycleIntent>, release: bool) -> Result<VaultLifecycleResult, AgentFailure>
    {
        caller.owner_actor().validate()?;
        if id.is_nil() { return Err(AgentFailure::InvalidInput); }
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.closing { return Err(AgentFailure::Interrupted); }
        if state.worker.is_none() {
            state.worker = Some(Worker::new(self.root.clone(), self.core.clone(),
                self.local_context.clone(), self.published.clone())?);
        }
        state.worker.as_ref().ok_or(AgentFailure::Interrupted)?.request(caller, id, intent, release)
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
                    Err(poisoned) => { failure.get_or_insert(AgentFailure::Interrupted); poisoned.into_inner() },
                };
            }
            return state.failure.or(failure).map_or(Ok(()), Err);
        }
        state.closing = true;
        let mut worker = state.worker.take();
        drop(state);
        let fence = catch_unwind(AssertUnwindSafe(|| close_published(&self.published, &self.core, true)))
            .unwrap_or(Err(AgentFailure::Interrupted));
        if let Err(error) = fence { failure.get_or_insert(error); }
        if let Some(worker) = worker.as_mut() {
            if let Err(error) = worker.shutdown() { failure.get_or_insert(error); }
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => { failure.get_or_insert(AgentFailure::Interrupted); poisoned.into_inner() },
        };
        state.failure = failure;
        state.finished = true;
        self.drained.notify_all();
        failure.map_or(Ok(()), Err)
    }
}
impl Drop for VaultBridge { fn drop(&mut self) { let _ = self.shutdown(); } }

struct Job {
    caller: CallerContext,
    id: Uuid,
    intent: VaultLifecycleIntent,
    cancellation: Cancellation,
    result: Mutex<Option<Result<VaultState, AgentFailure>>>,
}
struct Worker {
    sender: Option<mpsc::SyncSender<Arc<Job>>>,
    jobs: Mutex<HashMap<Uuid, Arc<Job>>>,
    closing: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(), AgentFailure>>>,
    failure: Option<AgentFailure>,
}
impl Worker {
    fn new(root: PathBuf, core: Arc<FloeCore>, local_context: Arc<LocalContextHost>,
        published: Arc<Mutex<Published>>) -> Result<Self, AgentFailure>
    {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2)
            .enable_all().build().map_err(|_| AgentFailure::VaultUnavailable)?;
        let (sender, receiver) = mpsc::sync_channel::<Arc<Job>>(MAX_PENDING);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        let thread = std::thread::Builder::new().name("floe-vault-lifecycle".into())
            .stack_size(8 * 1024 * 1024).spawn(move || {
                let mut current = None;
                let mut fatal = None;
                let mut shutdown_failure = None;
                // This receiver blocks only the dedicated queue thread; the
                // runtime workers remain available for admitted owner work.
                while let Ok(job) = receiver.recv() {
                    let outcome = if worker_closing.load(Ordering::Acquire) {
                        Err(AgentFailure::Interrupted)
                    } else if let Some(failure) = fatal { Err(failure) } else {
                        match catch_unwind(AssertUnwindSafe(|| runtime.block_on(async {
                            tokio::time::timeout(LIFECYCLE_TIMEOUT,
                                execute(&root, &core, &local_context, &published, &mut current, &job))
                                .await.map_err(|_| AgentFailure::DeadlineExceeded)?
                        }))) {
                            Ok(result) => result,
                            Err(_) => { fatal = Some(AgentFailure::Interrupted); Err(AgentFailure::Interrupted) },
                        }
                    };
                    if let Some(failure) = fatal {
                        let mut slot = published.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        slot.failure = Some(failure);
                    }
                    if matches!(outcome, Err(AgentFailure::VaultUnavailable | AgentFailure::VaultLocked
                        | AgentFailure::Interrupted | AgentFailure::DeadlineExceeded))
                    {
                        if let Err(failure) = retire(&runtime, &core, &published, &mut current, job.id) {
                            shutdown_failure.get_or_insert(failure);
                        }
                    }
                    match job.result.lock() {
                        Ok(mut slot) => *slot = Some(outcome),
                        Err(poisoned) => {
                            *poisoned.into_inner() = Some(Err(AgentFailure::Interrupted));
                            fatal = Some(AgentFailure::Interrupted);
                            published.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
                                .failure = fatal;
                            if let Err(failure) = retire(&runtime, &core, &published, &mut current, job.id) {
                                shutdown_failure.get_or_insert(failure);
                            }
                        }
                    }
                }
                if let Err(failure) = retire(&runtime, &core, &published, &mut current, Uuid::new_v4()) {
                    shutdown_failure.get_or_insert(failure);
                }
                runtime.shutdown_timeout(Duration::from_secs(5));
                fatal.or(shutdown_failure).map_or(Ok(()), Err)
            }).map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(Self { sender: Some(sender), jobs: Mutex::new(HashMap::new()), closing,
            thread: Some(thread), failure: None })
    }

    fn request(&self, caller: &CallerContext, id: Uuid, intent: Option<VaultLifecycleIntent>,
        release: bool) -> Result<VaultLifecycleResult, AgentFailure>
    {
        if self.closing.load(Ordering::Acquire) { return Err(AgentFailure::Interrupted); }
        let mut jobs = self.jobs.lock().map_err(|_| AgentFailure::Interrupted)?;
        if let Some(intent) = intent {
            if let Some(prior) = jobs.get(&id) {
                if prior.caller != *caller || prior.intent != intent { return Err(AgentFailure::Conflict); }
            } else {
                if jobs.len() >= MAX_RECEIPTS { return Err(AgentFailure::BudgetExceeded); }
                let pending = jobs.values().try_fold(0usize, |count, job| {
                    let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
                    Ok::<_, AgentFailure>(count + usize::from(result.is_none()))
                })?;
                if pending >= MAX_PENDING { return Err(AgentFailure::BudgetExceeded); }
                let job = Arc::new(Job { caller: caller.clone(), id, intent,
                    cancellation: Cancellation::new(), result: Mutex::new(None) });
                self.sender.as_ref().ok_or(AgentFailure::Interrupted)?.try_send(job.clone())
                    .map_err(|error| match error {
                        mpsc::TrySendError::Full(_) => AgentFailure::BudgetExceeded,
                        mpsc::TrySendError::Disconnected(_) => AgentFailure::Interrupted,
                    })?;
                jobs.insert(id, job);
            }
        }
        let job = jobs.get(&id).ok_or(AgentFailure::NotFound)?;
        if job.caller != *caller { return Err(AgentFailure::NotFound); }
        let result = job.result.lock().map_err(|_| AgentFailure::Interrupted)?;
        if release && result.is_none() { return Err(AgentFailure::Conflict); }
        // Release ends observation only. The immutable admission and outcome
        // remain among the bounded receipts; the same Create ID is never fresh.
        Ok(VaultLifecycleResult { operation_id: id, stage: job.intent.stage().into(),
            done: result.is_some(), state: result.as_ref().and_then(|value| value.as_ref().ok()).copied(),
            failure: result.as_ref().and_then(|value| value.as_ref().err()).copied() })
    }

    fn shutdown(&mut self) -> Result<(), AgentFailure> {
        self.closing.store(true, Ordering::Release);
        let jobs = match self.jobs.lock() {
            Ok(jobs) => jobs,
            Err(poisoned) => { self.failure.get_or_insert(AgentFailure::Interrupted); poisoned.into_inner() },
        };
        for job in jobs.values() { job.cancellation.cancel_with_reason(CancelReason::OwnerDropped); }
        drop(jobs);
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok(())) => {},
                Ok(Err(failure)) => { self.failure.get_or_insert(failure); },
                Err(_) => { self.failure.get_or_insert(AgentFailure::Interrupted); },
            }
        }
        self.failure.map_or(Ok(()), Err)
    }
}
impl Drop for Worker { fn drop(&mut self) { let _ = self.shutdown(); } }

async fn execute(root: &Path, core: &Arc<FloeCore>, local_context: &Arc<LocalContextHost>,
    published: &Mutex<Published>, current: &mut Option<OpenGeneration>, job: &Job)
    -> Result<VaultState, AgentFailure>
{
    if job.cancellation.is_cancelled() { return Err(AgentFailure::Interrupted); }
    if current.as_ref().is_some_and(|(caller, _)| *caller != job.caller) {
        return Err(AgentFailure::PolicyDenied);
    }
    match job.intent {
        VaultLifecycleIntent::Create | VaultLifecycleIntent::Unlock => {
            if current.is_some() { return Err(AgentFailure::Conflict); }
            let person = PersonId(job.caller.person_id());
            let vault = if job.intent == VaultLifecycleIntent::Create {
                match fs::DirBuilder::new().mode(0o700).create(root) {
                    Ok(()) => {},
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
                    Err(_) => return Err(AgentFailure::VaultUnavailable),
                }
                EncryptedAgentVault::create(root, person, PlatformVaultKeys).await?
            } else { EncryptedAgentVault::open(root, person, PlatformVaultKeys).await? };
            let generation = Arc::new(ReadyGeneration::activate(vault, core.clone(), local_context.clone(),
                job.caller.owner_actor(), job.id, job.cancellation.clone()).await?);
            *current = Some((job.caller.clone(), generation.clone()));
            generation.check_access()?;
            let mut slot = published.lock().map_err(|_| AgentFailure::Interrupted)?;
            if slot.closing || job.cancellation.is_cancelled() { return Err(AgentFailure::Interrupted); }
            slot.current = current.clone();
            Ok(VaultState::Ready)
        }
        VaultLifecycleIntent::Lock => {
            let fence = close_published(published, core, false);
            let drain = match current.take() {
                Some((_, open)) => tokio::time::timeout(DRAIN_TIMEOUT, open.shutdown(job.id))
                    .await.map_err(|_| AgentFailure::DeadlineExceeded)?,
                None => Ok(()),
            };
            fence.and(drain)?;
            Ok(VaultState::Locked)
        }
    }
}

fn close_published(published: &Mutex<Published>, core: &FloeCore, permanent: bool) -> Result<(), AgentFailure> {
    let (mut slot, failure) = match published.lock() {
        Ok(slot) => (slot, None),
        Err(poisoned) => (poisoned.into_inner(), Some(AgentFailure::Interrupted)),
    };
    slot.closing |= permanent;
    let open = slot.current.take();
    drop(slot);
    let gateway = catch_unwind(AssertUnwindSafe(|| {
        if permanent { core.product_gateway.close() } else { core.product_gateway.lock() }
    })).unwrap_or(Err(AgentFailure::Interrupted));
    let owners = match open {
        Some((_, open)) => open.owners().close_admission(),
        None => Ok(()),
    };
    failure.map_or(Ok(()), Err).and(gateway).and(owners)
}

fn retire(runtime: &tokio::runtime::Runtime, core: &FloeCore, published: &Mutex<Published>,
    current: &mut Option<OpenGeneration>, operation_id: Uuid) -> Result<(), AgentFailure>
{
    let fence = catch_unwind(AssertUnwindSafe(|| close_published(published, core, false)))
        .unwrap_or(Err(AgentFailure::Interrupted));
    // Even a close-admission panic cannot skip retiring the concrete opened
    // generation. Its drop guard seals the Vault if bounded drain fails.
    let drain = match current.take() {
        Some((_, open)) => catch_unwind(AssertUnwindSafe(|| runtime.block_on(async {
            tokio::time::timeout(DRAIN_TIMEOUT, open.shutdown(operation_id))
                .await.map_err(|_| AgentFailure::DeadlineExceeded)?
        }))).unwrap_or(Err(AgentFailure::Interrupted)),
        None => Ok(()),
    };
    fence.and(drain)
}

fn stored_vault_state(root: &Path, person: PersonId) -> Result<VaultState, AgentFailure> {
    for directory in [root.to_path_buf(), root.join(person.to_string())] {
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() => {},
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(VaultState::Missing),
            _ => return Err(AgentFailure::VaultUnavailable),
        }
    }
    let directory = root.join(person.to_string());
    let marker = directory.join("vault.id");
    let metadata = fs::symlink_metadata(&marker).map_err(|_| AgentFailure::VaultUnavailable)?;
    if !metadata.is_file() || metadata.len() != 36 { return Err(AgentFailure::VaultUnavailable); }
    let mut value = String::new();
    fs::File::open(marker).map_err(|_| AgentFailure::VaultUnavailable)?.take(37)
        .read_to_string(&mut value).map_err(|_| AgentFailure::VaultUnavailable)?;
    let id = Uuid::parse_str(&value).map_err(|_| AgentFailure::VaultUnavailable)?;
    if value.len() != 36 || id.is_nil() { return Err(AgentFailure::VaultUnavailable); }
    let database = fs::symlink_metadata(directory.join("sessions.db")).map_err(|_| AgentFailure::VaultUnavailable)?;
    if !database.is_file() || database.len() == 0 { return Err(AgentFailure::VaultUnavailable); }
    Ok(VaultState::Locked)
}
