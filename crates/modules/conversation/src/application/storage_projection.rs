use floe_agent_contract::{AgentFailure, AgentMessage as ContractMessage, Artifact as ContractArtifact, ArtifactPart as ContractArtifactPart, DependencyCoverage, EngineStep, MessageRole, RunId};
use uuid::Uuid;
use crate::{AgentMessage, RunReceipt, RunTerminal, SessionReceipt};

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

pub fn project_transcript(
    messages: &[AgentMessage],
    receipt: &RunReceipt,
) -> Result<Vec<ContractMessage>, AgentFailure> {
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
            task.result.clone().unwrap_or_else(|| format!("{}: {:?}", task.agent_id, task.state)),
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
fn step_interaction_refs(
    artifacts: &[ContractArtifact],
) -> Result<Vec<floe_agent_contract::UserInteractionRef>, AgentFailure> {
    let mut refs = Vec::new();
    for artifact in artifacts {
        for part in &artifact.parts {
            let ContractArtifactPart::Data { media_type, data } = part else {
                continue;
            };
            if media_type != floe_agent_contract::USER_INTERACTION_MEDIA_TYPE {
                continue;
            }
            let reference: floe_agent_contract::UserInteractionRef =
                serde_json::from_str(data).map_err(|_| AgentFailure::StorageUnavailable)?;
            reference
                .validate()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            refs.push(reference);
        }
    }
    Ok(refs)
}

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
            // Model-authored Answers never project refs: only the explicit
            // owner-set terminal linkage above and trusted-port step
            // artifacts become Interaction messages.
            EngineStep::Answer { text, .. } => messages.push(AgentMessage::Assistant {
                turn_id: run_id.as_uuid(),
                text: text.clone(),
            }),
            EngineStep::Delegation(receipt) => {
                messages.push(AgentMessage::Delegation {
                    turn_id: run_id.as_uuid(),
                    task: receipt.snapshot.clone(),
                });
                project_refs(
                    &mut messages,
                    step_interaction_refs(&receipt.snapshot.artifacts)?,
                )?;
            }
            EngineStep::Tool(result) => {
                project_refs(&mut messages, step_interaction_refs(&result.artifacts)?)?;
            }
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
pub fn project_run_accounting(receipt: &RunReceipt, entries: &[crate::JournalEntry]) -> Result<RunAccountingProjection, AgentFailure> {
    use floe_agent_contract::JournalEvent;
    let mut pending = std::collections::HashMap::new();
    let mut attempts = Vec::new();
    let mut tasks = Vec::new();
    let mut active_tasks = std::collections::HashSet::new();
    let mut usage = crate::AgentUsage::default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.revision != index as u64 + 1 { return Err(AgentFailure::StorageUnavailable); }
        match &entry.event {
            JournalEvent::ModelIntent { attempt_id, parent_task_id, reservation_ceiling, projection_ref, plan } => {
                plan.validate()?; reservation_ceiling.validate()?;
                if attempt_id.is_nil() || projection_ref.as_uuid().is_nil() || attempts.contains(attempt_id)
                    || plan.principal != receipt.principal || plan.device_id != receipt.device_id
                    || parent_task_id.is_some_and(|task| !active_tasks.contains(&task)) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                attempts.push(*attempt_id);
                pending.insert(*attempt_id, (*reservation_ceiling, *parent_task_id));
                usage.model_attempts = usage.model_attempts.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
            }
            JournalEvent::ModelResult { attempt_id, usage: charge, accounting } => {
                let (_, parent) = pending.remove(attempt_id).ok_or(AgentFailure::StorageUnavailable)?;
                if parent.is_some_and(|task| !active_tasks.contains(&task)) { return Err(AgentFailure::StorageUnavailable); }
                accounting.validate_charge(charge.tokens, charge.cost_micros)?;
                usage.tokens = usage.tokens.checked_add(charge.tokens).ok_or(AgentFailure::StorageUnavailable)?;
                usage.cost_micros = usage.cost_micros.checked_add(charge.cost_micros).ok_or(AgentFailure::StorageUnavailable)?;
                if accounting.unknown_tokens { usage.estimated_tokens = usage.estimated_tokens.checked_add(charge.tokens).ok_or(AgentFailure::StorageUnavailable)?; }
                if accounting.unknown_cost { usage.estimated_cost_micros = usage.estimated_cost_micros.checked_add(charge.cost_micros).ok_or(AgentFailure::StorageUnavailable)?; }
            }
            JournalEvent::DelegationIntent { request } => {
                if tasks.contains(&request.task_id.as_uuid()) { return Err(AgentFailure::StorageUnavailable); }
                tasks.push(request.task_id.as_uuid()); active_tasks.insert(request.task_id);
            }
            JournalEvent::DelegationResult { receipt } => {
                if !active_tasks.remove(&receipt.task_id) { return Err(AgentFailure::StorageUnavailable); }
            }
            JournalEvent::ToolIntent { .. } => {
                usage.capability_calls = usage.capability_calls.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
            }
            JournalEvent::Checkpoint { .. } => {
                usage.iterations = usage.iterations.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
            }
            _ => {},
        }
    }
    let mut unresolved = Vec::new();
    for attempt_id in &attempts {
        if let Some((ceiling, _)) = pending.get(attempt_id) {
            usage.tokens = usage.tokens.checked_add(ceiling.tokens).ok_or(AgentFailure::StorageUnavailable)?;
            usage.estimated_tokens = usage.estimated_tokens.checked_add(ceiling.tokens).ok_or(AgentFailure::StorageUnavailable)?;
            usage.cost_micros = usage.cost_micros.checked_add(ceiling.cost_micros).ok_or(AgentFailure::StorageUnavailable)?;
            usage.estimated_cost_micros = usage.estimated_cost_micros.checked_add(ceiling.cost_micros).ok_or(AgentFailure::StorageUnavailable)?;
            unresolved.push(crate::UnresolvedModelAttempt {
                attempt_id: *attempt_id, reservation_ceiling: *ceiling,
                accounting: floe_agent_contract::ModelAccounting { observed_tokens: None, observed_cost_micros: None, unknown_tokens: true, unknown_cost: true },
            });
        }
    }
    Ok(RunAccountingProjection { usage, attempt_refs: attempts, task_refs: tasks, unresolved_attempts: unresolved })
}

