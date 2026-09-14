use std::time::Duration;

use floe_agent_contract::{
    AgentMessage, AllowedCatalog, BoundedContext, DependencyCoverage, EngineRequest, EngineStep,
    MessageRole, ReplayReceipt, RoleSpec,
};
use floe_agent_runtime::{Engine, EnginePorts};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, RunId};

use crate::{
    ConversationPorts, ConversationRepository, FINALIZATION_OUTPUT_CONTRACT,
    FINALIZATION_ROLE_PROMPT, RunState, RunTerminal,
};

use super::recovery::project_active_journal;

const MAX_FINALIZATION_DURATION: Duration = Duration::from_secs(10);

pub(super) enum FinalizationOutcome {
    Replied(RunTerminal),
    NotAttempted(AgentFailure),
    AttemptedWithoutReply,
}

pub(super) async fn finalize_exhausted_run<Repository: ConversationRepository>(
    engine: &Engine,
    repository: &Repository,
    run_id: RunId,
    root_scope: &ExecutionScope,
    work_request: &EngineRequest,
    ports: ConversationPorts<'_>,
    issue: AgentFailure,
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
    if projected.messages.len() != projected.replay.len() {
        return Err(AgentFailure::StorageUnavailable);
    }
    let Some(user_message) = work_request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User && message.text == work_request.prompt)
        .cloned()
    else {
        return Ok(FinalizationOutcome::NotAttempted(issue));
    };
    if let Some(barrier) = projected.replay.iter().find_map(finalization_barrier) {
        return Ok(FinalizationOutcome::NotAttempted(barrier));
    }
    let usable = projected
        .messages
        .into_iter()
        .zip(projected.replay)
        .filter(|(message, receipt)| usable_observation(message, receipt))
        .collect::<Vec<_>>();
    if usable.is_empty() {
        return Ok(FinalizationOutcome::NotAttempted(issue));
    }
    let retained_start = usable
        .len()
        .saturating_sub(floe_agent_contract::MAX_AGENT_MESSAGES - 1);
    let mut messages = vec![user_message];
    let mut replay = Vec::with_capacity(usable.len() - retained_start);
    for (message, receipt) in usable.into_iter().skip(retained_start) {
        messages.push(message);
        replay.push(receipt);
    }
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
    let request = EngineRequest {
        principal: work_request.principal.clone(),
        role_spec: RoleSpec {
            role_id: "manager".into(),
            prompt: FINALIZATION_ROLE_PROMPT.into(),
            output_contract: FINALIZATION_OUTPUT_CONTRACT.into(),
        },
        prompt: work_request.prompt.clone(),
        scope,
        bounded_context: BoundedContext {
            text: String::new(),
            coverage: DependencyCoverage::Independent,
        },
        messages,
        allowed_catalog: AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: work_request.allowed_catalog.revision.max(1),
        },
        max_iterations: 1,
        max_output_bytes: work_request.max_output_bytes,
        replay: replay.clone(),
    };
    let report = engine
        .drive(
            request,
            EnginePorts {
                model: ports.model,
                tools: ports.tools,
                delegation: ports.delegation,
                journal: repository.journal(run_id)?.as_ref(),
                validator: ports.validator,
            },
        )
        .await;
    let Ok(report) = report else {
        return Ok(FinalizationOutcome::AttemptedWithoutReply);
    };
    let Some(output) = report.output else {
        return Ok(FinalizationOutcome::AttemptedWithoutReply);
    };
    let coverage = finalization_coverage(&replay, &report.steps)?;
    Ok(FinalizationOutcome::Replied(RunTerminal {
        state: RunState::Failed,
        output: Some(output),
        steps: report.steps,
        coverage,
        issue: Some(issue),
    }))
}

fn usable_observation(message: &AgentMessage, receipt: &ReplayReceipt) -> bool {
    message.coverage == DependencyCoverage::Independent
        && receipt.tool_issue.is_none()
        && receipt.task_issue.is_none()
        && receipt
            .tool_artifacts
            .iter()
            .chain(&receipt.task_artifacts)
            .all(|artifact| artifact.coverage == DependencyCoverage::Independent)
        && (receipt.tool_coverage == DependencyCoverage::Independent
            || receipt.task_coverage == DependencyCoverage::Independent)
}

fn finalization_barrier(receipt: &ReplayReceipt) -> Option<AgentFailure> {
    receipt.tool_issue.or(receipt.task_issue).filter(|failure| {
        matches!(
            failure,
            AgentFailure::ConsentRequired
                | AgentFailure::PolicyDenied
                | AgentFailure::CapabilityDenied
                | AgentFailure::CapabilityUnavailable
                | AgentFailure::VaultUnavailable
                | AgentFailure::StorageUnavailable
                | AgentFailure::Cancelled
                | AgentFailure::DeadlineExceeded
                | AgentFailure::Interrupted
        )
    })
}

fn finalization_coverage(
    replay: &[ReplayReceipt],
    steps: &[EngineStep],
) -> Result<DependencyCoverage, AgentFailure> {
    let mut coverage = DependencyCoverage::Independent;
    for receipt in replay {
        coverage = coverage
            .merge(if receipt.tool_id.is_some() {
                &receipt.tool_coverage
            } else {
                &receipt.task_coverage
            })
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
        for artifact in receipt.tool_artifacts.iter().chain(&receipt.task_artifacts) {
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
