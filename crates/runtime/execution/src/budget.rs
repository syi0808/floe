use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use floe_kernel::AgentFailure;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelUsage {
    pub attempts: u32,
    pub tokens: u64,
    pub cost_micros: u64,
    pub estimated_tokens: u64,
}

/// Compatibility projection over the canonical shared budget ledger. New
/// execution scopes should use [`BudgetLedger`] and [`BudgetLease`] directly.
#[derive(Clone, Debug)]
pub struct UsageLedger {
    budget: BudgetLedger,
}

impl Default for UsageLedger {
    fn default() -> Self {
        Self::new(u64::MAX, u64::MAX, ModelUsage::default())
    }
}

impl UsageLedger {
    pub fn new(max_tokens: u64, max_cost_micros: u64, usage: ModelUsage) -> Self {
        Self {
            budget: BudgetLedger::new(BudgetConfig::new(max_tokens, max_cost_micros), usage),
        }
    }

    pub fn snapshot(&self) -> ModelUsage {
        self.budget.usage()
    }

    pub fn work_lease(&self) -> BudgetLease {
        self.budget.work_lease()
    }

    pub fn finalization_lease(&self) -> Result<BudgetLease, AgentFailure> {
        self.budget.finalization_lease()
    }

    pub fn begin(&self, tokens: &mut u64, cost: &mut u64) -> Result<UsageAttempt, AgentFailure> {
        self.budget.work_lease().begin(tokens, cost)
    }
}

/// Identifies the root budget partition a lease may consume.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetPartition {
    Work,
    Finalization,
}

/// A root budget configuration. Finalization capacity is opt-in and clamped
/// to the total budget; a legacy work ledger does not reserve it implicitly.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub finalization_tokens: u64,
    pub finalization_cost_micros: u64,
}

impl BudgetConfig {
    pub fn new(max_tokens: u64, max_cost_micros: u64) -> Self {
        Self {
            max_tokens,
            max_cost_micros,
            finalization_tokens: 0,
            finalization_cost_micros: 0,
        }
    }

