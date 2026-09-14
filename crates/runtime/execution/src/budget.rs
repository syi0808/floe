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

#[derive(Clone, Debug)]
pub struct UsageLedger {
    inner: Arc<Mutex<LedgerState>>,
}

#[derive(Debug)]
struct LedgerState {
    active: bool,
    usage: ModelUsage,
    max_tokens: u64,
    max_cost_micros: u64,
}

impl Default for UsageLedger {
    fn default() -> Self {
        Self::new(u64::MAX, u64::MAX, ModelUsage::default())
    }
}

impl UsageLedger {
    pub fn new(max_tokens: u64, max_cost_micros: u64, usage: ModelUsage) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LedgerState {
                active: false,
                max_tokens,
                max_cost_micros,
                usage,
            })),
        }
    }

    pub fn snapshot(&self) -> ModelUsage {
        self.inner.lock().unwrap().usage
    }

    pub fn begin(&self, tokens: &mut u64, cost: &mut u64) -> Result<UsageAttempt, AgentFailure> {
        let mut state = self.inner.lock().unwrap();
        if state.active {
            return Err(AgentFailure::ModelUnavailable);
        }
        *tokens = (*tokens).min(state.max_tokens.saturating_sub(state.usage.tokens));
        *cost = (*cost).min(
            state
                .max_cost_micros
                .saturating_sub(state.usage.cost_micros),
        );
        if *tokens == 0 || state.usage.cost_micros > state.max_cost_micros {
            return Err(AgentFailure::BudgetExceeded);
        }
        let reserved = (*tokens).min(4096);
        state.usage.attempts = state
            .usage
            .attempts
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        state.usage.tokens = state.usage.tokens.saturating_add(reserved);
        state.usage.estimated_tokens = state.usage.estimated_tokens.saturating_add(reserved);
        state.active = true;
        Ok(UsageAttempt {
            ledger: self.clone(),
            reserved,
            dispatched: false,
            settled: false,
        })
    }
}

pub struct UsageAttempt {
    ledger: UsageLedger,
    reserved: u64,
    dispatched: bool,
    settled: bool,
}

impl UsageAttempt {
    pub fn mark_dispatched(&mut self) {
        self.dispatched = true;
    }

    pub fn estimated_tokens(&self) -> u64 {
        self.reserved
    }

    pub fn settle(mut self, tokens: u64, cost_micros: u64) -> Result<(), AgentFailure> {
        if !self.dispatched {
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self.ledger.inner.lock().unwrap();
        self.settled = true;
        state.active = false;
        state.usage.tokens = state
            .usage
            .tokens
            .saturating_sub(self.reserved)
            .saturating_add(tokens);
        state.usage.estimated_tokens = state.usage.estimated_tokens.saturating_sub(self.reserved);
        state.usage.cost_micros = state.usage.cost_micros.saturating_add(cost_micros);
        if state.usage.tokens > state.max_tokens || state.usage.cost_micros > state.max_cost_micros
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }
}

impl Drop for UsageAttempt {
    fn drop(&mut self) {
        if !self.settled {
            let mut state = self.ledger.inner.lock().unwrap();
            if !self.dispatched {
                state.usage.attempts = state.usage.attempts.saturating_sub(1);
                state.usage.tokens = state.usage.tokens.saturating_sub(self.reserved);
                state.usage.estimated_tokens =
                    state.usage.estimated_tokens.saturating_sub(self.reserved);
            }
            state.active = false;
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
}
