use std::sync::Arc;

use floe_agent_contract::{
    AgentMessage, DependencyCoverage, EngineRequest, EngineStep, MessageRole, input_digest,
};
use floe_agent_runtime::{Engine, EnginePorts};
use floe_execution::{ExecutionScope, budget::BudgetLedger};
use floe_kernel::{AgentFailure, RunId, TraceContext};

use crate::{
    ContinuationSnapshot, ConversationPorts, ConversationRepository, ManagerConfig,
    RecoveryReceipt, RecoveryRequest, RunReceipt, RunState, RunTerminal, TurnAdmission,
    TurnAdmissionRequest, TurnRequest,
};

use super::recovery::project_continuation;

pub struct ConversationService<Repository> {
    repository: Arc<Repository>,
    engine: Engine,
    config: ManagerConfig,
}

impl<Repository: ConversationRepository> ConversationService<Repository> {
    pub fn new(repository: Arc<Repository>, config: ManagerConfig) -> Result<Self, AgentFailure> {
        config.validate()?;
        Ok(Self {
            repository,
            engine: Engine::default(),
            config,
        })
    }

    pub async fn run_turn(
        &self,
        request: TurnRequest,
        ports: ConversationPorts<'_>,
    ) -> Result<RunReceipt, AgentFailure> {
        request.validate()?;
        let request_digest = turn_digest(&request);
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
                user_message: AgentMessage {
                    message_id: request.command_id.as_uuid(),
                    role: MessageRole::User,
                    text: request.prompt.clone(),
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
            || admitted.receipt.state != RunState::Working
            || admitted.transcript.last().is_none_or(|message| {
                message.message_id != request.command_id.as_uuid()
                    || message.role != MessageRole::User
            })
        {
            return Err(AgentFailure::StorageUnavailable);
        }
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
        let ledger = BudgetLedger::new(self.config.budget, Default::default());
        let scope = ExecutionScope::root(
            request.cancellation,
            request.deadline,
            ledger.work_lease(),
            TraceContext::new(request.command_id.as_uuid()).with_run_id(run_id),
        );
        let base_coverage = request.bounded_context.coverage.clone();
        let engine_request = EngineRequest {
            principal: request.principal,
            role_spec: self.config.role_spec.clone(),
            prompt: request.prompt,
            scope,
            bounded_context: request.bounded_context,
            messages: admitted.transcript,
            allowed_catalog: request.allowed_catalog,
            max_iterations: self.config.max_iterations,
            max_output_bytes: self.config.max_output_bytes,
            replay: request.replay,
        };
        let result = self
            .engine
            .drive(
                engine_request,
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
                None => RunTerminal::from_failure(AgentFailure::Stalled),
            },
            Err(failure) => RunTerminal::from_failure(failure),
        };
        self.repository
            .finish_run(run_id, expected_aggregate_revision, terminal)
            .await
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
    let entries = repository.load_journal(run_id).await?;
    project_continuation(&admitted, &entries)
}

fn turn_digest(request: &TurnRequest) -> [u8; 32] {
    input_digest(&format!(
        "{}\0{}\0{}\0{}\0{}\0{:?}",
        request.command_id,
        request.session_id,
        request.expected_session_revision,
        request.principal,
        request.prompt,
        request.request_context_digest
    ))
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
