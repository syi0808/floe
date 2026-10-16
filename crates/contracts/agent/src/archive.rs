use crate::{AgentFailure, AgentMessage, BoxFuture};
use floe_context_contract::PersonId;
use uuid::Uuid;

pub const MAX_ARCHIVE_PROJECTION_BYTES: usize = 128 * 1024;
pub const MAX_ARCHIVE_PROJECTION_MESSAGES: usize = crate::MAX_AGENT_MESSAGES;

/// Archive custody is explicit: an execution turn and a Person-facing entry
/// are different evidence identities and must never be cast into each other.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArchivePointer {
    Core {
        archive_id: Uuid,
        source_revision: u64,
        through_turn_id: Uuid,
        archived_message_count: usize,
    },
    Surface {
        archive_id: Uuid,
        source_revision: u64,
        through_entry_id: Uuid,
        through_sequence: u64,
        prefix_digest: [u8; 32],
        archived_message_count: usize,
    },
}
impl ArchivePointer {
    pub fn archive_id(&self) -> Uuid {
        match self {
            Self::Core { archive_id, .. } | Self::Surface { archive_id, .. } => *archive_id,
        }
    }
    pub fn source_revision(&self) -> u64 {
        match self {
            Self::Core {
                source_revision, ..
            }
            | Self::Surface {
                source_revision, ..
            } => *source_revision,
        }
    }
    pub fn archived_message_count(&self) -> usize {
        match self {
            Self::Core {
                archived_message_count,
                ..
            }
            | Self::Surface {
                archived_message_count,
                ..
            } => *archived_message_count,
        }
    }
    pub fn through_group(&self) -> ArchiveMessageGroup {
        match self {
            Self::Core {
                through_turn_id, ..
            } => ArchiveMessageGroup::CoreTurn(*through_turn_id),
            Self::Surface {
                through_entry_id, ..
            } => ArchiveMessageGroup::SurfaceEntry(*through_entry_id),
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.archive_id().is_nil()
            || self.source_revision() == 0
            || !self.through_group().is_valid()
            || self.archived_message_count() == 0
            || self.archived_message_count() > MAX_ARCHIVE_PROJECTION_MESSAGES
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let Self::Surface {
            through_sequence,
            prefix_digest,
            ..
        } = self
        {
            if *through_sequence == 0
                || *through_sequence > i64::MAX as u64
                || *prefix_digest == [0; 32]
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ArchiveMessageGroup {
    CoreTurn(Uuid),
    SurfaceEntry(Uuid),
    SurfaceSummary(Uuid),
}
impl ArchiveMessageGroup {
    pub fn is_valid(self) -> bool {
        match self {
            Self::CoreTurn(id) | Self::SurfaceEntry(id) | Self::SurfaceSummary(id) => !id.is_nil(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveReadRequest {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub pointer: ArchivePointer,
    pub max_messages: usize,
    pub max_bytes: usize,
}
impl ArchiveReadRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.pointer.validate()?;
        if self.person_id.0.is_nil()
            || self.session_id.is_nil()
            || self.max_messages == 0
            || self.max_messages > MAX_ARCHIVE_PROJECTION_MESSAGES
            || self.max_bytes < 2
            || self.max_bytes > MAX_ARCHIVE_PROJECTION_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivedMessage {
    pub group: ArchiveMessageGroup,
    pub message: AgentMessage,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveSnapshot {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub pointer: ArchivePointer,
    pub messages: Vec<ArchivedMessage>,
}
impl ArchiveSnapshot {
    pub fn validate(&self, request: &ArchiveReadRequest) -> Result<(), AgentFailure> {
        request.validate()?;
        if self.person_id != request.person_id
            || self.session_id != request.session_id
            || self.pointer != request.pointer
            || self.messages.len() != request.pointer.archived_message_count()
            || self.messages.is_empty()
            || self
                .messages
                .iter()
                .any(|message| !message.group.is_valid() || message.message.validate().is_err())
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut completed = std::collections::HashSet::new();
        let mut previous = None;
        for (index, message) in self.messages.iter().enumerate() {
            match (&self.pointer, message.group) {
                (ArchivePointer::Core { .. }, ArchiveMessageGroup::CoreTurn(_)) => {}
                (ArchivePointer::Surface { .. }, ArchiveMessageGroup::SurfaceEntry(_)) => {}
                (ArchivePointer::Surface { .. }, ArchiveMessageGroup::SurfaceSummary(_))
                    if index == 0 => {}
                _ => return Err(AgentFailure::StorageUnavailable),
            }
            if previous != Some(message.group) {
                if !completed.insert(message.group) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                previous = Some(message.group);
            } else if !matches!(self.pointer, ArchivePointer::Core { .. }) {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        if self.messages.last().map(|message| message.group)
            != Some(request.pointer.through_group())
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}
pub trait ArchiveReader: Send + Sync {
    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(group: ArchiveMessageGroup) -> ArchivedMessage {
        ArchivedMessage {
            group,
            message: AgentMessage {
                message_id: Uuid::new_v4(),
                role: crate::MessageRole::User,
                text: "Synthetic retained input".into(),
                call_id: None,
                coverage: crate::DependencyCoverage::Independent,
            },
        }
    }
    fn request(pointer: ArchivePointer) -> ArchiveReadRequest {
        ArchiveReadRequest {
            person_id: PersonId::new(),
            session_id: Uuid::new_v4(),
            pointer,
            max_messages: 128,
            max_bytes: 128 * 1024,
        }
    }
    fn snapshot(request: &ArchiveReadRequest, messages: Vec<ArchivedMessage>) -> ArchiveSnapshot {
        ArchiveSnapshot {
            person_id: request.person_id,
            session_id: request.session_id,
            pointer: request.pointer.clone(),
            messages,
        }
    }
    #[test]
    fn core_turn_and_surface_entry_are_not_interchangeable() {
        let id = Uuid::new_v4();
        let request = request(ArchivePointer::Core {
            archive_id: Uuid::new_v4(),
            source_revision: 2,
            through_turn_id: id,
            archived_message_count: 1,
        });
        snapshot(&request, vec![message(ArchiveMessageGroup::CoreTurn(id))])
            .validate(&request)
            .unwrap();
        assert!(
            snapshot(
                &request,
                vec![message(ArchiveMessageGroup::SurfaceEntry(id))]
            )
            .validate(&request)
            .is_err()
        );
    }
    #[test]
    fn surface_archive_keeps_prior_summary_and_exact_entry_cutoff() {
        let last = Uuid::new_v4();
        let previous = Uuid::new_v4();
        let request = request(ArchivePointer::Surface {
            archive_id: Uuid::new_v4(),
            source_revision: 3,
            through_entry_id: last,
            through_sequence: 10,
            prefix_digest: [7; 32],
            archived_message_count: 2,
        });
        let original = snapshot(
            &request,
            vec![
                message(ArchiveMessageGroup::SurfaceSummary(previous)),
                message(ArchiveMessageGroup::SurfaceEntry(last)),
            ],
        );
        original.validate(&request).unwrap();
        let mut late_summary = original.clone();
        late_summary.messages.swap(0, 1);
        assert!(late_summary.validate(&request).is_err());
        let mut changed_cutoff = original.clone();
        changed_cutoff.messages[1].group = ArchiveMessageGroup::SurfaceEntry(Uuid::new_v4());
        assert!(changed_cutoff.validate(&request).is_err());
        let mut duplicated = original;
        duplicated.messages[0].group = ArchiveMessageGroup::SurfaceEntry(last);
        assert!(duplicated.validate(&request).is_err());
    }
    #[test]
    fn core_archive_rejects_noncontiguous_turn_reentry() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let request = request(ArchivePointer::Core {
            archive_id: Uuid::new_v4(),
            source_revision: 3,
            through_turn_id: a,
            archived_message_count: 3,
        });
        let result = snapshot(
            &request,
            vec![
                message(ArchiveMessageGroup::CoreTurn(a)),
                message(ArchiveMessageGroup::CoreTurn(b)),
                message(ArchiveMessageGroup::CoreTurn(a)),
            ],
        );
        assert!(result.validate(&request).is_err());
    }
}
