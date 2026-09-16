//! Dispatching one capability call with a durable record on both sides of it.
//!
//! The record is written before the call leaves and again when it settles, and
//! each write must be acknowledged: a caller that cannot record the intent must
//! not make the call, and a caller that cannot record the outcome must not
//! report success. This is role-neutral — a root Run and a delegated Expert
//! both go through it.

use std::{future::Future, pin::Pin};

use tokio::time::Instant;

use floe_agent_contract::{
    AgentFailure, CapabilityExecution, CapabilityExecutionState, CapabilityJournal,
};
use floe_execution::Cancellation;

pub async fn execute_recorded(
    journal: &dyn CapabilityJournal,
    mut record: CapabilityExecution,
    deadline: Instant,
    cancellation: &Cancellation,
    max_output_bytes: usize,
    execute: Pin<Box<impl Future<Output = Result<String, AgentFailure>>>>,
) -> Result<Result<String, AgentFailure>, AgentFailure> {
    check_running(deadline, cancellation)?;
    journal.record(record.clone()).await?;
    check_running(deadline, cancellation)?;
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
        _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
        result = execute => result,
    };
    if result
        .as_ref()
        .is_ok_and(|output| output.len() > max_output_bytes)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    record.state = if matches!(
        result,
        Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded)
    ) {
        CapabilityExecutionState::Interrupted
    } else {
        CapabilityExecutionState::Settled
    };
    record.result = Some(result.clone());
    journal.record(record).await?;
    Ok(result)
}

fn check_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}
