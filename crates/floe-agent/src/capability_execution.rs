use std::{future::Future, pin::Pin};

use tokio::time::Instant;

use crate::{
    AgentFailure, Cancellation, CapabilityExecution, CapabilityExecutionState, UsageLedger,
};

pub(crate) async fn execute_recorded(
    ledger: &UsageLedger,
    mut record: CapabilityExecution,
    deadline: Instant,
    cancellation: &Cancellation,
    max_output_bytes: usize,
    execute: Pin<Box<impl Future<Output = Result<String, AgentFailure>>>>,
) -> Result<Result<String, AgentFailure>, AgentFailure> {
    check_running(deadline, cancellation)?;
    ledger.record_capability(record.clone()).await?;
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
    ledger.record_capability(record).await?;
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use uuid::Uuid;

    use super::*;
    use crate::model_journal::{JournalRecord, JournalUpdate};

    fn record() -> CapabilityExecution {
        CapabilityExecution {
            scope_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            call_id: Uuid::new_v4(),
            capability_id: "schedule.find_free_windows".into(),
            input: "{}".into(),
            state: CapabilityExecutionState::Started,
            result: None,
            replay: None,
        }
    }

    fn execution(update: &JournalUpdate) -> &CapabilityExecution {
        let JournalRecord::Capability(record) = &update.record else {
            panic!("expected capability execution");
        };
        record
    }

    #[tokio::test]
    async fn dispatch_and_result_wait_for_durable_acknowledgment() {
        for fail_start in [true, false] {
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let ledger = UsageLedger::default().with_journal(sender);
            let dispatched = Arc::new(AtomicUsize::new(0));
            let calls = dispatched.clone();
            let started = record();
            let expected = started.clone();
            let task = tokio::spawn(async move {
                execute_recorded(
                    &ledger,
                    started,
                    Instant::now() + Duration::from_secs(5),
                    &Cancellation::default(),
                    4096,
                    Box::pin(async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok("private result".into())
                    }),
                )
                .await
            });
            let update = receiver.recv().await.unwrap();
            assert_eq!(execution(&update), &expected);
            assert_eq!(dispatched.load(Ordering::SeqCst), 0);
            assert!(!task.is_finished());
            if fail_start {
                update
                    .acknowledged
                    .send(Err(AgentFailure::StorageUnavailable))
                    .unwrap();
            } else {
                update.acknowledged.send(Ok(())).unwrap();
                let settled = receiver.recv().await.unwrap();
                assert_eq!(execution(&settled).call_id, expected.call_id);
                assert_eq!(execution(&settled).scope_id, expected.scope_id);
                assert_eq!(execution(&settled).state, CapabilityExecutionState::Settled);
                assert_eq!(
                    execution(&settled).result,
                    Some(Ok("private result".into()))
                );
                assert!(!task.is_finished());
                settled
                    .acknowledged
                    .send(Err(AgentFailure::StorageUnavailable))
                    .unwrap();
            }
            assert_eq!(task.await.unwrap(), Err(AgentFailure::StorageUnavailable));
            assert_eq!(dispatched.load(Ordering::SeqCst), usize::from(!fail_start));
        }
    }

    #[tokio::test]
    async fn tool_failure_is_settled_but_oversized_output_is_not_retained() {
        for oversized in [true, false] {
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let ledger = UsageLedger::default().with_journal(sender);
            let task = tokio::spawn(async move {
                execute_recorded(
                    &ledger,
                    record(),
                    Instant::now() + Duration::from_secs(5),
                    &Cancellation::default(),
                    4,
                    Box::pin(async move {
                        if oversized {
                            Ok("oversized private result".into())
                        } else {
                            Err(AgentFailure::StaleContext)
                        }
                    }),
                )
                .await
            });
            receiver
                .recv()
                .await
                .unwrap()
                .acknowledged
                .send(Ok(()))
                .unwrap();
            if oversized {
                assert_eq!(task.await.unwrap(), Err(AgentFailure::BudgetExceeded));
                assert!(receiver.recv().await.is_none());
            } else {
                let settled = receiver.recv().await.unwrap();
                assert_eq!(
                    execution(&settled).result,
                    Some(Err(AgentFailure::StaleContext))
                );
                settled.acknowledged.send(Ok(())).unwrap();
                assert_eq!(task.await.unwrap(), Ok(Err(AgentFailure::StaleContext)));
            }
        }
    }

    #[tokio::test]
    async fn cancellation_during_intent_commit_prevents_dispatch() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let ledger = UsageLedger::default().with_journal(sender);
        let cancellation = Cancellation::default();
        let stop = cancellation.clone();
        let task = tokio::spawn(async move {
            execute_recorded(
                &ledger,
                record(),
                Instant::now() + Duration::from_secs(5),
                &cancellation,
                4096,
                Box::pin(async { panic!("cancelled intent must not dispatch") }),
            )
            .await
        });
        let started = receiver.recv().await.unwrap();
        stop.cancel();
        started.acknowledged.send(Ok(())).unwrap();
        assert_eq!(task.await.unwrap(), Err(AgentFailure::Cancelled));
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn dropping_in_flight_execution_leaves_only_started_intent() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let ledger = UsageLedger::default().with_journal(sender);
        let dispatched = Arc::new(tokio::sync::Notify::new());
        let signal = dispatched.clone();
        let task = tokio::spawn(async move {
            execute_recorded(
                &ledger,
                record(),
                Instant::now() + Duration::from_secs(5),
                &Cancellation::default(),
                4096,
                Box::pin(async move {
                    signal.notify_one();
                    std::future::pending().await
                }),
            )
            .await
        });
        let started = receiver.recv().await.unwrap();
        assert_eq!(execution(&started).state, CapabilityExecutionState::Started);
        started.acknowledged.send(Ok(())).unwrap();
        dispatched.notified().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(receiver.recv().await.is_none());
    }
}
