//! Ready generation handles and request execution resources. Owners retain all
//! domain validation and operations; this host admits only its verified actor.
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{AgentFailure, OwnerActor, TraceContext};
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
            conversation, experts, knowledge, actions,
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
    pub(crate) fn close_admission(&self) {
        self.available.store(false, Ordering::Release);
        self.conversation.close_admission();
        self.experts.close_admission();
        self.knowledge.close_admission();
        self.actions.shutdown();
        self.connections.shutdown();
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
        self.agent_vault.ready(caller)
    }
    pub fn execute_owner<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }
}
