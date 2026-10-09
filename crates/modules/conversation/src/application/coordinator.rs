use std::{future::Future, sync::Arc, time::Duration};

use floe_agent_contract::{
    AgentMessage, DependencyCoverage, EngineRequest, EngineResumeState, EngineStep, MessageRole,
    ModelConversation, ModelConversationEntry, ModelSelectionState,
};
use floe_agent_runtime::{Engine, EngineOutcome, EnginePorts, EngineReport};
use floe_execution::{ExecutionScope, budget::BudgetLedger};
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor, RunId, TraceContext};

use crate::{
    CONVERSATION_CONSUMER, CommandQuery, ContinuationSnapshot, ConversationInteraction,
    ConversationPorts, ConversationRepository, InteractionRepository, InteractionResumeRef,
    InteractionState, ManagerConfig, RunReceipt, RunState, RunTerminal, TurnAdmission,
    TurnAdmissionRequest, TurnMode, TurnRequest,
};

use super::finalization::{FinalizationOutcome, finalize_exhausted_run};
use super::recovery::{
    MAX_CONTINUATION_JOURNAL_ENTRIES, ResumeLineageFold, fold_resume_lineage,
    project_transcript_history, reconcile_resume_lineage,
};

const PRE_DISPATCH_STORAGE_RETRY_DELAY: Duration = Duration::from_millis(20);

/// StorageBusy before the first model/tool call is safe to retry: no external
/// dispatch has started. Re-run the complete local observation while the
/// admitted Run remains Working, then fail only on cancellation/deadline or a
/// non-transient owner result.
async fn retry_pre_dispatch_storage_busy<T, Attempt, AttemptFuture>(
    scope: &ExecutionScope,
    mut attempt: Attempt,
) -> Result<T, AgentFailure>
where
    Attempt: FnMut() -> AttemptFuture,
    AttemptFuture: Future<Output = Result<T, AgentFailure>>,
{
    loop {
        if scope.cancellation().is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if tokio::time::Instant::now() >= scope.deadline() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        match attempt().await {
            Err(AgentFailure::StorageBusy) => {
                tokio::select! {
                    _ = scope.cancellation().cancelled() => return Err(AgentFailure::Cancelled),
                    _ = tokio::time::sleep(PRE_DISPATCH_STORAGE_RETRY_DELAY) => {}
                }
            }
            result => return result,
        }
    }
}

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
    operations: Arc<floe_calendar_operations::CalendarOperationsService>,
    experts: Arc<dyn floe_experts::ExpertsOwner>,
}

impl<Repository: ConversationRepository + InteractionRepository> RunCoordinator<Repository> {
    pub(super) fn new(
        repository: Arc<Repository>,
        config: ManagerConfig,
        connections: Arc<floe_connections::ConnectionsService>,
        operations: Arc<floe_calendar_operations::CalendarOperationsService>,
        experts: Arc<dyn floe_experts::ExpertsOwner>,
    ) -> Result<Self, AgentFailure> {
        config.validate()?;
        Ok(Self {
            repository,
            engine: Engine::default(),
            config,
            connections,
            operations,
            experts,
        })
    }

