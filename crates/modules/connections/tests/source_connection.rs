use floe_connections::{
    ConnectionResource, ResourceMode, SourceConnection, SourceConnectionError, SourceState,
};
use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle};
use floe_kernel::PersonId;

fn resource(handle: &str, label: &str) -> ConnectionResource {
    ConnectionResource::new(ResourceHandle::try_new(handle).unwrap(), label.into()).unwrap()
}

fn source(connector: &str, resources: Vec<ConnectionResource>) -> SourceConnection {
    SourceConnection::establish(
        PersonId::new(),
        ConnectorId::try_new(connector).unwrap(),
        ConnectionId::new(),
        ExecutionOwnerId::try_new("device-1").unwrap(),
        ResourceMode::Selected,
        resources,
    )
    .unwrap()
}

#[test]
fn resource_changes_advance_authority_once_and_reordering_is_noop() {
    let mut connection = source(
        "calendar.fixture",
        vec![resource("b", "B"), resource("a", "A")],
    );
    assert_eq!(connection.revision(), 1);
    assert_eq!(connection.resources()[0].handle().as_str(), "a");
    let original = connection.source_authority();
    assert!(
        !connection
            .configure(
                1,
                ResourceMode::Selected,
                vec![resource("a", "A"), resource("b", "B")]
            )
            .unwrap()
    );
    assert_eq!(connection.source_authority(), original);
    assert!(
        connection
            .configure(
                1,
                ResourceMode::Selected,
                vec![resource("a", "A"), resource("b", "B"), resource("c", "C")]
            )
            .unwrap()
    );
    assert_eq!(connection.revision(), 2);
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.epoch().get() + 1
    );
    assert!(
        connection
            .configure(2, ResourceMode::Selected, vec![resource("a", "A")])
            .unwrap()
    );
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.epoch().get() + 2
    );
}

#[test]
fn label_only_change_does_not_change_source_authority() {
    let mut connection = source("calendar.fixture", vec![resource("a", "Old")]);
    let original = connection.source_authority();
    assert!(
        connection
            .configure(1, ResourceMode::Selected, vec![resource("a", "New")])
            .unwrap()
    );
    assert_eq!(connection.revision(), 2);
    assert_eq!(connection.source_authority(), original);
    assert_eq!(
        connection.configure(1, ResourceMode::Selected, vec![resource("a", "Other")]),
        Err(SourceConnectionError::Conflict)
    );
}

#[test]
fn native_subject_controls_serving_and_source_epoch() {
    let mut connection = source("calendar.event_kit", vec![resource("a", "A")]);
    assert_eq!(connection.state(), SourceState::Pending);
    assert!(!connection.is_serving());
    let original = connection.source_authority();
    let first = "a".repeat(64);
    assert!(connection.update_native_subject(1, first.clone()).unwrap());
    assert!(connection.is_serving());
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.epoch().get() + 1
    );
    assert!(!connection.update_native_subject(2, first).unwrap());
    assert_eq!(connection.revision(), 2);
    assert!(connection.update_native_subject(2, "b".repeat(64)).unwrap());
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.epoch().get() + 2
    );
    assert!(connection.disconnect(3).unwrap());
    assert!(!connection.is_serving());
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.epoch().get() + 3
    );
    assert_eq!(
        connection.configure(4, ResourceMode::Selected, vec![]),
        Err(SourceConnectionError::Disconnected)
    );
}

#[test]
fn restore_rejects_corrupt_revision_authority_and_resources() {
    let connection = source("calendar.fixture", vec![resource("a", "A")]);
    let mut value = serde_json::to_value(&connection).unwrap();
    value["revision"] = 0.into();
    let corrupt: SourceConnection = serde_json::from_value(value).unwrap();
    assert_eq!(
        corrupt.validate(),
        Err(SourceConnectionError::InvalidAuthority)
    );

    let mut value = serde_json::to_value(&connection).unwrap();
    let duplicate = value["resources"][0].clone();
    value["resources"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let corrupt: SourceConnection = serde_json::from_value(value).unwrap();
    assert_eq!(
        corrupt.validate(),
        Err(SourceConnectionError::InvalidResource)
    );
}
