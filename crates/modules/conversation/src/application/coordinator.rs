use std::sync::Arc;

use floe_agent_contract::{
    AgentMessage, BatchCursor, DependencyCoverage, EngineRequest, EngineResumeState, EngineStep,
    MessageRole, ModelConversation, ModelConversationEntry, UserInteractionRef,
    UserInteractionStatus, ValidatedModelBatch,
};
use floe_agent_runtime::{Engine, EngineOutcome, EnginePorts, EngineReport};
use floe_execution::{ExecutionScope, budget::BudgetLedger};
use floe_kernel::{AgentFailure, OwnerActor, RunId, TraceContext};

use crate::{
    CONVERSATION_CONSUMER, CommandQuery, ContinuationSnapshot, ConversationInteraction,
    ConversationPorts, ConversationRepository, InteractionOrigin, InteractionRepository,
    InteractionResumeRef, InteractionState, ManagerConfig,
    RecoveryReceipt, RecoveryRequest,
    RunReceipt, RunState, RunTerminal, TurnAdmission,
    TurnAdmissionRequest, TurnMode, TurnRequest,
};

use super::finalization::{FinalizationOutcome, finalize_exhausted_run};
use super::recovery::{JournalLineage, project_journal, project_transcript_history};

pub(super) enum RunAdmission {
    Existing(RunReceipt),
    Created(PreparedRun),
}

pub(super) struct PreparedRun {
    pub(super) admitted: crate::AdmittedTurn,
    continuation: Option<ContinuationSnapshot>,
    resume_origin: Option<RunReceipt>,
    intent: crate::CanonicalTurnIntent,
}

pub(super) struct RunCoordinator<Repository> {
    repository: Arc<Repository>,
    engine: Engine,
    config: ManagerConfig,
    connections: Arc<floe_connections::ConnectionsService>,
}

impl<Repository: ConversationRepository + InteractionRepository> RunCoordinator<Repository> {
    pub(super) fn new(repository: Arc<Repository>, config: ManagerConfig, connections: Arc<floe_connections::ConnectionsService>) -> Result<Self, AgentFailure> {
        config.validate()?;
        Ok(Self { repository, engine: Engine::default(), config, connections })
    }

