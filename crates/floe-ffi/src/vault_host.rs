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

use floe_agent::{
    AgentEvent, AgentFailure, AgentSession, Cancellation, KnowledgeActor, KnowledgeDecisionKind,
    KnowledgeKind, SessionStore,
};
use floe_core::{
    AgentFixtureTurn, CalendarActionState, EncryptedAgentVault, ExpertCalendarInspection,
    ExpertProposalReference, FloeCore, KeyringVaultKeys, VaultKeyProvider, recover_agent_sample,
};
use floe_domain::PersonId;
use floe_protocol::*;
use uuid::Uuid;

use super::{BridgeResult, agent_failure, check_version, parse_id, parse_person};

mod calendar_turn;
mod conversation_turn;
mod learner_worker;

const LEARNER_IDLE_DELAY: Duration = Duration::from_millis(750);
const LEARNER_EMPTY_DELAY: Duration = Duration::from_secs(30);
const LEARNER_ERROR_DELAY: Duration = Duration::from_secs(5);

pub(crate) struct VaultBridge {
    root: PathBuf,
    core: Arc<FloeCore>,
    worker: RefCell<Option<Worker>>,
}

impl VaultBridge {
    pub(crate) fn new(database_path: &str, core: Arc<FloeCore>) -> Self {
        Self {
            root: PathBuf::from(format!("{database_path}.agent-vaults")),
            core,
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
            *worker = Some(
                Worker::with_core(self.root.clone(), KeyringVaultKeys, self.core.clone())
                    .map_err(agent_failure)?,
            );
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
    foreground_pending: Arc<AtomicBool>,
    background: Arc<Mutex<Option<Cancellation>>>,
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
    calendar_turn: Option<AgentCalendarTurnResultDto>,
    proposal: Option<AgentProposalInspectionDto>,
    memory_review: Option<AgentMemoryReviewOverviewDto>,
    memory: Option<AgentMemoryOverviewDto>,
    failure: Option<AgentFailure>,
}

impl Worker {
    fn with_core<Keys: VaultKeyProvider + Clone + 'static>(
        root: PathBuf,
        keys: Keys,
        core: Arc<FloeCore>,
    ) -> Result<Self, AgentFailure> {
        let (sender, receiver) = mpsc::sync_channel::<Arc<Job>>(1);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        let foreground_pending = Arc::new(AtomicBool::new(false));
        let worker_foreground_pending = foreground_pending.clone();
        let background = Arc::new(Mutex::new(None));
        let worker_background = background.clone();
        std::thread::Builder::new()
            .name("floe-agent-vault".into())
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
                            worker_foreground_pending.store(false, Ordering::Release);
                            if worker_closing.load(Ordering::Acquire) {
                                break;
                            }
                            let result = catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => {
                                    runtime.block_on(execute(&root, &keys, &core, &mut vault, &job))
                                }
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
                                    Ok((
                                        state,
                                        session,
                                        registry,
                                        calendar_experts,
                                        calendar_turn,
                                        proposal,
                                        memory_review,
                                        memory,
                                    )) => {
                                        progress.state = Some(state);
                                        progress.session = session;
                                        progress.registry = registry;
                                        progress.calendar_experts = calendar_experts;
                                        progress.calendar_turn = calendar_turn;
                                        progress.proposal = proposal;
                                        progress.memory_review = memory_review;
                                        progress.memory = memory;
                                    }
                                    Err(failure) => {
                                        progress.state = Some(
                                            if matches!(
                                                failure,
                                                AgentFailure::VaultUnavailable
                                                    | AgentFailure::Interrupted
                                            ) || !vault
                                                .as_ref()
                                                .is_some_and(|(person, _)| *person == job.person)
                                            {
                                                AgentVaultStateDto::Unavailable
                                            } else {
                                                AgentVaultStateDto::Ready
                                            },
                                        );
                                        progress.failure = Some(failure);
                                    }
                                }
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
                            let cancellation = Cancellation::default();
                            if let Ok(mut active) = worker_background.lock() {
                                *active = Some(cancellation.clone());
                            } else {
                                learner_delay = LEARNER_ERROR_DELAY;
                                continue;
                            }
                            if worker_foreground_pending.load(Ordering::Acquire) {
                                if let Ok(mut active) = worker_background.lock() {
                                    *active = None;
                                }
                                continue;
                            }
                            let result = catch_unwind(AssertUnwindSafe(|| match &runtime {
                                Ok(runtime) => {
                                    runtime.block_on(learner_worker::run(open_vault, cancellation))
                                }
                                Err(_) => Err(AgentFailure::VaultUnavailable),
                            }))
                            .unwrap_or(Err(AgentFailure::Interrupted));
                            if let Ok(mut active) = worker_background.lock() {
                                *active = None;
                            }
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
            foreground_pending,
            background,
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
                self.foreground_pending.store(true, Ordering::Release);
                if let Ok(background) = self.background.lock() {
                    if let Some(cancellation) = background.as_ref() {
                        cancellation.cancel();
                    }
                }
                if self.sender.try_send(job.clone()).is_err() {
                    self.foreground_pending.store(false, Ordering::Release);
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
        let response = AgentVaultResultDto {
            request_id: id.to_string(),
            events: progress.events[after_sequence..].to_vec(),
            next_sequence: progress.events.len(),
            done: progress.done,
            state: progress.state,
            session: progress.session.clone(),
            registry: progress.registry.clone(),
            calendar_experts: progress.calendar_experts.clone(),
            calendar_turn: progress.calendar_turn.clone(),
            proposal: progress.proposal.clone(),
            memory_review: progress.memory_review.clone(),
            memory: progress.memory.clone(),
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
        if let Ok(background) = self.background.lock() {
            if let Some(cancellation) = background.as_ref() {
                cancellation.cancel();
            }
        }
    }
}

async fn execute<Keys: VaultKeyProvider + Clone>(
    root: &std::path::Path,
    keys: &Keys,
    core: &FloeCore,
    current: &mut Option<(PersonId, EncryptedAgentVault<Keys>)>,
    job: &Job,
) -> Result<
    (
        AgentVaultStateDto,
        Option<AgentSession>,
        Option<floe_agent::RegistryOverview>,
        Option<floe_agent::CalendarExpertOverview>,
        Option<AgentCalendarTurnResultDto>,
        Option<AgentProposalInspectionDto>,
        Option<AgentMemoryReviewOverviewDto>,
        Option<AgentMemoryOverviewDto>,
    ),
    AgentFailure,
> {
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
                return Ok((
                    AgentVaultStateDto::Ready,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ));
            }
            let state = match fs::symlink_metadata(root.join(job.person.to_string())) {
                Ok(metadata) if metadata.is_dir() => AgentVaultStateDto::Locked,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    AgentVaultStateDto::Missing
                }
                _ => AgentVaultStateDto::Unavailable,
            };
            Ok((state, None, None, None, None, None, None, None))
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
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::Unlock {} => {
            if current.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let vault = EncryptedAgentVault::open(root, job.person, keys.clone()).await?;
            *current = Some((job.person, vault));
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::Lock {} => {
            *current = None;
            Ok((
                AgentVaultStateDto::Locked,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ))
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
            Ok((
                AgentVaultStateDto::Ready,
                Some(session),
                None,
                None,
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::Registry { change } => {
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
            Ok((
                AgentVaultStateDto::Ready,
                None,
                registry,
                None,
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::CalendarExperts { setup } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if let Some(request) = setup {
                vault
                    .install_calendar_expert(request.clone(), job.cancellation.clone())
                    .await?;
            }
            let overview = vault.calendar_expert_overview().await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                Some(overview),
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::CalendarAccess { change } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let overview = vault
                .configure_calendar_access(change.clone(), job.cancellation.clone())
                .await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                Some(overview),
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::CalendarSession { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let session = match operation {
                AgentCalendarSessionOperationDto::Start { setup_id } => {
                    vault
                        .create_calendar_session(session_uuid(setup_id)?, job.cancellation.clone())
                        .await?
                }
                AgentCalendarSessionOperationDto::Resume { setup_id } => {
                    vault
                        .resume_calendar_session(session_uuid(setup_id)?, job.cancellation.clone())
                        .await?
                }
                AgentCalendarSessionOperationDto::Get { session_id } => {
                    vault.calendar_session(session_uuid(session_id)?).await?
                }
                AgentCalendarSessionOperationDto::Recover {
                    session_id,
                    expected_revision,
                } => {
                    vault
                        .recover_calendar_session(
                            session_uuid(session_id)?,
                            *expected_revision,
                            job.cancellation.clone(),
                        )
                        .await?
                }
            };
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            Ok((
                AgentVaultStateDto::Ready,
                Some(session),
                None,
                None,
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::CalendarTurn { request } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let (session, result) = calendar_turn::run(
                core,
                vault,
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
            )
            .await?;
            Ok((
                AgentVaultStateDto::Ready,
                Some(session),
                None,
                None,
                Some(result),
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::ConversationSession { operation } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
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
            Ok((
                AgentVaultStateDto::Ready,
                Some(session),
                None,
                None,
                None,
                None,
                None,
                None,
            ))
        }
        AgentVaultActionDto::ConversationTurn { request } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            let session = conversation_turn::run(
                core,
                vault,
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
            )
            .await?;
            Ok((
                AgentVaultStateDto::Ready,
                Some(session),
                None,
                None,
                None,
                None,
                None,
                None,
            ))
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
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                None,
                None,
                Some(proposal),
                None,
                None,
            ))
        }
        AgentVaultActionDto::MemoryReview { decision } => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let pending = vault.pending_knowledge_candidates().await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let decision = match decision {
                Some(request) => {
                    let candidate_id = session_uuid(&request.candidate_id)?;
                    if !pending.iter().any(|candidate| {
                        candidate.id == candidate_id && candidate.kind == KnowledgeKind::Memory
                    }) {
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
            let candidates = if decision.is_some() {
                vault.pending_knowledge_candidates().await?
            } else {
                pending
            }
            .into_iter()
            .filter(|candidate| candidate.kind == KnowledgeKind::Memory)
            .collect();
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                None,
                None,
                None,
                Some(AgentMemoryReviewOverviewDto {
                    schema_version: PROTOCOL_VERSION,
                    person_id: job.person.to_string(),
                    candidates,
                    decision,
                }),
                None,
            ))
        }
        AgentVaultActionDto::Memory {} => {
            let (_, vault) = current.as_ref().ok_or(AgentFailure::VaultUnavailable)?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let (saved_count, revisions) = vault.personal_memory_overview(100).await?;
            let pending_count = vault.pending_memory_candidate_count().await?;
            if job.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let memories = revisions
                .into_iter()
                .map(|revision| {
                    let floe_agent::KnowledgePayload::Memory { value } = revision.payload else {
                        return Err(AgentFailure::VaultUnavailable);
                    };
                    Ok(AgentMemorySummaryDto {
                        target_id: revision.target_id.to_string(),
                        revision: revision.revision,
                        statement: value.statement,
                        memory_kind: value.kind,
                        epistemic_status: value.epistemic_status,
                        confidence_millis: value.confidence_millis,
                        source_count: revision.source_refs.len(),
                        origin: match revision.created_by {
                            KnowledgeActor::User => AgentMemoryOriginDto::UserProvided,
                            _ => AgentMemoryOriginDto::Learned,
                        },
                        created_at: revision.created_at,
                        valid_from: value.valid_from,
                        valid_until: value.valid_until,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((
                AgentVaultStateDto::Ready,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(AgentMemoryOverviewDto {
                    schema_version: PROTOCOL_VERSION,
                    person_id: job.person.to_string(),
                    saved_count,
                    pending_count,
                    memories,
                }),
            ))
        }
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
    use floe_core::VaultKey;
    use std::{collections::HashMap, sync::Condvar, time::Instant};

    mod calendar_experts;
    mod calendar_sessions;
    mod calendar_turns;
    mod memory_review;
    mod proposals;

    impl Worker {
        fn new(root: PathBuf, keys: Keys) -> Result<Self, AgentFailure> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let core = runtime.block_on(FloeCore::open(":memory:")).unwrap();
            Self::with_core(root, keys, Arc::new(core))
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
    fn accepted_foreground_work_preempts_the_active_learner() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
        let learner = Cancellation::default();
        *worker.background.lock().unwrap() = Some(learner.clone());
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
