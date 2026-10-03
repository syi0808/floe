use crate::{AgentMessage, RunReceipt, RunTerminal, SessionReceipt};
use floe_agent_contract::{
    AgentFailure, AgentMessage as ContractMessage, Artifact as ContractArtifact,
    ArtifactPart as ContractArtifactPart, DependencyCoverage, EngineStep, MessageRole, RunId,
};
use uuid::Uuid;

pub fn project_run_receipt(record: crate::RunRecord) -> Result<RunReceipt, AgentFailure> {
    let receipt = RunReceipt {
        expert_environment: record.expert_environment,
        run_id: record.run_id,
        command_id: record.command_id,
        session_id: record.session_id,
        principal: record.person_id.to_string(),
        device_id: record.device_id,
        user_message_id: record.user_message_id,
        request_digest: record.request_digest,
        state: record.state,
        output: record.output,
        coverage: record.coverage,
        issue: record.issue,
        blocked: record.blocked,
        session_revision: record.session_revision,
        aggregate_revision: record.aggregate_revision,
        executor_generation: record.executor_generation,
        continuation_of: record.continuation_of,
        continuation_executor_generation: record.continuation_executor_generation,
        continuation_level: record.continuation_level,
        retry_of: record.retry_of,
        resume_of: record.resume_of,
        resume_lineage: record.resume_lineage,
        attempt_refs: vec![],
        unresolved_attempts: vec![],
        task_refs: vec![],
    };
    receipt.validate()?;
    Ok(receipt)
}

pub fn project_session_receipt(
    session: crate::AgentSession,
) -> Result<SessionReceipt, AgentFailure> {
    if session.scope.is_some()
        || session.data_classes != [floe_agent_contract::DataClass::Personal]
        || session.person_id.0.is_nil()
        || session.id.is_nil()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let receipt = SessionReceipt {
        principal: session.person_id.to_string(),
        session_id: session.id,
        session_revision: session.revision,
    };
    receipt.validate()?;
    Ok(receipt)
}

pub fn project_transcript(messages: &[AgentMessage]) -> Result<Vec<ContractMessage>, AgentFailure> {
    messages
        .iter()
        .map(|message| {
            let turn_id = message.turn_id();
            let message_id = match message {
                AgentMessage::User { message_id, .. } => *message_id,
                _ => turn_id,
            };
            contract_message(
                message,
                message_id,
                if matches!(
                    message,
                    AgentMessage::User { .. } | AgentMessage::Interaction { .. }
                ) {
                    DependencyCoverage::Independent
                } else {
                    DependencyCoverage::Unknown
                },
            )
        })
        .collect()
}

pub fn contract_message(
    message: &AgentMessage,
    message_id: Uuid,
    coverage: DependencyCoverage,
) -> Result<ContractMessage, AgentFailure> {
    let (role, text, call_id) = match message {
        AgentMessage::Compaction { summary, .. } => (MessageRole::Assistant, summary.clone(), None),
        AgentMessage::Preamble { text, .. } => (MessageRole::Preamble, text.clone(), None),
        AgentMessage::User { text, .. } => (MessageRole::User, text.clone(), None),
        AgentMessage::Assistant { text, .. } => (MessageRole::Assistant, text.clone(), None),
        AgentMessage::Capability {
            call_id, result, ..
        } => (
            MessageRole::Tool,
            result
                .as_ref()
                .cloned()
                .unwrap_or_else(|failure| format!("unavailable: {failure:?}")),
            Some(*call_id),
        ),
        AgentMessage::Delegation { task, .. } => (
            MessageRole::Delegation,
            task.result
                .clone()
                .unwrap_or_else(|| format!("{}: {:?}", task.agent_id, task.state)),
            None,
        ),
        // Bare metadata only: the opaque interaction id plus its generic kind
        // label. Requirement, target and status stay in the interaction row,
        // where the trusted lookup reads them.
        AgentMessage::Interaction {
            interaction_id,
            interaction_kind,
            ..
        } => (
            MessageRole::Assistant,
            format!(
                "interaction {interaction_id} {}",
                match interaction_kind {
                    floe_agent_contract::UserInteractionKind::SourceAccess => "source_access",
                    floe_agent_contract::UserInteractionKind::ExpertBinding => "expert_binding",
                }
            ),
            None,
        ),
    };
    let message = ContractMessage {
        message_id,
        role,
        text,
        call_id,
        coverage,
    };
    message.validate()?;
    Ok(message)
}

