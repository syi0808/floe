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
    let connector = connection.connector_id().as_str();
    if floe_access::local_calendar_provider(connector).is_some()
        && floe_access::local_calendar_connection_id_for_connector(connector)
            .is_none_or(|expected| connection.connection_id().as_str() == expected)
        || matches!(connector, "calendar.google" | "calendar.microsoft")
    {
        Some(connector)
    } else {
        None
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
