//! Lifecycle of one Conversation-owned summarization execution.
use crate::{
    CompactionJournalHead, CompactionReceipt, CompactionRecovery, CompactionSource,
    recover_compaction_journal, validate_compaction_journal,
};
use floe_agent_contract::JournalEntry;
use floe_kernel::{AgentFailure, OwnerActor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionClaim {
    pub execution_id: Uuid,
    pub executor_generation: u64,
}
impl CompactionClaim {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.execution_id.is_nil() || self.executor_generation == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompactionDisposition {
    Pending,
    Retired { failure: AgentFailure },
    Committed { receipt: CompactionReceipt },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionExecution {
    pub claim: CompactionClaim,
    pub revision: u64,
    pub source_byte_limit: usize,
    pub source: CompactionSource,
    pub head: CompactionJournalHead,
    pub entries: Vec<JournalEntry>,
    pub disposition: CompactionDisposition,
}
impl CompactionExecution {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.claim
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if self.revision == 0
            || self.claim.execution_id != self.head.execution_id
            || !(2..=crate::COMPACTION_TRIGGER_BYTES).contains(&self.source_byte_limit)
            || serde_json::to_vec(&self.source.messages)
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .len()
                > self.source_byte_limit
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        validate_compaction_journal(&self.head, &self.source, &self.entries)?;
        if let CompactionDisposition::Committed { receipt } = &self.disposition {
            receipt.validate()?;
            if receipt.session_id != self.source.session_id
                || self.source.session_revision.checked_add(1) != Some(receipt.session_revision)
                || receipt.pointer.source_revision != self.source.session_revision
                || receipt.pointer.through_turn_id != self.source.through_turn_id
                || receipt.pointer.archived_message_count != self.source.messages.len()
                || self
                    .entries
                    .iter()
                    .rev()
                    .find_map(|entry| match &entry.event {
                        floe_agent_contract::JournalEvent::ValidatedBatch { batch } => {
                            Some(&batch.projection_coverage)
                        }
                        _ => None,
                    })
                    != Some(&receipt.summary.coverage)
                || recover_compaction_journal(&self.head, &self.source, &self.entries)?
                    != CompactionRecovery::Output(receipt.summary.text.clone())
            {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        Ok(())
    }

    pub fn authorize_actor(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        self.validate()?;
        if actor.person_id != self.head.person_id || actor.device_id != self.head.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    pub fn retire(&self, actor: &OwnerActor, failure: AgentFailure) -> Result<Self, AgentFailure> {
        self.authorize_actor(actor)?;
        if self.disposition != CompactionDisposition::Pending {
            return Err(AgentFailure::Conflict);
        }
        let mut next = self.clone();
        next.revision = next.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
        next.disposition = CompactionDisposition::Retired { failure };
        Ok(next)
    }

    /// Only a strictly newer active executor may adopt abandoned local work.
    /// Pending/uncertain effects become a retained terminal record, never a new
    /// model attempt. The immutable execution identity and journal are unchanged.
    pub fn recover(
        &self,
        actor: &OwnerActor,
        executor_generation: u64,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        self.validate()?;
        if actor.person_id != self.head.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        if self.disposition != CompactionDisposition::Pending
            || executor_generation <= self.claim.executor_generation
        {
            return Err(AgentFailure::Conflict);
        }
        let mut next = self.clone();
        next.revision = next.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
        next.claim.executor_generation = executor_generation;
        if actor.device_id != self.head.device_id
            || recover_compaction_journal(&self.head, &self.source, &self.entries)?
                == CompactionRecovery::Interrupted
        {
            next.disposition = CompactionDisposition::Retired {
                failure: AgentFailure::Interrupted,
            };
        }
        next.validate()?;
        Ok(next)
    }
}
