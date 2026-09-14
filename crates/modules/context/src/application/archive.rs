use std::collections::HashSet;
use std::future::Future;

use floe_agent_contract::{AgentFailure, DependencyCoverage};
use uuid::Uuid;

use crate::{
    ArchiveProjection, ArchiveReadRequest, ArchiveReader, ArchiveSnapshot, ArchivedMessage,
    project_coverage,
};

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
    validate_snapshot(request, &snapshot)?;

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

fn validate_snapshot(
    request: &ArchiveReadRequest,
    snapshot: &ArchiveSnapshot,
) -> Result<(), AgentFailure> {
    if snapshot.person_id != request.person_id
        || snapshot.session_id != request.session_id
        || snapshot.pointer != request.pointer
        || snapshot.messages.len() != request.pointer.archived_message_count
        || snapshot.messages.is_empty()
        || snapshot
            .messages
            .iter()
            .any(|message| message.turn_id.is_nil() || message.message.validate().is_err())
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let mut completed = HashSet::new();
    let mut previous = None;
    for message in &snapshot.messages {
        if previous != Some(message.turn_id) {
            if !completed.insert(message.turn_id) {
                return Err(AgentFailure::StorageUnavailable);
            }
            previous = Some(message.turn_id);
        }
    }
    if snapshot.messages.last().map(|message| message.turn_id)
        != Some(request.pointer.through_turn_id)
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use floe_agent_contract::{AgentMessage, BoxFuture, MessageRole};
    use floe_context_contract::PersonId;

    use super::*;
    use crate::{ArchivePointer, ArchiveReader};

    struct Reader(ArchiveSnapshot);

    impl ArchiveReader for Reader {
        fn read_archive<'a>(
            &'a self,
            _: &'a ArchiveReadRequest,
        ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>> {
            Box::pin(async move { Ok(self.0.clone()) })
        }
    }

    fn message(turn_id: Uuid, text: &str, coverage: DependencyCoverage) -> ArchivedMessage {
        ArchivedMessage {
            turn_id,
            message: AgentMessage {
                message_id: Uuid::new_v4(),
                role: MessageRole::Assistant,
                text: text.into(),
                call_id: None,
                coverage,
            },
        }
    }

    fn fixture(messages: Vec<ArchivedMessage>) -> (Reader, ArchiveReadRequest) {
        let person_id = PersonId::new();
        let session_id = Uuid::new_v4();
        let pointer = ArchivePointer {
            archive_id: Uuid::new_v4(),
            source_revision: 7,
            through_turn_id: messages.last().unwrap().turn_id,
            archived_message_count: messages.len(),
        };
        let snapshot = ArchiveSnapshot {
            person_id,
            session_id,
            pointer: pointer.clone(),
            messages,
        };
        (
            Reader(snapshot),
            ArchiveReadRequest {
                person_id,
                session_id,
                pointer,
                max_messages: 128,
                max_bytes: 128 * 1024,
            },
        )
    }

    #[tokio::test]
    async fn projects_whole_turns_and_excludes_unknown() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let (reader, request) = fixture(vec![
            message(first, "question", DependencyCoverage::Independent),
            message(first, "private", DependencyCoverage::Unknown),
            message(second, "later", DependencyCoverage::Independent),
        ]);
        let snapshot = read_authorized_archive(&reader, &request, |_| async { Ok(true) })
            .await
            .unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.messages[0].turn_id, second);
    }

    #[tokio::test]
    async fn rechecks_dependent_authority_before_returning_text() {
        use chrono::{TimeZone, Utc};
        use floe_context_contract::{
            ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
            GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
            GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
        };
        let person_id = PersonId::new();
        let dependency = floe_context_contract::ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            GrantSourceBinding::try_new(
                person_id,
                ConnectionId::try_new("connection").unwrap(),
                ConnectorId::try_new("connector").unwrap(),
                ExecutionOwnerId::try_new("owner").unwrap(),
                SourceAuthority::new(),
            )
            .unwrap(),
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"archive".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 5, 0).unwrap(),
        )
        .unwrap();
        let turn = Uuid::new_v4();
        let (reader, request) = fixture(vec![message(
            turn,
            "derived summary",
            DependencyCoverage::dependent(dependency).unwrap(),
        )]);
        let snapshot = read_authorized_archive(&reader, &request, |_| async { Ok(false) })
            .await
            .unwrap();
        assert!(snapshot.messages.is_empty());
    }

    #[tokio::test]
    async fn byte_bound_never_splits_latest_turn() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let (reader, mut request) = fixture(vec![
            message(first, "old", DependencyCoverage::Independent),
            message(second, "한글", DependencyCoverage::Independent),
            message(second, "pair", DependencyCoverage::Independent),
        ]);
        let later_bytes = 2
            + reader.0.messages[1..]
                .iter()
                .map(|message| serde_json::to_vec(&message.message).unwrap().len())
                .sum::<usize>()
            + 1;
        request.max_bytes = later_bytes;
        let snapshot = read_authorized_archive(&reader, &request, |_| async { Ok(true) })
            .await
            .unwrap();
        assert_eq!(snapshot.messages.len(), 2);
        assert!(
            snapshot
                .messages
                .iter()
                .all(|message| message.turn_id == second)
        );
    }
}
