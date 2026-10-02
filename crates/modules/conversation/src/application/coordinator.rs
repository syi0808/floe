use std::sync::Arc;

use floe_agent_contract::{
    AgentMessage, BatchCursor, DependencyCoverage, EngineRequest, EngineResumeState, EngineStep,
    MessageRole, ModelConversation, ModelConversationEntry, UserInteractionRef,
    UserInteractionStatus, ValidatedModelBatch,
};
use floe_agent_runtime::{Engine, EngineBlocked, EngineOutcome, EnginePorts, EngineReport};
use floe_execution::{ExecutionScope, budget::BudgetLedger};
use floe_kernel::{AgentFailure, RunId, TraceContext};

use crate::{
    CONVERSATION_MODEL_CONSUMER, CancelRunRequest, CancelRunStatus, CommandQuery,
    CompactionReceipt, CompactionRequest, ContinuationSnapshot, ConversationInteraction,
    ConversationPorts, ConversationRepository, InteractionOrigin, InteractionRepository,
    InteractionResumeRef, InteractionState, MODEL_CONSENT_LIMITATION, ManagerConfig,
    ProfileSelection, PublishAdmission, PublishModelRequirement, RecoveryReceipt, RecoveryRequest,
    RunCancellationRegistry, RunQuery, RunReceipt, RunState, RunTerminal, TurnAdmission,
    TurnAdmissionRequest, TurnMode, TurnRequest,
};

use super::finalization::{FinalizationOutcome, finalize_exhausted_run};
use super::interactions::{publish_model_requirement, rescope_blocked_requirement};
use super::recovery::{JournalLineage, project_journal, project_transcript_history};

pub struct ConversationService<Repository> {
    repository: Arc<Repository>,
    run_cancellations: Arc<RunCancellationRegistry>,
    engine: Engine,
    config: ManagerConfig,
}

impl<Repository: ConversationRepository + InteractionRepository> ConversationService<Repository> {
    pub fn new(repository: Arc<Repository>, config: ManagerConfig) -> Result<Self, AgentFailure> {
        Self::with_run_cancellations(
            repository,
            config,
            Arc::new(RunCancellationRegistry::default()),
        )
    }

    pub fn with_run_cancellations(
        repository: Arc<Repository>,
        config: ManagerConfig,
        run_cancellations: Arc<RunCancellationRegistry>,
    ) -> Result<Self, AgentFailure> {
        config.validate()?;
        Ok(Self {
            repository,
            run_cancellations,
            engine: Engine::default(),
            config,
        })
    }

    pub async fn run_turn(
        &self,
        request: TurnRequest,
        ports: ConversationPorts<'_>,
    ) -> Result<RunReceipt, AgentFailure> {
        self.run_turn_observed(request, ports, |_| {}).await
    }