    pub fn with_finalization_reserve(mut self, tokens: u64, cost_micros: u64) -> Self {
        self.finalization_tokens = tokens.min(self.max_tokens);
        self.finalization_cost_micros = cost_micros.min(self.max_cost_micros);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetSnapshot {
    pub settled: ModelUsage,
    pub usage: ModelUsage,
    pub reserved_tokens: u64,
    pub reserved_cost_micros: u64,
    pub unknown_tokens: u64,
    pub unknown_cost_micros: u64,
}

#[derive(Debug)]
struct PartitionBudget {
    max_tokens: u64,
    max_cost_micros: u64,
    settled: ModelUsage,
    reserved_tokens: u64,
    reserved_cost_micros: u64,
    reserved_estimate_tokens: u64,
    reserved_estimate_cost_micros: u64,
    unknown_tokens: u64,
    unknown_cost_micros: u64,
    unknown_attempts: u32,
    pending_attempts: u32,
    finalization_dispatched: bool,
    finalization_in_flight: bool,
}

impl PartitionBudget {
    fn new(max_tokens: u64, max_cost_micros: u64, settled: ModelUsage) -> Self {
        let unknown_tokens = settled.estimated_tokens.min(settled.tokens);
        let settled = ModelUsage {
            tokens: settled.tokens.saturating_sub(unknown_tokens),
            estimated_tokens: 0,
            ..settled
        };
        Self {
            max_tokens,
            max_cost_micros,
            settled,
            reserved_tokens: 0,
            reserved_cost_micros: 0,
            reserved_estimate_tokens: 0,
            reserved_estimate_cost_micros: 0,
            unknown_tokens,
            unknown_cost_micros: 0,
            unknown_attempts: 0,
            pending_attempts: 0,
            finalization_dispatched: false,
            finalization_in_flight: false,
        }
    }

    fn outstanding_tokens(&self) -> u64 {
        self.settled
            .tokens
            .saturating_add(self.reserved_tokens)
            .saturating_add(self.unknown_tokens)
    }

    fn outstanding_cost(&self) -> u64 {
        self.settled
            .cost_micros
            .saturating_add(self.reserved_cost_micros)
            .saturating_add(self.unknown_cost_micros)
    }

    fn snapshot_usage(&self) -> ModelUsage {
        ModelUsage {
            attempts: self
                .settled
                .attempts
                .saturating_add(self.pending_attempts)
                .saturating_add(self.unknown_attempts),
            tokens: self
                .settled
                .tokens
                .saturating_add(self.reserved_estimate_tokens)
                .saturating_add(self.unknown_tokens),
            cost_micros: self.settled.cost_micros,
            estimated_tokens: self
                .reserved_estimate_tokens
                .saturating_add(self.unknown_tokens),
        }
    }
}

#[derive(Debug)]
struct BudgetState {
    max_tokens: u64,
    max_cost_micros: u64,
    work: PartitionBudget,
    finalization: PartitionBudget,
}

#[derive(Debug, Default)]
struct QuotaState {
    reserved_tokens: u64,
    reserved_cost_micros: u64,
    consumed_tokens: u64,
    consumed_cost_micros: u64,
}

#[derive(Debug)]
struct LeaseQuota {
    parent: Option<Arc<LeaseQuota>>,
    max_tokens: u64,
    max_cost_micros: u64,
    state: Mutex<QuotaState>,
}

impl LeaseQuota {
    fn new(parent: Option<Arc<LeaseQuota>>, max_tokens: u64, max_cost_micros: u64) -> Self {
        Self::with_consumed(parent, max_tokens, max_cost_micros, 0, 0)
    }

    fn with_consumed(
        parent: Option<Arc<LeaseQuota>>,
        max_tokens: u64,
        max_cost_micros: u64,
        consumed_tokens: u64,
        consumed_cost_micros: u64,
    ) -> Self {
        Self {
            parent,
            max_tokens,
            max_cost_micros,
            state: Mutex::new(QuotaState {
                consumed_tokens,
                consumed_cost_micros,
                ..QuotaState::default()
            }),
        }
    }

    fn try_reserve(&self, tokens: u64, cost_micros: u64) -> bool {
        if let Some(parent) = &self.parent
            && !parent.try_reserve(tokens, cost_micros)
        {
            return false;
        }
        let accepted = {
            let mut state = self.state.lock().unwrap();
            let accepted = state
                .consumed_tokens
                .saturating_add(state.reserved_tokens)
                .saturating_add(tokens)
                <= self.max_tokens
                && state
                    .consumed_cost_micros
                    .saturating_add(state.reserved_cost_micros)
                    .saturating_add(cost_micros)
                    <= self.max_cost_micros;
            if accepted {
                state.reserved_tokens = state.reserved_tokens.saturating_add(tokens);
                state.reserved_cost_micros = state.reserved_cost_micros.saturating_add(cost_micros);
            }
            accepted
        };
        if !accepted && let Some(parent) = &self.parent {
            parent.release(tokens, cost_micros);
        }
        accepted
    }

    fn available(&self) -> (u64, u64) {
        let own = {
            let state = self.state.lock().unwrap();
            (
                self.max_tokens
                    .saturating_sub(state.consumed_tokens.saturating_add(state.reserved_tokens)),
                self.max_cost_micros.saturating_sub(
                    state
                        .consumed_cost_micros
                        .saturating_add(state.reserved_cost_micros),
                ),
            )
        };
        self.parent.as_ref().map_or(own, |parent| {
            let parent_available = parent.available();
            (own.0.min(parent_available.0), own.1.min(parent_available.1))
        })
    }

    fn release(&self, tokens: u64, cost_micros: u64) {
        {
            let mut state = self.state.lock().unwrap();
            state.reserved_tokens = state.reserved_tokens.saturating_sub(tokens);
            state.reserved_cost_micros = state.reserved_cost_micros.saturating_sub(cost_micros);
        }
        if let Some(parent) = &self.parent {
            parent.release(tokens, cost_micros);
        }
    }

    fn settle(
        &self,
        reserved_tokens: u64,
        reserved_cost_micros: u64,
        tokens: u64,
        cost_micros: u64,
    ) {
        {
            let mut state = self.state.lock().unwrap();
            state.reserved_tokens = state.reserved_tokens.saturating_sub(reserved_tokens);
            state.reserved_cost_micros = state
                .reserved_cost_micros
                .saturating_sub(reserved_cost_micros);
            state.consumed_tokens = state.consumed_tokens.saturating_add(tokens);
            state.consumed_cost_micros = state.consumed_cost_micros.saturating_add(cost_micros);
        }
        if let Some(parent) = &self.parent {
            parent.settle(reserved_tokens, reserved_cost_micros, tokens, cost_micros);
        }
    }

    fn abandon(
        &self,
        reserved_tokens: u64,
        reserved_cost_micros: u64,
        unknown_tokens: u64,
        unknown_cost_micros: u64,
    ) {
        self.settle(
            reserved_tokens,
            reserved_cost_micros,
            unknown_tokens,
            unknown_cost_micros,
        );
    }
}

/// Canonical root budget ledger. Clones share one reservation state and can
/// therefore be safely handed to independent execution scopes.
#[derive(Clone, Debug)]
pub struct BudgetLedger {
    inner: Arc<Mutex<BudgetState>>,
    work_quota: Arc<LeaseQuota>,
    finalization_quota: Arc<LeaseQuota>,
}

impl BudgetLedger {
    pub fn new(config: BudgetConfig, usage: ModelUsage) -> Self {
        let finalization_tokens = config.finalization_tokens.min(config.max_tokens);
        let finalization_cost = config.finalization_cost_micros.min(config.max_cost_micros);
        let work_usage = usage;
        Self {
            inner: Arc::new(Mutex::new(BudgetState {
                max_tokens: config.max_tokens,
                max_cost_micros: config.max_cost_micros,
                work: PartitionBudget::new(
                    config.max_tokens.saturating_sub(finalization_tokens),
                    config.max_cost_micros.saturating_sub(finalization_cost),
                    work_usage,
                ),
                finalization: PartitionBudget::new(
                    finalization_tokens,
                    finalization_cost,
                    ModelUsage::default(),
                ),
            })),
            work_quota: Arc::new(LeaseQuota::with_consumed(
                None,
                config.max_tokens.saturating_sub(finalization_tokens),
                config.max_cost_micros.saturating_sub(finalization_cost),
                work_usage.tokens,
                work_usage.cost_micros,
            )),
            finalization_quota: Arc::new(LeaseQuota::new(
                None,
                finalization_tokens,
                finalization_cost,
            )),
        }
    }

    pub fn snapshot(&self) -> BudgetSnapshot {
        let state = self.inner.lock().unwrap();
        let work = &state.work;
        let finalization = &state.finalization;
        let work_usage = work.snapshot_usage();
        let finalization_usage = finalization.snapshot_usage();
        BudgetSnapshot {
            settled: ModelUsage {
                attempts: work
                    .settled
                    .attempts
                    .saturating_add(finalization.settled.attempts),
                tokens: work
                    .settled
                    .tokens
                    .saturating_add(finalization.settled.tokens),
                cost_micros: work
                    .settled
                    .cost_micros
                    .saturating_add(finalization.settled.cost_micros),
                estimated_tokens: 0,
            },
            usage: ModelUsage {
                attempts: work_usage
                    .attempts
                    .saturating_add(finalization_usage.attempts),
                tokens: work_usage.tokens.saturating_add(finalization_usage.tokens),
                cost_micros: work_usage
                    .cost_micros
                    .saturating_add(finalization_usage.cost_micros),
                estimated_tokens: work_usage
                    .estimated_tokens
                    .saturating_add(finalization_usage.estimated_tokens),
            },
            reserved_tokens: work.reserved_tokens + finalization.reserved_tokens,
            reserved_cost_micros: work.reserved_cost_micros + finalization.reserved_cost_micros,
            unknown_tokens: work.unknown_tokens + finalization.unknown_tokens,
            unknown_cost_micros: work.unknown_cost_micros + finalization.unknown_cost_micros,
        }
    }

    pub fn usage(&self) -> ModelUsage {
        self.snapshot().usage
    }

    pub fn root_lease(&self) -> BudgetLease {
        self.lease(BudgetPartition::Work)
    }

    pub fn work_lease(&self) -> BudgetLease {
        self.lease(BudgetPartition::Work)
    }

    pub fn finalization_lease(&self) -> Result<BudgetLease, AgentFailure> {
        let lease = self.lease(BudgetPartition::Finalization);
        let unavailable = {
            let state = self.inner.lock().unwrap();
            state.finalization.finalization_dispatched || state.finalization.finalization_in_flight
        };
        if lease.max_tokens == 0 || unavailable {
            Err(AgentFailure::BudgetExceeded)
        } else {
            Ok(lease)
        }
    }

    fn lease(&self, partition: BudgetPartition) -> BudgetLease {
        let state = self.inner.lock().unwrap();
        let budget = match partition {
            BudgetPartition::Work => &state.work,
            BudgetPartition::Finalization => &state.finalization,
        };
        BudgetLease {
            ledger: self.clone(),
            partition,
            max_tokens: budget.max_tokens,
            max_cost_micros: budget.max_cost_micros,
            quota: match partition {
                BudgetPartition::Work => self.work_quota.clone(),
                BudgetPartition::Finalization => self.finalization_quota.clone(),
            },
        }
    }

    fn partition_mut(state: &mut BudgetState, partition: BudgetPartition) -> &mut PartitionBudget {
        match partition {
            BudgetPartition::Work => &mut state.work,
            BudgetPartition::Finalization => &mut state.finalization,
        }
    }

    fn total_outstanding_tokens(state: &BudgetState) -> u64 {
        state
            .work
            .outstanding_tokens()
            .saturating_add(state.finalization.outstanding_tokens())
    }

    fn total_outstanding_cost(state: &BudgetState) -> u64 {
        state
            .work
            .outstanding_cost()
            .saturating_add(state.finalization.outstanding_cost())
    }
}

/// A cloneable view over one root ledger. Child leases retain the root cap but
/// can impose a smaller per-scope allowance.
#[derive(Clone, Debug)]
pub struct BudgetLease {
    ledger: BudgetLedger,
    partition: BudgetPartition,
    max_tokens: u64,
    max_cost_micros: u64,
    quota: Arc<LeaseQuota>,
}

impl BudgetLease {
    pub fn partition(&self) -> BudgetPartition {
        self.partition
    }

    pub fn max_tokens(&self) -> u64 {
        self.max_tokens
    }

    pub fn max_cost_micros(&self) -> u64 {
        self.max_cost_micros
    }

    pub fn finalization_lease(&self) -> Result<Self, AgentFailure> {
        if self.quota.parent.is_some() {
            return Err(AgentFailure::PolicyDenied);
        }
        self.ledger.finalization_lease()
    }

    pub fn child_lease(&self, max_tokens: u64, max_cost_micros: u64) -> Self {
        Self {
            ledger: self.ledger.clone(),
            partition: self.partition,
            max_tokens: max_tokens.min(self.max_tokens),
            max_cost_micros: max_cost_micros.min(self.max_cost_micros),
            quota: Arc::new(LeaseQuota::new(
                Some(self.quota.clone()),
                max_tokens.min(self.max_tokens),
                max_cost_micros.min(self.max_cost_micros),
            )),
        }
    }

    pub fn child(&self, max_tokens: u64, max_cost_micros: u64) -> Self {
        self.child_lease(max_tokens, max_cost_micros)
    }

    pub fn begin(
        &self,
        tokens: &mut u64,
        cost_micros: &mut u64,
    ) -> Result<BudgetAttempt, AgentFailure> {
        let mut state = self.ledger.inner.lock().unwrap();
        let finalization_blocked = self.partition == BudgetPartition::Finalization
            && (state.finalization.finalization_dispatched
                || state.finalization.finalization_in_flight);
        if finalization_blocked {
            return Err(AgentFailure::BudgetExceeded);
        }
        let total_available_tokens = state
            .max_tokens
            .saturating_sub(BudgetLedger::total_outstanding_tokens(&state));
        let total_available_cost = state
            .max_cost_micros
            .saturating_sub(BudgetLedger::total_outstanding_cost(&state));
        let (
            partition_max_tokens,
            partition_max_cost,
            partition_outstanding_tokens,
            partition_outstanding_cost,
        ) = match self.partition {
            BudgetPartition::Work => (
                state.work.max_tokens,
                state.work.max_cost_micros,
                state.work.outstanding_tokens(),
                state.work.outstanding_cost(),
            ),
            BudgetPartition::Finalization => (
                state.finalization.max_tokens,
                state.finalization.max_cost_micros,
                state.finalization.outstanding_tokens(),
                state.finalization.outstanding_cost(),
            ),
        };
        let partition_available_tokens =
            partition_max_tokens.saturating_sub(partition_outstanding_tokens);
        let partition_available_cost =
            partition_max_cost.saturating_sub(partition_outstanding_cost);
        let (quota_available_tokens, quota_available_cost) = self.quota.available();
        let allowance_tokens = (*tokens)
            .min(self.max_tokens)
            .min(total_available_tokens)
            .min(partition_available_tokens)
            .min(quota_available_tokens);
        let allowance_cost = (*cost_micros)
            .min(self.max_cost_micros)
            .min(total_available_cost)
            .min(partition_available_cost)
            .min(quota_available_cost);
        if allowance_tokens == 0 {
            return Err(AgentFailure::BudgetExceeded);
        }
        *tokens = allowance_tokens;
        *cost_micros = allowance_cost;
        let estimate_tokens = allowance_tokens.min(4096);
        if !self.quota.try_reserve(allowance_tokens, allowance_cost) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let partition = BudgetLedger::partition_mut(&mut state, self.partition);
        partition.reserved_tokens = partition.reserved_tokens.saturating_add(allowance_tokens);
        partition.reserved_cost_micros = partition
            .reserved_cost_micros
            .saturating_add(allowance_cost);
        partition.reserved_estimate_tokens = partition
            .reserved_estimate_tokens
            .saturating_add(estimate_tokens);
        partition.reserved_estimate_cost_micros = partition
            .reserved_estimate_cost_micros
            .saturating_add(allowance_cost);
        partition.pending_attempts = partition.pending_attempts.saturating_add(1);
        if self.partition == BudgetPartition::Finalization {
            partition.finalization_in_flight = true;
        }
        Ok(BudgetAttempt {
            ledger: self.ledger.clone(),
            partition: self.partition,
            allowance_tokens,
            allowance_cost_micros: allowance_cost,
            estimate_tokens,
            quota: self.quota.clone(),
            dispatched: false,
            settled: false,
        })
    }

    pub fn snapshot(&self) -> BudgetSnapshot {
        self.ledger.snapshot()
    }
}

pub type UsageAttempt = BudgetAttempt;
pub type BudgetLeaseAttempt = BudgetAttempt;

#[derive(Debug)]
pub struct BudgetAttempt {
    ledger: BudgetLedger,
    partition: BudgetPartition,
    allowance_tokens: u64,
    allowance_cost_micros: u64,
    estimate_tokens: u64,
    quota: Arc<LeaseQuota>,
    dispatched: bool,
    settled: bool,
}

impl BudgetAttempt {
    pub fn estimated_tokens(&self) -> u64 {
        self.estimate_tokens
    }

    pub fn reserved_tokens(&self) -> u64 {
        self.allowance_tokens
    }

    pub fn mark_dispatched(&mut self) {
        if self.dispatched || self.settled {
            return;
        }
        let mut state = self.ledger.inner.lock().unwrap();
        let partition = BudgetLedger::partition_mut(&mut state, self.partition);
        partition.pending_attempts = partition.pending_attempts.saturating_sub(1);
        partition.unknown_attempts = partition.unknown_attempts.saturating_add(1);
        if self.partition == BudgetPartition::Finalization {
            partition.finalization_in_flight = false;
            partition.finalization_dispatched = true;
        }
        self.dispatched = true;
    }

    pub fn settle(mut self, tokens: u64, cost_micros: u64) -> Result<(), AgentFailure> {
        if !self.dispatched {
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self.ledger.inner.lock().unwrap();
        let partition_overrun = {
            let partition = BudgetLedger::partition_mut(&mut state, self.partition);
            partition.reserved_tokens = partition
                .reserved_tokens
                .saturating_sub(self.allowance_tokens);
            partition.reserved_cost_micros = partition
                .reserved_cost_micros
                .saturating_sub(self.allowance_cost_micros);
            partition.reserved_estimate_tokens = partition
                .reserved_estimate_tokens
                .saturating_sub(self.estimate_tokens);
            partition.reserved_estimate_cost_micros = partition
                .reserved_estimate_cost_micros
                .saturating_sub(self.allowance_cost_micros);
            partition.unknown_attempts = partition.unknown_attempts.saturating_sub(1);
            partition.settled.attempts = partition.settled.attempts.saturating_add(1);
            partition.settled.tokens = partition.settled.tokens.saturating_add(tokens);
            partition.settled.cost_micros =
                partition.settled.cost_micros.saturating_add(cost_micros);
            tokens > self.allowance_tokens
                || cost_micros > self.allowance_cost_micros
                || tokens > self.quota.max_tokens
                || cost_micros > self.quota.max_cost_micros
                || partition.settled.tokens > partition.max_tokens
                || partition.settled.cost_micros > partition.max_cost_micros
        };
        self.settled = true;
        if BudgetLedger::total_outstanding_tokens(&state) > state.max_tokens
            || BudgetLedger::total_outstanding_cost(&state) > state.max_cost_micros
            || partition_overrun
        {
            self.quota.settle(
                self.allowance_tokens,
                self.allowance_cost_micros,
                tokens,
                cost_micros,
            );
            return Err(AgentFailure::BudgetExceeded);
        }
        self.quota.settle(
            self.allowance_tokens,
            self.allowance_cost_micros,
            tokens,
            cost_micros,
        );
        Ok(())
    }
}

impl Drop for BudgetAttempt {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let mut state = self.ledger.inner.lock().unwrap();
        let partition = BudgetLedger::partition_mut(&mut state, self.partition);
        partition.reserved_tokens = partition
            .reserved_tokens
            .saturating_sub(self.allowance_tokens);
        partition.reserved_cost_micros = partition
            .reserved_cost_micros
            .saturating_sub(self.allowance_cost_micros);
        partition.reserved_estimate_tokens = partition
            .reserved_estimate_tokens
            .saturating_sub(self.estimate_tokens);
        partition.reserved_estimate_cost_micros = partition
            .reserved_estimate_cost_micros
            .saturating_sub(self.allowance_cost_micros);
        if self.dispatched {
            partition.unknown_tokens = partition
                .unknown_tokens
                .saturating_add(self.estimate_tokens);
            partition.unknown_cost_micros = partition
                .unknown_cost_micros
                .saturating_add(self.allowance_cost_micros);
            self.quota.abandon(
                self.allowance_tokens,
                self.allowance_cost_micros,
                self.estimate_tokens,
                self.allowance_cost_micros,
            );
        } else {
            partition.pending_attempts = partition.pending_attempts.saturating_sub(1);
            if self.partition == BudgetPartition::Finalization {
                partition.finalization_in_flight = false;
            }
            self.quota
                .release(self.allowance_tokens, self.allowance_cost_micros);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undispatched_lease_drop_restores_budget_without_charging_an_attempt() {
        let ledger = UsageLedger::new(30, 7, ModelUsage::default());
        let reservation = ledger.begin(&mut 30, &mut 7).unwrap();
        assert_eq!(ledger.snapshot().estimated_tokens, 30);
        drop(reservation);
        assert_eq!(ledger.snapshot(), ModelUsage::default());

        let mut reservation = ledger.begin(&mut 30, &mut 7).unwrap();
        reservation.mark_dispatched();
        reservation.settle(10, 2).unwrap();
        assert_eq!(
            ledger.snapshot(),
            ModelUsage {
                attempts: 1,
                tokens: 10,
                cost_micros: 2,
                estimated_tokens: 0,
            }
        );
    }

    #[test]
    fn undispatched_lease_cannot_settle_provider_usage() {
        let ledger = UsageLedger::new(30, 7, ModelUsage::default());
        let reservation = ledger.begin(&mut 30, &mut 7).unwrap();
        assert_eq!(reservation.settle(10, 2), Err(AgentFailure::InvalidInput));
        assert_eq!(ledger.snapshot(), ModelUsage::default());
        assert!(ledger.begin(&mut 30, &mut 7).is_ok());
    }

    #[test]
    fn shared_budget_leases_reserve_concurrently_without_oversubscription() {
        let ledger = BudgetLedger::new(
            BudgetConfig::new(100, 10).with_finalization_reserve(20, 2),
            ModelUsage::default(),
        );
        let first_lease = ledger.work_lease();
        let second_lease = first_lease.clone();
        let mut first_tokens = 40;
        let mut first_cost = 4;
        let mut first = first_lease
            .begin(&mut first_tokens, &mut first_cost)
            .unwrap();
        let mut second_tokens = 40;
        let mut second_cost = 4;
        let second = second_lease
            .begin(&mut second_tokens, &mut second_cost)
            .unwrap();
        assert_eq!((first_tokens, first_cost), (40, 4));
        assert_eq!((second_tokens, second_cost), (40, 4));
        let mut rejected_tokens = 1;
        let mut rejected_cost = 1;
        assert!(matches!(
            first_lease.begin(&mut rejected_tokens, &mut rejected_cost),
            Err(AgentFailure::BudgetExceeded)
        ));
        first.mark_dispatched();
        first.settle(10, 1).unwrap();
        drop(second);
        assert_eq!(ledger.snapshot().settled.tokens, 10);
        assert_eq!(ledger.snapshot().reserved_tokens, 0);
    }

    #[test]
    fn finalization_is_explicit_and_single_dispatch() {
        let ledger = BudgetLedger::new(
            BudgetConfig::new(100, 10).with_finalization_reserve(20, 2),
            ModelUsage::default(),
        );
        let lease = ledger.finalization_lease().unwrap();
        let mut tokens = 20;
        let mut cost = 2;
        let mut attempt = lease.begin(&mut tokens, &mut cost).unwrap();
        attempt.mark_dispatched();
        assert!(matches!(
            lease.begin(&mut tokens, &mut cost),
            Err(AgentFailure::BudgetExceeded)
        ));
        attempt.settle(8, 1).unwrap();
        assert_eq!(ledger.snapshot().settled.tokens, 8);
        assert!(matches!(
            ledger.finalization_lease(),
            Err(AgentFailure::BudgetExceeded)
        ));
    }

    #[test]
    fn child_lease_caps_allowance_and_dispatch_drop_keeps_one_unknown_estimate() {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 10), ModelUsage::default());
        let lease = ledger.work_lease().child(30, 3);
        let mut tokens = 100;
        let mut cost = 100;
        let mut attempt = lease.begin(&mut tokens, &mut cost).unwrap();
        assert_eq!((tokens, cost), (30, 3));
        assert_eq!(attempt.estimated_tokens(), 30);
        attempt.mark_dispatched();
        drop(attempt);
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.unknown_tokens, 30);
        assert_eq!(snapshot.unknown_cost_micros, 3);
        assert_eq!(snapshot.settled.tokens, 0);
    }

    #[test]
    fn provider_overrun_is_recorded_before_budget_error() {
        let ledger = BudgetLedger::new(BudgetConfig::new(10, 2), ModelUsage::default());
        let lease = ledger.work_lease();
        let mut tokens = 10;
        let mut cost = 2;
        let mut attempt = lease.begin(&mut tokens, &mut cost).unwrap();
        attempt.mark_dispatched();
        assert_eq!(attempt.settle(12, 3), Err(AgentFailure::BudgetExceeded));
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.settled.tokens, 12);
        assert_eq!(snapshot.settled.cost_micros, 3);
    }

