use std::future::Future;

use floe_agent_contract::{AgentFailure, ModelPlacement};
use floe_execution::budget::ModelUsage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelAttemptState {
    Started,
    Accepted,
    Rejected,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAttemptRecord {
    pub id: Uuid,
    pub turn_id: Uuid,
    pub scope_id: Uuid,
    pub attempt: u8,
    pub placement: ModelPlacement,
    pub state: ModelAttemptState,
    pub failure: Option<AgentFailure>,
    pub usage: ModelUsage,
}

pub trait AttemptJournal: Sync {
    fn record_attempt(
        &self,
        record: ModelAttemptRecord,
    ) -> impl Future<Output = Result<(), AgentFailure>> + Send;
}

pub struct AttemptLifecycle<'journal, Journal: AttemptJournal> {
    journal: &'journal Journal,
    record: ModelAttemptRecord,
}

impl<'journal, Journal: AttemptJournal> AttemptLifecycle<'journal, Journal> {
    pub async fn start(
        journal: &'journal Journal,
        turn_id: Uuid,
        scope_id: Uuid,
        ordinal: u8,
        placement: ModelPlacement,
        usage: ModelUsage,
    ) -> Result<Self, AgentFailure> {
        let record = ModelAttemptRecord {
            id: Uuid::new_v4(),
            turn_id,
            scope_id,
            attempt: ordinal,
            placement,
            state: ModelAttemptState::Started,
            failure: None,
            usage,
        };
        journal.record_attempt(record.clone()).await?;
        Ok(Self { journal, record })
    }

    pub async fn finish(
        mut self,
        failure: Option<AgentFailure>,
        usage: ModelUsage,
    ) -> Result<(), AgentFailure> {
        self.record.state = match failure {
            None => ModelAttemptState::Accepted,
            Some(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded) => {
                ModelAttemptState::Interrupted
            }
            Some(_) => ModelAttemptState::Rejected,
        };
        self.record.failure = failure;
        self.record.usage = usage;
        self.journal.record_attempt(self.record).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Journal {
        records: Mutex<Vec<ModelAttemptRecord>>,
        fail_state: Option<ModelAttemptState>,
    }

    impl AttemptJournal for Journal {
        async fn record_attempt(&self, record: ModelAttemptRecord) -> Result<(), AgentFailure> {
            if self.fail_state == Some(record.state) {
                return Err(AgentFailure::StorageUnavailable);
            }
            self.records.lock().unwrap().push(record);
            Ok(())
        }
    }

    #[tokio::test]
    async fn acknowledged_start_and_terminal_record_share_one_identity() {
        for (failure, expected) in [
            (None, ModelAttemptState::Accepted),
            (
                Some(AgentFailure::PolicyDenied),
                ModelAttemptState::Rejected,
            ),
            (
                Some(AgentFailure::DeadlineExceeded),
                ModelAttemptState::Interrupted,
            ),
        ] {
            let journal = Journal::default();
            let turn_id = Uuid::new_v4();
            let scope_id = Uuid::new_v4();
            let initial = ModelUsage {
                attempts: 1,
                tokens: 20,
                estimated_tokens: 20,
                cost_micros: 0,
            };
            let attempt = AttemptLifecycle::start(
                &journal,
                turn_id,
                scope_id,
                1,
                ModelPlacement::Remote,
                initial,
            )
            .await
            .unwrap();
            let actual = ModelUsage {
                attempts: 1,
                tokens: 8,
                estimated_tokens: 0,
                cost_micros: 2,
            };
            attempt.finish(failure, actual).await.unwrap();
            let records = journal.records.lock().unwrap();
            assert_eq!(records.len(), 2);
            assert_eq!(records[0].state, ModelAttemptState::Started);
            assert_eq!(records[0].usage, initial);
            assert_eq!(records[0].id, records[1].id);
            assert_eq!(records[1].turn_id, turn_id);
            assert_eq!(records[1].scope_id, scope_id);
            assert_eq!(records[1].state, expected);
            assert_eq!(records[1].failure, failure);
            assert_eq!(records[1].usage, actual);
        }
    }

    #[tokio::test]
    async fn failed_start_never_returns_an_admitted_attempt() {
        let journal = Journal {
            fail_state: Some(ModelAttemptState::Started),
            ..Default::default()
        };
        assert!(matches!(
            AttemptLifecycle::start(
                &journal,
                Uuid::new_v4(),
                Uuid::new_v4(),
                1,
                ModelPlacement::DeviceLocal,
                ModelUsage::default()
            )
            .await,
            Err(AgentFailure::StorageUnavailable)
        ));
        assert!(journal.records.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_terminal_acknowledgement_is_not_reported_as_finished() {
        let journal = Journal {
            fail_state: Some(ModelAttemptState::Accepted),
            ..Default::default()
        };
        let attempt = AttemptLifecycle::start(
            &journal,
            Uuid::new_v4(),
            Uuid::new_v4(),
            1,
            ModelPlacement::DeviceLocal,
            ModelUsage::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            attempt.finish(None, ModelUsage::default()).await,
            Err(AgentFailure::StorageUnavailable)
        );
        let records = journal.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].state, ModelAttemptState::Started);
    }
}
