//! Ready generation handles and request execution resources. Owners retain all
//! domain validation and operations; this host admits only its verified actor.
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{AgentFailure, OwnerActor, TraceContext};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use uuid::Uuid;

pub struct ReadyOwners {
    actor: OwnerActor,
    available: AtomicBool,
    pub connections: Arc<floe_connections::ConnectionsService>,
    pub conversation: Arc<dyn floe_conversation::ConversationOwner>,
    pub experts: Arc<dyn floe_experts::ExpertsOwner>,
    pub knowledge: Arc<dyn floe_knowledge::KnowledgeOwner>,
    pub actions: Arc<floe_actions::ActionsService>,
}
impl ReadyOwners {
    pub(crate) fn new(
        actor: OwnerActor,
        connections: Arc<floe_connections::ConnectionsService>,
        conversation: Arc<dyn floe_conversation::ConversationOwner>,
        experts: Arc<dyn floe_experts::ExpertsOwner>,
        knowledge: Arc<dyn floe_knowledge::KnowledgeOwner>,
        actions: Arc<floe_actions::ActionsService>,
    ) -> Self {
        Self {
            actor,
            available: AtomicBool::new(true),
            connections,
            conversation,
            experts,
            knowledge,
            actions,
        }
    }
    pub(crate) fn check(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if !self.available.load(Ordering::Acquire) {
            return Err(AgentFailure::VaultUnavailable);
        }
        if actor != &self.actor {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub(crate) fn close_admission(&self) -> Result<(), AgentFailure> {
        self.available.store(false, Ordering::Release);
        // Every owner is fenced even when a different close hook panics.
        let conversation = close_owner(|| self.conversation.close_admission());
        let experts = close_owner(|| self.experts.close_admission());
        let knowledge = close_owner(|| self.knowledge.close_admission());
        let actions = close_owner(|| self.actions.shutdown());
        let connections = close_owner(|| self.connections.shutdown());
        conversation
            .and(experts)
            .and(knowledge)
            .and(actions)
            .and(connections)
    }
}
fn close_owner(close: impl FnOnce()) -> Result<(), AgentFailure> {
    match catch_unwind(AssertUnwindSafe(close)) {
        Ok(()) => Ok(()),
        Err(payload) => {
            // A custom panic payload may itself panic when dropped. Retirement
            // must remain safe even when invoked during another unwind.
            std::mem::forget(payload);
            Err(AgentFailure::Interrupted)
        }
    }
}

/// Request scopes bound owner admission and reads. Background owner operations
/// detach only after durable admission and retain their own bounded lifetime.
pub fn host_scope(
    request_id: Uuid,
    cancellation: Cancellation,
    timeout: Duration,
) -> ExecutionScope {
    let ledger = BudgetLedger::new(BudgetConfig::new(1, 1), ModelUsage::default());
    ExecutionScope::root(
        cancellation,
        tokio::time::Instant::now() + timeout,
        ledger.root_lease(),
        TraceContext::new(request_id),
    )
}
impl crate::AppComposition {
    pub fn ready_owners(
        &self,
        caller: &crate::CallerContext,
    ) -> Result<Arc<ReadyOwners>, AgentFailure> {
        self.runtime_preparation.ready(caller)
    }
    pub fn execute_owner<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        execute_on_runtime(&self.runtime, future)
    }
}

/// Domain/storage polling belongs on Rust-owned stacks, never the foreign FFI
/// caller's stack. Keep the same budget as the physical Vault queue.
pub(crate) const EXECUTOR_STACK_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn execute_on_runtime<F>(runtime: &tokio::runtime::Runtime, future: F) -> F::Output
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    execute_on_handle(runtime.handle(), future)
}

pub(crate) fn execute_on_handle<F>(handle: &tokio::runtime::Handle, future: F) -> F::Output
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Enter the synchronous runtime boundary before spawning. A reentrant
    // caller must fail before work is detached from its host admission guard.
    // The caller polls only this dispatch envelope and the JoinHandle.
    let future = Box::pin(future);
    match handle.block_on(async move { tokio::spawn(future).await }) {
        Ok(output) => output,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(_) => panic!("owner executor stopped before returning its result"),
    }
}
