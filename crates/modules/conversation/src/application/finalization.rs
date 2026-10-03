use std::time::Duration;

use floe_agent_contract::{
    AllowedCatalog, DependencyCoverage, EngineRequest, EngineStep, ModelConversation,
    ModelConversationEntry, RoleSpec,
};
use floe_agent_runtime::{Engine, EngineOutcome, EnginePorts, EngineReport};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, RunId};

use crate::{
    ConversationPorts, ConversationRepository, FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_ID,
    FINALIZATION_ROLE_PROMPT, InteractionRepository, RunState, RunTerminal, TurnRequest,
};

use super::recovery::project_active_journal;

const MAX_FINALIZATION_DURATION: Duration = Duration::from_secs(10);

pub(super) enum FinalizationOutcome {
    Replied(RunTerminal),
    Blocked(crate::BlockedRunCommit),
    NotAttempted(AgentFailure),
    AttemptedWithoutReply,
}

pub(super) async fn finalize_exhausted_run<
    Repository: ConversationRepository + InteractionRepository,
>(
    engine: &Engine,
    repository: &Repository,
    connections: &floe_connections::ConnectionsService,
    experts: &dyn floe_experts::ExpertsOwner,
    actor: &floe_kernel::OwnerActor,
    run_id: RunId,
    root_scope: &ExecutionScope,
    work_request: &EngineRequest,
    ports: ConversationPorts<'_>,
    issue: AgentFailure,
    turn: &TurnRequest,
) -> Result<FinalizationOutcome, AgentFailure> {
    if !matches!(issue, AgentFailure::BudgetExceeded | AgentFailure::Stalled) {
        return Ok(FinalizationOutcome::NotAttempted(issue));
    }
    let receipt = repository
        .load_receipt(run_id)
        .await?
        .ok_or(AgentFailure::StorageUnavailable)?;
    let projected = match project_active_journal(&receipt, &repository.load_journal(run_id).await?)
    {
        Ok(projected) => projected,
        Err(AgentFailure::Interrupted) => return Ok(FinalizationOutcome::NotAttempted(issue)),
        Err(failure) => return Err(failure),
    };
    let Some(user_message) = work_request
        .conversation
        .current_turn
        .iter()
        .find(|entry| matches!(entry, ModelConversationEntry::User { .. }))
        .cloned()
    else {
        return Ok(FinalizationOutcome::NotAttempted(issue));
    };
    let exchanges = projected.model_conversation.current_turn;
    if let Some(barrier) = exchanges.iter().find_map(exchange_barrier) {
        return Ok(FinalizationOutcome::NotAttempted(barrier));
    }
    let usable = exchanges
        .into_iter()
        .filter(usable_exchange)
        .collect::<Vec<_>>();
    if usable.is_empty() {
        return Ok(FinalizationOutcome::NotAttempted(issue));
    }
    let retained_start = usable
        .len()
        .saturating_sub(floe_agent_contract::MAX_AGENT_MESSAGES - 1);
    let usable = usable.into_iter().skip(retained_start).collect::<Vec<_>>();
    let usable_ids = usable
        .iter()
        .filter_map(exchange_call_id)
        .collect::<std::collections::HashSet<_>>();
    let replay = projected
        .replay
        .into_iter()
        .filter(|receipt| usable_ids.contains(&receipt.call_id))
        .collect::<Vec<_>>();
    let scope = match root_scope.finalization_scope(MAX_FINALIZATION_DURATION) {
        Ok(scope) => scope,
        Err(AgentFailure::BudgetExceeded) => {
            return Ok(FinalizationOutcome::NotAttempted(issue));
        }
        Err(
            failure @ (AgentFailure::Cancelled
            | AgentFailure::DeadlineExceeded
            | AgentFailure::Interrupted),
        ) => {
            return Ok(FinalizationOutcome::NotAttempted(failure));
        }
        Err(failure) => return Err(failure),
    };
    let Some(prior_execution_id) = projected.execution_id else {
        return Ok(FinalizationOutcome::NotAttempted(issue));
    };
    let acknowledgment = root_scope
        .run(repository.journal(run_id)?.checkpoint(
            floe_agent_contract::JournalEvent::FinalizationStarted {
                prior_execution_id,
                abandoned_cursor: projected.cursor.clone(),
                prior_exhaustion: issue,
            },
        ))
        .await?;
    if !matches!(
        acknowledgment,
        floe_agent_contract::JournalAck::Accepted { .. }
    ) {
        return Err(AgentFailure::Conflict);
    }
    let mut current_turn = Vec::with_capacity(usable.len() + 1);
    current_turn.push(user_message);
    current_turn.extend(usable.clone());
    let request = EngineRequest {
        execution_id: uuid::Uuid::new_v5(&run_id.as_uuid(), b"floe.conversation.finalization"),
        principal: work_request.principal.clone(),
        device_id: work_request.device_id.clone(),
        role_spec: RoleSpec {
            role_id: FINALIZATION_ROLE_ID.into(),
            instructions: FINALIZATION_ROLE_PROMPT.into(),
            output_contract: FINALIZATION_OUTPUT_CONTRACT.into(),
            output_format: floe_agent_contract::ModelOutputFormat::Text,
        },
        scope,
        conversation: ModelConversation {
            history: Vec::new(),
            current_turn,
        },
        allowed_catalog: AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: work_request.allowed_catalog.revision,
        },
        purpose: work_request.purpose.clone(),
        consumer: work_request.consumer.clone(),
        max_iterations: 1,
        max_output_bytes: work_request.max_output_bytes,
        replay,
        resume: None,
        // Finalization never delegates: its catalog carries no cards, so no
        // execution context is required.
        delegation_context: None,
    };
    let outcome = engine
        .drive(
            request,
            EnginePorts {
                projection: ports.projection,
                model: ports.model,
                tools: ports.tools,
                delegation: ports.delegation,
                journal: repository.journal(run_id)?.as_ref(),
                validator: ports.validator,
            },
        )
        .await?;
    match outcome {
        EngineOutcome::Blocked(blocked) => {
            let prior = match issue {
                AgentFailure::BudgetExceeded => crate::PriorExhaustion::BudgetExceeded,
                _ => crate::PriorExhaustion::Stalled,
            };
            let commit = super::source_review::build_blocked_run_commit(
                repository,
                connections,
                experts,
                actor,
                run_id,
                blocked,
                Some(prior),
                turn.now_unix_ms,
                root_scope,
            )
            .await?;
            Ok(FinalizationOutcome::Blocked(commit))
        }
        EngineOutcome::Completed(report) => {
            let EngineReport {
                steps,
                output,
                answering_projection_coverage,
                ..
            } = report;
            let Some(output) = output else {
                return Ok(FinalizationOutcome::AttemptedWithoutReply);
            };
            let mut all_steps =
                super::source_review::settled_steps(&repository.load_journal(run_id).await?)?;
            for step in steps {
                if !all_steps.contains(&step) {
                    all_steps.push(step);
                }
            }
            let steps = all_steps;
            let coverage = finalization_coverage(&usable, answering_projection_coverage, &steps)?;
            Ok(FinalizationOutcome::Replied(RunTerminal {
                state: RunState::Failed,
                output: Some(output),
                steps,
                coverage,
                issue: Some(issue),
                blocked: None,
                interactions: vec![],
            }))
        }
    }
}

