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
    let operation = admission.operation();
    let purpose = admission.purpose();
    let consumer = admission.consumer();
    if !admission.scope().operations().contains(&operation)
        || !admission.scope().purposes().contains(&purpose)
        || !admission.scope().consumers().contains(consumer)
    {
        return Err(AgentFailure::CapabilityDenied);
    }
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
        operation,
        purpose,
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

#[cfg(test)]
mod tests {
    use super::{CalendarQueryKey, calendar_dependency};
    use chrono::{Duration, Utc};
    use floe_access::CalendarReadAccessAdmission;
    use floe_agent_contract::AgentFailure;
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantAuthority, GrantConsumer,
        GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
        ProcessingRestriction, ResourceHandle, SourceAuthority, connection_view_resource,
    };
    use floe_kernel::PersonId;
    use uuid::Uuid;

    fn multi_consumer_admission(consumer: GrantConsumer) -> CalendarReadAccessAdmission {
        let person_id = PersonId::new();
        let connection_id = ConnectionId::try_new("calendar.fixture.local").unwrap();
        let scope = GrantScope::try_new(
            vec![connection_view_resource("calendar.timeline", &connection_id).unwrap()],
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![
                GrantConsumer::builtin("floe.builtin.commitments").unwrap(),
                GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                GrantConsumer::builtin("floe.builtin.focus-attention").unwrap(),
                GrantConsumer::builtin("floe.builtin.wellbeing").unwrap(),
            ],
            ProcessingRestriction::DeviceOnly,
        )
        .unwrap();
        let source = GrantSourceBinding::try_new(
            person_id,
            connection_id,
            ConnectorId::try_new("calendar.fixture").unwrap(),
            ExecutionOwnerId::try_new("fixture:qa-device").unwrap(),
        )
        .unwrap();
        CalendarReadAccessAdmission::device_local(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            SourceAuthority::new(),
            scope,
            consumer,
        )
    }

    fn key(person_id: PersonId) -> CalendarQueryKey {
        CalendarQueryKey {
            invocation_id: Uuid::new_v4(),
            person_id,
            handle: Uuid::new_v4(),
            device_id: "qa-device".into(),
            calendar_ids: vec!["fixture.calendar.team".into()],
            range_start_unix_ms: 1_800_000_000_000,
            range_end_unix_ms: 1_800_003_600_000,
            cursor: None,
            timezone_offset_seconds: 0,
            end_timezone_offset_seconds: Some(0),
            max_items: 8,
            max_bytes: 65_536,
        }
    }

    #[test]
    fn calendar_dependency_binds_exact_consumer_from_shared_multi_consumer_grant() {
        let consumer = GrantConsumer::builtin("floe.builtin.schedule").unwrap();
        let admission = multi_consumer_admission(consumer.clone());
        let observed_at = Utc::now();
        let dependency = calendar_dependency(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &admission,
            &key(admission.person_id()),
            observed_at,
            observed_at + Duration::minutes(5),
        )
        .unwrap();

        assert_eq!(dependency.consumer(), &consumer);
        assert_eq!(dependency.processing(), admission.processing());
        assert_eq!(dependency.source().connector().as_str(), "calendar.fixture");
        assert_eq!(
            dependency.source_resources(),
            &[ResourceHandle::try_new("fixture.calendar.team").unwrap()]
        );
        assert_eq!(admission.scope().consumers().len(), 4);
    }

    #[test]
    fn calendar_dependency_rejects_consumer_outside_the_grant_scope() {
        let outside = GrantConsumer::builtin("floe.builtin.schedule_untrusted").unwrap();
        let admission = multi_consumer_admission(outside);
        let observed_at = Utc::now();

        let result = calendar_dependency(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &admission,
            &key(admission.person_id()),
            observed_at,
            observed_at + Duration::minutes(5),
        );

        assert!(matches!(result, Err(AgentFailure::CapabilityDenied)));
    }
}
