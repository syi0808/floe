use floe_agent_contract::{AgentFailure, PersonId};
use floe_connections::{ConnectorSnapshot, ResourceMode, SourceConnection};
use floe_context_contract::{
    ATTENTION_VIEW_ID, CALENDAR_CONTEXT_VIEW_ID, ConnectionId, ConnectorId, ExecutionOwnerId,
    PEOPLE_VIEW_ID, ResourceHandle, SourceSelectionReference, WELLBEING_VIEW_ID,
    connection_view_resource,
};
use sha2::{Digest, Sha256};

pub const LOCAL_CONTEXT_CONNECTOR: &str = "floe.local.context";

pub fn validate_local_source_selection(
    selected: &SourceSelectionReference,
    device_id: &str,
) -> Result<(), AgentFailure> {
    selected
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let (connector, connection, owner, resource) = match selected.capability_id.as_str() {
        "floe.tasks" | "memory.confirmed" => (
            LOCAL_CONTEXT_CONNECTOR.to_owned(),
            LOCAL_CONTEXT_CONNECTOR.to_owned(),
            format!("device:{device_id}"),
            selected.capability_id.clone(),
        ),
        _ => return Err(AgentFailure::CapabilityDenied),
    };
    if selected.contract_version != 1
        || selected.connector_id.as_str() != connector
        || selected.connection_id.as_str() != connection
        || selected.execution_owner_id.as_str() != owner
        || selected.resource.as_str() != resource
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub struct SourceCandidateRequest<'a> {
    pub person_id: PersonId,
    pub device_id: &'a str,
    pub capability: &'a str,
    pub contract_version: u32,
    pub remote_connections: &'a [ConnectorSnapshot],
    pub remote_execution_owner: Option<&'a str>,
    pub source_connections: &'a [SourceConnection],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCandidate {
    pub candidate_id: String,
    pub reference: SourceSelectionReference,
    pub title: String,
    pub detail: String,
}