    pub(super) async fn prepare_run(
        &self,
        actor: &OwnerActor,
        request: &TurnRequest,
    ) -> Result<RunAdmission, AgentFailure> {
        actor.validate()?;
        request.validate()?;
        if actor.person_id.to_string() != request.principal || actor.device_id != request.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let intent = request.canonical_intent()?;
        let request_digest = intent.digest(&request.principal)?;
        let command_query = CommandQuery {
            principal: request.principal.clone(),
            command_id: request.command_id,
        };
        if let Some(receipt) = self.repository.find_command(command_query).await? {
            verify_existing(&request, request_digest, &receipt)?;
            return Ok(RunAdmission::Existing(receipt));
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
        let input = match &request.mode {
            TurnMode::New => crate::TurnInput::NewMessage(AgentMessage {
                message_id: request.command_id.as_uuid(), role: MessageRole::User,
                text: intent.text.clone(), call_id: None, coverage: DependencyCoverage::Independent,
            }),
            TurnMode::Continue(_) => crate::TurnInput::ExistingMessage {
                message_id: continuation.as_ref().ok_or(AgentFailure::Conflict)?.user_message_id,
            },
            TurnMode::Resume(_) => crate::TurnInput::ExistingMessage {
                message_id: resume_origin.as_ref().ok_or(AgentFailure::Conflict)?.user_message_id,
            },
        };
        let admission_request = TurnAdmissionRequest {
            expert_environment: request.expert_environment, run_id, command_id: request.command_id,
            session_id: request.session_id, expected_session_revision: request.expected_session_revision,
            principal: request.principal.clone(), device_id: request.device_id.clone(), request_digest,
            mode: request.mode.clone(), retry_of: request.retry_of, input,
        };
        let admission = match &request.mode {
            TurnMode::Resume(reference) => {
                let pending = self.repository.pending_resume_requests(64).await?.into_iter()
                    .find(|pending| pending.origin_run_id == reference.origin_run_id)
                    .ok_or(AgentFailure::Conflict)?;
                self.repository.claim_resume(crate::ResumeChildAdmission { request: pending, child: admission_request }).await?
            }
            _ => self.repository.admit_turn(admission_request).await?,
        };
        let admitted = match admission {
            TurnAdmission::Created(admitted) => admitted,
            TurnAdmission::Existing(receipt) => {
                verify_existing(&request, request_digest, &receipt)?;
                return Ok(RunAdmission::Existing(receipt));
            }
            TurnAdmission::Resumed(receipt) => {
                let TurnMode::Resume(reference) = &request.mode else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                verify_resumed(&request, reference, &receipt)?;
                    return Ok(RunAdmission::Existing(receipt));
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
        Ok(RunAdmission::Created(PreparedRun { admitted, continuation, resume_origin, intent }))
    }

    pub(super) async fn drive_run(
        &self,
        actor: &OwnerActor,
        request: TurnRequest,
        ports: ConversationPorts<'_>,
        prepared: PreparedRun,
        _cancellation_guard: super::cancellation::RunCancellationGuard,
    ) -> Result<RunReceipt, AgentFailure> {
        let PreparedRun { admitted, continuation, resume_origin, intent } = prepared;
        let run_id = admitted.receipt.run_id;
        let expected_aggregate_revision = admitted.receipt.aggregate_revision;
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
                    origin.user_message_id,
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
        let original = admitted.transcript.iter().find(|message|
            message.message_id == admitted.receipt.user_message_id && message.role == MessageRole::User)
            .ok_or(AgentFailure::StorageUnavailable)?;
        if original.text != intent.text { return Err(AgentFailure::Conflict); }
        let user_entry = ModelConversationEntry::User {
            message_id: original.message_id, text: original.text.clone(),
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
                        history: snapshot.model_conversation.history.into_iter().filter(|entry|
                            !matches!(entry, ModelConversationEntry::User { message_id, .. } if *message_id == admitted.receipt.user_message_id)).collect(),
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
                        history: project_transcript_history(&admitted.transcript)?.into_iter().filter(|entry|
                            !matches!(entry, ModelConversationEntry::User { message_id, .. } if *message_id == user_message_id)).collect(),
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
        let engine_request = EngineRequest {
            principal: request.principal.clone(),
            device_id: request.device_id.clone(),
            role_spec: self.config.role_spec.clone(),
            scope,
            conversation: model_conversation,
            allowed_catalog: request.allowed_catalog,
            purpose: self.config.purpose.clone(),
            consumer: CONVERSATION_CONSUMER.into(),
            max_iterations: self.config.max_iterations - completed_iterations,
            max_output_bytes: self.config.max_output_bytes,
            replay: continuation_replay,
            resume,
            delegation_context: request.delegation_context,
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
                            blocked: None,
                            interactions: vec![],
                        },
                        Err(failure) => RunTerminal::from_failure(failure),
                    },
                    None => {
                        return self.finalize_exhaustion(
                            actor,
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
            Ok(EngineOutcome::NeedsSourceReview(blocked)) => {
                let commit = super::source_review::build_blocked_run_commit(self.repository.as_ref(),
                    self.connections.as_ref(), actor, run_id, blocked, None, turn_context.now_unix_ms,
                    &engine_request.scope).await;
                return match commit {
                    Ok(commit) => self.repository.finish_blocked_run(commit).await,
                    Err(failure) => self.repository.finish_run(run_id, expected_aggregate_revision,
                        RunTerminal::from_failure(failure)).await,
                };
            }
            Err(failure @ (AgentFailure::BudgetExceeded | AgentFailure::Stalled)) => {
                return self.finalize_exhaustion(
                    actor,
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

    async fn finalize_exhaustion(
        &self,
        actor: &OwnerActor,
        run_id: RunId,
        scope: &ExecutionScope,
        request: &EngineRequest,
        ports: ConversationPorts<'_>,
        issue: AgentFailure,
        turn: &TurnRequest,
    ) -> Result<RunReceipt, AgentFailure> {
        let outcome = finalize_exhausted_run(&self.engine, self.repository.as_ref(), self.connections.as_ref(),
            actor, run_id, scope, request, ports, issue, turn).await;
        let terminal = match outcome {
            Ok(FinalizationOutcome::Blocked(commit)) => return self.repository.finish_blocked_run(commit).await,
            Ok(FinalizationOutcome::Replied(terminal)) => terminal,
            Ok(FinalizationOutcome::NotAttempted(failure)) => RunTerminal::from_failure(failure),
            Ok(FinalizationOutcome::AttemptedWithoutReply) => RunTerminal::from_failure(AgentFailure::Stalled),
            Err(failure) => RunTerminal::from_failure(failure),
        };
        let receipt = self.repository.load_receipt(run_id).await?.ok_or(AgentFailure::StorageUnavailable)?;
        self.repository.finish_run(run_id, receipt.aggregate_revision, terminal).await
    }

    /// The origin a linked resume continues, verified before admission.
    ///
    /// Fail-fast only: the Vault re-verifies the origin, the exact user message, the
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
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(origin)
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
        usage.estimated_tokens = usage.estimated_tokens.checked_add(projected.usage.estimated_tokens)
            .ok_or(AgentFailure::StorageUnavailable)?;
        usage.estimated_cost_micros = usage.estimated_cost_micros.checked_add(projected.usage.estimated_cost_micros)
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
        user_message_id: current.user_message_id,
        session_id: current.session_id,
        session_revision: current.session_revision,
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
/// session, the principal and the original user message; the digest is the winner's
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
