use std::time::Duration;

use floe_agent_contract::{
    AllowedCatalog, DependencyCoverage, EngineRequest, EngineStep, ModelConversation,
    ModelConversationEntry, RoleSpec, UserInteractionRef, UserInteractionStatus,
};
use floe_agent_runtime::{Engine, EngineOutcome, EnginePorts, EngineReport};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, RunId};

use crate::{
    ConversationPorts, ConversationRepository, FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_ID,
    FINALIZATION_ROLE_PROMPT, InteractionOrigin, InteractionRepository, MODEL_CONSENT_LIMITATION,
    PublishAdmission, PublishModelRequirement, RunState, RunTerminal, TurnRequest,
};

use super::interactions::{publish_model_requirement, rescope_blocked_requirement};
use super::recovery::project_active_journal;

const MAX_FINALIZATION_DURATION: Duration = Duration::from_secs(10);

pub(super) enum FinalizationOutcome {
    Replied(RunTerminal),
    NotAttempted(AgentFailure),
    AttemptedWithoutReply,
}

pub(super) async fn finalize_exhausted_run<
    Repository: ConversationRepository + InteractionRepository,
>(
    engine: &Engine,
    repository: &Repository,
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
    let mut current_turn = Vec::with_capacity(usable.len() + 1);
    current_turn.push(user_message);
    current_turn.extend(usable.clone());
    let request = EngineRequest {
        principal: work_request.principal.clone(),
        role_spec: RoleSpec {
            role_id: FINALIZATION_ROLE_ID.into(),
            instructions: FINALIZATION_ROLE_PROMPT.into(),
            output_contract: FINALIZATION_OUTPUT_CONTRACT.into(),
        },
        scope,
        conversation: ModelConversation {
            history: Vec::new(),
            current_turn,
        },
        allowed_catalog: AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: work_request.allowed_catalog.revision.max(1),
        },
        purpose: work_request.purpose.clone(),
        consumer: work_request.consumer.clone(),
        preferred_profile_id: work_request.preferred_profile_id.clone(),
        max_iterations: 1,
        max_output_bytes: work_request.max_output_bytes,
        replay,
        resume: None,
        // Finalization never delegates: its catalog carries no cards, so no
        // execution context is required.
        delegation_context: None,
        lineage: work_request.lineage,
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
        .await;
    let Ok(outcome) = outcome else {
        return Ok(FinalizationOutcome::AttemptedWithoutReply);
    };
    match outcome {
        // A blocked finalization dispatch: publish the durable card under
        // the exact attempted origin and complete with the deterministic
        // limitation. The exhaustion issue is superseded: no output was
        // produced, and the fresh review unblocks a linked resume.
        EngineOutcome::Blocked(blocked) => {
            // A linked resume dispatches under origin-carried lineage; the
            // fresh review re-scopes to this attempting Run before
            // publication.
            let Ok(requirement) =
                rescope_blocked_requirement(blocked.requirement, turn.session_id, run_id)
            else {
                return Ok(FinalizationOutcome::AttemptedWithoutReply);
            };
            let published = match publish_model_requirement(
                repository,
                repository,
                PublishModelRequirement {
                    principal: turn.principal.clone(),
                    session_id: turn.session_id,
                    origin_run_id: run_id,
                    origin: InteractionOrigin::Model {
                        attempt_id: blocked.attempt_id,
                    },
                    requirement,
                    device_id: turn.device_id.clone(),
                },
                turn.now_unix_ms,
            )
            .await
            {
                Ok(PublishAdmission::Created(record)) => record,
                Ok(PublishAdmission::Existing(record)) => record,
                Err(_) => return Ok(FinalizationOutcome::AttemptedWithoutReply),
            };
            Ok(FinalizationOutcome::Replied(RunTerminal {
                state: RunState::Completed,
                output: Some(MODEL_CONSENT_LIMITATION.into()),
                steps: vec![EngineStep::Answer {
                    text: MODEL_CONSENT_LIMITATION.into(),
                    artifacts: vec![],
                }],
                coverage: DependencyCoverage::Independent,
                issue: None,
                interactions: vec![UserInteractionRef {
                    interaction_id: published.id,
                    kind: published.kind,
                    status: UserInteractionStatus::Pending,
                }],
            }))
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
            let coverage = finalization_coverage(&usable, answering_projection_coverage, &steps)?;
            Ok(FinalizationOutcome::Replied(RunTerminal {
                state: RunState::Failed,
                output: Some(output),
                steps,
                coverage,
                issue: Some(issue),
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

#[cfg(test)]
mod tests {
    use floe_agent_contract::{
        DelegationRequest, InvocationKey, OutcomeIssue, TaskReceipt, TaskSnapshot, TaskState,
        ToolCall, ToolResult,
    };
    use uuid::Uuid;

    use super::*;

    fn soft_tool_exchange() -> ModelConversationEntry {
        let call_id = Uuid::new_v4();
        ModelConversationEntry::ToolExchange {
            call: ToolCall {
                call_id,
                invocation_key: InvocationKey::new(),
                tool_id: "missing.tool".into(),
                definition_revision: 1,
                input: "{}".into(),
            },
            result: ToolResult {
                call_id,
                text: "tool is not registered".into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: Some(OutcomeIssue {
                    failure: AgentFailure::InvalidModelOutput,
                    retryable: true,
                }),
            },
        }
    }

    fn soft_delegation_exchange() -> ModelConversationEntry {
        let task_id = floe_agent_contract::TaskId::new();
        ModelConversationEntry::DelegationExchange {
            request: DelegationRequest {
                task_id,
                parent_run_id: Some(Uuid::new_v4()),
                principal: "person:test".into(),
                invocation_key: InvocationKey::new(),
                selected_agent_id: "missing-expert".into(),
                selected_definition_revision: 1,
                message: "summarize".into(),
                context_refs: vec![],
                execution_context: floe_agent_contract::DelegationExecutionContext {
                    session_id: Uuid::new_v4(),
                    device_id: "test-device".into(),
                    agent_context: floe_agent_contract::AgentContext {
                        projection_version: 1,
                        persona: None,
                        memories: vec![],
                        optional_context_issues: vec![],
                        evidence: vec![],
                    },
                    max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
                },
            },
            receipt: TaskReceipt {
                task_id,
                snapshot: TaskSnapshot {
                    task_id,
                    parent_run_id: Some(Uuid::new_v4()),
                    principal: "person:test".into(),
                    agent_id: "missing-expert".into(),
                    definition_revision: 1,
                    state: TaskState::Rejected,
                    result: None,
                    artifacts: vec![],
                    coverage: DependencyCoverage::Independent,
                    issue: Some(AgentFailure::InvalidModelOutput),
                },
                replay: None,
            },
        }
    }

    #[test]
    fn soft_observations_are_neither_usable_nor_barriers() {
        // Host soft failures are never presented as observations, and a
        // model-side error never vetoes a reply built from usable ones.
        for entry in [soft_tool_exchange(), soft_delegation_exchange()] {
            assert!(!usable_exchange(&entry));
            assert_eq!(exchange_barrier(&entry), None);
        }
    }
}
