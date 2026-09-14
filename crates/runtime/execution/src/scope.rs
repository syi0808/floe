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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::budget::{BudgetConfig, BudgetLedger, ModelUsage};

    use super::*;

    #[tokio::test]
    async fn child_scope_preserves_root_identity_deadline_and_cancel_isolation() {
        let run_id = RunId::new();
        let task_id = TaskId::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 20), ModelUsage::default());
        let root = ExecutionScope::root(
            Cancellation::new(),
            deadline,
            ledger.work_lease(),
            TraceContext::new(run_id.as_uuid()).with_run_id(run_id),
        );
        let child = root.child_scope(deadline + Duration::from_secs(10), 30, 5, Some(task_id));
        let sibling = root.child_scope(deadline, 30, 5, None);
        assert_ne!(child.scope_id(), root.scope_id());
        assert_eq!(root.clone().scope_id(), root.scope_id());
        assert_eq!(child.parent_scope_id(), Some(root.scope_id()));
        assert_eq!(child.root_run_id(), Some(run_id));
        assert_eq!(child.task_id(), Some(task_id));
        assert_eq!(child.deadline(), root.deadline());
        child.cancellation().cancel();
        assert_eq!(
            child.run(async { Ok(42) }).await,
            Err(AgentFailure::Cancelled)
        );
        assert_eq!(sibling.run(async { Ok(42) }).await, Ok(42));
        assert!(!root.cancellation().is_cancelled());
    }

    #[test]
    fn child_attempts_consume_one_cumulative_scope_allowance() {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 20), ModelUsage::default());
        let root = ExecutionScope::root(
            Cancellation::new(),
            Instant::now() + Duration::from_secs(5),
            ledger.work_lease(),
            TraceContext::new(RunId::new().as_uuid()),
        );
        let child = root.child_scope(root.deadline(), 20, 5, None);
        let mut first = child.budget().begin(&mut 20, &mut 5).unwrap();
        first.mark_dispatched();
        first.settle(15, 3).unwrap();
        let grandchild = child.child_scope(root.deadline(), 100, 20, None);
        let mut tokens = 20;
        let mut cost = 5;
        let mut second = grandchild.budget().begin(&mut tokens, &mut cost).unwrap();
        assert_eq!((tokens, cost), (5, 2));
        second.mark_dispatched();
        second.settle(5, 2).unwrap();
        assert!(child.budget().begin(&mut 1, &mut 1).is_err());
        assert_eq!(ledger.snapshot().settled.tokens, 20);
    }

    #[test]
    fn only_root_can_use_one_bounded_finalization_after_child_failure() {
        let ledger = BudgetLedger::new(
            BudgetConfig::new(100, 20).with_finalization_reserve(10, 2),
            ModelUsage::default(),
        );
        let root = ExecutionScope::root(
            Cancellation::new(),
            Instant::now() + Duration::from_secs(2),
            ledger.work_lease(),
            TraceContext::new(RunId::new().as_uuid()),
        );
        let child = root.child_scope(root.deadline(), 90, 18, None);
        let mut work = child.budget().begin(&mut 90, &mut 18).unwrap();
        work.mark_dispatched();
        work.settle(90, 18).unwrap();
        child.cancellation().cancel();
        assert!(child.finalization_scope(Duration::from_secs(10)).is_err());
        let finalization = root.finalization_scope(Duration::from_secs(10)).unwrap();
        assert_eq!(finalization.deadline(), root.deadline());
        assert_eq!(finalization.parent_scope_id(), Some(root.scope_id()));
        let mut attempt = finalization.budget().begin(&mut 10, &mut 2).unwrap();
        attempt.mark_dispatched();
        attempt.settle(5, 1).unwrap();
        assert!(finalization.budget().begin(&mut 1, &mut 1).is_err());
        root.cancellation().cancel();
        assert!(matches!(
            root.finalization_scope(Duration::from_secs(10)),
            Err(AgentFailure::Cancelled)
        ));
        assert_eq!(ledger.snapshot().settled.tokens, 95);
    }
}
