//! Wait for the exact pre-dispatch intent, not for another execution attempt.
use floe_agent_contract::{AgentFailure, ExecutionJournal, JournalAck, JournalEvent};
use floe_execution::ExecutionScope;
use std::time::Duration;

pub(super) async fn record_intent(
    journal: &dyn ExecutionJournal,
    scope: &ExecutionScope,
    event: JournalEvent,
) -> Result<JournalAck, AgentFailure> {
    let mut delay = Duration::from_millis(25);
    loop {
        // The Run's cancellation/deadline also bounds a journal future. An
        // adapter must retain uncertainty if that future is abandoned in SQL.
        match scope.run(journal.record_intent(event.clone())).await {
            Err(AgentFailure::StorageBusy) => {
                scope
                    .run(async {
                        tokio::time::sleep(delay).await;
                        Ok(())
                    })
                    .await?;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::{BoxFuture, InvocationKey, ToolCall, TraceContext};
    use floe_execution::{
        Cancellation,
        budget::{BudgetConfig, BudgetLedger},
    };
    use std::sync::Mutex;
    use uuid::Uuid;

    struct Journal {
        busy: usize,
        result: Result<JournalAck, AgentFailure>,
        events: Mutex<Vec<JournalEvent>>,
    }
    impl ExecutionJournal for Journal {
        fn record_intent<'a>(
            &'a self,
            event: JournalEvent,
        ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            Box::pin(async move {
                let mut events = self.events.lock().unwrap();
                events.push(event);
                if events.len() <= self.busy {
                    Err(AgentFailure::StorageBusy)
                } else {
                    self.result.clone()
                }
            })
        }
        fn record_result<'a>(
            &'a self,
            _: JournalEvent,
        ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            panic!("must not publish a result")
        }
        fn record_output<'a>(
            &'a self,
            _: JournalEvent,
        ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            panic!("must not publish output")
        }
        fn checkpoint<'a>(
            &'a self,
            _: JournalEvent,
        ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            panic!("must not change execution progress")
        }
    }
    fn scope(duration: Duration) -> ExecutionScope {
        ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + duration,
            BudgetLedger::new(BudgetConfig::new(0, 0), Default::default()).root_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }
    fn event() -> JournalEvent {
        JournalEvent::ToolIntent {
            call: ToolCall {
                call_id: Uuid::new_v4(),
                invocation_key: InvocationKey::new(),
                tool_id: "fixture".into(),
                definition_revision: 1,
                input: "original".into(),
            },
        }
    }
    #[tokio::test]
    async fn contention_beyond_adapter_retry_window_keeps_one_exact_intent() {
        let journal = Journal {
            busy: 4,
            result: Ok(JournalAck::Accepted { revision: 7 }),
            events: Mutex::new(vec![]),
        };
        let event = event();
        let result = record_intent(&journal, &scope(Duration::from_secs(3)), event.clone()).await;
        assert_eq!(result, Ok(JournalAck::Accepted { revision: 7 }));
        let events = journal.events.lock().unwrap();
        assert_eq!(events.len(), 5);
        assert!(
            events
                .iter()
                .all(|value| serde_json::to_value(value).unwrap()
                    == serde_json::to_value(&event).unwrap())
        );
    }
    #[tokio::test]
    async fn uncertainty_and_semantic_failures_are_not_retried() {
        for failure in [
            AgentFailure::StorageUnavailable,
            AgentFailure::VaultUnavailable,
            AgentFailure::Conflict,
            AgentFailure::PolicyDenied,
            AgentFailure::Cancelled,
            AgentFailure::DeadlineExceeded,
        ] {
            let journal = Journal {
                busy: 0,
                result: Err(failure),
                events: Mutex::new(vec![]),
            };
            assert_eq!(
                record_intent(&journal, &scope(Duration::from_secs(1)), event()).await,
                Err(failure)
            );
            assert_eq!(journal.events.lock().unwrap().len(), 1);
        }
    }
    #[tokio::test]
    async fn sustained_contention_is_bounded_by_deadline_and_cancellation() {
        let journal = Journal {
            busy: usize::MAX,
            result: Ok(JournalAck::Accepted { revision: 1 }),
            events: Mutex::new(vec![]),
        };
        assert_eq!(
            record_intent(&journal, &scope(Duration::from_millis(10)), event()).await,
            Err(AgentFailure::DeadlineExceeded)
        );
        assert_eq!(journal.events.lock().unwrap().len(), 1);
        let scope = scope(Duration::from_secs(3));
        let cancellation = scope.cancellation().clone();
        let (result, ()) = tokio::join!(record_intent(&journal, &scope, event()), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancellation.cancel();
        });
        assert_eq!(result, Err(AgentFailure::Cancelled));
        assert_eq!(journal.events.lock().unwrap().len(), 2);
    }
}