pub fn validate_terminal_steps(terminal: &RunTerminal, journal: &[crate::JournalEntry]) -> Result<(), AgentFailure> {
    use floe_agent_contract::JournalEvent;
    terminal.validate()?;
    if terminal.output.is_none() && terminal.state != crate::RunState::Blocked { return Ok(()); }
    let expected = journal.iter().filter_map(|entry| match &entry.event {
        JournalEvent::ToolResult { result } => Some(EngineStep::Tool(result.clone())),
        JournalEvent::DelegationResult { receipt } => Some(EngineStep::Delegation(receipt.clone())),
        JournalEvent::Output { text, artifacts } => Some(EngineStep::Answer { text: text.clone(), artifacts: artifacts.clone() }),
        _ => None,
    }).collect::<Vec<_>>();
    if expected != terminal.steps { return Err(AgentFailure::StorageUnavailable); }
    Ok(())
}

/// The pure terminal state transition applied by the encrypted transaction.
pub fn apply_terminal(
    record: &crate::RunRecord,
    session: &crate::AgentSession,
    terminal: &RunTerminal,
    journal: &[crate::JournalEntry],
) -> Result<(crate::RunRecord, crate::AgentSession), AgentFailure> {
    record.validate(session.person_id)?; terminal.validate()?;
    validate_terminal_steps(terminal, journal)?;
    if record.state != crate::RunState::Working || record.session_id != session.id
        || record.session_revision != session.revision || session.active_turn != Some(record.run_id.as_uuid()) {
        return Err(AgentFailure::Conflict);
    }
    let receipt = project_run_receipt(record.clone())?;
    let accounting = project_run_accounting(&receipt, journal)?;
    if matches!(terminal.state, crate::RunState::Completed | crate::RunState::Blocked)
        && !accounting.unresolved_attempts.is_empty() { return Err(AgentFailure::Interrupted); }
    let mut session = session.clone();
    session.messages.extend(terminal_messages(record.run_id, terminal)?);
    session.usage = accounting.usage;
    session.active_turn = None;
    session.continuation = (terminal.output.is_none() && matches!((terminal.state, terminal.issue),
        (crate::RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
        | (crate::RunState::Failed, Some(AgentFailure::BudgetExceeded))))
        .then(|| record.continuation_level.checked_add(1)).flatten().filter(|level| *level <= 3)
        .filter(|_| accounting.unresolved_attempts.is_empty())
        .map(|_| crate::AgentContinuation { turn_id: record.run_id.as_uuid(), level: record.continuation_level, usage: session.usage });
    session.last_outcome = Some(match terminal.state {
        crate::RunState::Completed => crate::AgentOutcome::Completed,
        crate::RunState::Blocked => crate::AgentOutcome::Blocked {
            run_id: record.run_id, review_group_id: terminal.blocked.as_ref().ok_or(AgentFailure::InvalidInput)?.review_group_id,
        },
        _ => crate::AgentOutcome::Halted { reason: terminal.issue.ok_or(AgentFailure::InvalidInput)? },
    });
    session.revision = session.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
    let next = crate::RunRecord {
        state: terminal.state, output: terminal.output.clone(), coverage: terminal.coverage.clone(), issue: terminal.issue,
        blocked: terminal.blocked.clone(), session_revision: session.revision,
        aggregate_revision: record.aggregate_revision.checked_add(1).ok_or(AgentFailure::Conflict)?, ..record.clone()
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
    let (mut record, session) = apply_terminal(record, session, &RunTerminal::from_failure(AgentFailure::Interrupted), journal)?;
    record.executor_generation = next_generation;
    record.validate(session.person_id)?;
    Ok((record, session))
}
