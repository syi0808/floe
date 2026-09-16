use std::sync::Arc;

use floe_agent_contract::{
    AgentMessage, DependencyCoverage, EngineRequest, EngineStep, MessageRole,
};
use floe_agent_runtime::{Engine, EnginePorts};
use floe_execution::{ExecutionScope, budget::BudgetLedger};
use floe_kernel::{AgentFailure, RunId, TraceContext};

use crate::{CancelRunRequest, CancelRunStatus, CommandQuery, CompactionReceipt, CompactionRequest, ContinuationSnapshot, ConversationPorts, ConversationRepository, ManagerConfig, RecoveryReceipt, RecoveryRequest, RunCancellationRegistry, RunQuery, RunReceipt, RunState, RunTerminal, TurnAdmission, TurnAdmissionRequest, TurnMode, TurnRequest};

use super::finalization::{FinalizationOutcome, finalize_exhausted_run};
use super::recovery::project_journal;

pub struct ConversationService<Repository> {
    repository: Arc<Repository>,
    run_cancellations: Arc<RunCancellationRegistry>,
    engine: Engine,
    config: ManagerConfig,
}

impl<Repository: ConversationRepository> ConversationService<Repository> {
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
            TurnMode::New => None,
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
                Some(snapshot)
            }
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
                run_id,
                command_id: request.command_id,
                session_id: request.session_id,
                expected_session_revision: request.expected_session_revision,
                principal: request.principal.clone(),
                request_digest,
                mode: request.mode.clone(),
                retry_of: request.retry_of,
                execution_profile: request.execution_profile.clone(),
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
        };
        admitted.validate()?;
        if admitted.receipt.run_id != run_id
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
        let ledger = BudgetLedger::new(self.config.budget, prior_usage);
        let scope = ExecutionScope::root(
            request.cancellation,
            request.deadline,
            ledger.work_lease(),
            TraceContext::new(request.command_id.as_uuid()).with_run_id(run_id),
        );
        let base_coverage = request.bounded_context.coverage.clone();
        let (messages, mut continuation_replay) = continuation
            .map(|snapshot| (snapshot.messages, snapshot.replay))
            .unwrap_or_else(|| (admitted.transcript, Vec::new()));
        continuation_replay.extend(request.replay);
        let engine_request = EngineRequest {
            principal: request.principal,
            role_spec: self.config.role_spec.clone(),
            prompt: intent.text,
            scope,
            bounded_context: request.bounded_context,
            messages,
            allowed_catalog: request.allowed_catalog,
            max_iterations: self.config.max_iterations - completed_iterations,
            max_output_bytes: self.config.max_output_bytes,
            replay: continuation_replay,
        };
        let result = self
            .engine
            .drive(
                engine_request.clone(),
                EnginePorts {
                    model: ports.model,
                    tools: ports.tools,
                    delegation: ports.delegation,
                    journal: journal.as_ref(),
                    validator: ports.validator,
                },
            )
            .await;
        let terminal = match result {
            Ok(report) => match report.output {
                Some(output) => match report_coverage(base_coverage, &report.steps) {
                    Ok(coverage) => RunTerminal {
                        state: RunState::Completed,
                        output: Some(output),
                        steps: report.steps,
                        coverage,
                        issue: None,
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
                    )
                    .await
                }
            },
            Err(failure @ (AgentFailure::BudgetExceeded | AgentFailure::Stalled)) => {
                self.finalize_exhaustion(
                    run_id,
                    &engine_request.scope,
                    &engine_request,
                    ports,
                    failure,
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
        run_id: RunId,
        scope: &ExecutionScope,
        request: &EngineRequest,
        ports: ConversationPorts<'_>,
        issue: AgentFailure,
    ) -> RunTerminal {
        match finalize_exhausted_run(
            &self.engine,
            self.repository.as_ref(),
            run_id,
            scope,
            request,
            ports,
            issue,
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
            || parent.execution_profile != current.execution_profile
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

    let mut messages = admitted.transcript;
    let mut replay = Vec::new();
    let mut completed_iterations = 0_u32;
    let mut usage = floe_execution::budget::ModelUsage::default();
    let mut total_entries = 0_usize;
    let mut message_ids = messages
        .iter()
        .map(|message| message.message_id)
        .collect::<std::collections::HashSet<_>>();
    let mut replay_invocations = std::collections::HashSet::new();
    let mut replay_calls = std::collections::HashSet::new();
    for receipt in chain {
        let entries = repository.load_journal(receipt.run_id).await?;
        total_entries = total_entries
            .checked_add(entries.len())
            .ok_or(AgentFailure::StorageUnavailable)?;
        if total_entries > 512 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let projected = project_journal(&receipt, &entries)?;
        if projected
            .messages
            .iter()
            .any(|message| !message_ids.insert(message.message_id))
            || projected.replay.iter().any(|receipt| {
                !replay_invocations.insert(receipt.invocation_key)
                    || !replay_calls.insert(receipt.call_id)
            })
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        messages.extend(projected.messages);
        replay.extend(projected.replay);
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
    }
    if messages.len() > floe_agent_contract::MAX_AGENT_MESSAGES || replay.len() > 128 {
        return Err(AgentFailure::BudgetExceeded);
    }
    messages.iter().try_for_each(AgentMessage::validate)?;
    Ok(ContinuationSnapshot {
        reference: current.continuation().ok_or(AgentFailure::Conflict)?,
        session_id: current.session_id,
        session_revision: current.session_revision,
        execution_profile: current.execution_profile,
        messages,
        replay,
        completed_iterations,
        usage,
    })
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

fn report_coverage(
    mut coverage: DependencyCoverage,
    steps: &[EngineStep],
) -> Result<DependencyCoverage, AgentFailure> {
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
