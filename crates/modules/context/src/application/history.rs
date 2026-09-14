use std::collections::{BTreeMap, HashSet};

use floe_agent_contract::AgentFailure;
use floe_context_contract::DependencyCoverage;
use uuid::Uuid;

use crate::ports::evidence_reader::EvidenceReader;

pub async fn read_history_coverage(
    reader: &impl EvidenceReader,
    session_id: Uuid,
    turn_ids: impl IntoIterator<Item = Uuid>,
) -> Result<BTreeMap<Uuid, DependencyCoverage>, AgentFailure> {
    if session_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }

    let turn_ids = turn_ids.into_iter().collect::<Vec<_>>();
    if turn_ids.iter().any(Uuid::is_nil) {
        return Err(AgentFailure::InvalidInput);
    }

    let mut seen = HashSet::with_capacity(turn_ids.len());
    let mut coverage = BTreeMap::new();
    for turn_id in turn_ids {
        if !seen.insert(turn_id) {
            continue;
        }
        let value = reader.read_turn_coverage(session_id, turn_id).await?;
        value
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        coverage.insert(turn_id, value);
    }
    Ok(coverage)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    struct FixtureReader {
        calls: Arc<Mutex<Vec<Uuid>>>,
        result: Result<DependencyCoverage, AgentFailure>,
    }

    impl EvidenceReader for FixtureReader {
        fn read_turn_coverage(
            &self,
            _session_id: Uuid,
            turn_id: Uuid,
        ) -> impl std::future::Future<Output = Result<DependencyCoverage, AgentFailure>> + Send
        {
            self.calls.lock().unwrap().push(turn_id);
            let result = self.result.clone();
            async move { result }
        }
    }

    fn reader(result: Result<DependencyCoverage, AgentFailure>) -> FixtureReader {
        FixtureReader {
            calls: Arc::new(Mutex::new(Vec::new())),
            result,
        }
    }

    #[tokio::test]
    async fn reads_distinct_turns_in_input_order() {
        let session_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let reader = reader(Ok(DependencyCoverage::Independent));

        let result = read_history_coverage(&reader, session_id, [first, second, first])
            .await
            .unwrap();

        assert_eq!(*reader.calls.lock().unwrap(), vec![first, second]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[&first], DependencyCoverage::Independent);
        assert_eq!(result[&second], DependencyCoverage::Independent);
    }

    #[tokio::test]
    async fn rejects_invalid_request_before_reading() {
        let reader = reader(Ok(DependencyCoverage::Independent));
        for (session_id, turn_ids) in [
            (Uuid::nil(), vec![Uuid::new_v4()]),
            (Uuid::new_v4(), vec![Uuid::nil()]),
        ] {
            assert_eq!(
                read_history_coverage(&reader, session_id, turn_ids).await,
                Err(AgentFailure::InvalidInput)
            );
        }
        assert!(reader.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_history_does_not_read_evidence() {
        let reader = reader(Err(AgentFailure::VaultUnavailable));
        assert_eq!(
            read_history_coverage(&reader, Uuid::new_v4(), []).await,
            Ok(BTreeMap::new())
        );
        assert!(reader.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_coverage_is_storage_failure_without_partial_result() {
        let reader = reader(Ok(DependencyCoverage::Dependent {
            dependencies: vec![],
        }));
        assert_eq!(
            read_history_coverage(&reader, Uuid::new_v4(), [Uuid::new_v4()]).await,
            Err(AgentFailure::StorageUnavailable)
        );
    }

    #[tokio::test]
    async fn reader_errors_are_fatal() {
        let reader = reader(Err(AgentFailure::VaultUnavailable));
        assert_eq!(
            read_history_coverage(&reader, Uuid::new_v4(), [Uuid::new_v4(), Uuid::new_v4()]).await,
            Err(AgentFailure::VaultUnavailable)
        );
    }
}
