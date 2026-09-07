use std::{
    cell::RefCell,
    fs,
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

use floe_agent::{AgentEvent, AgentFailure, AgentSession, Cancellation, SessionStore};
use floe_core::{
    AgentFixtureTurn, EncryptedAgentVault, KeyringVaultKeys, VaultKeyProvider,
    recover_agent_sample, run_agent_sample,
};
use floe_domain::PersonId;
use floe_protocol::*;
use uuid::Uuid;

use super::{BridgeResult, agent_failure, check_version, parse_id, parse_person};

pub(crate) struct VaultBridge {
    root: PathBuf,
    worker: RefCell<Option<Worker>>,
}

impl VaultBridge {
    pub(crate) fn new(database_path: &str) -> Self {
        Self {
            root: PathBuf::from(format!("{database_path}.agent-vaults")),
            worker: RefCell::new(None),
        }
    }

    pub(crate) fn request(
        &self,
        request: AgentVaultRequestDto,
    ) -> BridgeResult<AgentVaultResultDto> {
        check_version(request.schema_version)?;
        let person = parse_person(&request.person_id)?;
        let id = parse_id(&request.request_id, "request_id", |id| id)?;
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker =
                Some(Worker::new(self.root.clone(), KeyringVaultKeys).map_err(agent_failure)?);
        }
        worker
            .as_ref()
            .unwrap()
            .request(person, id, request.operation)
            .map_err(agent_failure)
    }
}

struct Worker {
    sender: mpsc::SyncSender<Arc<Job>>,
    active: Mutex<Option<Arc<Job>>>,
    closing: Arc<AtomicBool>,
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
    failure: Option<AgentFailure>,
}

impl Worker {
    fn new<Keys: VaultKeyProvider + Clone + 'static>(
        root: PathBuf,
        keys: Keys,
    ) -> Result<Self, AgentFailure> {
        let (sender, receiver) = mpsc::sync_channel::<Arc<Job>>(1);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        std::thread::Builder::new()
            .name("floe-agent-vault".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let mut vault = None;
                while let Ok(job) = receiver.recv() {
                    if worker_closing.load(Ordering::Acquire) {
                        break;
                    }
                    let result = catch_unwind(AssertUnwindSafe(|| match &runtime {
                        Ok(runtime) => runtime.block_on(execute(&root, &keys, &mut vault, &job)),
                        Err(_) => Err(AgentFailure::VaultUnavailable),
                    }))
                    .unwrap_or(Err(AgentFailure::Interrupted));
                    if matches!(
                        result,
                        Err(AgentFailure::VaultUnavailable | AgentFailure::Interrupted)
                    ) {
                        vault = None;
                    }
                    if let Ok(mut progress) = job.progress.lock() {
                        match result {
                            Ok((state, session)) => {
                                progress.state = Some(state);
                                progress.session = session;
                            }
                            Err(failure) => {
                                progress.state = Some(AgentVaultStateDto::Unavailable);
                                progress.failure = Some(failure);
                            }
                        }
                        progress.done = true;
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
        })
    }

    fn request(
        &self,
        person: PersonId,
        id: Uuid,
        operation: AgentVaultOperationDto,
    ) -> Result<AgentVaultResultDto, AgentFailure> {
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
                self.sender
                    .try_send(job.clone())
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
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
        let response = AgentVaultResultDto {
            request_id: id.to_string(),
            events: progress.events[after_sequence..].to_vec(),
            next_sequence: progress.events.len(),
            done: progress.done,
            state: progress.state,
            session: progress.session.clone(),
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
        if let Ok(active) = self.active.lock() {
            if let Some(job) = active.as_ref() {
                job.cancellation.cancel();
            }
        }
    }
}

async fn execute<Keys: VaultKeyProvider + Clone>(
    root: &std::path::Path,
    keys: &Keys,
    current: &mut Option<(PersonId, EncryptedAgentVault<Keys>)>,
    job: &Job,
) -> Result<(AgentVaultStateDto, Option<AgentSession>), AgentFailure> {
    if current
        .as_ref()
        .is_some_and(|(person, _)| *person != job.person)
    {
        return Err(AgentFailure::NotFound);
    }
    if job.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    match &job.action {
        AgentVaultActionDto::Status {} => {
            if let Some((_, vault)) = current {
                vault.check_access()?;
                return Ok((AgentVaultStateDto::Ready, None));
            }
            let state = match fs::symlink_metadata(root.join(job.person.to_string())) {
                Ok(metadata) if metadata.is_dir() => AgentVaultStateDto::Locked,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    AgentVaultStateDto::Missing
                }
                _ => AgentVaultStateDto::Unavailable,
            };
            Ok((state, None))
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
            let vault = EncryptedAgentVault::create(root, job.person, keys.clone()).await?;
            *current = Some((job.person, vault));
            Ok((AgentVaultStateDto::Ready, None))
        }
        AgentVaultActionDto::Unlock {} => {
            if current.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let vault = EncryptedAgentVault::open(root, job.person, keys.clone()).await?;
            *current = Some((job.person, vault));
            Ok((AgentVaultStateDto::Ready, None))
        }
        AgentVaultActionDto::Lock {} => {
            *current = None;
            Ok((AgentVaultStateDto::Locked, None))
        }
        AgentVaultActionDto::Session { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let session = match operation {
                AgentFixtureOperationDto::Start {} => vault.create_sample_session().await?,
                AgentFixtureOperationDto::Resume {} => vault.resume_sample_session().await?,
                AgentFixtureOperationDto::Get { session_id } => {
                    sample_session(vault, job.person, session_uuid(session_id)?).await?
                }
                AgentFixtureOperationDto::Recover {
                    session_id,
                    expected_revision,
                } => {
                    sample_session(vault, job.person, session_uuid(session_id)?).await?;
                    recover_agent_sample(
                        vault,
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
                    run_agent_sample(
                        vault,
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
            Ok((AgentVaultStateDto::Ready, Some(session)))
        }
    }
}

fn session_uuid(value: &str) -> Result<Uuid, AgentFailure> {
    Uuid::parse_str(value).map_err(|_| AgentFailure::InvalidInput)
}

async fn sample_session(
    store: &impl SessionStore,
    person: PersonId,
    id: Uuid,
) -> Result<AgentSession, AgentFailure> {
    let session = store.load(person, id).await?;
    if session.data_classes != [floe_agent::DataClass::Synthetic] {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_core::VaultKey;
    use std::{collections::HashMap, sync::Condvar, time::Instant};

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

    fn wait(worker: &Worker, person: PersonId, id: Uuid) -> AgentVaultResultDto {
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

    fn perform(
        worker: &Worker,
        person: PersonId,
        action: AgentVaultActionDto,
    ) -> AgentVaultResultDto {
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
            AgentFixtureOperationDto::Resume {},
        ] {
            let result = perform(&worker, person, AgentVaultActionDto::Session { operation });
            assert_eq!(result.failure, Some(AgentFailure::PolicyDenied));
            assert!(result.session.is_none());
            assert!(result.events.is_empty());
        }
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
