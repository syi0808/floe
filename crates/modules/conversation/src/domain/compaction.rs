//! Conversation-owned input to one bounded semantic-compaction execution.
use floe_agent_contract::{AgentFailure, AgentMessage, ArchiveMessageGroup};
use floe_conversation_contract::{
    ConversationCheckpoint, ConversationReference, TranscriptReference,
};
use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const COMPACTION_TRIGGER_MESSAGES: usize = 96;
pub const COMPACTION_TRIGGER_BYTES: usize = 96 * 1024;
pub const COMPACTION_MIN_RETAIN_ENTRIES: u64 = 2;
pub const COMPACTION_PREFERRED_RETAIN_ENTRIES: u64 = 16;
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompactionAuthority {
    Idle,
    BeforeFirstModel { run_id: floe_kernel::RunId },
}
impl CompactionAuthority {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if matches!(self, Self::BeforeFirstModel { run_id } if !run_id.is_valid()) {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoreCompactionPrefix {
    pub head: ConversationReference,
    pub previous_checkpoint: Option<ConversationCheckpoint>,
    pub through: TranscriptReference,
    pub prefix_digest: [u8; 32],
    pub through_turn_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceCheckpointRef {
    pub execution_id: Uuid,
    pub through: crate::SurfaceAnchor,
}
impl SurfaceCheckpointRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.through.validate()?;
        if self.execution_id.is_nil()
            || self.through.sequence == 0
            || self.through.digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn summary_id(&self) -> Uuid {
        surface_summary_id(self.execution_id)
    }
}
pub fn surface_summary_id(execution_id: Uuid) -> Uuid {
    Uuid::new_v5(&execution_id, b"surface-compaction-summary")
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceCompactionPrefix {
    pub person_id: PersonId,
    pub head: crate::SurfaceAnchor,
    /// The selected input is always kept original and outside the summary.
    pub protected_input: crate::SurfaceAnchor,
    pub previous_checkpoint: Option<SurfaceCheckpointRef>,
    pub through: crate::SurfaceAnchor,
    pub through_entry_id: Uuid,
    /// Typed source identities preserve archive grouping without pretending
    /// asynchronous Surface entries are Core execution turns.
    pub groups: Vec<ArchiveMessageGroup>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "prefix",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CompactionPrefix {
    Core(CoreCompactionPrefix),
    Surface(SurfaceCompactionPrefix),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionSource {
    pub authority: CompactionAuthority,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub data_classes: Vec<floe_agent_contract::DataClass>,
    pub prefix: CompactionPrefix,
    pub messages: Vec<AgentMessage>,
}
impl CompactionSource {
    pub fn person_id(&self) -> PersonId {
        match &self.prefix {
            CompactionPrefix::Core(p) => p.head.identity.person_id,
            CompactionPrefix::Surface(p) => p.person_id,
        }
    }
    pub fn core_prefix(&self) -> Result<&CoreCompactionPrefix, AgentFailure> {
        match &self.prefix {
            CompactionPrefix::Core(p) => Ok(p),
            _ => Err(AgentFailure::Conflict),
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.authority
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if self.session_id.is_nil()
            || self.session_revision == 0
            || self.person_id().0.is_nil()
            || self.data_classes.is_empty()
            || self.data_classes.len() > floe_agent_contract::MAX_INPUT_DATA_CLASSES
            || self
                .data_classes
                .iter()
                .enumerate()
                .any(|(i, c)| self.data_classes[..i].contains(c))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let count = match &self.prefix {
            CompactionPrefix::Core(p) => {
                p.head
                    .validate()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                p.through
                    .validate()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                let start = if let Some(previous) = &p.previous_checkpoint {
                    previous
                        .validate()
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
                    if previous.through.conversation_id != p.head.conversation_id
                        || previous.through.branch_id != p.head.branch_id
                        || self.messages.first().is_none_or(|m| {
                            m.role != floe_agent_contract::MessageRole::Assistant
                                || m.text != previous.summary
                        })
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    previous.through.sequence
                } else {
                    0
                };
                if p.through_turn_id.is_nil()
                    || p.prefix_digest == [0; 32]
                    || p.through.conversation_id != p.head.conversation_id
                    || p.through.branch_id != p.head.branch_id
                    || p.through.sequence <= start
                    || p.head.head_revision.saturating_sub(p.through.sequence)
                        < COMPACTION_MIN_RETAIN_ENTRIES
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                p.through
                    .sequence
                    .checked_sub(start)
                    .and_then(|n| n.checked_add(u64::from(p.previous_checkpoint.is_some())))
                    .ok_or(AgentFailure::StorageUnavailable)?
            }
            CompactionPrefix::Surface(p) => {
                p.head.validate()?;
                p.protected_input.validate()?;
                p.through.validate()?;
                let start = if let Some(previous) = &p.previous_checkpoint {
                    previous.validate()?;
                    if previous.through.session_id != self.session_id
                        || self.messages.first().is_none_or(|m| {
                            m.role != floe_agent_contract::MessageRole::Assistant
                                || m.message_id != previous.summary_id()
                        })
                        || p.groups.first()
                            != Some(&ArchiveMessageGroup::SurfaceSummary(previous.execution_id))
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    previous.through.sequence
                } else {
                    0
                };
                if p.head.session_id != self.session_id
                    || p.through.session_id != self.session_id
                    || p.protected_input.session_id != self.session_id
                    || p.through.sequence <= start
                    || p.through.sequence >= p.protected_input.sequence
                    || p.protected_input.sequence > p.head.sequence
                    || p.head.sequence.saturating_sub(p.through.sequence)
                        < COMPACTION_MIN_RETAIN_ENTRIES
                    || p.head.digest == [0; 32]
                    || p.protected_input.digest == [0; 32]
                    || p.through.digest == [0; 32]
                    || p.through_entry_id.is_nil()
                    || p.groups.len() != self.messages.len()
                    || p.groups.last()
                        != Some(&ArchiveMessageGroup::SurfaceEntry(p.through_entry_id))
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let mut groups = std::collections::HashSet::new();
                for (i, group) in p.groups.iter().enumerate() {
                    if !group.is_valid()
                        || !groups.insert(*group)
                        || !matches!(group, ArchiveMessageGroup::SurfaceEntry(_))
                            && !(i == 0
                                && p.previous_checkpoint.as_ref().is_some_and(|previous| {
                                    *group
                                        == ArchiveMessageGroup::SurfaceSummary(
                                            previous.execution_id,
                                        )
                                }))
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
                p.through
                    .sequence
                    .checked_sub(start)
                    .and_then(|n| n.checked_add(u64::from(p.previous_checkpoint.is_some())))
                    .ok_or(AgentFailure::StorageUnavailable)?
            }
        };
        if self.messages.len() as u64 != count || count > COMPACTION_TRIGGER_MESSAGES as u64 {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.messages.iter().try_for_each(AgentMessage::validate)?;
        let mut ids = std::collections::HashSet::new();
        if self
            .messages
            .iter()
            .any(|m| m.message_id.is_nil() || !ids.insert(m.message_id))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        if serde_json::to_vec(&self.messages)
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .len()
            > COMPACTION_TRIGGER_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    #[test]
    fn surface_source_keeps_selected_input_outside_exact_prefix() {
        let actor = floe_kernel::OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture".into(),
            runtime_epoch: 1,
        };
        let session_id = Uuid::new_v4();
        let mut anchor = crate::SurfaceAnchor::empty(session_id).unwrap();
        let mut inputs = Vec::new();
        for n in 0..4 {
            let receipt = crate::AppendSurfaceInput {
                command_id: floe_kernel::CommandId::new(),
                expected: anchor,
                input: crate::SurfaceInput {
                    message_id: floe_conversation_contract::MessageId::new(),
                    text: format!("Complete retained input {n}"),
                    origin: crate::SurfaceInputOrigin::Text,
                },
            }
            .admit(&actor)
            .unwrap();
            anchor = receipt.through;
            inputs.push(receipt);
        }
        let prefix = SurfaceCompactionPrefix {
            person_id: actor.person_id,
            head: anchor,
            protected_input: inputs[3].through,
            previous_checkpoint: None,
            through: inputs[1].through,
            through_entry_id: inputs[1].input.message_id.as_uuid(),
            groups: inputs[..2]
                .iter()
                .map(|i| ArchiveMessageGroup::SurfaceEntry(i.input.message_id.as_uuid()))
                .collect(),
        };
        let source = CompactionSource {
            authority: CompactionAuthority::BeforeFirstModel {
                run_id: floe_kernel::RunId::new(),
            },
            session_id,
            session_revision: 1,
            data_classes: vec![floe_agent_contract::DataClass::Synthetic],
            prefix: CompactionPrefix::Surface(prefix.clone()),
            messages: inputs[..2]
                .iter()
                .map(|i| AgentMessage {
                    message_id: i.input.message_id.as_uuid(),
                    role: floe_agent_contract::MessageRole::User,
                    text: i.input.text.clone(),
                    call_id: None,
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                })
                .collect(),
        };
        source.validate().unwrap();
        let mut changed = source.clone();
        let CompactionPrefix::Surface(ref mut p) = changed.prefix else {
            unreachable!()
        };
        p.protected_input = inputs[1].through;
        assert!(
            changed.validate().is_err(),
            "selected input cannot be swallowed by its history summary"
        );
        let mut changed = source.clone();
        let CompactionPrefix::Surface(ref mut p) = changed.prefix else {
            unreachable!()
        };
        p.groups[0] = ArchiveMessageGroup::CoreTurn(inputs[0].input.message_id.as_uuid());
        assert!(
            changed.validate().is_err(),
            "a Surface entry cannot impersonate Core turn custody"
        );
        let mut changed = source;
        let CompactionPrefix::Surface(ref mut p) = changed.prefix else {
            unreachable!()
        };
        p.head.session_id = Uuid::new_v4();
        assert!(changed.validate().is_err());
    }
}