    pub(super) async fn prepare_run(
        &self,
        actor: &OwnerActor,
        request: &TurnRequest,
        scope: &ExecutionScope,
    ) -> Result<RunAdmission, CommandFailure<AgentFailure>> {
        actor.validate().map_err(CommandFailure::NotAdmitted)?;
        if !request.command_id.is_valid() {
            return Err(CommandFailure::NotApplied(AgentFailure::InvalidInput));
        }
        if actor.person_id.to_string() != request.principal || actor.device_id != request.device_id
        {
            return Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied));
        }
        let command_query = CommandQuery {
            principal: request.principal.clone(),
            command_id: request.command_id,
        };
        if let Some(receipt) = self
            .repository
            .find_command(command_query)
            .await
            .map_err(CommandFailure::Indeterminate)?
        {
            let request_digest = request
                .canonical_intent()
                .and_then(|intent| intent.digest(&request.principal))
                .map_err(CommandFailure::Indeterminate)?;
            verify_existing(&request, request_digest, &receipt)
                .map_err(CommandFailure::Indeterminate)?;
            return Ok(RunAdmission::Existing(receipt));
        }
        let occupant = self
            .repository
            .command_occupant(request.command_id)
            .await
            .map_err(CommandFailure::Indeterminate)?;
        if occupant.is_some() {
            return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
        }
        request.validate().map_err(CommandFailure::NotApplied)?;
        let intent = request
            .canonical_intent()
            .map_err(CommandFailure::NotApplied)?;
        let request_digest = intent
            .digest(&request.principal)
            .map_err(CommandFailure::NotApplied)?;
        let (continuation, resume_origin, run_id, admission_request) = async {
            let continuation = match &request.mode {
                TurnMode::New | TurnMode::Resume(_) => None,
                TurnMode::Continue(reference) => {
                    let snapshot = continuation(
                        self.repository.as_ref(),
                        reference.run_id,
                        &request.principal,
                        self.experts.as_ref(),
                        actor,
                        scope,
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
                TurnMode::Resume(reference) => Some(self.resume_origin(reference, request).await?),
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
                    message_id: request.command_id.as_uuid(),
                    role: MessageRole::User,
                    text: intent.text.clone(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                }),
                TurnMode::Continue(_) => crate::TurnInput::ExistingMessage {
                    message_id: continuation
                        .as_ref()
                        .ok_or(AgentFailure::Conflict)?
                        .user_message_id,
                },
                TurnMode::Resume(_) => crate::TurnInput::ExistingMessage {
                    message_id: resume_origin
                        .as_ref()
                        .ok_or(AgentFailure::Conflict)?
                        .user_message_id,
                },
            };
            let admission_request = TurnAdmissionRequest {
                expert_environment: request.expert_environment,
                run_id,
                command_id: request.command_id,
                session_id: request.session_id,
                expected_session_revision: request.expected_session_revision,
                principal: request.principal.clone(),
                device_id: request.device_id.clone(),
                request_digest,
                mode: request.mode.clone(),
                retry_of: request.retry_of,
                input,
            };
            Ok::<_, AgentFailure>((continuation, resume_origin, run_id, admission_request))
        }
        .await
        .map_err(CommandFailure::NotApplied)?;
        let admission = match &request.mode {
            TurnMode::Resume(reference) => {
                let pending = self
                    .repository
                    .pending_resume_request(actor, reference.origin_run_id)
                    .await
                    .map_err(CommandFailure::NotApplied)?
                    .ok_or(CommandFailure::NotApplied(AgentFailure::Conflict))?;
                self.repository
                    .claim_resume(crate::ResumeChildAdmission {
                        request: pending,
                        child: admission_request,
                    })
                    .await
                    .map_err(CommandFailure::Indeterminate)?
            }
            _ => self.repository.admit_turn(admission_request).await?,
        };
        let admitted = match admission {
            TurnAdmission::Created(admitted) => admitted,
            TurnAdmission::Existing(receipt) => {
                verify_existing(&request, request_digest, &receipt)
                    .map_err(CommandFailure::Indeterminate)?;
                return Ok(RunAdmission::Existing(receipt));
            }
            TurnAdmission::Resumed(receipt) => {
                let TurnMode::Resume(reference) = &request.mode else {
                    return Err(CommandFailure::Indeterminate(
                        AgentFailure::StorageUnavailable,
                    ));
                };
                verify_resumed(&request, reference, &receipt)
                    .map_err(CommandFailure::Indeterminate)?;
                return Ok(RunAdmission::Existing(receipt));
            }
        };
        admitted.validate().map_err(CommandFailure::Admitted)?;
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
            return Err(CommandFailure::Admitted(AgentFailure::StorageUnavailable));
        }
        Ok(RunAdmission::Created(PreparedRun {
            admitted,
            continuation,
            resume_origin,
            intent,
        }))
    }

    pub(super) async fn drive_run(
        &self,
        actor: &OwnerActor,
        request: TurnRequest,
        ports: ConversationPorts<'_>,
        prepared: PreparedRun,
        _cancellation_guard: &super::cancellation::RunCancellationGuard,
    ) -> Result<RunReceipt, AgentFailure> {
        let PreparedRun {
            admitted,
            continuation,
            resume_origin,
            intent,
        } = prepared;
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
        // This scope exists before any post-admission reads so transient Vault
        // writer contention remains pre-dispatch and can be retried safely.
        let resume_scope = ExecutionScope::root(
            request.cancellation.clone(),
            request.deadline,
            BudgetLedger::new(self.config.budget, Default::default()).work_lease(),
            TraceContext::new(request.command_id.as_uuid()).with_run_id(run_id),
        );
        let journal = match retry_pre_dispatch_storage_busy(&resume_scope, || {
            std::future::ready(self.repository.journal(run_id))
        })
        .await
        {
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
                let group = match retry_pre_dispatch_storage_busy(&resume_scope, || {
                    super::interactions::list_run_interactions(
                        self.repository.as_ref(),
                        &request.principal,
                        origin.run_id,
                    )
                })
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
                let marker = match retry_pre_dispatch_storage_busy(&resume_scope, || {
                    self.resume_marker_text(actor, origin, &group, &resume_scope)
                })
                .await
                {
                    Ok(marker) => marker,
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
                Some((origin.user_message_id, marker))
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
        let original = admitted
            .transcript
            .iter()
            .find(|message| {
                message.message_id == admitted.receipt.user_message_id
                    && message.role == MessageRole::User
            })
            .ok_or(AgentFailure::StorageUnavailable)?;
        if original.text != intent.text {
            return Err(AgentFailure::Conflict);
        }
        let user_entry = ModelConversationEntry::User {
            message_id: original.message_id,
            text: original.text.clone(),
        };
        let (model_conversation, resume, mut continuation_replay, model_selection) =
            match continuation {
            // Continuation and resume both derive from the validated request
            // mode, so the marker arm below only runs for a linked resume.
            Some(snapshot) => {
                let model_selection = if snapshot.pending_batch.is_some() {
                    snapshot.model_selection.clone()
                } else {
                    ModelSelectionState::Fresh
                };
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
                    model_selection,
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
                    ModelSelectionState::Fresh,
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
                        ModelSelectionState::Fresh,
                    )
                }
            },
        };
        continuation_replay.extend(request.replay);
        let execution_id = resume.as_ref().map_or(
            run_id.as_uuid(),
            |resume: &floe_agent_contract::EngineResumeState| resume.validated_batch.execution_id,
        );
        let engine_request = EngineRequest {
            execution_id,
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
            model_selection,
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
                        return self
                            .finalize_exhaustion(
                                actor,
                                run_id,
                                &engine_request.scope,
                                &engine_request,
                                ports,
                                AgentFailure::Stalled,
                                &turn_context,
                            )
                            .await;
                    }
                }
            }
            Ok(EngineOutcome::Blocked(blocked)) => {
                let commit = super::source_review::build_blocked_run_commit(
                    self.repository.as_ref(),
                    self.connections.as_ref(),
                    self.experts.as_ref(),
                    actor,
                    run_id,
                    blocked,
                    None,
                    turn_context.now_unix_ms,
                    &engine_request.scope,
                )
                .await;
                return match commit {
                    Ok(commit) => self.repository.finish_blocked_run(commit).await,
                    Err(failure) => {
                        self.repository
                            .finish_run(
                                run_id,
                                expected_aggregate_revision,
                                RunTerminal::from_failure(failure),
                            )
                            .await
                    }
                };
            }
            Err(failure @ (AgentFailure::BudgetExceeded | AgentFailure::Stalled)) => {
                return self
                    .finalize_exhaustion(
                        actor,
                        run_id,
                        &engine_request.scope,
                        &engine_request,
                        ports,
                        failure,
                        &turn_context,
                    )
                    .await;
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
        let outcome = finalize_exhausted_run(
            &self.engine,
            self.repository.as_ref(),
            self.connections.as_ref(),
            self.experts.as_ref(),
            actor,
            run_id,
            scope,
            request,
            ports,
            issue,
            turn,
        )
        .await;
        let terminal = match outcome {
            Ok(FinalizationOutcome::Blocked(commit)) => {
                return self.repository.finish_blocked_run(commit).await;
            }
            Ok(FinalizationOutcome::Replied(terminal)) => terminal,
            Ok(FinalizationOutcome::NotAttempted(failure)) => RunTerminal::from_failure(failure),
            Ok(FinalizationOutcome::AttemptedWithoutReply) => {
                RunTerminal::from_failure(AgentFailure::Stalled)
            }
            Err(failure) => RunTerminal::from_failure(failure),
        };
        let receipt = self
            .repository
            .load_receipt(run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        self.repository
            .finish_run(run_id, receipt.aggregate_revision, terminal)
            .await
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

    async fn resume_marker_text(
        &self,
        actor: &floe_kernel::OwnerActor,
        origin: &RunReceipt,
        group: &[ConversationInteraction],
        scope: &ExecutionScope,
    ) -> Result<String, AgentFailure> {
        const MAX_MARKER_ENTRIES: usize = 8;
        let depth = origin
            .resume()
            .map_or(origin.resume_lineage, |link| link.lineage);
        let mut marker = format!(
            "Linked resume of run {} at depth {depth}. The person finished reviewing its interactions; re-derive every read under current authority.",
            origin.run_id.as_uuid(),
        );
        for interaction in group.iter().take(MAX_MARKER_ENTRIES) {
            if let crate::ReviewedTarget::OperationApproval(review) = &interaction.target {
                let context = self.operations.resume_context(actor, review, scope).await?;
                if context.operation_id != review.operation_id || context.review != *review {
                    return Err(AgentFailure::Conflict);
                }
                let interaction_state = match &interaction.state {
                    InteractionState::Resolved { .. } => "resolved",
                    InteractionState::Denied { .. } => "denied: unavailable",
                    InteractionState::Cancelled { .. } => "cancelled: unavailable",
                    InteractionState::Superseded { .. } => "superseded: unavailable",
                    InteractionState::Expired => "expired: unavailable",
                    InteractionState::Pending | InteractionState::Resolving { .. } => {
                        "still pending owner review"
                    }
                };
                let decision = match (
                    context.decision_command_id,
                    context.decision_approval_id,
                    context.decision_kind,
                ) {
                    (Some(command), Some(approval), Some(kind)) => format!(
                        "Access decision command {command}, approval {approval}, choice {kind:?}"
                    ),
                    (None, None, None) => match &interaction.state {
                        InteractionState::Resolving {
                            owner_command_id, ..
                        } => format!(
                            "Access decision is being reconciled under command {owner_command_id}"
                        ),
                        _ => "No Access decision receipt is recorded".into(),
                    },
                    _ => return Err(AgentFailure::Conflict),
                };
                let effect_digest: String = context
                    .review
                    .effect_digest
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                marker.push_str(&format!(
                    "\nCalendar operation {}: review {}; effect digest {}; actor person {} device {}; policy revision {}; expires {}; interaction {interaction_state}; current Calendar Operations state {:?}. {decision}.",
                    context.operation_id,
                    context.review.id,
                    effect_digest,
                    context.review.person_id,
                    context.review.device_id,
                    context.review.policy_revision.map_or_else(|| "none".into(), |revision| revision.to_string()),
                    context.review.expires_at.to_rfc3339(),
                    context.status,
                ));
                marker.push_str(
                    " Use this existing operation ID and its owner state. Do not re-propose, recreate, or submit this Calendar write. Approved is not evidence of success; Unknown means execution is uncertain and must remain so until the owner reconciles it.",
                );
                continue;
            }
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
        Ok(marker)
    }
}

pub async fn continuation<Repository: ConversationRepository>(
    repository: &Repository,
    run_id: RunId,
    principal: &str,
    experts: &dyn floe_experts::ExpertsOwner,
    actor: &OwnerActor,
    scope: &ExecutionScope,
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
    let mut own_accounting = Vec::new();
    let mut delegated_accounting = Vec::new();
    let mut lineage_fold = ResumeLineageFold::default();
    let mut total_entries = 0_usize;
    let mut seen_exchanges = std::collections::HashSet::new();
    let mut replay_invocations = std::collections::HashSet::new();
    let mut replay_calls = std::collections::HashSet::new();
    let mut original_delegations = std::collections::BTreeMap::new();
    for receipt in chain.iter() {
        let mut entries = repository.load_journal(receipt.run_id).await?;
        crate::validate_run_journal(receipt, &entries)?;
        if receipt.principal != actor.person_id.to_string() || receipt.device_id != actor.device_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut pending = std::collections::BTreeMap::new();
        for entry in &entries {
            match &entry.event {
                floe_agent_contract::JournalEvent::DelegationIntent { request } => {
                    original_delegations
                        .entry(request.task_id)
                        .or_insert_with(|| request.clone());
                    pending.insert(request.task_id, request.clone());
                }
                floe_agent_contract::JournalEvent::DelegationResult { receipt } => {
                    pending.remove(&receipt.task_id);
                }
                _ => {}
            }
        }
        for request in pending.values() {
            let original = original_delegations
                .get(&request.task_id)
                .ok_or(AgentFailure::StorageUnavailable)?;
            let mut normalized = request.clone();
            normalized.parent_run_id = original.parent_run_id;
            if normalized != *original {
                return Err(AgentFailure::Conflict);
            }
            let mut recovered = experts.recover_delegation(actor, original, scope).await?;
            if request.parent_run_id != original.parent_run_id {
                recovered.replay = Some(
                    replay
                        .iter()
                        .find(|entry: &&floe_agent_contract::ReplayReceipt| {
                            entry.task_id == Some(request.task_id)
                        })
                        .cloned()
                        .ok_or(AgentFailure::Conflict)?,
                );
            }
            scope
                .run(repository.reconcile_delegation(receipt.run_id, recovered))
                .await?;
        }
        if !pending.is_empty() {
            entries = repository.load_journal(receipt.run_id).await?;
        }
        total_entries = total_entries
            .checked_add(entries.len())
            .ok_or(AgentFailure::StorageUnavailable)?;
        if total_entries > MAX_CONTINUATION_JOURNAL_ENTRIES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let projected = floe_agent_runtime::project_execution_journal(
            &super::recovery::journal_binding(receipt, &entries),
            &entries,
            floe_agent_runtime::JournalProjectionMode::ContinueSettledDelegation,
        )?;
        let projected_selection = projected.model_selection.clone();
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
        own_accounting.push(projected.own_accounting);
        delegated_accounting.extend(projected.delegated_receipts);
        // Cross-run resume lineage: a newer run supersedes an older pending
        // batch only after durably re-recording the exact batch and starting
        // cursor. A child that crashed before takeover leaves the parent
        // pending state authoritative.
        let live = projected
            .pending_batch
            .clone()
            .zip(projected.cursor.clone());
        lineage_fold = fold_resume_lineage(
            &lineage_fold,
            &projected.lineage,
            live,
            &projected_selection,
        )?;
    }
    let usage =
        floe_agent_runtime::aggregate_model_accounting(&own_accounting, &delegated_accounting)?
            .usage;
    let model_conversation = ModelConversation {
        history,
        current_turn,
    };
    if model_conversation.len() > floe_agent_contract::MAX_AGENT_MESSAGES || replay.len() > 128 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let (pending_batch, batch_cursor) = lineage_fold
        .pending
        .clone()
        .map_or((None, None), |(batch, cursor)| (Some(batch), Some(cursor)));
    let model_selection = if pending_batch.is_some() {
        lineage_fold.carried_selection
    } else {
        ModelSelectionState::Fresh
    };
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
        model_selection,
        completed_iterations,
        usage,
    })
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

/// Authenticate reuse of one immutable Task across a bounded continuation chain.
/// Storage loads all rows and journals from one transaction before calling this.
pub fn validate_task_delegation_lineage(
    chain: &[(RunReceipt, Vec<crate::JournalEntry>)],
    task: &floe_experts::TaskRecord,
) -> Result<(), AgentFailure> {
    use floe_agent_contract::{JournalEvent, delegation_request_digest};
    if chain.is_empty() || chain.len() > 4 {
        return Err(AgentFailure::Conflict);
    }
    task.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
    let first = &chain[0].0;
    if first.continuation_of.is_some() {
        return Err(AgentFailure::Conflict);
    }
    let mut seen = std::collections::HashSet::new();
    let mut carried = None;
    let mut original = None;
    let mut current_intent = false;
    for (index, (run, entries)) in chain.iter().enumerate() {
        run.validate()?;
        if !seen.insert(run.run_id)
            || run.principal != task.snapshot.principal
            || run.device_id != task.device_id
            || run.session_id != first.session_id
            || run.user_message_id != first.user_message_id
        {
            return Err(AgentFailure::Conflict);
        }
        if index > 0 {
            let parent = &chain[index - 1].0;
            let reference = parent.continuation().ok_or(AgentFailure::Conflict)?;
            if run.continuation_of != Some(parent.run_id)
                || run.continuation_executor_generation != Some(parent.executor_generation)
                || run.continuation_level != reference.level
            {
                return Err(AgentFailure::Conflict);
            }
        }
        let mode = if index + 1 < chain.len() {
            floe_agent_runtime::JournalProjectionMode::ContinueSettledDelegation
        } else {
            floe_agent_runtime::JournalProjectionMode::DurablePrefix
        };
        let projected = floe_agent_runtime::project_execution_journal(
            &super::recovery::journal_binding(run, entries),
            entries,
            mode,
        )?;
        let live = projected
            .pending_batch
            .clone()
            .zip(projected.cursor.clone());
        carried = reconcile_resume_lineage(carried, &projected.lineage, live)?;
        current_intent = false;
        for entry in entries {
            if let JournalEvent::DelegationIntent { request } = &entry.event {
                if request.task_id != task.snapshot.task_id {
                    continue;
                }
                let mut normalized = request.clone();
                normalized.parent_run_id = task.snapshot.parent_run_id;
                if request.principal != task.snapshot.principal
                    || request.selected_agent_id != task.snapshot.agent_id
                    || request.selected_definition_revision != task.snapshot.definition_revision
                    || request.execution_context.session_id != run.session_id
                    || request.execution_context.device_id != task.device_id
                    || request.invocation_key != task.invocation_key
                    || delegation_request_digest(&normalized) != task.request_digest
                {
                    return Err(AgentFailure::Conflict);
                }
                if task.snapshot.parent_run_id == Some(run.run_id.as_uuid()) {
                    if original.replace(normalized).is_some() {
                        return Err(AgentFailure::Conflict);
                    }
                } else if original.as_ref() != Some(&normalized) {
                    return Err(AgentFailure::Conflict);
                }
                current_intent = true;
            }
        }
    }
    if original.is_none() || !current_intent {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

#[cfg(test)]
mod model_selection_tests {
    use super::*;
    use crate::JournalLineage;
    use crate::application::recovery::reconcile_resume_selection;
    use floe_agent_contract::{
        BatchCursor, BudgetProvenance, CatalogMetadataStatus, ModelBindingDigest,
        ModelBudgetProfile, ModelCapabilities, ModelExecutionSelection, ModelSelectionCommitment,
        ModelTokenLimit, OperatorConfigurationStatus, ProcessingBoundary, TokenLimitSource,
        TokenLimitStatus, ValidatedModelBatch,
    };

    fn selection() -> ModelExecutionSelection {
        ModelExecutionSelection {
            commitment: ModelSelectionCommitment([1; 32]),
            principal: "00000000-0000-4000-8000-000000000001".into(),
            device_id: "device-1".into(),
            purpose: "everyday_assistance".into(),
            consumer: CONVERSATION_CONSUMER.into(),
            capabilities: ModelCapabilities::chat(),
            boundary: ProcessingBoundary::Device,
            binding_digest: ModelBindingDigest([2; 32]),
            budget_profile: ModelBudgetProfile::unknown(),
        }
    }

    fn configured_profile() -> ModelBudgetProfile {
        let mut profile = ModelBudgetProfile::unknown();
        profile.context_window = ModelTokenLimit {
            status: TokenLimitStatus::Known,
            tokens: Some(32_000),
            source: TokenLimitSource::OperatorConfiguration,
        };
        profile.max_output = ModelTokenLimit {
            status: TokenLimitStatus::Known,
            tokens: Some(4_096),
            source: TokenLimitSource::OperatorConfiguration,
        };
        profile.selected_output_reservation = ModelTokenLimit {
            status: TokenLimitStatus::Known,
            tokens: Some(2_048),
            source: TokenLimitSource::OperatorConfiguration,
        };
        profile.estimator.provider_overhead_tokens = Some(128);
        profile.estimator.safety_margin_tokens = Some(256);
        profile.sources.operator_configuration = OperatorConfigurationStatus::Configured;
        profile.sources.operator_configuration_version = Some(1);
        profile.sources.catalog.status = CatalogMetadataStatus::Available;
        profile.sources.catalog.revision = Some(1);
        profile.sources.catalog.context_window_tokens = Some(32_000);
        profile.sources.catalog.max_output_tokens = Some(4_096);
        profile.sources.catalog.provenance = Some(BudgetProvenance {
            source: "test fixture".into(),
            verified_at: "2026-10-08".into(),
        });
        profile.validate().expect("valid known budget profile");
        profile
    }

    #[test]
    fn resumed_selection_preserves_pin_and_fails_closed_on_every_selection_dimension() {
        let pinned = selection();
        assert_eq!(
            reconcile_resume_selection(
                &ModelSelectionState::Pinned(pinned.clone()),
                &ModelSelectionState::Pinned(pinned.clone()),
            ),
            Ok(ModelSelectionState::Pinned(pinned.clone()))
        );
        assert_eq!(
            reconcile_resume_selection(
                &ModelSelectionState::Pinned(pinned.clone()),
                &ModelSelectionState::Fresh
            ),
            Ok(ModelSelectionState::Pinned(pinned.clone())),
            "a pending batch child with no local ModelIntent inherits its parent's pin"
        );

        let mut changed = pinned.clone();
        changed.commitment = ModelSelectionCommitment([3; 32]);
        let mut variants = vec![changed];
        let mut changed = pinned.clone();
        changed.boundary = ProcessingBoundary::Gateway;
        variants.push(changed);
        let mut changed = pinned.clone();
        changed.binding_digest = ModelBindingDigest([4; 32]);
        variants.push(changed);
        let mut changed = pinned.clone();
        changed.budget_profile = configured_profile();
        variants.push(changed);
        for changed in variants {
            assert_eq!(
                reconcile_resume_selection(
                    &ModelSelectionState::Pinned(pinned.clone()),
                    &ModelSelectionState::Pinned(changed),
                ),
                Err(AgentFailure::StorageUnavailable)
            );
        }
    }

    #[test]
    fn unproven_historical_selection_never_becomes_a_dispatch_pin() {
        assert_eq!(
            reconcile_resume_selection(&ModelSelectionState::Unproven, &ModelSelectionState::Fresh,),
            Ok(ModelSelectionState::Unproven)
        );
        assert_eq!(
            reconcile_resume_selection(
                &ModelSelectionState::Unproven,
                &ModelSelectionState::Pinned(selection()),
            ),
            Err(AgentFailure::StorageUnavailable)
        );
    }

    #[test]
    fn pending_batch_takeover_restores_the_parent_selection() {
        let execution_id = uuid::Uuid::new_v4();
        let batch_id = uuid::Uuid::new_v4();
        let batch = ValidatedModelBatch {
            execution_id,
            attempt_id: uuid::Uuid::new_v4(),
            projection_ref: floe_agent_contract::ProjectionRef::new(),
            batch_id,
            steps: vec![floe_agent_contract::ModelStep::Answer {
                text: "persisted answer".into(),
                artifacts: vec![],
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
            delegation_context: None,
        };
        let cursor = BatchCursor {
            batch_id,
            next_step_index: 0,
        };
        let parent_state = Some((batch.clone(), cursor.clone()));
        let child_batch_only = JournalLineage::ResumeBatchOnly {
            batch: batch.clone(),
        };
        assert_eq!(
            reconcile_resume_lineage(parent_state.clone(), &child_batch_only, None),
            Ok(parent_state.clone()),
            "the parent remains authoritative until the child acknowledges its cursor"
        );
        let child_claimed = JournalLineage::ResumeClaimed {
            batch: batch.clone(),
            cursor: cursor.clone(),
        };
        assert_eq!(
            reconcile_resume_lineage(
                parent_state.clone(),
                &child_claimed,
                Some((batch.clone(), cursor.clone())),
            ),
            Ok(parent_state)
        );
        let pinned = selection();
        assert_eq!(
            reconcile_resume_selection(
                &ModelSelectionState::Pinned(pinned.clone()),
                &ModelSelectionState::Fresh
            ),
            Ok(ModelSelectionState::Pinned(pinned)),
            "the child has no local ModelIntent, so it inherits the pending execution pin"
        );
    }

    #[test]
    fn shared_lineage_fold_carries_pin_across_unclaimed_runs_and_resets_after_claim() {
        let pinned = selection();
        let execution_id = uuid::Uuid::new_v4();
        let batch = ValidatedModelBatch {
            execution_id,
            attempt_id: uuid::Uuid::new_v4(),
            projection_ref: floe_agent_contract::ProjectionRef::new(),
            batch_id: uuid::Uuid::new_v4(),
            steps: vec![floe_agent_contract::ModelStep::Answer {
                text: "persisted answer".into(),
                artifacts: vec![],
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
            delegation_context: None,
        };
        let cursor = BatchCursor {
            batch_id: batch.batch_id,
            next_step_index: 0,
        };
        let ancestor = fold_resume_lineage(
            &ResumeLineageFold::default(),
            &JournalLineage::Fresh,
            Some((batch.clone(), cursor.clone())),
            &ModelSelectionState::Pinned(pinned.clone()),
        )
        .expect("fold pinned ancestor with pending batch");
        assert_eq!(
            ancestor.carried_selection,
            ModelSelectionState::Pinned(pinned.clone())
        );

        for middle_lineage in [
            JournalLineage::Empty,
            JournalLineage::ResumeBatchOnly {
                batch: batch.clone(),
            },
        ] {
            let middle = fold_resume_lineage(
                &ancestor,
                &middle_lineage,
                None,
                &ModelSelectionState::Fresh,
            )
            .expect("unclaimed intermediate preserves parent carry");
            assert_eq!(middle.pending, Some((batch.clone(), cursor.clone())));
            assert_eq!(
                middle.carried_selection,
                ModelSelectionState::Pinned(pinned.clone())
            );

            let claimed = JournalLineage::ResumeClaimed {
                batch: batch.clone(),
                cursor: cursor.clone(),
            };
            let current = fold_resume_lineage(&middle, &claimed, None, &ModelSelectionState::Fresh)
                .expect("claimed batch finishes while current execution keeps its pin");
            assert_eq!(current.pending, None);
            assert_eq!(
                current.execution_selection,
                ModelSelectionState::Pinned(pinned.clone())
            );
            assert_eq!(current.carried_selection, ModelSelectionState::Fresh);

            let next = fold_resume_lineage(
                &current,
                &JournalLineage::Empty,
                None,
                &ModelSelectionState::Fresh,
            )
            .expect("later Run starts fresh after carry is consumed");
            assert_eq!(next.execution_selection, ModelSelectionState::Fresh);
            assert_eq!(next.carried_selection, ModelSelectionState::Fresh);

            let mut changed = pinned.clone();
            changed.commitment = ModelSelectionCommitment([9; 32]);
            assert_eq!(
                fold_resume_lineage(
                    &middle,
                    &claimed,
                    None,
                    &ModelSelectionState::Pinned(changed),
                ),
                Err(AgentFailure::StorageUnavailable),
                "a claimed child cannot switch selection before finishing the batch"
            );
        }
    }
}
