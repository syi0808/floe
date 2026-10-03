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