pub fn source_candidate_id(reference: &SourceSelectionReference) -> Result<String, AgentFailure> {
    reference
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let bytes = serde_json::to_vec(&("floe.source-selection-candidate.sha256.v1", reference))
        .map_err(|_| AgentFailure::InvalidInput)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn validate_personal_source_selection(
    selected: &SourceSelectionReference,
    connection: &SourceConnection,
    person_id: PersonId,
    device_id: &str,
) -> Result<(), AgentFailure> {
    selected
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let request = SourceCandidateRequest {
        person_id,
        device_id,
        capability: &selected.capability_id,
        contract_version: selected.contract_version,
        remote_connections: &[],
        remote_execution_owner: None,
        source_connections: std::slice::from_ref(connection),
    };
    if selected.contract_version != 1
        || !serves_personal(connection, &request)
        || selected.connector_id != *connection.connector_id()
        || selected.connection_id != *connection.connection_id()
        || selected.execution_owner_id != *connection.execution_owner_id()
        || selected.resource
            != connection_view_resource(&selected.capability_id, connection.connection_id())
                .map_err(|_| AgentFailure::InvalidInput)?
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}

pub fn discover_source_candidates(
    request: SourceCandidateRequest<'_>,
) -> Result<Vec<SourceCandidate>, AgentFailure> {
    if request.device_id.is_empty() || request.contract_version != 1 {
        return Ok(vec![]);
    }
    let mut candidates = Vec::new();
    let mut add = |connector: &str,
                   connection: &str,
                   owner: &str,
                   resource: &str,
                   title: String,
                   detail: String| {
        let reference = SourceSelectionReference {
            connector_id: ConnectorId::try_new(connector)
                .map_err(|_| AgentFailure::InvalidInput)?,
            connection_id: ConnectionId::try_new(connection)
                .map_err(|_| AgentFailure::InvalidInput)?,
            execution_owner_id: ExecutionOwnerId::try_new(owner)
                .map_err(|_| AgentFailure::InvalidInput)?,
            capability_id: request.capability.to_owned(),
            resource: ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?,
            contract_version: request.contract_version,
        };
        let candidate_id = source_candidate_id(&reference)?;
        candidates.push(SourceCandidate {
            candidate_id,
            reference,
            title,
            detail,
        });
        Ok::<(), AgentFailure>(())
    };
    match request.capability {
        "mail.communication" | "work.context" | "life.logistics" => {
            if let Some(owner) = request.remote_execution_owner {
                for snapshot in request.remote_connections {
                    let connection = &snapshot.connection;
                    if !connection.state.is_serving()
                        || connection.person_id.as_deref() != Some(&request.person_id.to_string())
                        || !remote_connector_supports(request.capability, &connection.connector_id)
                    {
                        continue;
                    }
                    let Some(connection_id) = connection.connection_id.as_deref() else {
                        continue;
                    };
                    let resource = connection_view_resource(
                        request.capability,
                        &ConnectionId::try_new(connection_id)
                            .map_err(|_| AgentFailure::InvalidInput)?,
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?;
                    add(
                        &connection.connector_id,
                        connection_id,
                        owner,
                        resource.as_str(),
                        snapshot.descriptor.provider.clone(),
                        "Connected account".into(),
                    )?;
                }
            }
        }
        "calendar.timeline" => {
            for connection in request.source_connections {
                let connector = connection.connector_id().as_str();
                let expected_owner = floe_access::local_calendar_execution_owner_for_connector(
                    connector,
                    request.device_id,
                )
                .or_else(|| request.remote_execution_owner.map(str::to_owned));
                let supported_connector = floe_access::local_calendar_provider(connector).is_some()
                    || matches!(connector, "calendar.google" | "calendar.microsoft");
                let connection_identity_matches =
                    floe_access::local_calendar_connection_id_for_connector(connector)
                        .is_none_or(|expected| connection.connection_id().as_str() == expected);
                if connection.is_serving()
                    && connection.person_id() == request.person_id
                    && expected_owner.as_deref() == Some(connection.execution_owner_id().as_str())
                    && connection_identity_matches
                    && supported_connector
                {
                    let resource = connection_view_resource(
                        CALENDAR_CONTEXT_VIEW_ID,
                        connection.connection_id(),
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?;
                    add(
                        connector,
                        connection.connection_id().as_str(),
                        connection.execution_owner_id().as_str(),
                        resource.as_str(),
                        if connector == "calendar.fixture" {
                            "Synthetic QA Calendar".into()
                        } else {
                            "Calendar".into()
                        },
                        if connector == "calendar.event_kit" {
                            "Connected calendar device".into()
                        } else if connector == "calendar.fixture" {
                            "Synthetic QA calendar".into()
                        } else {
                            "Connected calendar account".into()
                        },
                    )?;
                }
            }
        }
        ATTENTION_VIEW_ID | PEOPLE_VIEW_ID | WELLBEING_VIEW_ID => {
            for connection in request.source_connections {
                if !serves_personal(connection, &request) {
                    continue;
                }
                let resource =
                    connection_view_resource(request.capability, connection.connection_id())
                        .map_err(|_| AgentFailure::InvalidInput)?;
                let title = match request.capability {
                    ATTENTION_VIEW_ID => "Attention",
                    PEOPLE_VIEW_ID => "Contacts",
                    WELLBEING_VIEW_ID => "Wellbeing",
                    _ => unreachable!(),
                };
                add(
                    connection.connector_id().as_str(),
                    connection.connection_id().as_str(),
                    connection.execution_owner_id().as_str(),
                    resource.as_str(),
                    title.into(),
                    "This device".into(),
                )?;
            }
        }
        "floe.tasks" | "memory.confirmed" => add(
            LOCAL_CONTEXT_CONNECTOR,
            LOCAL_CONTEXT_CONNECTOR,
            &format!("device:{}", request.device_id),
            request.capability,
            if request.capability == "floe.tasks" {
                "Tasks"
            } else {
                "Confirmed memory"
            }
            .into(),
            "On this device".into(),
        )?,
        "relationships.confirmed_interactions" => {}
        _ => {}
    }
    candidates.sort_by(|left, right| left.reference.cmp(&right.reference));
    if candidates
        .windows(2)
        .any(|pair| pair[0].reference == pair[1].reference)
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(candidates)
}

fn serves_personal(connection: &SourceConnection, request: &SourceCandidateRequest<'_>) -> bool {
    if !connection.is_serving()
        || connection.person_id() != request.person_id
        || connection.native_subject_fingerprint().is_none()
    {
        return false;
    }
    let (connectors, mode, owner, singleton): (&[&str], _, _, _) = match request.capability {
        ATTENTION_VIEW_ID => (
            &["attention.macos"],
            ResourceMode::AllAvailable,
            format!("macos:{}", request.device_id),
            Some(ATTENTION_VIEW_ID),
        ),
        PEOPLE_VIEW_ID => (
            &["contacts.apple", "contacts.android"],
            ResourceMode::Selected,
            format!("apple:{}", request.device_id),
            None,
        ),
        WELLBEING_VIEW_ID => (
            &["health.apple"],
            ResourceMode::AllAvailable,
            format!("apple:{}", request.device_id),
            Some(WELLBEING_VIEW_ID),
        ),
        _ => return false,
    };
    let actual_owner = if request.capability == PEOPLE_VIEW_ID
        && connection.connector_id().as_str() == "contacts.android"
    {
        format!("android:{}", request.device_id)
    } else {
        owner
    };
    connectors.contains(&connection.connector_id().as_str())
        && connection.resource_mode() == mode
        && connection.execution_owner_id().as_str() == actual_owner
        && match singleton {
            Some(handle) => {
                connection.resources().len() == 1
                    && connection.resources()[0].handle().as_str() == handle
            }
            None => !connection.resources().is_empty() && connection.resources().len() <= 64,
        }
}

fn remote_connector_supports(capability: &str, connector: &str) -> bool {
    match capability {
        "mail.communication" => matches!(connector, "gmail" | "microsoft.mail"),
        "work.context" => matches!(
            connector,
            "slack.conversations" | "microsoft.teams" | "github.issues" | "google_drive.files"
        ),
        "life.logistics" => matches!(connector, "gmail" | "home_assistant.states"),
        _ => false,
    }
}
