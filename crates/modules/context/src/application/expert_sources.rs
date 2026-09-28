use floe_agent_contract::{AGENT_VERSION, AgentFailure, BoxFuture, PersonId};
use floe_context_contract::{
    CalendarViewQuery, ContextDependency, GrantConsumer, GrantPurpose, SourceReadOutcome,
    source_access_id_for_capability,
};
use floe_execution::Cancellation;
use serde_json::Value;
use tokio::time::Instant;

use crate::{ContextService, SelectedSourceReader, SourceView};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarReviewClassification {
    pub reason: floe_context_contract::SourceAccessRequirementKind,
    pub observed: Option<floe_context_contract::ObservedGrant>,
}

pub fn current_calendar_connector(connection: &floe_connections::SourceConnection) -> Option<&str> {
    match connection.connector_id().as_str() {
        "calendar.fixture" | "calendar.event_kit" | "calendar.google" | "calendar.microsoft"
        | "calendar.android" => Some(connection.connector_id().as_str()),
        _ => None,
    }
}

pub fn observe_calendar_binding(
    grants: &[floe_access::DataAccessGrant],
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
) -> Result<Option<floe_context_contract::ObservedGrant>, AgentFailure> {
    let grant = current_calendar_grant(grants, person_id, connector_id, connection_id)?;
    grant
        .map(|grant| {
            floe_context_contract::ObservedGrant::try_new(grant.id(), grant.authority())
                .map_err(|_| AgentFailure::StaleContext)
        })
        .transpose()
}

fn current_calendar_grant<'a>(
    grants: &'a [floe_access::DataAccessGrant],
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
) -> Result<Option<&'a floe_access::DataAccessGrant>, AgentFailure> {
    let mut binding = grants.iter().filter(|grant| {
        grant.state() != floe_access::GrantState::Revoked
            && grant.source().person_id() == person_id
            && grant.source().connector().as_str() == connector_id
            && grant.source().connection_id().as_str() == connection_id
    });
    let grant = binding.next();
    if binding.next().is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(grant)
}

pub fn classify_calendar_review(
    grants: &[floe_access::DataAccessGrant],
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
) -> Result<CalendarReviewClassification, AgentFailure> {
    let grant = current_calendar_grant(grants, person_id, connector_id, connection_id)?;
    let observed = grant
        .map(|grant| {
            floe_context_contract::ObservedGrant::try_new(grant.id(), grant.authority())
                .map_err(|_| AgentFailure::StaleContext)
        })
        .transpose()?;
    let reason = match grant {
        None => floe_context_contract::SourceAccessRequirementKind::EnableObserve,
        Some(grant) => {
            if grant.state() == floe_access::GrantState::Paused {
                floe_context_contract::SourceAccessRequirementKind::EnableObserve
            } else {
                floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource
            }
        }
    };
    Ok(CalendarReviewClassification { reason, observed })
}

pub struct DeclaredSourceRequirement<'a> {
    pub key: &'a str,
    pub capability: &'a str,
    pub contract_version: u32,
    pub selected_refs: &'a [floe_context_contract::SourceSelectionReference],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalExpertSource {
    Calendar,
    People,
    Attention,
    Wellbeing,
    ConfirmedInteractions,
    ConfirmedMemory,
    Tasks,
}

pub trait LocalExpertSourceDriver: Sync {
    fn read<'a>(
        &'a self,
        source: LocalExpertSource,
        source_access_id: &'static str,
        selected_refs: &'a [floe_context_contract::SourceSelectionReference],
        query: Value,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>>;
}

pub struct DeclaredSourceValue {
    pub payload: Value,
    pub dependencies: Vec<ContextDependency>,
    pub held: Option<SourceView<Value>>,
}

