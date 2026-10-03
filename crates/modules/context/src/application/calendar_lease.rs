use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_context_contract::ContextDependency;
use serde::Serialize;
use uuid::Uuid;

use floe_access::CalendarReadAccessAdmission;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct CalendarQueryKey {
    pub invocation_id: Uuid,
    pub person_id: PersonId,
    pub handle: Uuid,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub cursor: Option<String>,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub max_items: usize,
    pub max_bytes: usize,
}

pub fn calendar_dependency(
    invocation_id: Uuid,
    process_incarnation: Uuid,
    observation_id: Uuid,
    admission: &CalendarReadAccessAdmission,
    key: &CalendarQueryKey,
    observed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<ContextDependency, AgentFailure> {
    let query_fingerprint = serde_json::to_vec(key).map_err(|_| AgentFailure::InvalidInput)?;
    let [operation] = admission.scope().operations() else {
        return Err(AgentFailure::InvalidInput);
    };
    let [purpose] = admission.scope().purposes() else {
        return Err(AgentFailure::InvalidInput);
    };
    let [consumer] = admission.scope().consumers() else {
        return Err(AgentFailure::InvalidInput);
    };
    ContextDependency::try_new(
        admission.person_id(),
        admission.grant_id(),
        admission.grant_authority(),
        admission.source().clone(),
        admission.scope().resources().to_vec(),
        admission.source_authority(),
        key.calendar_ids
            .iter()
            .map(|id| floe_context_contract::ResourceHandle::try_new(id.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AgentFailure::InvalidInput)?,
        admission.scope().categories().to_vec(),
        *operation,
        *purpose,
        consumer.clone(),
        admission.processing().clone(),
        observation_id,
        query_fingerprint,
        invocation_id,
        process_incarnation,
        observed_at,
        expires_at,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}