    #[test]
    fn abandoned_unknown_charge_survives_sibling_settlement() {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 10), ModelUsage::default());
        let lease = ledger.work_lease();
        let mut first_tokens = 40;
        let mut first_cost = 4;
        let mut first = lease.begin(&mut first_tokens, &mut first_cost).unwrap();
        let mut second_tokens = 40;
        let mut second_cost = 4;
        let mut second = lease.begin(&mut second_tokens, &mut second_cost).unwrap();
        first.mark_dispatched();
        drop(first);
        second.mark_dispatched();
        second.settle(10, 1).unwrap();
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.unknown_tokens, 40);
        assert_eq!(snapshot.unknown_cost_micros, 4);
        assert_eq!(snapshot.settled.tokens, 10);
        assert_eq!(snapshot.settled.cost_micros, 1);
    }

    #[test]
    fn resumed_usage_preserves_unknown_estimates_without_recharging() {
        let initial = ModelUsage {
            attempts: 2,
            tokens: 13,
            cost_micros: 3,
            estimated_tokens: 5,
        };
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 10), initial);
        assert_eq!(ledger.usage(), initial);
        assert_eq!(ledger.snapshot().settled.tokens, 8);
        assert_eq!(ledger.snapshot().unknown_tokens, 5);
        let mut attempt = ledger.work_lease().begin(&mut 10, &mut 2).unwrap();
        attempt.mark_dispatched();
        attempt.settle(7, 1).unwrap();
        assert_eq!(ledger.usage().tokens, 20);
        assert_eq!(ledger.usage().estimated_tokens, 5);
        assert_eq!(ledger.usage().attempts, 3);
    }

    #[test]
    fn active_dispatch_retains_full_allowance_not_only_unknown_estimate() {
        let ledger = BudgetLedger::new(BudgetConfig::new(10_000, 10), ModelUsage::default());
        let lease = ledger.work_lease();
        let mut first = lease.begin(&mut 10_000, &mut 10).unwrap();
        first.mark_dispatched();
        assert_eq!(first.estimated_tokens(), 4096);
        assert_eq!(ledger.snapshot().reserved_tokens, 10_000);
        assert!(lease.begin(&mut 1, &mut 0).is_err());
        drop(first);
        assert_eq!(ledger.snapshot().reserved_tokens, 0);
        assert_eq!(ledger.snapshot().unknown_tokens, 4096);
    }
}
