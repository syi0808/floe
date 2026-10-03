use std::future::Future;
use std::time::Duration;

use floe_kernel::{AgentFailure, RunId, ScopeId, TaskId, TraceContext};
use tokio::time::Instant;

use crate::budget::{BudgetLease, BudgetPartition};
use crate::{Cancellation, tasks};

#[derive(Clone, Debug)]
pub struct ExecutionScope {
    scope_id: ScopeId,
    parent_scope_id: Option<ScopeId>,
    cancellation: Cancellation,
    deadline: Instant,
    budget: BudgetLease,
    trace_context: TraceContext,
}

impl ExecutionScope {
    pub fn root(
        cancellation: Cancellation,
        deadline: Instant,
        budget: BudgetLease,
        trace_context: TraceContext,
    ) -> Self {
        Self {
            scope_id: ScopeId::new(),
            parent_scope_id: None,
            cancellation,
            deadline,
            budget,
            trace_context,
        }
    }

    pub fn child_scope(
        &self,
        deadline: Instant,
        max_tokens: u64,
        max_cost_micros: u64,
        task_id: Option<TaskId>,
    ) -> Self {
        let trace_context = task_id.map_or(self.trace_context, |task_id| {
            self.trace_context.with_task_id(task_id)
        });
        Self {
            scope_id: ScopeId::new(),
            parent_scope_id: Some(self.scope_id),
            cancellation: self.cancellation.child_scope(),
            deadline: deadline.min(self.deadline),
            budget: self.budget.child(max_tokens, max_cost_micros),
            trace_context,
        }
    }

    pub fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    pub fn finalization_scope(&self, max_duration: Duration) -> Result<Self, AgentFailure> {
        if self.parent_scope_id.is_some() || self.budget.partition() != BudgetPartition::Work {
            return Err(AgentFailure::InvalidInput);
        }
        if self.cancellation.is_cancelled() {
            return Err(tasks::cancellation_failure(&self.cancellation));
        }
        let now = Instant::now();
        if now >= self.deadline || max_duration.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        Ok(Self {
            scope_id: ScopeId::new(),
            parent_scope_id: Some(self.scope_id),
            cancellation: self.cancellation.child_scope(),
            deadline: now
                .checked_add(max_duration)
                .unwrap_or(self.deadline)
                .min(self.deadline),
            budget: self.budget.finalization_lease()?,
            trace_context: self.trace_context,
        })
    }

    pub fn parent_scope_id(&self) -> Option<ScopeId> {
        self.parent_scope_id
    }

    pub fn root_run_id(&self) -> Option<RunId> {
        self.trace_context.run_id()
    }

    pub fn task_id(&self) -> Option<TaskId> {
        self.trace_context.task_id()
    }

    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn budget(&self) -> &BudgetLease {
        &self.budget
    }

    pub fn trace_context(&self) -> TraceContext {
        self.trace_context
    }

    pub async fn run<Value>(
        &self,
        future: impl Future<Output = Result<Value, AgentFailure>>,
    ) -> Result<Value, AgentFailure> {
        tasks::run_bounded(future, self.deadline, &self.cancellation).await
    }
}
