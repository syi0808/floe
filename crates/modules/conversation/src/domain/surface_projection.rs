//! Product projection of the Person-facing Surface, not Core execution turns.
use crate::{
    MAX_SURFACE_PAGE_ITEMS, PendingWorkCancellationEntry, RunRecord, RunState, SurfaceAnchor,
    SurfaceInputOrigin, SurfaceInputReceipt, SurfaceOutcomeReceipt, SurfaceOutcomeReference,
    SurfaceResultCompleteness, SurfaceResultReceipt, SurfaceResultReference, SurfaceWorkReceipt,
    WorkId,
};
use floe_conversation_contract::MessageId;
use floe_kernel::{AgentFailure, RunId};
use serde::Serialize;
use uuid::Uuid;

pub const MAX_SURFACE_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceReadRequest {
    pub session_id: Uuid,
    pub through: Option<SurfaceAnchor>,
    pub limit: usize,
}
impl SurfaceReadRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil() || self.limit == 0 || self.limit > MAX_SURFACE_PAGE_ITEMS {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(through) = self.through {
            through.validate()?;
            if through.session_id != self.session_id {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceInputKind {
    Text,
    Voice,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceMessageContent {
    Input {
        text: String,
        origin: SurfaceInputKind,
    },
    Result {
        run_id: RunId,
        completeness: SurfaceResultCompleteness,
        text: String,
    },
    Outcome {
        run_id: RunId,
        state: RunState,
        issue: Option<AgentFailure>,
        interaction_refs: Vec<Uuid>,
    },
    PendingWorkCancelled {
        work_id: WorkId,
        input_id: MessageId,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SurfaceMessage {
    pub message_id: MessageId,
    pub before: SurfaceAnchor,
    pub through: SurfaceAnchor,
    pub content: SurfaceMessageContent,
}
impl SurfaceMessage {
    fn validate_shape(&self) -> Result<(), AgentFailure> {
        let valid = match &self.content {
            SurfaceMessageContent::Input { text, .. } => {
                crate::normalize_turn_text(text).is_ok_and(|normalized| normalized == *text)
            }
            SurfaceMessageContent::Result { run_id, text, .. } => {
                run_id.is_valid()
                    && !text.trim().is_empty()
                    && text.len() <= floe_agent_contract::MAX_OUTPUT_BYTES
            }
            SurfaceMessageContent::Outcome {
                run_id,
                state,
                issue,
                interaction_refs,
            } => {
                let mut seen = std::collections::HashSet::new();
                run_id.is_valid()
                    && state.is_terminal()
                    && *state != RunState::Completed
                    && interaction_refs.len() <= crate::MAX_ACTIVE_INTERACTIONS_PER_RUN
                    && interaction_refs
                        .iter()
                        .all(|id| !id.is_nil() && seen.insert(*id))
                    && if *state == RunState::Blocked {
                        issue.is_none() && !interaction_refs.is_empty()
                    } else {
                        issue.is_some() && interaction_refs.is_empty()
                    }
            }
            SurfaceMessageContent::PendingWorkCancelled { work_id, input_id } => {
                !work_id.as_uuid().is_nil() && input_id.is_valid()
            }
        };
        if valid {
            Ok(())
        } else {
            Err(AgentFailure::StorageUnavailable)
        }
    }

    pub fn input(receipt: &SurfaceInputReceipt) -> Result<Self, AgentFailure> {
        receipt.validate()?;
        Ok(Self {
            message_id: receipt.input.message_id,
            before: receipt.before,
            through: receipt.through,
            content: SurfaceMessageContent::Input {
                text: receipt.input.text.clone(),
                origin: match &receipt.input.origin {
                    SurfaceInputOrigin::Text => SurfaceInputKind::Text,
                    SurfaceInputOrigin::Voice { .. } => SurfaceInputKind::Voice,
                },
            },
        })
    }
    pub fn result(receipt: &SurfaceResultReceipt, run: &RunRecord) -> Result<Self, AgentFailure> {
        receipt.validate()?;
        if SurfaceResultReference::for_run(run)? != receipt.result
            || run.person_id != receipt.person_id
            || run.device_id != receipt.device_id
            || run.session_id != receipt.through.session_id
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Self {
            message_id: receipt.result.message_id(),
            before: receipt.before,
            through: receipt.through,
            content: SurfaceMessageContent::Result {
                run_id: run.run_id,
                completeness: receipt.result.completeness,
                text: run.output.clone().ok_or(AgentFailure::StorageUnavailable)?,
            },
        })
    }
    pub fn outcome(receipt: &SurfaceOutcomeReceipt, run: &RunRecord) -> Result<Self, AgentFailure> {
        receipt.validate()?;
        if SurfaceOutcomeReference::for_run(run)? != receipt.outcome
            || run.person_id != receipt.person_id
            || run.device_id != receipt.device_id
            || run.session_id != receipt.through.session_id
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Self {
            message_id: receipt.outcome.message_id(),
            before: receipt.before,
            through: receipt.through,
            content: SurfaceMessageContent::Outcome {
                run_id: run.run_id,
                state: run.state,
                issue: run.issue,
                interaction_refs: run
                    .blocked
                    .as_ref()
                    .map_or_else(Vec::new, |block| block.interaction_refs()),
            },
        })
    }
    pub fn cancellation(
        receipt: &PendingWorkCancellationEntry,
        work: &SurfaceWorkReceipt,
    ) -> Result<Self, AgentFailure> {
        receipt.validate()?;
        work.validate()?;
        if work.command_id != receipt.cancellation.work_command_id
            || work.work_id != receipt.cancellation.work_id
            || work.person_id != receipt.cancellation.person_id
            || work.anchor.session_id != receipt.through.session_id
            || work.anchor.sequence > receipt.before.sequence
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Self {
            message_id: receipt.message_id(),
            before: receipt.before,
            through: receipt.through,
            content: SurfaceMessageContent::PendingWorkCancelled {
                work_id: work.work_id,
                input_id: work.input_id,
            },
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SurfaceSnapshot {
    pub through: SurfaceAnchor,
    pub messages: Vec<SurfaceMessage>,
    pub older: Option<SurfaceAnchor>,
}
impl SurfaceSnapshot {
    /// Bounded product pages never insert summaries or clip original bodies.
    /// Callers supply a verified contiguous suffix ending at the requested head.
    pub fn from_suffix(
        through: SurfaceAnchor,
        mut messages: Vec<SurfaceMessage>,
    ) -> Result<Self, AgentFailure> {
        through.validate()?;
        if messages.len() > MAX_SURFACE_PAGE_ITEMS || (through.sequence == 0) != messages.is_empty()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut after = messages.first().map(|message| message.before);
        let mut ids = std::collections::HashSet::new();
        for message in &messages {
            message.validate_shape()?;
            message.before.validate()?;
            message.through.validate()?;
            if !message.message_id.is_valid()
                || !ids.insert(message.message_id)
                || message.through.session_id != through.session_id
                || message.before.session_id != through.session_id
                || Some(message.before) != after
                || message.before.sequence.checked_add(1) != Some(message.through.sequence)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            after = Some(message.through);
        }
        if through.sequence > 0 && after != Some(through) {
            return Err(AgentFailure::StorageUnavailable);
        }
        // Compute entry costs once; leave a conservative bound for two anchors,
        // keys and array punctuation, then verify the exact final serialization.
        let mut bytes = 2048usize;
        let mut start = messages.len();
        while start > 0 {
            let size = serde_json::to_vec(&messages[start - 1])
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .len();
            if bytes
                .checked_add(size + 1)
                .is_none_or(|next| next > MAX_SURFACE_SNAPSHOT_BYTES)
            {
                break;
            }
            bytes += size + 1;
            start -= 1;
        }
        if !messages.is_empty() && start == messages.len() {
            return Err(AgentFailure::BudgetExceeded);
        }
        messages.drain(..start);
        let older = messages
            .first()
            .map(|message| message.before)
            .filter(|anchor| anchor.sequence > 0);
        let snapshot = Self {
            through,
            messages,
            older,
        };
        if serde_json::to_vec(&snapshot)
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .len()
            > MAX_SURFACE_SNAPSHOT_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests;