    pub async fn run_turn_observed(
        &self,
        request: TurnRequest,
        ports: ConversationPorts<'_>,
        mut on_admitted: impl FnMut(&RunReceipt),
    ) -> Result<RunReceipt, AgentFailure> {
        request.validate()?;
        let intent = request.canonical_intent()?;
        let request_digest = intent.digest(&request.principal)?;
        let command_query = CommandQuery {
            principal: request.principal.clone(),
            command_id: request.command_id,
        };
        if let Some(receipt) = self.repository.find_command(command_query).await? {
            verify_existing(&request, request_digest, &receipt)?;
            on_admitted(&receipt);
            return Ok(receipt);
        }
        let continuation = match &request.mode {
            TurnMode::New | TurnMode::Resume(_) => None,
            TurnMode::Continue(reference) => {
                let snapshot = continuation(
                    self.repository.as_ref(),
                    reference.run_id,
                    &request.principal,
                )
                .await?;
                if snapshot.reference != *reference
                    || snapshot.session_id != request.session_id
                    || snapshot.session_revision != request.expected_session_revision
                {
                    return Err(AgentFailure::Conflict);
                }
                if snapshot.profile != request.profile {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if let Some(batch) = &snapshot.pending_batch {
                    if snapshot.expert_environment != request.expert_environment
                        || batch.catalog_revision != request.expert_environment.revision
                    {
                        return Err(AgentFailure::Conflict);
                    }
                }
                Some(snapshot)
            }
        };
        let resume_origin = match &request.mode {
            TurnMode::Resume(reference) => Some(self.resume_origin(reference, &request).await?),
            TurnMode::New | TurnMode::Continue(_) => None,
        };
        if let Some(retry_of) = request.retry_of {
            let source = self
                .repository
                .load_receipt(retry_of)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            source.validate()?;
            if source.principal != request.principal
                || source.session_id != request.session_id
                || !source.state.is_terminal()
                || source.session_revision != request.expected_session_revision
            {
                return Err(AgentFailure::Conflict);
            }
        }
        let run_id = RunId::new();
        let admission = self
            .repository
            .admit_turn(TurnAdmissionRequest {
                expert_environment: request.expert_environment,
                run_id,
                command_id: request.command_id,
                session_id: request.session_id,
                expected_session_revision: request.expected_session_revision,
                principal: request.principal.clone(),
                request_digest,
                mode: request.mode.clone(),
                retry_of: request.retry_of,
                profile: request.profile.clone(),
                user_message: AgentMessage {
                    message_id: request.command_id.as_uuid(),
                    role: MessageRole::User,
                    text: intent.text.clone(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                },
            })
            .await?;
        let admitted = match admission {
            TurnAdmission::Created(admitted) => admitted,
            TurnAdmission::Existing(receipt) => {
                verify_existing(&request, request_digest, &receipt)?;
                return Ok(receipt);
            }
            TurnAdmission::Resumed(receipt) => {
                let TurnMode::Resume(reference) = &request.mode else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                verify_resumed(&request, reference, &receipt)?;
                on_admitted(&receipt);
                return Ok(receipt);
            }
        };
        admitted.validate()?;
        if admitted.receipt.run_id != run_id
            || admitted.receipt.expert_environment != request.expert_environment
            || admitted.receipt.command_id != request.command_id
            || admitted.receipt.session_id != request.session_id
            || admitted.receipt.principal != request.principal
            || admitted.receipt.request_digest != request_digest
            || admitted.receipt.retry_of != request.retry_of
            || admitted.receipt.profile != request.profile
            || admitted.receipt.state != RunState::Working
            || match &request.mode {
                TurnMode::New => {
                    admitted.receipt.continuation_of.is_some()
                        || admitted.receipt.continuation_executor_generation.is_some()
                        || admitted.receipt.continuation_level != 0
                        || admitted.transcript.last().is_none_or(|message| {
                            message.message_id != request.command_id.as_uuid()
                                || message.role != MessageRole::User
                        })
                }
                TurnMode::Continue(reference) => {
                    admitted.receipt.continuation_of != Some(reference.run_id)
                        || admitted.receipt.continuation_executor_generation
                            != Some(reference.executor_generation)
                        || admitted.receipt.continuation_level != reference.level
                        || admitted
                            .transcript
                            .iter()
                            .any(|message| message.message_id == request.command_id.as_uuid())
                }
                TurnMode::Resume(reference) => {
                    admitted.receipt.resume_of != Some(reference.origin_run_id)
                        || admitted.receipt.resume_lineage != reference.lineage
                        || admitted.receipt.continuation_of.is_some()
                        || admitted.receipt.continuation_level != 0
                        || admitted.receipt.retry_of.is_some()
                        || admitted
                            .transcript
                            .iter()
                            .any(|message| message.message_id == request.command_id.as_uuid())
                }
            }
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let expected_aggregate_revision = admitted.receipt.aggregate_revision;
        let _cancellation_guard = match self.run_cancellations.register(
            run_id,
            request.command_id,
            &request.principal,
            request.cancellation.clone(),
        ) {
            Ok(guard) => guard,
            Err(failure) => {
                return self
                    .repository
                    .finish_run(
                        run_id,
                        expected_aggregate_revision,
                        RunTerminal::from_failure(failure),
                    )
                    .await;
            }
        };
        on_admitted(&admitted.receipt);
        let now = tokio::time::Instant::now();
        if request.deadline <= now {
            return self
                .repository
                .finish_run(
                    run_id,
                    expected_aggregate_revision,
                    RunTerminal::from_failure(AgentFailure::DeadlineExceeded),
                )
                .await;
        }
        if request.deadline > now + self.config.max_run_duration {
            return self
                .repository
                .finish_run(
                    run_id,
                    expected_aggregate_revision,
                    RunTerminal::from_failure(AgentFailure::InvalidInput),
                )
                .await;
        }
        let journal = match self.repository.journal(run_id) {
            Ok(journal) => journal,
            Err(failure) => {
                return self
                    .repository
                    .finish_run(
                        run_id,
                        expected_aggregate_revision,
                        RunTerminal::from_failure(failure),
                    )
                    .await;
            }
        };
        // The resume context re-reads the origin group after admission:
        // the group gate itself was decided atomically inside admission,
        // while this listing only tells the fresh Manager what the person
        // resolved. A listing failure fails the admitted child closed. The
        // restated User entry keeps the origin command's identity: it is
        // the origin's own utterance, not a new one.
        let resume_context = match &resume_origin {
            None => None,
            Some(origin) => {
                let group = match super::interactions::list_run_interactions(
                    self.repository.as_ref(),
                    &request.principal,
                    origin.run_id,
                )
                .await
                {
                    Ok(group) => group,
                    Err(failure) => {
                        return self
                            .repository
                            .finish_run(
                                run_id,
                                expected_aggregate_revision,
                                RunTerminal::from_failure(failure),
                            )
                            .await;
                    }
                };
                Some((
                    origin.command_id.as_uuid(),
                    resume_marker_text(origin, &group),
                ))
            }
        };
        let completed_iterations = continuation
            .as_ref()
            .map_or(0, |snapshot| snapshot.completed_iterations);
        let prior_usage = continuation
            .as_ref()
            .map_or_else(Default::default, |snapshot| snapshot.usage);
        if completed_iterations >= self.config.max_iterations {
            return self
                .repository
                .finish_run(
                    run_id,
                    expected_aggregate_revision,
                    RunTerminal::from_failure(AgentFailure::BudgetExceeded),
                )
                .await;
        }
        // Retained for blockage publication after the Engine consumes the
        // request below.
        let turn_context = request.clone();
        let ledger = BudgetLedger::new(self.config.budget, prior_usage);
        let scope = ExecutionScope::root(
            request.cancellation,
            request.deadline,
            ledger.work_lease(),
            TraceContext::new(request.command_id.as_uuid()).with_run_id(run_id),
        );
        let user_entry = ModelConversationEntry::User {
            message_id: request.command_id.as_uuid(),
            text: intent.text.clone(),
        };
        let (model_conversation, resume, mut continuation_replay) = match continuation {
            // Continuation and resume both derive from the validated request
            // mode, so the marker arm below only runs for a linked resume.
            Some(snapshot) => {
                // The new user message leads; the settled exchanges of the
                // continued execution follow as context for it.
                let mut current_turn =
                    Vec::with_capacity(snapshot.model_conversation.current_turn.len() + 1);
                current_turn.push(user_entry);
                current_turn.extend(snapshot.model_conversation.current_turn);
                let resume = snapshot.pending_batch.zip(snapshot.batch_cursor).map(
                    |(validated_batch, cursor)| EngineResumeState {
                        validated_batch,
                        cursor,
                    },
                );
                (
                    ModelConversation {
                        history: snapshot.model_conversation.history,
                        current_turn,
                    },
                    resume,
                    snapshot.replay,
                )
            }
            None => match resume_context {
                // A linked resume takes no batch: the origin's own
                // exchanges stay in history, one host-owned marker frames
                // the fresh turn, and the owner-derived original intent
                // follows as the turn's User entry. The transcript gains
                // no message; only the model turn restates the intent.
                Some((user_message_id, marker)) => (
                    ModelConversation {
                        history: project_transcript_history(&admitted.transcript)?,
                        current_turn: vec![
                            ModelConversationEntry::Preamble {
                                message_id: request.command_id.as_uuid(),
                                text: marker,
                            },
                            ModelConversationEntry::User {
                                message_id: user_message_id,
                                text: intent.text,
                            },
                        ],
                    },
                    None,
                    Vec::new(),
                ),
                None => {
                    let history = project_transcript_history(&admitted.transcript)?
                        .into_iter()
                        .filter(|entry| {
                            !matches!(
                                entry,
                                ModelConversationEntry::User { message_id, .. }
                                    if *message_id == request.command_id.as_uuid()
                            )
                        })
                        .collect();
                    (
                        ModelConversation {
                            history,
                            current_turn: vec![user_entry],
                        },
                        None,
                        Vec::new(),
                    )
                }
            },
        };
        continuation_replay.extend(request.replay);
        // The lineage binds this explicit intent: a linked resume carries
        // its origin, so consent granted under the origin's reviewed
        // lineage still scopes the child; anything else carries itself.
        let lineage_origin = resume_origin
            .as_ref()
            .map_or(run_id.as_uuid(), |origin| origin.run_id.as_uuid());
        let lineage =
            floe_agent_contract::RecipientLineage::try_new(request.session_id, lineage_origin)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
        let engine_request = EngineRequest {
            principal: request.principal.clone(),
            role_spec: self.config.role_spec.clone(),
            scope,
            conversation: model_conversation,
            allowed_catalog: request.allowed_catalog,
            purpose: self.config.purpose.clone(),
            consumer: CONVERSATION_MODEL_CONSUMER.into(),
            preferred_profile_id: match request.profile {
                ProfileSelection::Auto => None,
                ProfileSelection::Explicit(profile) => Some(profile),
            },
            max_iterations: self.config.max_iterations - completed_iterations,
            max_output_bytes: self.config.max_output_bytes,
            replay: continuation_replay,
            resume,
            delegation_context: request.delegation_context,
            lineage: Some(lineage),
        };
        let pending_coverage = engine_request
            .resume
            .as_ref()
            .map(|resume| resume.validated_batch.projection_coverage.clone());
        if let Some(coverage) = &pending_coverage {
            if let Err(failure) = floe_context::revalidate_turn_coverage(
                coverage.clone(),
                ports.coverage_resolver,
                &floe_context::DependencyAuthorization {
                    deadline: engine_request.scope.deadline(),
                    cancellation: engine_request.scope.cancellation().clone(),
                },
            )
            .await
            {
                return self
                    .repository
                    .finish_run(
                        run_id,
                        expected_aggregate_revision,
                        RunTerminal::from_failure(failure),
                    )
                    .await;
            }
        }
        let result = self
            .engine
            .drive(
                engine_request.clone(),
                EnginePorts {
                    projection: ports.projection,
                    model: ports.model,
                    tools: ports.tools,
                    delegation: ports.delegation,
                    journal: journal.as_ref(),
                    validator: ports.validator,
                },
            )
            .await;
        let terminal = match result {
            Ok(EngineOutcome::Completed(report)) => {
                let EngineReport {
                    steps,
                    output,
                    answering_projection_coverage,
                    ..
                } = report;
                match output {
                    Some(output) => match async {
                        if let Some(coverage) = &pending_coverage {
                            floe_context::revalidate_turn_coverage(
                                coverage.clone(),
                                ports.coverage_resolver,
                                &floe_context::DependencyAuthorization {
                                    deadline: engine_request.scope.deadline(),
                                    cancellation: engine_request.scope.cancellation().clone(),
                                },
                            )
                            .await?;
                        }
                        report_coverage(answering_projection_coverage, &steps)
                    }
                    .await
                    {
                        Ok(coverage) => RunTerminal {
                            state: RunState::Completed,
                            output: Some(output),
                            steps,
                            coverage,
                            issue: None,
                            interactions: vec![],
                        },
                        Err(failure) => RunTerminal::from_failure(failure),
                    },
                    None => {
                        self.finalize_exhaustion(
                            run_id,
                            &engine_request.scope,
                            &engine_request,
                            ports,
                            AgentFailure::Stalled,
                            &turn_context,
                        )
                        .await
                    }
                }
            }
            // A blocked first (or mid-run) dispatch: publish the durable
            // card under the exact attempted origin, commit the
            // deterministic source-independent limitation, and complete the
            // original Run without fabricating model output.
            Ok(EngineOutcome::Blocked(blocked)) => {
                self.complete_blocked_run(run_id, &turn_context, blocked)
                    .await
            }
            Err(failure @ (AgentFailure::BudgetExceeded | AgentFailure::Stalled)) => {
                self.finalize_exhaustion(
                    run_id,
                    &engine_request.scope,
                    &engine_request,
                    ports,
                    failure,
                    &turn_context,
                )
                .await
            }
            Err(failure) => RunTerminal::from_failure(failure),
        };
        self.repository
            .finish_run(run_id, expected_aggregate_revision, terminal)
            .await
    }

    /// Complete a Run whose model dispatch blocked on exact-recipient
    /// consent: publish the card, then finish Completed with the
    /// deterministic limitation plus the immutable interaction linkage.
    ///
    /// Coverage merges the limitation's independent base with settled
    /// steps, so previously settled work is preserved honestly and no
    /// successful source coverage is fabricated. A publication or coverage
    /// failure fails the Run closed instead of completing without a card.
    async fn complete_blocked_run(
        &self,
        run_id: RunId,
        request: &TurnRequest,
        blocked: EngineBlocked,
    ) -> RunTerminal {
        let mut steps = blocked.steps;
        steps.push(EngineStep::Answer {
            text: MODEL_CONSENT_LIMITATION.into(),
            artifacts: vec![],
        });
        let coverage = match report_coverage(Some(DependencyCoverage::Independent), &steps) {
            Ok(coverage) => coverage,
            Err(failure) => return RunTerminal::from_failure(failure),
        };
        // A linked resume dispatches under origin-carried lineage; the fresh
        // review re-scopes to this attempting Run before publication.
        let requirement =
            match rescope_blocked_requirement(blocked.requirement, request.session_id, run_id) {
                Ok(requirement) => requirement,
                Err(failure) => return RunTerminal::from_failure(failure),
            };
        let published = match publish_model_requirement(
            self.repository.as_ref(),
            self.repository.as_ref(),
            PublishModelRequirement {
                principal: request.principal.clone(),
                session_id: request.session_id,
                origin_run_id: run_id,
                origin: InteractionOrigin::Model {
                    attempt_id: blocked.attempt_id,
                },
                requirement,
                device_id: request.device_id.clone(),
            },
            request.now_unix_ms,
        )
        .await
        {
            Ok(PublishAdmission::Created(record)) => record,
            Ok(PublishAdmission::Existing(record)) => record,
            Err(failure) => return RunTerminal::from_failure(failure),
        };
        let reference = UserInteractionRef {
            interaction_id: published.id,
            kind: published.kind,
            status: UserInteractionStatus::Pending,
        };
        RunTerminal {
            state: RunState::Completed,
            output: Some(MODEL_CONSENT_LIMITATION.into()),
            steps,
            coverage,
            issue: None,
            interactions: vec![reference],
        }
    }

    async fn finalize_exhaustion(
        &self,
        run_id: RunId,
        scope: &ExecutionScope,
        request: &EngineRequest,
        ports: ConversationPorts<'_>,
        issue: AgentFailure,
        turn: &TurnRequest,
    ) -> RunTerminal {
        match finalize_exhausted_run(
            &self.engine,
            self.repository.as_ref(),
            run_id,
            scope,
            request,
            ports,
            issue,
            turn,
        )
        .await
        {
            Ok(FinalizationOutcome::Replied(terminal)) => terminal,
            Ok(FinalizationOutcome::NotAttempted(failure)) => RunTerminal::from_failure(failure),
            Ok(FinalizationOutcome::AttemptedWithoutReply) => {
                RunTerminal::from_failure(AgentFailure::Stalled)
            }
            Err(failure) => RunTerminal::from_failure(failure),
        }
    }

    pub async fn recover_session(
        &self,
        request: RecoveryRequest,
    ) -> Result<RecoveryReceipt, AgentFailure> {
        recover_session(self.repository.as_ref(), request).await
    }

    pub async fn continuation(
        &self,
        run_id: RunId,
        principal: &str,
    ) -> Result<ContinuationSnapshot, AgentFailure> {
        continuation(self.repository.as_ref(), run_id, principal).await
    }

    /// The origin a linked resume continues, verified before admission.
    ///
    /// Fail-fast only: the Vault re-verifies the origin, the profile, the
    /// interaction group and the resume slot atomically inside admission.
    async fn resume_origin(
        &self,
        reference: &InteractionResumeRef,
        request: &TurnRequest,
    ) -> Result<RunReceipt, AgentFailure> {
        let origin = self
            .repository
            .load_receipt(reference.origin_run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        origin.validate()?;
        if origin.principal != request.principal
            || origin.session_id != request.session_id
            || origin.resume().as_ref() != Some(reference)
            || origin.profile != request.profile
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(origin)
    }

    pub async fn get_command(
        &self,
        query: CommandQuery,
    ) -> Result<Option<RunReceipt>, AgentFailure> {
        super::query::get_command(self.repository.as_ref(), query).await
    }

    pub async fn get_run(&self, query: RunQuery) -> Result<Option<RunReceipt>, AgentFailure> {
        super::query::get_run(self.repository.as_ref(), query).await
    }

    pub async fn cancel_run(
        &self,
        request: CancelRunRequest,
    ) -> Result<CancelRunStatus, AgentFailure> {
        let receipt = self
            .get_run(RunQuery {
                run_id: request.run_id,
                principal: request.principal.clone(),
            })
            .await?;
        match receipt {
            Some(receipt) if receipt.state == RunState::Working => {
                self.run_cancellations.cancel_run(request)
            }
            Some(_) => Ok(CancelRunStatus::Inactive),
            None => Ok(CancelRunStatus::Unknown),
        }
    }

    pub async fn compact_session(
        &self,
        request: CompactionRequest,
    ) -> Result<CompactionReceipt, AgentFailure>
    where
        Repository: crate::SessionArchiveRepository,
    {
        super::archive::compact_session(self.repository.as_ref(), request).await
    }

    pub async fn read_archive<Authorize, AuthorizationFuture>(
        &self,
        request: &floe_agent_contract::ArchiveReadRequest,
        authorize: Authorize,
    ) -> Result<floe_context::ArchiveProjection, AgentFailure>
    where
        Repository: crate::SessionArchiveRepository,
        Authorize: FnMut(floe_context::ContextDependency) -> AuthorizationFuture,
        AuthorizationFuture: std::future::Future<Output = Result<bool, AgentFailure>>,
    {
        super::archive::read_archive(self.repository.as_ref(), request, authorize).await
    }
}

pub async fn recover_session<Repository: ConversationRepository>(
    repository: &Repository,
    request: RecoveryRequest,
) -> Result<RecoveryReceipt, AgentFailure> {
    request.validate()?;
    let expected_session_id = request.session_id;
    let expected_revision = request.expected_session_revision;
    let receipt = repository.recover_session(request).await?;
    receipt.validate()?;
    if receipt.session_id != expected_session_id || receipt.session_revision != expected_revision {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(receipt)
}

pub async fn continuation<Repository: ConversationRepository>(
    repository: &Repository,
    run_id: RunId,
    principal: &str,
) -> Result<ContinuationSnapshot, AgentFailure> {
    if !run_id.is_valid()
        || principal.trim() != principal
        || principal.is_empty()
        || principal.len() > 256
        || principal.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let admitted = repository
        .load_run(run_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if admitted.receipt.principal != principal {
        return Err(AgentFailure::CapabilityDenied);
    }
    admitted.validate()?;
    let current = admitted.receipt.clone();
    let mut chain = vec![current.clone()];
    let mut seen_runs = std::collections::HashSet::from([current.run_id]);
    while let Some(parent_run_id) = chain.last().and_then(|receipt| receipt.continuation_of) {
        if chain.len() >= 4 || !seen_runs.insert(parent_run_id) {
            return Err(AgentFailure::StorageUnavailable);
        }
        let child = chain.last().ok_or(AgentFailure::StorageUnavailable)?;
        let parent = repository
            .load_receipt(parent_run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        parent.validate()?;
        if parent.principal != principal
            || parent.session_id != current.session_id
            || parent.profile != current.profile
            || child.continuation_executor_generation != Some(parent.executor_generation)
            || parent.continuation().as_ref().is_none_or(|reference| {
                reference.run_id != parent_run_id || reference.level != child.continuation_level
            })
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        chain.push(parent);
    }
    chain.reverse();

    let history = project_transcript_history(&admitted.transcript)?;
    let mut current_turn = Vec::new();
    let mut replay = Vec::new();
    let mut completed_iterations = 0_u32;
    let mut usage = floe_execution::budget::ModelUsage::default();
    let mut carried: Option<(ValidatedModelBatch, BatchCursor)> = None;
    let mut total_entries = 0_usize;
    let mut seen_exchanges = std::collections::HashSet::new();
    let mut replay_invocations = std::collections::HashSet::new();
    let mut replay_calls = std::collections::HashSet::new();
    for receipt in chain.iter() {
        let entries = repository.load_journal(receipt.run_id).await?;
        total_entries = total_entries
            .checked_add(entries.len())
            .ok_or(AgentFailure::StorageUnavailable)?;
        if total_entries > 512 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let projected = project_journal(receipt, &entries)?;
        // A resumed run re-journals the steps it replays, so the same logical
        // exchange can appear in several runs: keep the first, skip repeats.
        // Duplicates inside one journal are still rejected by projection.
        for entry in projected.model_conversation.current_turn {
            match exchange_identity(&entry) {
                Some(id) if !seen_exchanges.insert(id) => continue,
                _ => current_turn.push(entry),
            }
        }
        for receipt in projected.replay {
            if !replay_invocations.insert(receipt.invocation_key)
                || !replay_calls.insert(receipt.call_id)
            {
                continue;
            }
            replay.push(receipt);
        }
        completed_iterations = completed_iterations
            .checked_add(projected.completed_iterations)
            .ok_or(AgentFailure::StorageUnavailable)?;
        usage.attempts = usage
            .attempts
            .checked_add(projected.usage.attempts)
            .ok_or(AgentFailure::StorageUnavailable)?;
        usage.tokens = usage
            .tokens
            .checked_add(projected.usage.tokens)
            .ok_or(AgentFailure::StorageUnavailable)?;
        usage.cost_micros = usage
            .cost_micros
            .checked_add(projected.usage.cost_micros)
            .ok_or(AgentFailure::StorageUnavailable)?;
        // Cross-run resume lineage: a newer run supersedes an older pending
        // batch only after durably re-recording the exact batch and starting
        // cursor. A child that crashed before takeover leaves the parent
        // pending state authoritative.
        let live = projected
            .pending_batch
            .clone()
            .zip(projected.cursor.clone());
        carried = reconcile_resume_lineage(carried, &projected.lineage, live)?;
    }
    let model_conversation = ModelConversation {
        history,
        current_turn,
    };
    if model_conversation.len() > floe_agent_contract::MAX_AGENT_MESSAGES || replay.len() > 128 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let (pending_batch, batch_cursor) =
        carried.map_or((None, None), |(batch, cursor)| (Some(batch), Some(cursor)));
    Ok(ContinuationSnapshot {
        expert_environment: current.expert_environment,
        reference: current.continuation().ok_or(AgentFailure::Conflict)?,
        session_id: current.session_id,
        session_revision: current.session_revision,
        profile: current.profile.clone(),
        model_conversation,
        replay,
        pending_batch,
        batch_cursor,
        completed_iterations,
        usage,
    })
}

/// Oldest → newest resume-lineage reconciliation. `carried` is the inherited
/// parent pending batch/cursor, `lineage` describes how the child journal
/// began, and `live` is the child's projected pending batch/cursor.
fn reconcile_resume_lineage(
    carried: Option<(ValidatedModelBatch, BatchCursor)>,
    lineage: &JournalLineage,
    live: Option<(ValidatedModelBatch, BatchCursor)>,
) -> Result<Option<(ValidatedModelBatch, BatchCursor)>, AgentFailure> {
    match (carried, lineage) {
        // No inherited pending work: only a journal that never claimed a
        // resume may carry live state forward.
        (None, JournalLineage::Empty | JournalLineage::Fresh) => Ok(live),
        (None, JournalLineage::ResumeBatchOnly { .. } | JournalLineage::ResumeClaimed { .. }) => {
            Err(AgentFailure::StorageUnavailable)
        }
        // The child has not taken over yet; the parent stays authoritative.
        (Some(parent), JournalLineage::Empty) => Ok(Some(parent)),
        // Batch-only re-records never started: an exact batch keeps the
        // parent authoritative, anything else is corruption.
        (Some((parent_batch, parent_cursor)), JournalLineage::ResumeBatchOnly { batch }) => {
            if *batch == parent_batch {
                Ok(Some((parent_batch, parent_cursor)))
            } else {
                Err(AgentFailure::StorageUnavailable)
            }
        }
        // Exact batch/cursor takeover confirmed: the child's live state
        // becomes authoritative from this point on.
        (Some((parent_batch, parent_cursor)), JournalLineage::ResumeClaimed { batch, cursor }) => {
            if *batch == parent_batch && *cursor == parent_cursor {
                Ok(live)
            } else {
                Err(AgentFailure::StorageUnavailable)
            }
        }
        // A fresh model plan cannot skip parent pending work.
        (Some(_), JournalLineage::Fresh) => Err(AgentFailure::StorageUnavailable),
    }
}

fn exchange_identity(entry: &ModelConversationEntry) -> Option<uuid::Uuid> {
    match entry {
        ModelConversationEntry::ToolExchange { call, .. } => Some(call.call_id),
        ModelConversationEntry::DelegationExchange { request, .. } => {
            Some(request.task_id.as_uuid())
        }
        ModelConversationEntry::User { .. }
        | ModelConversationEntry::Preamble { .. }
        | ModelConversationEntry::Assistant { .. } => None,
    }
}

fn verify_existing(
    request: &TurnRequest,
    request_digest: [u8; 32],
    receipt: &RunReceipt,
) -> Result<(), AgentFailure> {
    receipt.validate()?;
    if receipt.command_id != request.command_id
        || receipt.session_id != request.session_id
        || receipt.principal != request.principal
        || receipt.request_digest != request_digest
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

/// The host-owned marker that opens a linked resume turn.
///
/// Model-safe linkage only: the origin identity, the chain depth, and each
/// reviewed interaction's opaque id, kind and terminal status. Resolved
/// cards name a target-digest prefix so the fresh Manager can tell them
/// apart; anything the person did not resolve stays unavailable by
/// instruction, never by re-prompting in a loop. Bounded: at most eight
/// entries name ids, the rest count.
fn resume_marker_text(origin: &RunReceipt, group: &[ConversationInteraction]) -> String {
    const MAX_MARKER_ENTRIES: usize = 8;
    let depth = origin
        .resume()
        .map_or(origin.resume_lineage, |link| link.lineage);
    let mut marker = format!(
        "Linked resume of run {} at depth {depth}. The person finished reviewing its interactions; re-derive every read under current authority.",
        origin.run_id.as_uuid(),
    );
    for interaction in group.iter().take(MAX_MARKER_ENTRIES) {
        let status = match &interaction.state {
            InteractionState::Resolved { .. } => "resolved",
            InteractionState::Denied { .. } => "denied: unavailable",
            InteractionState::Cancelled { .. } => "cancelled: unavailable",
            InteractionState::Superseded { .. } => "superseded: unavailable",
            InteractionState::Expired => "expired: unavailable",
            InteractionState::Pending | InteractionState::Resolving { .. } => {
                "still pending review: do not wait for it"
            }
        };
        let digest: String = interaction.target_digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        marker.push_str(&format!(
            "\nInteraction {} ({:?}, target {digest}): {status}.",
            interaction.id, interaction.kind,
        ));
    }
    if group.len() > MAX_MARKER_ENTRIES {
        marker.push_str(&format!(
            "\nAnd {} more reviewed interactions.",
            group.len() - MAX_MARKER_ENTRIES
        ));
    }
    marker.push_str(
        "\nProceed with resolved and still-authorized information. Do not retry unavailable interactions, and do not re-ask the person about them.",
    );
    marker
}

/// A slot rejoin across commands: the receipt is the canonical child the
/// origin slot already admitted. Identity binds the origin linkage, the
/// session, the principal and the kept profile; the digest is the winner's
/// and is never compared against the loser's request.
fn verify_resumed(
    request: &TurnRequest,
    reference: &InteractionResumeRef,
    receipt: &RunReceipt,
) -> Result<(), AgentFailure> {
    receipt.validate()?;
    if receipt.session_id != request.session_id
        || receipt.principal != request.principal
        || receipt.resume_of != Some(reference.origin_run_id)
        || receipt.resume_lineage != reference.lineage
        || receipt.profile != request.profile
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

fn report_coverage(
    answering: Option<DependencyCoverage>,
    steps: &[EngineStep],
) -> Result<DependencyCoverage, AgentFailure> {
    // The answering projection coverage is the base: the final answer may
    // depend on reauthorized history even when this run executed nothing new.
    // Engine-step coverage merges on top; exact duplicates merge idempotently
    // while conflicting coverage fails closed.
    let mut coverage = answering.ok_or(AgentFailure::InvalidModelOutput)?;
    for step in steps {
        match step {
            EngineStep::Tool(result) => {
                coverage = coverage
                    .merge(&result.coverage)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?;
                for artifact in &result.artifacts {
                    coverage = coverage
                        .merge(&artifact.coverage)
                        .map_err(|_| AgentFailure::InvalidModelOutput)?;
                }
            }
            EngineStep::Delegation(receipt) => {
                coverage = coverage
                    .merge(&receipt.snapshot.coverage)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?;
                for artifact in &receipt.snapshot.artifacts {
                    coverage = coverage
                        .merge(&artifact.coverage)
                        .map_err(|_| AgentFailure::InvalidModelOutput)?;
                }
            }
            EngineStep::Answer { artifacts, .. } => {
                for artifact in artifacts {
                    coverage = coverage
                        .merge(&artifact.coverage)
                        .map_err(|_| AgentFailure::InvalidModelOutput)?;
                }
            }
        }
    }
    Ok(coverage)
}
