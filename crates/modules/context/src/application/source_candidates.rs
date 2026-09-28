use floe_access::{
    ATTENTION_CONNECTION, ATTENTION_CONNECTOR, ATTENTION_RESOURCE, PEOPLE_RESOURCE,
    WELLBEING_CONNECTION, WELLBEING_CONNECTOR, WELLBEING_RESOURCE, apple_execution_owner,
    attention_execution_owner, contacts_connection, contacts_execution_owner,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_connections::{ConnectorSnapshot, SourceConnection};
use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle, SourceSelectionReference,
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
        "attention.coarse" => (
            ATTENTION_CONNECTOR.to_owned(),
            ATTENTION_CONNECTION.to_owned(),
            attention_execution_owner(device_id),
            ATTENTION_RESOURCE.to_owned(),
        ),
        "people.identity" => (
            "contacts.apple".to_owned(),
            contacts_connection("contacts.apple"),
            contacts_execution_owner("contacts.apple", device_id),
            PEOPLE_RESOURCE.to_owned(),
        ),
        "wellbeing.derived" => (
            WELLBEING_CONNECTOR.to_owned(),
            WELLBEING_CONNECTION.to_owned(),
            apple_execution_owner(device_id),
            WELLBEING_RESOURCE.to_owned(),
        ),
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
    pub calendar_connection: Option<&'a SourceConnection>,
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
            if let Some(connection) = request.calendar_connection {
                let connector = connection.connector_id().as_str();
                let expected_owner = if connector == "calendar.event_kit" {
                    Some(request.device_id)
                } else {
                    request.remote_execution_owner
                };
                if connection.is_serving()
                    && connection.person_id() == request.person_id
                    && expected_owner == Some(connection.execution_owner_id().as_str())
                    && matches!(
                        connector,
                        "calendar.event_kit"
                            | "calendar.google"
                            | "calendar.microsoft"
                            | "calendar.fixture"
                    )
                {
                    for calendar in connection.resources() {
                        add(
                            connector,
                            connection.connection_id().as_str(),
                            connection.execution_owner_id().as_str(),
                            calendar.handle().as_str(),
                            calendar.label().into(),
                            "Selected calendar".into(),
                        )?;
                    }
                }
            }
        }
        "attention.coarse" => add(
            ATTENTION_CONNECTOR,
            ATTENTION_CONNECTION,
            &attention_execution_owner(request.device_id),
            ATTENTION_RESOURCE,
            "Attention".into(),
            "This device".into(),
        )?,
        "people.identity" => add(
            "contacts.apple",
            &contacts_connection("contacts.apple"),
            &contacts_execution_owner("contacts.apple", request.device_id),
            PEOPLE_RESOURCE,
            "Contacts".into(),
            "This device".into(),
        )?,
        "wellbeing.derived" => add(
            WELLBEING_CONNECTOR,
            WELLBEING_CONNECTION,
            &apple_execution_owner(request.device_id),
            WELLBEING_RESOURCE,
            "Wellbeing".into(),
            "This device".into(),
        )?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use floe_connections::{
        ConnectionResource, ConnectionState, ConnectorConnectionSnapshot, ConnectorDescriptor,
        ExecutionLocation, ResourceMode,
    };

    fn request<'a>(
        person_id: PersonId,
        capability: &'a str,
        calendar: Option<&'a SourceConnection>,
    ) -> SourceCandidateRequest<'a> {
        SourceCandidateRequest {
            person_id,
            device_id: "mac-local",
            capability,
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            calendar_connection: calendar,
        }
    }

    #[test]
    fn native_and_intrinsic_selection_is_exactly_device_pinned() {
        let person_id = PersonId::new();
        for capability in [
            "attention.coarse",
            "people.identity",
            "wellbeing.derived",
            "floe.tasks",
            "memory.confirmed",
        ] {
            let selected = discover_source_candidates(request(person_id, capability, None))
                .unwrap()
                .remove(0)
                .reference;
            validate_local_source_selection(&selected, "mac-local").unwrap();
            assert_eq!(
                validate_local_source_selection(&selected, "other-device"),
                Err(AgentFailure::StaleContext),
            );
        }
    }

    #[test]
    fn intrinsic_reference_is_not_an_access_grant_and_has_stable_id() {
        let candidates =
            discover_source_candidates(request(PersonId::new(), "floe.tasks", None)).unwrap();
        assert_eq!(candidates.len(), 1);
        let source = &candidates[0].reference;
        assert_eq!(source.connector_id.as_str(), LOCAL_CONTEXT_CONNECTOR);
        assert_eq!(source.connection_id.as_str(), LOCAL_CONTEXT_CONNECTOR);
        assert_eq!(source.execution_owner_id.as_str(), "device:mac-local");
        assert_eq!(source.resource.as_str(), "floe.tasks");
        assert_eq!(
            source_candidate_id(source).unwrap(),
            candidates[0].candidate_id
        );
        assert_eq!(candidates[0].candidate_id.len(), 64);
        assert!(
            discover_source_candidates(request(
                PersonId::new(),
                "relationships.confirmed_interactions",
                None
            ))
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn calendar_addition_produces_a_new_candidate_without_changing_the_old_reference() {
        let person_id = PersonId::new();
        let mut connection = SourceConnection::establish(
            person_id,
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ConnectionId::try_new("calendar-account").unwrap(),
            ExecutionOwnerId::try_new("mac-local").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(
                    ResourceHandle::try_new("calendar-a").unwrap(),
                    "Personal".into(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        connection.update_native_subject(1, "a".repeat(64)).unwrap();
        let initial =
            discover_source_candidates(request(person_id, "calendar.timeline", Some(&connection)))
                .unwrap();
        assert_eq!(initial.len(), 1);
        assert!(
            discover_source_candidates(request(
                PersonId::new(),
                "calendar.timeline",
                Some(&connection)
            ))
            .unwrap()
            .is_empty()
        );
        let mut foreign_device = request(person_id, "calendar.timeline", Some(&connection));
        foreign_device.device_id = "other-device";
        assert!(
            discover_source_candidates(foreign_device)
                .unwrap()
                .is_empty()
        );
        connection
            .configure(
                2,
                ResourceMode::Selected,
                vec![
                    ConnectionResource::new(
                        ResourceHandle::try_new("calendar-a").unwrap(),
                        "Personal".into(),
                    )
                    .unwrap(),
                    ConnectionResource::new(
                        ResourceHandle::try_new("calendar-b").unwrap(),
                        "Work".into(),
                    )
                    .unwrap(),
                ],
            )
            .unwrap();
        let refreshed =
            discover_source_candidates(request(person_id, "calendar.timeline", Some(&connection)))
                .unwrap();
        assert_eq!(refreshed.len(), 2);
        assert_eq!(refreshed[0], initial[0]);
        assert_ne!(refreshed[0].candidate_id, refreshed[1].candidate_id);
    }

    #[test]
    fn remote_accounts_require_pinned_producer_and_keep_exact_distinct_targets() {
        let person_id = PersonId::new();
        let snapshot = |connection_id: &str, person: PersonId| ConnectorSnapshot {
            descriptor: ConnectorDescriptor {
                schema_version: 1,
                id: "gmail".into(),
                version: "1".into(),
                provider: "Google Mail".into(),
                execution: ExecutionLocation::Server,
                capabilities: vec![],
                views: vec![],
            },
            connection: ConnectorConnectionSnapshot {
                schema_version: 1,
                connector_id: "gmail".into(),
                connection_id: Some(connection_id.into()),
                person_id: Some(person.to_string()),
                device_binding: None,
                state: ConnectionState::Ready,
                granted_scopes: vec![],
                observed_at_unix_ms: 1,
                last_success_at_unix_ms: None,
                last_failure: None,
            },
            views: vec![],
        };
        let sources = [
            snapshot("mail-a", person_id),
            snapshot("mail-b", person_id),
            snapshot("foreign", PersonId::new()),
        ];
        let mut request = SourceCandidateRequest {
            person_id,
            device_id: "mac-local",
            capability: "mail.communication",
            contract_version: 1,
            remote_connections: &sources,
            remote_execution_owner: None,
            calendar_connection: None,
        };
        assert!(discover_source_candidates(request).unwrap().is_empty());
        request.remote_execution_owner = Some("server:paired");
        let candidates = discover_source_candidates(request).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].reference.resource.as_str(),
            "mail.communication:mail-a"
        );
        assert_eq!(
            candidates[1].reference.resource.as_str(),
            "mail.communication:mail-b"
        );
        assert_ne!(candidates[0].candidate_id, candidates[1].candidate_id);
    }
}