/// The interaction refs one settled step carries, in step order.
///
/// Only trusted-port artifacts are projected: Tool results and Delegation
/// receipts come from App-owned ports, never from model text. A malformed ref
/// is corrupt durable state and fails closed rather than projecting a
/// dangling card.
pub fn terminal_messages(
    run_id: RunId,
    terminal: &RunTerminal,
) -> Result<Vec<AgentMessage>, AgentFailure> {
    let mut messages = Vec::new();
    let mut projected: Vec<Uuid> = Vec::new();
    let mut project_refs = |messages: &mut Vec<AgentMessage>,
                            refs: Vec<floe_agent_contract::UserInteractionRef>|
     -> Result<(), AgentFailure> {
        for reference in refs {
            if projected.contains(&reference.interaction_id) {
                continue;
            }
            if projected.len() >= crate::MAX_STORED_INTERACTIONS_PER_RUN {
                return Err(AgentFailure::StorageUnavailable);
            }
            projected.push(reference.interaction_id);
            messages.push(AgentMessage::Interaction {
                turn_id: run_id.as_uuid(),
                interaction_id: reference.interaction_id,
                interaction_kind: reference.kind,
            });
        }
        Ok(())
    };
    // Owner-set linkage for completions the model did not author
    // (deterministic no-model limitation): deduplicated with step refs.
    // Projected first so a Completed transcript still ends with its
    // Assistant answer, matching source-blocker turns where the ref
    // precedes the explanation.
    for reference in &terminal.interactions {
        reference
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
    }
    project_refs(&mut messages, terminal.interactions.clone())?;
    for step in &terminal.steps {
        match step {
            // Only the owner-authenticated blocked publication above projects
            // interaction references. No artifact JSON becomes authority.
            EngineStep::Answer { text, .. } => messages.push(AgentMessage::Assistant {
                turn_id: run_id.as_uuid(),
                text: text.clone(),
            }),
            EngineStep::Delegation(receipt) => {
                messages.push(AgentMessage::Delegation {
                    turn_id: run_id.as_uuid(),
                    task: receipt.snapshot.clone(),
                    execution_receipt: match &receipt.execution {
                        floe_agent_contract::TaskExecutionEvidence::Admitted(value) => Some(value.reference.clone()),
                        floe_agent_contract::TaskExecutionEvidence::Unadmitted => None,
                    },
                });
            }
            EngineStep::Tool(_) => {}
        }
    }
    Ok(messages)
}

#[derive(Clone, Debug)]
pub struct RunAccountingProjection {
    pub usage: crate::AgentUsage,
    pub attempt_refs: Vec<Uuid>,
    pub task_refs: Vec<Uuid>,
    pub unresolved_attempts: Vec<crate::UnresolvedModelAttempt>,
}

/// Project acknowledged charges and unresolved conservative ceilings from the
/// sole journal. No absent observation becomes a reported provider bill.
pub fn project_run_accounting(
    receipt: &RunReceipt,
    entries: &[crate::JournalEntry],
) -> Result<RunAccountingProjection, AgentFailure> {
    let projected = floe_agent_runtime::project_execution_journal(
        &super::recovery::journal_binding(receipt, entries), entries,
        floe_agent_runtime::JournalProjectionMode::DurablePrefix,
    )?;
    let accounting = floe_agent_runtime::aggregate_model_accounting(&[projected.own_accounting.clone()], &projected.delegated_receipts)?;
    Ok(RunAccountingProjection {
        usage: crate::AgentUsage {
            unknown_token_attempts: accounting.unknown_token_attempts,
            unknown_cost_attempts: accounting.unknown_cost_attempts,
            model_attempts: projected.usage.attempts,
            iterations: projected.completed_iterations,
            capability_calls: u32::try_from(entries.iter().filter(|entry| matches!(entry.event, floe_agent_contract::JournalEvent::ToolIntent { .. })).count())
                .map_err(|_| AgentFailure::StorageUnavailable)?,
            tokens: projected.usage.tokens,
            cost_micros: projected.usage.cost_micros,
            estimated_tokens: projected.usage.estimated_tokens,
            estimated_cost_micros: projected.usage.estimated_cost_micros,
        },
        attempt_refs: projected.attempt_refs,
        task_refs: projected.task_refs.into_iter().map(|id| id.as_uuid()).collect(),
        unresolved_attempts: projected.unresolved_attempts,
    })
}