pub async fn read_declared_source(
    remote_reader: Option<&dyn SelectedSourceReader>,
    local_driver: &dyn LocalExpertSourceDriver,
    person_id: PersonId,
    consumer: &str,
    requirements: &[DeclaredSourceRequirement<'_>],
    key: &str,
    query: Value,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<SourceReadOutcome<DeclaredSourceValue>, AgentFailure> {
    let requirement = requirements
        .iter()
        .find(|requirement| requirement.key == key)
        .ok_or(AgentFailure::CapabilityDenied)?;
    let local = match requirement.capability {
        "calendar.timeline" => Some(LocalExpertSource::Calendar),
        "people.identity" => Some(LocalExpertSource::People),
        "attention.coarse" => Some(LocalExpertSource::Attention),
        "wellbeing.derived" => Some(LocalExpertSource::Wellbeing),
        "relationships.confirmed_interactions" => Some(LocalExpertSource::ConfirmedInteractions),
        "memory.confirmed" => Some(LocalExpertSource::ConfirmedMemory),
        "floe.tasks" => Some(LocalExpertSource::Tasks),
        "mail.communication" | "work.context" | "life.logistics" => None,
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    if let Some(source) = local {
        let source_access_id = source_access_id_for_capability(requirement.capability)
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if source == LocalExpertSource::Calendar {
            let calendar: CalendarViewQuery =
                serde_json::from_value(query.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            calendar.validate()?;
        } else if source != LocalExpertSource::ConfirmedInteractions
            && query != serde_json::json!({"schema_version": AGENT_VERSION})
        {
            return Err(AgentFailure::InvalidInput);
        }
        if serde_json::to_vec(&query)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > 65_536
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let acquired = local_driver
            .read(
                source,
                source_access_id,
                requirement.selected_refs,
                query,
                deadline,
                cancellation,
            )
            .await?;
        return Ok(match acquired {
            SourceReadOutcome::Ready((payload, dependencies)) => {
                SourceReadOutcome::Ready(DeclaredSourceValue {
                    payload,
                    dependencies,
                    held: None,
                })
            }
            SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
            SourceReadOutcome::NeedsUserAction(blockers) => {
                SourceReadOutcome::NeedsUserAction(blockers)
            }
        });
    }
    Ok(
        match read_declared_remote_source(
            remote_reader,
            person_id,
            consumer,
            requirement,
            query,
            deadline,
            cancellation,
        )
        .await?
        {
            SourceReadOutcome::Ready(read) => SourceReadOutcome::Ready(DeclaredSourceValue {
                payload: read.payload().clone(),
                dependencies: read
                    .bindings()
                    .iter()
                    .map(|binding| binding.dependency.clone())
                    .collect(),
                held: Some(read),
            }),
            SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
            SourceReadOutcome::NeedsUserAction(blockers) => {
                SourceReadOutcome::NeedsUserAction(blockers)
            }
        },
    )
}

async fn read_declared_remote_source(
    reader: Option<&dyn SelectedSourceReader>,
    person_id: PersonId,
    consumer: &str,
    requirement: &DeclaredSourceRequirement<'_>,
    query: Value,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<SourceReadOutcome<SourceView<Value>>, AgentFailure> {
    if !matches!(
        requirement.capability,
        "mail.communication" | "work.context" | "life.logistics"
    ) {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    crate::validate_remote_view_query(requirement.capability, &query)?;
    let prepared = ContextService::new(None).prepare(person_id)?;
    let request = prepared.source_request(
        requirement.capability,
        GrantConsumer::builtin(consumer).map_err(|_| AgentFailure::InvalidInput)?,
        GrantPurpose::Assistant,
        query,
        deadline,
        cancellation.clone(),
    )?;
    let reader = reader.ok_or(AgentFailure::CapabilityUnavailable)?;
    if requirement.selected_refs.is_empty()
        || requirement.selected_refs.iter().any(|selected| {
            selected.capability_id != requirement.capability
                || selected.contract_version != requirement.contract_version
        })
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    prepared
        .read_selected_source(&request, reader, requirement.selected_refs)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct ProbeDriver(Mutex<Vec<LocalExpertSource>>);

    fn selected_reference(
        resource: &str,
        capability: &str,
    ) -> floe_context_contract::SourceSelectionReference {
        floe_context_contract::SourceSelectionReference {
            connector_id: floe_context_contract::ConnectorId::try_new("test.connector").unwrap(),
            connection_id: floe_context_contract::ConnectionId::try_new("test-connection").unwrap(),
            execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new("test-owner")
                .unwrap(),
            capability_id: capability.into(),
            resource: floe_context_contract::ResourceHandle::try_new(resource).unwrap(),
            contract_version: 1,
        }
    }

    struct SelectedProbeDriver;

    impl LocalExpertSourceDriver for SelectedProbeDriver {
        fn read<'a>(
            &'a self,
            _: LocalExpertSource,
            source_access_id: &'static str,
            selected_refs: &'a [floe_context_contract::SourceSelectionReference],
            _: Value,
            _: Instant,
            _: &'a Cancellation,
        ) -> BoxFuture<'a, Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>>
        {
            Box::pin(async move {
                Ok(SourceReadOutcome::Ready((
                    serde_json::json!({
                        "resource": selected_refs[0].resource.as_str(),
                        "source_access_id": source_access_id,
                    }),
                    vec![],
                )))
            })
        }
    }

    #[tokio::test]
    async fn duplicate_requirement_keys_keep_distinct_selected_refs() {
        let first = [selected_reference("calendar-a", "calendar.timeline")];
        let second = [selected_reference("calendar-b", "calendar.timeline")];
        let requirements = [
            DeclaredSourceRequirement {
                key: "calendar_a",
                capability: "calendar.timeline",
                contract_version: 1,
                selected_refs: &first,
            },
            DeclaredSourceRequirement {
                key: "calendar_b",
                capability: "calendar.timeline",
                contract_version: 1,
                selected_refs: &second,
            },
        ];
        let query = serde_json::json!({"range_start_unix_ms": 1, "range_end_unix_ms": 1000, "cursor": null, "limit": 1});
        for (key, expected) in [("calendar_a", "calendar-a"), ("calendar_b", "calendar-b")] {
            let outcome = read_declared_source(
                None,
                &SelectedProbeDriver,
                PersonId::new(),
                "example.test.expert",
                &requirements,
                key,
                query.clone(),
                Instant::now() + std::time::Duration::from_secs(5),
                &Cancellation::default(),
            )
            .await
            .unwrap();
            let SourceReadOutcome::Ready(read) = outcome else {
                panic!("expected selected read")
            };
            assert_eq!(read.payload["resource"], expected);
            assert_eq!(read.payload["source_access_id"], "floe.source.calendar");
        }
    }

    struct SelectedProbeReader(Mutex<Vec<String>>);

    impl SelectedSourceReader for SelectedProbeReader {
        fn read_selected<'a>(
            &'a self,
            _: &'a crate::SourceReadRequest,
            selected: &'a [floe_context_contract::SourceSelectionReference],
        ) -> BoxFuture<'a, Result<SourceReadOutcome<crate::SourceRead>, AgentFailure>> {
            Box::pin(async move {
                self.0
                    .lock()
                    .unwrap()
                    .push(selected[0].resource.as_str().into());
                Ok(SourceReadOutcome::Unavailable(
                    floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
                ))
            })
        }
    }

    #[tokio::test]
    async fn selected_remote_port_receives_exact_requirement_refs() {
        let first = [selected_reference("work-a", "work.context")];
        let second = [selected_reference("work-b", "work.context")];
        let requirements = [
            DeclaredSourceRequirement {
                key: "work_a",
                capability: "work.context",
                contract_version: 1,
                selected_refs: &first,
            },
            DeclaredSourceRequirement {
                key: "work_b",
                capability: "work.context",
                contract_version: 1,
                selected_refs: &second,
            },
        ];
        let reader = SelectedProbeReader(Mutex::new(vec![]));
        for key in ["work_a", "work_b"] {
            assert!(matches!(
                read_declared_source(
                    Some(&reader),
                    &SelectedProbeDriver,
                    PersonId::new(),
                    "example.test.expert",
                    &requirements,
                    key,
                    serde_json::json!({"schema_version": AGENT_VERSION}),
                    Instant::now() + std::time::Duration::from_secs(5),
                    &Cancellation::default(),
                )
                .await,
                Ok(SourceReadOutcome::Unavailable(_))
            ));
        }
        assert_eq!(*reader.0.lock().unwrap(), vec!["work-a", "work-b"]);
    }

    impl LocalExpertSourceDriver for ProbeDriver {
        fn read<'a>(
            &'a self,
            source: LocalExpertSource,
            _: &'static str,
            _: &'a [floe_context_contract::SourceSelectionReference],
            _: Value,
            _: Instant,
            _: &'a Cancellation,
        ) -> BoxFuture<'a, Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>>
        {
            Box::pin(async move {
                self.0.lock().unwrap().push(source);
                Ok(SourceReadOutcome::Ready((serde_json::json!([]), vec![])))
            })
        }
    }

    #[tokio::test]
    async fn declared_local_capability_selects_only_its_driver() {
        let driver = ProbeDriver(Mutex::new(vec![]));
        let requirements = [DeclaredSourceRequirement {
            key: "calendar",
            capability: "calendar.timeline",
            contract_version: 1,
            selected_refs: &[],
        }];
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let cancellation = Cancellation::default();
        let query = serde_json::json!({"range_start_unix_ms": 1, "range_end_unix_ms": 1000, "cursor": null, "limit": 1});
        assert!(matches!(
            read_declared_source(
                None,
                &driver,
                PersonId::new(),
                "example.test.expert",
                &requirements,
                "calendar",
                query.clone(),
                deadline,
                &cancellation
            )
            .await,
            Ok(SourceReadOutcome::Ready(_))
        ));
        assert_eq!(*driver.0.lock().unwrap(), vec![LocalExpertSource::Calendar]);
        assert!(matches!(
            read_declared_source(
                None,
                &driver,
                PersonId::new(),
                "example.test.expert",
                &requirements,
                "other",
                query,
                deadline,
                &cancellation
            )
            .await,
            Err(AgentFailure::CapabilityDenied)
        ));
        assert_eq!(driver.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn undeclared_key_and_unbounded_query_fail_before_source_selection() {
        let requirements = [DeclaredSourceRequirement {
            key: "mail",
            capability: "mail.communication",
            contract_version: 1,
            selected_refs: &[],
        }];
        let person = PersonId::new();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let cancellation = Cancellation::default();
        assert!(matches!(
            read_declared_source(
                None,
                &ProbeDriver(Mutex::new(vec![])),
                person,
                "example.test.expert",
                &requirements,
                "work",
                serde_json::json!({}),
                deadline,
                &cancellation
            )
            .await,
            Err(AgentFailure::CapabilityDenied)
        ));
        assert!(matches!(
            read_declared_remote_source(
                None,
                person,
                "example.test.expert",
                &requirements[0],
                serde_json::json!({"limit": 100_000}),
                deadline,
                &cancellation
            )
            .await,
            Err(AgentFailure::InvalidInput)
        ));
    }
}