fn exchange_call_id(entry: &ModelConversationEntry) -> Option<uuid::Uuid> {
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

fn usable_exchange(entry: &ModelConversationEntry) -> bool {
    match entry {
        ModelConversationEntry::ToolExchange { result, .. } => {
            result.coverage == DependencyCoverage::Independent
                && result.issue.is_none()
                && result
                    .artifacts
                    .iter()
                    .all(|artifact| artifact.coverage == DependencyCoverage::Independent)
        }
        ModelConversationEntry::DelegationExchange { receipt, .. } => {
            receipt.snapshot.coverage == DependencyCoverage::Independent
                && receipt.snapshot.issue.is_none()
                && receipt
                    .snapshot
                    .artifacts
                    .iter()
                    .all(|artifact| artifact.coverage == DependencyCoverage::Independent)
        }
        ModelConversationEntry::User { .. }
        | ModelConversationEntry::Preamble { .. }
        | ModelConversationEntry::Assistant { .. } => false,
    }
}

fn exchange_barrier(entry: &ModelConversationEntry) -> Option<AgentFailure> {
    let failure = match entry {
        ModelConversationEntry::ToolExchange { result, .. } => {
            result.issue.as_ref().map(|issue| issue.failure)
        }
        ModelConversationEntry::DelegationExchange { receipt, .. } => receipt.snapshot.issue,
        ModelConversationEntry::User { .. }
        | ModelConversationEntry::Preamble { .. }
        | ModelConversationEntry::Assistant { .. } => None,
    }?;
    Some(failure).filter(|failure| {
        matches!(
            failure,
            AgentFailure::ConsentRequired
                | AgentFailure::PolicyDenied
                | AgentFailure::CapabilityDenied
                | AgentFailure::CapabilityUnavailable
                | AgentFailure::VaultUnavailable
                | AgentFailure::VaultLocked
                | AgentFailure::StorageUnavailable
                | AgentFailure::Cancelled
                | AgentFailure::DeadlineExceeded
                | AgentFailure::Interrupted
        )
    })
}

fn finalization_coverage(
    exchanges: &[ModelConversationEntry],
    answering: Option<DependencyCoverage>,
    steps: &[EngineStep],
) -> Result<DependencyCoverage, AgentFailure> {
    // Same base as a completed run: the coverage of the projection the
    // answering model saw. Finalization only shows Independent observations,
    // so this is Independent in practice, but the rule stays explicit.
    let mut coverage = answering.ok_or(AgentFailure::InvalidModelOutput)?;
    for entry in exchanges {
        let (exchange_coverage, artifacts) = match entry {
            ModelConversationEntry::ToolExchange { result, .. } => {
                (&result.coverage, result.artifacts.as_slice())
            }
            ModelConversationEntry::DelegationExchange { receipt, .. } => (
                &receipt.snapshot.coverage,
                receipt.snapshot.artifacts.as_slice(),
            ),
            ModelConversationEntry::User { .. }
            | ModelConversationEntry::Preamble { .. }
            | ModelConversationEntry::Assistant { .. } => continue,
        };
        coverage = coverage
            .merge(exchange_coverage)
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
        for artifact in artifacts {
            coverage = coverage
                .merge(&artifact.coverage)
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
        }
    }
    for step in steps {
        if let EngineStep::Answer { artifacts, .. } = step {
            for artifact in artifacts {
                coverage = coverage
                    .merge(&artifact.coverage)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?;
            }
        }
    }
    Ok(coverage)
}