pub fn validate_terminal_steps(
    terminal: &RunTerminal,
    journal: &[crate::JournalEntry],
) -> Result<(), AgentFailure> {
    use floe_agent_contract::JournalEvent;
    terminal.validate()?;
    if terminal.output.is_none() && terminal.state != crate::RunState::Blocked {
        return Ok(());
    }
    let expected = journal
        .iter()
        .filter_map(|entry| match &entry.event {
            JournalEvent::ToolResult { result } => Some(EngineStep::Tool(result.clone())),
            JournalEvent::DelegationResult { receipt } => {
                Some(EngineStep::Delegation(receipt.clone()))
            }
            JournalEvent::Output { text, artifacts } => Some(EngineStep::Answer {
                text: text.clone(),
                artifacts: artifacts.clone(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    if expected != terminal.steps {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

/// The pure terminal state transition applied by the encrypted transaction.
pub fn apply_terminal(
    record: &crate::RunRecord,
    session: &crate::AgentSession,
    terminal: &RunTerminal,
    journal: &[crate::JournalEntry],
) -> Result<(crate::RunRecord, crate::AgentSession), AgentFailure> {
    record.validate(session.person_id)?;
    terminal.validate()?;
    validate_terminal_steps(terminal, journal)?;
    if record.state != crate::RunState::Working
        || record.session_id != session.id
        || record.session_revision != session.revision
        || session.active_turn != Some(record.run_id.as_uuid())
    {
        return Err(AgentFailure::Conflict);
    }
    let receipt = project_run_receipt(record.clone())?;
    let accounting = project_run_accounting(&receipt, journal)?;
    if matches!(
        terminal.state,
        crate::RunState::Completed | crate::RunState::Blocked
    ) && !accounting.unresolved_attempts.is_empty()
    {
        return Err(AgentFailure::Interrupted);
    }
    let mut session = session.clone();
    for message in terminal_messages(record.run_id, terminal)? {
        if let AgentMessage::Delegation { task, execution_receipt, .. } = &message {
            let prior = session.messages.iter().filter_map(|stored| match stored {
                AgentMessage::Delegation { task: previous, execution_receipt: reference, .. }
                    if previous.task_id == task.task_id => Some((previous, reference)),
                _ => None,
            }).collect::<Vec<_>>();
            if prior.len() > 1 || prior.first().is_some_and(|(previous, reference)|
                *previous != task || *reference != execution_receipt) {
                return Err(AgentFailure::Conflict);
            }
            // A continued Run re-journals actual immutable Task evidence, but
            // the causal Session transcript shows that same acknowledged Task once.
            if !prior.is_empty() { continue; }
        }
        session.messages.push(message);
    }
    session.usage = accounting.usage;
    session.active_turn = None;
    session.continuation = (terminal.output.is_none()
        && matches!(
            (terminal.state, terminal.issue),
            (
                crate::RunState::TimedOut,
                Some(AgentFailure::DeadlineExceeded)
            ) | (crate::RunState::Failed, Some(AgentFailure::BudgetExceeded))
        ))
    .then(|| record.continuation_level.checked_add(1))
    .flatten()
    .filter(|level| *level <= 3)
    .filter(|_| accounting.unresolved_attempts.is_empty())
    .map(|_| crate::AgentContinuation {
        turn_id: record.run_id.as_uuid(),
        level: record.continuation_level,
        usage: session.usage,
    });
    session.last_outcome = Some(match terminal.state {
        crate::RunState::Completed => crate::AgentOutcome::Completed,
        crate::RunState::Blocked => crate::AgentOutcome::Blocked {
            run_id: record.run_id,
            review_group_id: terminal
                .blocked
                .as_ref()
                .ok_or(AgentFailure::InvalidInput)?
                .review_group_id,
        },
        _ => crate::AgentOutcome::Halted {
            reason: terminal.issue.ok_or(AgentFailure::InvalidInput)?,
        },
    });
    session.revision = session
        .revision
        .checked_add(1)
        .ok_or(AgentFailure::Conflict)?;
    let next = crate::RunRecord {
        state: terminal.state,
        output: terminal.output.clone(),
        coverage: terminal.coverage.clone(),
        issue: terminal.issue,
        blocked: terminal.blocked.clone(),
        session_revision: session.revision,
        aggregate_revision: record
            .aggregate_revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?,
        ..record.clone()
    };
    next.validate(record.person_id)?;
    Ok((next, session))
}

/// Activation interrupts abandoned work under a strictly newer executor fence.
/// All acknowledged and unresolved accounting comes from the original journal.
pub fn interrupt_for_activation(
    record: &crate::RunRecord,
    session: &crate::AgentSession,
    journal: &[crate::JournalEntry],
    next_generation: u64,
) -> Result<(crate::RunRecord, crate::AgentSession), AgentFailure> {
    if record.state != crate::RunState::Working || next_generation <= record.executor_generation {
        return Err(AgentFailure::Conflict);
    }
    let receipt = project_run_receipt(record.clone())?;
    super::recovery::validate_run_journal(&receipt, journal)?;
    let (mut record, session) = apply_terminal(
        record,
        session,
        &RunTerminal::from_failure(AgentFailure::Interrupted),
        journal,
    )?;
    record.executor_generation = next_generation;
    record.validate(session.person_id)?;
    Ok((record, session))
}
