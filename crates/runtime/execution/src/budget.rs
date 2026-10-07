use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use serde::{Deserialize, Serialize};

use floe_kernel::AgentFailure;

/// Includes acknowledged tombstones; admission fails before any new charge.
pub const MAX_MODEL_ATTEMPTS_PER_SCOPE: usize = 256;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelUsage {
    pub attempts: u32,
    pub tokens: u64,
    pub cost_micros: u64,
    pub estimated_tokens: u64,
    pub estimated_cost_micros: u64,
}

/// A durable conservative upper bound fixed before the intent is acknowledged.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelReservationCeiling {
    pub tokens: u64,
    pub cost_micros: u64,
}
impl ModelReservationCeiling {
    /// Return the configured lease cap, not a snapshot of remaining allowance.
    pub fn for_lease(lease: &BudgetLease) -> Self {
        Self {
            tokens: lease.max_tokens(),
            cost_micros: lease.max_cost_micros(),
        }
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.tokens == 0 {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }
}

/// Provider observations and the certainty of each effective charge.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAccounting {
    pub observed_tokens: Option<u64>,
    pub observed_cost_micros: Option<u64>,
    pub unknown_tokens: bool,
    pub unknown_cost: bool,
}

impl ModelAccounting {
    pub fn validate_charge(&self, tokens: u64, cost_micros: u64) -> Result<(), AgentFailure> {
        let valid = |observed: Option<u64>, unknown: bool, charged: u64| match observed {
            Some(value) => !unknown && value == charged,
            None => unknown || charged == 0,
        };
        if !valid(self.observed_tokens, self.unknown_tokens, tokens)
            || !valid(self.observed_cost_micros, self.unknown_cost, cost_micros)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Exactly one terminal receipt for an admitted attempt in this live scope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAttemptReceipt {
    pub attempt_id: Uuid,
    pub charged_tokens: u64,
    pub charged_cost_micros: u64,
    pub accounting: ModelAccounting,
    pub dispatched: bool,
}

#[derive(Debug)]
enum ModelAttemptState {
    Pending { dispatched: bool },
    Terminal(ModelAttemptReceipt),
    Acknowledged,
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
        let unknown_cost_micros = settled.estimated_cost_micros.min(settled.cost_micros);
        let settled = ModelUsage {
            tokens: settled.tokens.saturating_sub(unknown_tokens),
            cost_micros: settled.cost_micros.saturating_sub(unknown_cost_micros),
            estimated_tokens: 0,
            estimated_cost_micros: 0,
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
            unknown_cost_micros,
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
            cost_micros: self
                .settled
                .cost_micros
                .saturating_add(self.unknown_cost_micros),
            estimated_tokens: self
                .reserved_estimate_tokens
                .saturating_add(self.unknown_tokens),
            estimated_cost_micros: self.unknown_cost_micros,
        }
    }
}

#[derive(Debug)]
struct BudgetState {
    max_tokens: u64,
    max_cost_micros: u64,
    work: PartitionBudget,
    finalization: PartitionBudget,
    model_attempts: HashMap<Uuid, ModelAttemptState>,
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
                model_attempts: HashMap::new(),
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
                estimated_cost_micros: 0,
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
                estimated_cost_micros: work_usage
                    .estimated_cost_micros
                    .saturating_add(finalization_usage.estimated_cost_micros),
            },
            reserved_tokens: work
                .reserved_tokens
                .saturating_add(finalization.reserved_tokens),
            reserved_cost_micros: work
                .reserved_cost_micros
                .saturating_add(finalization.reserved_cost_micros),
            unknown_tokens: work
                .unknown_tokens
                .saturating_add(finalization.unknown_tokens),
            unknown_cost_micros: work
                .unknown_cost_micros
                .saturating_add(finalization.unknown_cost_micros),
        }
    }

    pub fn usage(&self) -> ModelUsage {
        self.snapshot().usage
    }

    pub fn model_attempt_admitted(&self, attempt_id: Uuid) -> bool {
        self.inner
            .lock()
            .unwrap()
            .model_attempts
            .contains_key(&attempt_id)
    }

    pub fn model_attempt_receipt(&self, attempt_id: Uuid) -> Option<ModelAttemptReceipt> {
        match self.inner.lock().unwrap().model_attempts.get(&attempt_id) {
            Some(ModelAttemptState::Terminal(receipt)) => Some(*receipt),
            _ => None,
        }
    }

    /// Release a receipt only after its owner acknowledged durable ModelResult.
    /// The admitted identity remains reserved until this bounded ledger is dropped.
    pub fn acknowledge_model_attempt(&self, attempt_id: Uuid) -> Result<(), AgentFailure> {
        let mut state = self.inner.lock().unwrap();
        match state.model_attempts.get_mut(&attempt_id) {
            Some(value @ ModelAttemptState::Terminal(_)) => {
                *value = ModelAttemptState::Acknowledged;
                Ok(())
            }
            Some(ModelAttemptState::Acknowledged) => Ok(()),
            _ => Err(AgentFailure::Conflict),
        }
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

    /// Snapshot a conservative ceiling for the next model attempt.
    ///
    /// This is not a reservation: concurrent scopes can consume part or all of
    /// the returned allowance before dispatch. `begin_model_attempt` remains
    /// the atomic reservation point and clamps to the live ledger, partition,
    /// and ancestor quotas. Callers persist this ceiling before dispatch so a
    /// journal can account for an unresolved attempt conservatively.
    pub fn remaining_reservation_ceiling(&self) -> Result<ModelReservationCeiling, AgentFailure> {
        let state = self.ledger.inner.lock().unwrap();
        let (tokens, cost_micros) = self.available_allowance(&state);
        if tokens == 0 {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(ModelReservationCeiling {
            tokens,
            cost_micros,
        })
    }

    fn available_allowance(&self, state: &BudgetState) -> (u64, u64) {
        if state.model_attempts.len() >= MAX_MODEL_ATTEMPTS_PER_SCOPE
            || (self.partition == BudgetPartition::Finalization
                && (state.finalization.finalization_dispatched
                    || state.finalization.finalization_in_flight))
        {
            return (0, 0);
        }

        let total_available_tokens = state
            .max_tokens
            .saturating_sub(BudgetLedger::total_outstanding_tokens(state));
        let total_available_cost = state
            .max_cost_micros
            .saturating_sub(BudgetLedger::total_outstanding_cost(state));
        let partition = match self.partition {
            BudgetPartition::Work => &state.work,
            BudgetPartition::Finalization => &state.finalization,
        };
        let partition_available_tokens =
            partition.max_tokens.saturating_sub(partition.outstanding_tokens());
        let partition_available_cost = partition
            .max_cost_micros
            .saturating_sub(partition.outstanding_cost());
        let (quota_available_tokens, quota_available_cost) = self.quota.available();

        (
            self.max_tokens
                .min(total_available_tokens)
                .min(partition_available_tokens)
                .min(quota_available_tokens),
            self.max_cost_micros
                .min(total_available_cost)
                .min(partition_available_cost)
                .min(quota_available_cost),
        )
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

    pub fn begin_model_attempt(
        &self,
        attempt_id: Uuid,
        tokens: &mut u64,
        cost_micros: &mut u64,
    ) -> Result<BudgetAttempt, AgentFailure> {
        if attempt_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self.ledger.inner.lock().unwrap();
        if state.model_attempts.contains_key(&attempt_id) {
            return Err(AgentFailure::Conflict);
        }
        if state.model_attempts.len() >= MAX_MODEL_ATTEMPTS_PER_SCOPE {
            return Err(AgentFailure::BudgetExceeded);
        }
        let (available_tokens, available_cost) = self.available_allowance(&state);
        let allowance_tokens = (*tokens).min(available_tokens);
        let allowance_cost = (*cost_micros).min(available_cost);
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
        state
            .model_attempts
            .insert(attempt_id, ModelAttemptState::Pending { dispatched: false });
        Ok(BudgetAttempt {
            attempt_id,
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

    pub fn model_attempt_admitted(&self, attempt_id: Uuid) -> bool {
        self.ledger.model_attempt_admitted(attempt_id)
    }

    pub fn model_attempt_receipt(&self, attempt_id: Uuid) -> Option<ModelAttemptReceipt> {
        self.ledger.model_attempt_receipt(attempt_id)
    }

    pub fn acknowledge_model_attempt(&self, attempt_id: Uuid) -> Result<(), AgentFailure> {
        self.ledger.acknowledge_model_attempt(attempt_id)
    }
}

#[derive(Debug)]
pub struct BudgetAttempt {
    attempt_id: Uuid,
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
        if let Some(ModelAttemptState::Pending { dispatched }) =
            state.model_attempts.get_mut(&self.attempt_id)
        {
            *dispatched = true;
        }
        self.dispatched = true;
    }

    pub fn settle(self, tokens: u64, cost_micros: u64) -> Result<(), AgentFailure> {
        self.settle_observed(Some(tokens), Some(cost_micros))
            .map(|_| ())
    }

    pub fn settle_observed(
        mut self,
        tokens: Option<u64>,
        cost_micros: Option<u64>,
    ) -> Result<ModelAttemptReceipt, AgentFailure> {
        if !self.dispatched {
            return Err(AgentFailure::InvalidInput);
        }
        let charged_tokens = tokens.unwrap_or(self.estimate_tokens);
        let charged_cost_micros = cost_micros.unwrap_or(self.allowance_cost_micros);
        let receipt = ModelAttemptReceipt {
            attempt_id: self.attempt_id,
            charged_tokens,
            charged_cost_micros,
            accounting: ModelAccounting {
                observed_tokens: tokens,
                observed_cost_micros: cost_micros,
                unknown_tokens: tokens.is_none(),
                unknown_cost: cost_micros.is_none(),
            },
            dispatched: true,
        };
        let mut state = self.ledger.inner.lock().unwrap();
        let partition_overrun = {
            let partition = BudgetLedger::partition_mut(&mut state, self.partition);
            self.release_reservation(partition);
            partition.unknown_attempts = partition.unknown_attempts.saturating_sub(1);
            partition.settled.attempts = partition.settled.attempts.saturating_add(1);
            if let Some(tokens) = tokens {
                partition.settled.tokens = partition.settled.tokens.saturating_add(tokens);
            } else {
                partition.unknown_tokens = partition.unknown_tokens.saturating_add(charged_tokens);
            }
            if let Some(cost) = cost_micros {
                partition.settled.cost_micros = partition.settled.cost_micros.saturating_add(cost);
            } else {
                partition.unknown_cost_micros = partition
                    .unknown_cost_micros
                    .saturating_add(charged_cost_micros);
            }
            charged_tokens > self.allowance_tokens
                || charged_cost_micros > self.allowance_cost_micros
                || charged_tokens > self.quota.max_tokens
                || charged_cost_micros > self.quota.max_cost_micros
                || partition.outstanding_tokens() > partition.max_tokens
                || partition.outstanding_cost() > partition.max_cost_micros
        };
        // Publish once even for over-budget observations: accounting is never refunded.
        state
            .model_attempts
            .insert(self.attempt_id, ModelAttemptState::Terminal(receipt));
        self.settled = true;
        self.quota.settle(
            self.allowance_tokens,
            self.allowance_cost_micros,
            charged_tokens,
            charged_cost_micros,
        );
        if BudgetLedger::total_outstanding_tokens(&state) > state.max_tokens
            || BudgetLedger::total_outstanding_cost(&state) > state.max_cost_micros
            || partition_overrun
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(receipt)
    }

    fn release_reservation(&self, partition: &mut PartitionBudget) {
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
    }
}

impl Drop for BudgetAttempt {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let mut state = self.ledger.inner.lock().unwrap();
        let partition = BudgetLedger::partition_mut(&mut state, self.partition);
        self.release_reservation(partition);
        let (charged_tokens, charged_cost_micros) = if self.dispatched {
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
            (self.estimate_tokens, self.allowance_cost_micros)
        } else {
            partition.pending_attempts = partition.pending_attempts.saturating_sub(1);
            if self.partition == BudgetPartition::Finalization {
                partition.finalization_in_flight = false;
            }
            self.quota
                .release(self.allowance_tokens, self.allowance_cost_micros);
            (0, 0)
        };
        let receipt = ModelAttemptReceipt {
            attempt_id: self.attempt_id,
            charged_tokens,
            charged_cost_micros,
            accounting: ModelAccounting {
                observed_tokens: None,
                observed_cost_micros: None,
                unknown_tokens: self.dispatched,
                unknown_cost: self.dispatched,
            },
            dispatched: self.dispatched,
        };
        state
            .model_attempts
            .insert(self.attempt_id, ModelAttemptState::Terminal(receipt));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BudgetConfig, BudgetLedger, MAX_MODEL_ATTEMPTS_PER_SCOPE, ModelReservationCeiling,
        ModelUsage,
    };
    use floe_kernel::AgentFailure;
    use uuid::Uuid;

    #[test]
    fn remaining_ceiling_tracks_sequential_positive_cost_attempts() {
        let ledger = BudgetLedger::new(BudgetConfig::new(200, 200), ModelUsage::default());
        let task = ledger.work_lease().child_lease(100, 100);

        for (expected_tokens, expected_cost) in [(50, 100), (50, 77), (50, 54)] {
            let model_scope = task.child_lease(50, 100);
            assert_eq!(
                model_scope.remaining_reservation_ceiling().unwrap(),
                ModelReservationCeiling {
                    tokens: expected_tokens,
                    cost_micros: expected_cost,
                }
            );
            // The existing constructor continues to mean the configured cap.
            assert_eq!(
                ModelReservationCeiling::for_lease(&model_scope),
                ModelReservationCeiling {
                    tokens: 50,
                    cost_micros: 100,
                }
            );

            let ceiling = model_scope.remaining_reservation_ceiling().unwrap();
            let mut tokens = ceiling.tokens;
            let mut cost = ceiling.cost_micros;
            let mut attempt = model_scope
                .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
                .unwrap();
            assert_eq!((tokens, cost), (ceiling.tokens, ceiling.cost_micros));
            attempt.mark_dispatched();
            attempt.settle_observed(Some(3), Some(23)).unwrap();
        }

        assert_eq!(
            task.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 91,
                cost_micros: 31,
            }
        );
    }

    #[test]
    fn remaining_ceiling_observes_partition_ancestor_and_pending_allowances() {
        let ledger = BudgetLedger::new(
            BudgetConfig::new(100, 1_000).with_finalization_reserve(40, 400),
            ModelUsage::default(),
        );
        let work = ledger.work_lease();
        let wide_child = work.child_lease(1_000, 10_000);
        assert_eq!(
            wide_child.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 60,
                cost_micros: 600,
            }
        );
        assert_eq!(
            ledger
                .finalization_lease()
                .unwrap()
                .remaining_reservation_ceiling()
                .unwrap(),
            ModelReservationCeiling {
                tokens: 40,
                cost_micros: 400,
            }
        );

        let mut tokens = 10;
        let mut cost = 100;
        let pending = work
            .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
            .unwrap();
        assert_eq!(
            wide_child.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 50,
                cost_micros: 500,
            }
        );
        drop(pending);
        assert_eq!(
            wide_child.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 60,
                cost_micros: 600,
            }
        );

        let ancestor = work.child_lease(30, 300);
        let narrow_child = ancestor.child_lease(1_000, 10_000);
        assert_eq!(
            narrow_child.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 30,
                cost_micros: 300,
            }
        );
        let mut tokens = 5;
        let mut cost = 50;
        let mut attempt = ancestor
            .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
            .unwrap();
        attempt.mark_dispatched();
        attempt.settle_observed(Some(5), Some(50)).unwrap();
        assert_eq!(
            narrow_child.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 25,
                cost_micros: 250,
            }
        );
    }

    #[test]
    fn dispatch_reservation_stays_within_ceiling_after_concurrent_usage() {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 1_000), ModelUsage::default());
        let root = ledger.work_lease();
        let model_scope = root.child_lease(80, 800);
        let durable_ceiling = model_scope.remaining_reservation_ceiling().unwrap();
        assert_eq!(
            durable_ceiling,
            ModelReservationCeiling {
                tokens: 80,
                cost_micros: 800,
            }
        );

        let mut sibling_tokens = 50;
        let mut sibling_cost = 700;
        let mut sibling = root
            .begin_model_attempt(Uuid::new_v4(), &mut sibling_tokens, &mut sibling_cost)
            .unwrap();
        sibling.mark_dispatched();
        sibling.settle_observed(Some(10), Some(700)).unwrap();

        let mut tokens = durable_ceiling.tokens;
        let mut cost = durable_ceiling.cost_micros;
        let mut attempt = model_scope
            .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
            .unwrap();
        assert!(tokens <= durable_ceiling.tokens);
        assert!(cost <= durable_ceiling.cost_micros);
        assert_eq!((tokens, cost), (80, 300));
        attempt.mark_dispatched();
        attempt.settle_observed(Some(1), Some(100)).unwrap();
    }

    #[test]
    fn remaining_ceiling_charges_unresolved_attempts_and_reports_token_exhaustion() {
        let ledger = BudgetLedger::new(BudgetConfig::new(20, 100), ModelUsage::default());
        let task = ledger.work_lease().child_lease(20, 100);
        let model_scope = task.child_lease(10, 50);
        let ceiling = model_scope.remaining_reservation_ceiling().unwrap();
        let mut tokens = ceiling.tokens;
        let mut cost = ceiling.cost_micros;
        let mut unresolved = model_scope
            .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
            .unwrap();
        unresolved.mark_dispatched();
        drop(unresolved);

        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.unknown_tokens, 10);
        assert_eq!(snapshot.unknown_cost_micros, 50);
        assert_eq!(
            task.child_lease(10, 50)
                .remaining_reservation_ceiling()
                .unwrap(),
            ModelReservationCeiling {
                tokens: 10,
                cost_micros: 50,
            }
        );

        let next_scope = task.child_lease(10, 50);
        let next_ceiling = next_scope.remaining_reservation_ceiling().unwrap();
        let mut tokens = next_ceiling.tokens;
        let mut cost = next_ceiling.cost_micros;
        let mut settled = next_scope
            .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
            .unwrap();
        settled.mark_dispatched();
        settled.settle_observed(Some(10), Some(50)).unwrap();
        assert_eq!(
            task.child_lease(10, 50).remaining_reservation_ceiling(),
            Err(AgentFailure::BudgetExceeded)
        );

        let zero_cost_ledger =
            BudgetLedger::new(BudgetConfig::new(100, 100), ModelUsage::default());
        let zero_cost_scope = zero_cost_ledger.work_lease();
        let mut tokens = 1;
        let mut cost = 100;
        let mut attempt = zero_cost_scope
            .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
            .unwrap();
        attempt.mark_dispatched();
        attempt.settle_observed(Some(1), Some(100)).unwrap();
        assert_eq!(
            zero_cost_scope.remaining_reservation_ceiling().unwrap(),
            ModelReservationCeiling {
                tokens: 99,
                cost_micros: 0,
            }
        );

        let attempt_ledger =
            BudgetLedger::new(BudgetConfig::new(1, 0), ModelUsage::default());
        let attempt_scope = attempt_ledger.work_lease();
        for _ in 0..MAX_MODEL_ATTEMPTS_PER_SCOPE {
            let mut tokens = 1;
            let mut cost = 0;
            let attempt = attempt_scope
                .begin_model_attempt(Uuid::new_v4(), &mut tokens, &mut cost)
                .unwrap();
            drop(attempt);
        }
        assert_eq!(
            attempt_scope.remaining_reservation_ceiling(),
            Err(AgentFailure::BudgetExceeded)
        );
    }
}
