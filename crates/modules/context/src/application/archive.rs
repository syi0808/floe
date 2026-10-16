use std::collections::HashSet;
use std::future::Future;

use floe_agent_contract::{
    AgentFailure, ArchiveMessageGroup, ArchiveReadRequest, ArchivedMessage, DependencyCoverage,
};

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

    let mut coverage_by_group = Vec::<(ArchiveMessageGroup, DependencyCoverage)>::new();
    for archived in &snapshot.messages {
        if let Some((group_id, coverage)) = coverage_by_group.last_mut()
            && *group_id == archived.group
        {
            *coverage = coverage
                .merge(&archived.message.coverage)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
        } else {
            coverage_by_group.push((archived.group, archived.message.coverage.clone()));
        }
    }
    let mut retained_groups = HashSet::new();
    for (group_id, coverage) in coverage_by_group {
        let projection = project_coverage(&coverage, &mut authorize).await?;
        if projection.retain_derived() {
            retained_groups.insert(group_id);
        }
    }
    snapshot
        .messages
        .retain(|message| retained_groups.contains(&message.group));
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
        let group_id = messages[start - 1].group;
        let mut group_start = start - 1;
        while group_start > 0 && messages[group_start - 1].group == group_id {
            group_start -= 1;
        }
        let mut next_bytes = bytes;
        for message in &messages[group_start..start] {
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
        start = group_start;
    }
    if start == messages.len() {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(messages[start..].to_vec())
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    use floe_agent_contract::{
        AgentMessage, ArchivePointer, ArchiveSnapshot, BoxFuture, MessageRole,
    };
    use uuid::Uuid;
    struct Reader(ArchiveSnapshot);
    impl ArchiveReader for Reader {
        fn read_archive<'a>(
            &'a self,
            _: &'a ArchiveReadRequest,
        ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>> {
            Box::pin(async move { Ok(self.0.clone()) })
        }
    }
    #[tokio::test]
    async fn unknown_surface_result_does_not_erase_independent_person_input() {
        let user_id = Uuid::new_v4();
        let result_id = Uuid::new_v4();
        let request = ArchiveReadRequest {
            person_id: floe_context_contract::PersonId::new(),
            session_id: Uuid::new_v4(),
            pointer: ArchivePointer::Surface {
                archive_id: Uuid::new_v4(),
                source_revision: 2,
                through_entry_id: result_id,
                through_sequence: 2,
                prefix_digest: [7; 32],
                archived_message_count: 2,
            },
            max_messages: 128,
            max_bytes: 128 * 1024,
        };
        let user = ArchivedMessage {
            group: ArchiveMessageGroup::SurfaceEntry(user_id),
            message: AgentMessage {
                message_id: user_id,
                role: MessageRole::User,
                text: "Original Person input".into(),
                call_id: None,
                coverage: DependencyCoverage::Independent,
            },
        };
        let derived = ArchivedMessage {
            group: ArchiveMessageGroup::SurfaceEntry(result_id),
            message: AgentMessage {
                message_id: Uuid::new_v4(),
                role: MessageRole::Assistant,
                text: "Unproven derived result".into(),
                call_id: None,
                coverage: DependencyCoverage::Unknown,
            },
        };
        let reader = Reader(ArchiveSnapshot {
            person_id: request.person_id,
            session_id: request.session_id,
            pointer: request.pointer.clone(),
            messages: vec![user.clone(), derived],
        });
        let projected = read_authorized_archive(&reader, &request, |_| async {
            panic!("Independent/Unknown require no fabricated source authority")
        })
        .await
        .unwrap();
        assert_eq!(projected.messages, vec![user]);
    }
}
