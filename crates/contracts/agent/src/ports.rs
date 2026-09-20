use std::future::Future;

use uuid::Uuid;

use crate::{
    AgentFailure, AllowedCatalog, AuthorizedModelProjection, DelegationRequest, DependencyCoverage,
    ModelProjectionRequest, ModelRequest, ModelResponse, ModelStep, ProjectionRef, ReplayReceipt,
    TaskReceipt, ToolCall, ToolResult,
};
use serde::{Deserialize, Serialize};

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalAck {
    Accepted { revision: u64 },
    Replayed(Box<ReplayReceipt>),
}

/// One validated model batch: every step below was identity-, bound-, and
/// schema-checked before anything was dispatched. The pinned revisions record
/// what the batch was validated against, so a resume can fail closed when the
/// current catalog no longer carries them. The projection coverage is the exact
/// coverage of the authorized projection the answering model saw, so a batch
/// resumed after a crash commits the same dependency it was validated under
/// without re-projecting history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatedModelBatch {
    pub execution_id: Uuid,
    pub attempt_id: Uuid,
    pub projection_ref: ProjectionRef,
    pub batch_id: Uuid,
    pub steps: Vec<ModelStep>,
    pub catalog_revision: u64,
    pub tool_revisions: Vec<PinnedToolRevision>,
    pub agent_revisions: Vec<PinnedAgentRevision>,
    pub projection_coverage: DependencyCoverage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedToolRevision {
    pub tool_id: String,
    pub definition_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedAgentRevision {
    pub agent_id: String,
    pub definition_revision: u64,
}

/// Progress through a validated batch: the next step index to execute.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BatchCursor {
    pub batch_id: Uuid,
    pub next_step_index: u32,
}

impl BatchCursor {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.batch_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

impl ValidatedModelBatch {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        if self.execution_id.is_nil()
            || self.attempt_id.is_nil()
            || self.projection_ref.as_uuid().is_nil()
            || self.batch_id.is_nil()
            || self.steps.is_empty()
            || self.steps.len() > 1024
            || self.tool_revisions.iter().any(|pinned| {
                pinned.tool_id.trim().is_empty() || pinned.definition_revision == 0
            })
            || self.agent_revisions.iter().any(|pinned| {
                pinned.agent_id.trim().is_empty() || pinned.definition_revision == 0
            })
            || has_duplicate_tool_pins(&self.tool_revisions)
            || has_duplicate_agent_pins(&self.agent_revisions)
            || self.projection_coverage.validate().is_err()
            || serde_json::to_vec(&self.steps)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    /// Every step below was validated against these pins; the current catalog
    /// must still carry each one before the stored steps may execute.
    pub fn pinned_revisions_hold(&self, catalog: &AllowedCatalog) -> bool {
        self.tool_revisions.iter().all(|pinned| {
            catalog.tools.iter().any(|descriptor| {
                descriptor.id == pinned.tool_id
                    && descriptor.definition_revision == pinned.definition_revision
            })
        }) && self.agent_revisions.iter().all(|pinned| {
            catalog.cards.iter().any(|definition| {
                definition.card.id == pinned.agent_id
                    && definition.definition_revision == pinned.definition_revision
            })
        })
    }
}

fn has_duplicate_tool_pins(pins: &[PinnedToolRevision]) -> bool {
    let mut seen = std::collections::HashSet::new();
    pins.iter().any(|pinned| !seen.insert(&pinned.tool_id))
}

fn has_duplicate_agent_pins(pins: &[PinnedAgentRevision]) -> bool {
    let mut seen = std::collections::HashSet::new();
    pins.iter().any(|pinned| !seen.insert(&pinned.agent_id))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum JournalEvent {
    ModelIntent {
        attempt_id: uuid::Uuid,
        projection_ref: ProjectionRef,
    },
    ModelResult {
        attempt_id: uuid::Uuid,
        usage: crate::ModelUsage,
    },
    ToolIntent {
        call: ToolCall,
    },
    ToolResult {
        result: ToolResult,
    },
    DelegationIntent {
        request: DelegationRequest,
    },
    DelegationResult {
        receipt: Box<TaskReceipt>,
    },
    Output {
        text: String,
        artifacts: Vec<crate::Artifact>,
    },
    Checkpoint {
        iteration: u32,
    },
    ValidatedBatch {
        batch: ValidatedModelBatch,
    },
    BatchProgress {
        cursor: BatchCursor,
    },
}

pub trait ModelProjectionPort: Sync {
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>>;
}

pub trait ModelPort: Sync {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>>;
}

pub trait ToolPort: Sync {
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>>;
}

pub trait DelegationPort: Sync {
    fn delegate<'a>(
        &'a self,
        request: DelegationRequest,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>>;
}

pub trait ExecutionJournal: Send + Sync {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>>;
    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>>;
    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>>;
    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>>;
}
