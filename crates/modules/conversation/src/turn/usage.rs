use crate::{AgentUsage};
use floe_agent_contract::{AgentFailure};

pub use floe_execution::budget::{ModelUsage, UsageAttempt};

#[derive(Clone, Debug)]
pub struct UsageLedger {
    budget: floe_execution::budget::UsageLedger,
    journal: Option<tokio::sync::mpsc::UnboundedSender<crate::turn::journal::JournalUpdate>>,
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
            budget: floe_execution::budget::UsageLedger::new(
                max_tokens,
                max_cost_micros,
                ModelUsage {
                    attempts: usage.model_attempts,
                    tokens: usage.tokens,
                    cost_micros: usage.cost_micros,
                    estimated_tokens: usage.estimated_tokens,
                },
            ),
        }
    }

    pub fn snapshot(&self) -> ModelUsage {
        self.budget.snapshot()
    }

    pub(crate) fn with_journal(
        mut self,
        journal: tokio::sync::mpsc::UnboundedSender<crate::turn::journal::JournalUpdate>,
    ) -> Self {
        self.journal = Some(journal);
        self
    }

    pub(crate) async fn record_capability(
        &self,
        record: crate::turn::CapabilityExecution,
    ) -> Result<(), AgentFailure> {
        self.record_entry(crate::turn::journal::JournalRecord::Capability(Box::new(
            record,
        )))
        .await
    }

    async fn record_entry(
        &self,
        record: crate::turn::journal::JournalRecord,
    ) -> Result<(), AgentFailure> {
        if let Some(journal) = &self.journal {
            let (acknowledged, receiver) = tokio::sync::oneshot::channel();
            journal
                .send(crate::turn::journal::JournalUpdate {
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
        self.budget.begin(tokens, cost)
    }
}

impl floe_inference::AttemptJournal for UsageLedger {
    async fn record_attempt(&self, record: floe_inference::ModelAttemptRecord) -> Result<(), AgentFailure> {
        self.record_entry(crate::turn::journal::JournalRecord::Model(record))
            .await
    }
}
