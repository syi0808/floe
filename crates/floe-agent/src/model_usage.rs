use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{AgentFailure, AgentUsage};

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
    journal: Option<tokio::sync::mpsc::UnboundedSender<crate::model_journal::JournalUpdate>>,
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
        Self::new(u64::MAX, u64::MAX, AgentUsage::default())
    }
}

impl UsageLedger {
    pub fn new(max_tokens: u64, max_cost_micros: u64, usage: AgentUsage) -> Self {
        Self {
            journal: None,
            inner: Arc::new(Mutex::new(LedgerState {
                active: false,
                max_tokens,
                max_cost_micros,
                usage: ModelUsage {
                    attempts: usage.model_attempts,
                    tokens: usage.tokens,
                    cost_micros: usage.cost_micros,
                    estimated_tokens: usage.estimated_tokens,
                },
            })),
        }
    }

    pub fn snapshot(&self) -> ModelUsage {
        self.inner.lock().unwrap().usage
    }

    pub(crate) fn undispatched(&self, reserved: u64) {
        let mut state = self.inner.lock().unwrap();
        state.usage.attempts = state.usage.attempts.saturating_sub(1);
        state.usage.tokens = state.usage.tokens.saturating_sub(reserved);
        state.usage.estimated_tokens = state.usage.estimated_tokens.saturating_sub(reserved);
        state.active = false;
    }

    pub(crate) fn with_journal(
        mut self,
        journal: tokio::sync::mpsc::UnboundedSender<crate::model_journal::JournalUpdate>,
    ) -> Self {
        self.journal = Some(journal);
        self
    }

    pub(crate) async fn record(
        &self,
        record: crate::ModelAttemptRecord,
    ) -> Result<(), AgentFailure> {
        self.record_entry(crate::model_journal::JournalRecord::Model(record))
            .await
    }

    pub(crate) async fn record_capability(
        &self,
        record: crate::CapabilityExecution,
    ) -> Result<(), AgentFailure> {
        self.record_entry(crate::model_journal::JournalRecord::Capability(record))
            .await
    }

    async fn record_entry(
        &self,
        record: crate::model_journal::JournalRecord,
    ) -> Result<(), AgentFailure> {
        if let Some(journal) = &self.journal {
            let (acknowledged, receiver) = tokio::sync::oneshot::channel();
            journal
                .send(crate::model_journal::JournalUpdate {
                    record,
                    acknowledged,
                })
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            receiver
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)??;
        }
        Ok(())
    }

    pub fn sync(&self, usage: &mut AgentUsage) {
        let model = self.snapshot();
        usage.tokens = model.tokens;
        usage.cost_micros = model.cost_micros;
        usage.model_attempts = model.attempts;
        usage.estimated_tokens = model.estimated_tokens;
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
            settled: false,
        })
    }
}

pub struct UsageAttempt {
    ledger: UsageLedger,
    reserved: u64,
    settled: bool,
}

impl UsageAttempt {
    pub fn settle(mut self, tokens: u64, cost_micros: u64) -> Result<(), AgentFailure> {
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
            self.ledger.inner.lock().unwrap().active = false;
        }
    }
}
