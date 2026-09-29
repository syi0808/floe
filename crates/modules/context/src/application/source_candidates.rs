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
                        "Calendar".into(),
                        if connector == "calendar.event_kit" {
                            "Connected calendar device".into()
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
            source_connections: calendar.map(std::slice::from_ref).unwrap_or(&[]),
        }
    }

    #[test]
    fn intrinsic_selection_is_exactly_device_pinned() {
        let person_id = PersonId::new();
        for capability in ["floe.tasks", "memory.confirmed"] {
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
    fn native_calendar_resource_change_preserves_single_connection_view_candidate() {
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
        assert_eq!(
            initial[0].reference.resource.as_str(),
            "calendar.timeline:calendar-account"
        );
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
        assert_eq!(refreshed.len(), 1);
        assert_eq!(refreshed[0], initial[0]);
        connection
            .configure(
                3,
                ResourceMode::Selected,
                vec![
                    ConnectionResource::new(
                        ResourceHandle::try_new("calendar-a").unwrap(),
                        "Renamed".into(),
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
        assert_eq!(
            discover_source_candidates(request(person_id, "calendar.timeline", Some(&connection)))
                .unwrap(),
            initial
        );
        connection.disconnect(4).unwrap();
        assert!(
            discover_source_candidates(request(person_id, "calendar.timeline", Some(&connection)))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn hosted_calendar_resource_change_preserves_one_logical_candidate() {
        let person_id = PersonId::new();
        for connector in ["calendar.google", "calendar.microsoft"] {
            let mut connection = SourceConnection::establish(
                person_id,
                ConnectorId::try_new(connector).unwrap(),
                ConnectionId::try_new("calendar-account").unwrap(),
                ExecutionOwnerId::try_new("server-owner").unwrap(),
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
            let candidates = |connection: &SourceConnection| {
                let mut candidate_request =
                    request(person_id, "calendar.timeline", Some(connection));
                candidate_request.remote_execution_owner = Some("server-owner");
                discover_source_candidates(candidate_request).unwrap()
            };
            let initial = candidates(&connection);
            assert_eq!(initial.len(), 1);
            assert_eq!(
                initial[0].reference.resource.as_str(),
                "calendar.timeline:calendar-account"
            );
            let revision = connection.revision();
            connection
                .configure(
                    revision,
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
            assert_eq!(candidates(&connection), initial);
        }
    }

    #[test]
    fn personal_candidates_require_serving_connections_and_survive_source_edits() {
        let person_id = PersonId::new();
        let discover = |capability: &str, connections: &[SourceConnection]| {
            discover_source_candidates(SourceCandidateRequest {
                person_id,
                device_id: "mac-local",
                capability,
                contract_version: 1,
                remote_connections: &[],
                remote_execution_owner: None,
                source_connections: connections,
            })
            .unwrap()
        };
        assert!(discover(PEOPLE_VIEW_ID, &[]).is_empty());
        let mut contacts = SourceConnection::establish_reviewed_native(
            person_id,
            ConnectorId::try_new("contacts.apple").unwrap(),
            ConnectionId::try_new("contacts.apple.local").unwrap(),
            ExecutionOwnerId::try_new("apple:mac-local").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("a").unwrap(), "A".into()).unwrap(),
            ],
            "a".repeat(64),
        )
        .unwrap();
        let initial = discover(PEOPLE_VIEW_ID, std::slice::from_ref(&contacts));
        assert_eq!(initial.len(), 1);
        assert_eq!(
            initial[0].reference.resource.as_str(),
            "people.identity:contacts.apple.local"
        );
        contacts
            .configure_reviewed_native(
                1,
                ResourceMode::Selected,
                vec![
                    ConnectionResource::new(ResourceHandle::try_new("a").unwrap(), "A".into())
                        .unwrap(),
                    ConnectionResource::new(ResourceHandle::try_new("b").unwrap(), "B".into())
                        .unwrap(),
                ],
                "b".repeat(64),
            )
            .unwrap();
        assert_eq!(
            discover(PEOPLE_VIEW_ID, std::slice::from_ref(&contacts)),
            initial
        );
        contacts.disconnect(2).unwrap();
        assert!(discover(PEOPLE_VIEW_ID, std::slice::from_ref(&contacts)).is_empty());
    }

    #[test]
    fn personal_candidate_modes_and_singletons_are_checked_per_connection() {
        let person_id = PersonId::new();
        let sources = [
            (
                "attention.macos",
                ATTENTION_VIEW_ID,
                "macos:mac-local",
                ResourceMode::AllAvailable,
            ),
            (
                "health.apple",
                WELLBEING_VIEW_ID,
                "apple:mac-local",
                ResourceMode::AllAvailable,
            ),
        ];
        for (connector, view, owner, mode) in sources {
            let connection = SourceConnection::establish_reviewed_native(
                person_id,
                ConnectorId::try_new(connector).unwrap(),
                ConnectionId::try_new(format!("{connector}.local")).unwrap(),
                ExecutionOwnerId::try_new(owner).unwrap(),
                mode,
                vec![
                    ConnectionResource::new(ResourceHandle::try_new(view).unwrap(), view.into())
                        .unwrap(),
                ],
                "a".repeat(64),
            )
            .unwrap();
            let discover = |connection: &SourceConnection| {
                discover_source_candidates(SourceCandidateRequest {
                    person_id,
                    device_id: "mac-local",
                    capability: view,
                    contract_version: 1,
                    remote_connections: &[],
                    remote_execution_owner: None,
                    source_connections: std::slice::from_ref(connection),
                })
                .unwrap()
            };
            let initial = discover(&connection);
            assert_eq!(initial.len(), 1);
            assert_eq!(
                initial[0].reference.resource.as_str(),
                format!("{view}:{connector}.local")
            );
            let mut changed = connection.clone();
            changed
                .configure_reviewed_native(1, mode, changed.resources().to_vec(), "b".repeat(64))
                .unwrap();
            assert_eq!(discover(&changed), initial);
            changed
                .configure_reviewed_native(
                    2,
                    ResourceMode::Selected,
                    changed.resources().to_vec(),
                    "b".repeat(64),
                )
                .unwrap();
            assert!(discover(&changed).is_empty());
        }
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
            source_connections: &[],
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
