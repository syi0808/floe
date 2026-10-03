use std::collections::HashSet;
use std::future::Future;

use floe_agent_contract::{AgentFailure, ArchiveReadRequest, ArchivedMessage, DependencyCoverage};
use uuid::Uuid;

use crate::{ArchiveProjection, ArchiveReader, project_coverage};

pub async fn read_authorized_archive<Authorize, AuthorizationFuture>(
    reader: &dyn ArchiveReader,
    request: &ArchiveReadRequest,
    mut authorize: Authorize,
) -> Result<ArchiveProjection, AgentFailure>
where
    Authorize: FnMut(floe_context_contract::ContextDependency) -> AuthorizationFuture,
    AuthorizationFuture: Future<Output = Result<bool, AgentFailure>>,
{
    request.validate()?;
    let mut snapshot = reader.read_archive(request).await?;
    snapshot.validate(request)?;

    let mut coverage_by_turn = Vec::<(Uuid, DependencyCoverage)>::new();
    for archived in &snapshot.messages {
        if let Some((turn_id, coverage)) = coverage_by_turn.last_mut()
            && *turn_id == archived.turn_id
        {
            *coverage = coverage
                .merge(&archived.message.coverage)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
        } else {
            coverage_by_turn.push((archived.turn_id, archived.message.coverage.clone()));
        }
    }
    let mut retained_turns = HashSet::new();
    for (turn_id, coverage) in coverage_by_turn {
        let projection = project_coverage(&coverage, &mut authorize).await?;
        if projection.retain_derived() {
            retained_turns.insert(turn_id);
        }
    }
    snapshot
        .messages
        .retain(|message| retained_turns.contains(&message.turn_id));
    let messages = bounded_tail(snapshot.messages, request.max_messages, request.max_bytes)?;
    Ok(ArchiveProjection {
        person_id: snapshot.person_id,
        session_id: snapshot.session_id,
        pointer: snapshot.pointer,
        messages,
    })
}

fn bounded_tail(
    messages: Vec<ArchivedMessage>,
    max_messages: usize,
    max_bytes: usize,
) -> Result<Vec<ArchivedMessage>, AgentFailure> {
    if messages.is_empty() {
        return Ok(messages);
    }
    let mut start = messages.len();
    let mut count = 0usize;
    let mut bytes = 2usize;
    while start > 0 {
        let turn_id = messages[start - 1].turn_id;
        let mut turn_start = start - 1;
        while turn_start > 0 && messages[turn_start - 1].turn_id == turn_id {
            turn_start -= 1;
        }
        let mut next_bytes = bytes;
        for message in &messages[turn_start..start] {
            let encoded = serde_json::to_vec(&message.message)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            next_bytes = next_bytes
                .checked_add(encoded.len())
                .and_then(|value| value.checked_add(usize::from(count > 0)))
                .ok_or(AgentFailure::BudgetExceeded)?;
            count = count.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
        }
        if count > max_messages || next_bytes > max_bytes {
            break;
        }
        bytes = next_bytes;
        start = turn_start;
    }
    if start == messages.len() {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(messages[start..].to_vec())
}
