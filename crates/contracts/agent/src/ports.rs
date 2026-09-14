use std::future::Future;

use crate::{
    DelegationRequest, ModelRequest, ModelResponse, ReplayReceipt, TaskReceipt, ToolCall,
    ToolResult,
};
use floe_kernel::AgentFailure;

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalAck {
    Accepted { revision: u64 },
    Replayed(Box<ReplayReceipt>),
}

#[derive(Clone, Debug)]
pub enum JournalEvent {
    ModelIntent {
        attempt_id: uuid::Uuid,
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

pub trait ExecutionJournal: Sync {
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
