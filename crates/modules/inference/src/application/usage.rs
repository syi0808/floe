//! The attempt-accounting ledger one invocation runs against.
//!
//! Inference owns model attempts, so it owns the ledger that budgets them and
//! the journal they are reported to. Callers that keep their own records — a
//! Conversation capability execution, for example — journal those separately.

use tokio::sync::{mpsc::UnboundedSender, oneshot};

use floe_agent_contract::AgentFailure;
use floe_execution::budget::{ModelUsage, UsageAttempt};

use super::{AttemptJournal, ModelAttemptRecord};

/// One attempt record and the acknowledgement its writer waits for.
#[derive(Debug)]
pub struct AttemptUpdate {
    pub record: ModelAttemptRecord,
    pub acknowledged: oneshot::Sender<Result<(), AgentFailure>>,
}

/// The budget one invocation spends, and where its attempts are recorded.
///
/// Without a journal the ledger still enforces the budget; the owner attaches
/// one for the span in which it can durably accept attempt records.
#[derive(Clone, Debug, Default)]
pub struct UsageLedger {
    budget: floe_execution::budget::UsageLedger,
    journal: Option<UnboundedSender<AttemptUpdate>>,
}

impl UsageLedger {
    pub fn new(max_tokens: u64, max_cost_micros: u64, usage: ModelUsage) -> Self {
        Self {
            budget: floe_execution::budget::UsageLedger::new(max_tokens, max_cost_micros, usage),
            journal: None,
        }
    }

    /// Report this ledger's attempts to `journal` until the returned ledger is
    /// dropped. The original ledger keeps whatever journal it already had.
    pub fn with_journal(mut self, journal: UnboundedSender<AttemptUpdate>) -> Self {
        self.journal = Some(journal);
        self
    }

    pub fn snapshot(&self) -> ModelUsage {
        self.budget.snapshot()
    }

    pub fn begin(&self, tokens: &mut u64, cost: &mut u64) -> Result<UsageAttempt, AgentFailure> {
        self.budget.begin(tokens, cost)
    }
}

impl AttemptJournal for UsageLedger {
    async fn record_attempt(&self, record: ModelAttemptRecord) -> Result<(), AgentFailure> {
        let Some(journal) = &self.journal else {
            return Ok(());
        };
        let (acknowledged, receiver) = oneshot::channel();
        journal
            .send(AttemptUpdate {
                record,
                acknowledged,
            })
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        receiver
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
    }
}
