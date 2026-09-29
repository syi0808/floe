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
    value["resources"].as_array_mut().unwrap().push(duplicate);
    let corrupt: SourceConnection = serde_json::from_value(value).unwrap();
    assert_eq!(
        corrupt.validate(),
        Err(SourceConnectionError::InvalidResource)
    );
}

#[test]
fn persistence_successor_rejects_identity_and_authority_forgery() {
    let original = source("calendar.fixture", vec![resource("a", "A")]);
    let mut updated = original.clone();
    updated
        .configure(1, ResourceMode::Selected, vec![resource("a", "Renamed")])
        .unwrap();
    original.validate_successor(&updated).unwrap();

    let mut value = serde_json::to_value(&updated).unwrap();
    value["person_id"] = serde_json::to_value(floe_kernel::PersonId::new()).unwrap();
    let forged: SourceConnection = serde_json::from_value(value).unwrap();
    assert_eq!(
        original.validate_successor(&forged),
        Err(SourceConnectionError::InvalidTransition)
    );

    let mut value = serde_json::to_value(&updated).unwrap();
    value["source_authority"] =
        serde_json::to_value(floe_context_contract::SourceAuthority::new()).unwrap();
    let forged: SourceConnection = serde_json::from_value(value).unwrap();
    assert_eq!(
        original.validate_successor(&forged),
        Err(SourceConnectionError::InvalidTransition)
    );
}

#[test]
fn duplicate_resource_and_invalid_native_subject_fail_closed() {
    assert_eq!(
        SourceConnection::establish(
            PersonId::new(),
            ConnectorId::try_new("calendar.fixture").unwrap(),
            ConnectionId::new(),
            ExecutionOwnerId::try_new("device-1").unwrap(),
            ResourceMode::Selected,
            vec![resource("a", "A"), resource("a", "Duplicate")],
        ),
        Err(SourceConnectionError::InvalidResource)
    );
    let mut native = source("calendar.event_kit", vec![resource("a", "A")]);
    assert_eq!(
        native.update_native_subject(1, "untrusted".into()),
        Err(SourceConnectionError::InvalidSubject)
    );
    assert_eq!(native.revision(), 1);
}

#[test]
fn inventory_reconciliation_requires_all_available_scope() {
    let mut connection = source("calendar.fixture", vec![resource("a", "A")]);
    assert_eq!(
        connection.reconcile_inventory(1, vec![resource("a", "A"), resource("b", "B")]),
        Err(SourceConnectionError::InvalidResource)
    );
    assert!(
        connection
            .configure(1, ResourceMode::AllAvailable, vec![resource("a", "A")])
            .unwrap()
    );
    let authority = connection.source_authority();
    assert!(
        connection
            .reconcile_inventory(2, vec![resource("a", "A"), resource("b", "B")])
            .unwrap()
    );
    assert_eq!(
        connection.source_authority().epoch().get(),
        authority.epoch().get() + 1
    );
    assert!(
        !connection
            .reconcile_inventory(3, vec![resource("b", "B"), resource("a", "A")])
            .unwrap()
    );
    assert_eq!(connection.revision(), 3);
}

#[test]
fn reviewed_native_creation_is_ready_at_first_revision() {
    for connector in ["contacts.apple", "attention.macos", "health.apple"] {
        let mode = if connector == "contacts.apple" {
            ResourceMode::Selected
        } else {
            ResourceMode::AllAvailable
        };
        let connection = SourceConnection::establish_reviewed_native(
            PersonId::new(),
            ConnectorId::try_new(connector).unwrap(),
            ConnectionId::new(),
            ExecutionOwnerId::try_new("device-1").unwrap(),
            mode,
            vec![resource("a", "A")],
            "a".repeat(64),
        )
        .unwrap();
        assert_eq!(connection.revision(), 1);
        assert_eq!(connection.state(), SourceState::Ready);
        assert!(connection.source_authority().is_valid());
        assert_eq!(
            connection.native_subject_fingerprint(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        connection.validate().unwrap();
    }
}

#[test]
fn reviewed_contacts_configuration_advances_once_for_resources_and_subject() {
    let mut connection = SourceConnection::establish_reviewed_native(
        PersonId::new(),
        ConnectorId::try_new("contacts.apple").unwrap(),
        ConnectionId::new(),
        ExecutionOwnerId::try_new("device-1").unwrap(),
        ResourceMode::Selected,
        vec![resource("a", "A")],
        "a".repeat(64),
    )
    .unwrap();
    let original = connection.clone();
    assert!(
        connection
            .configure_reviewed_native(
                1,
                ResourceMode::Selected,
                vec![resource("b", "B"), resource("a", "A")],
                "b".repeat(64),
            )
            .unwrap()
    );
    original.validate_successor(&connection).unwrap();
    assert_eq!(connection.revision(), 2);
    assert_eq!(
        connection.source_authority().incarnation(),
        original.source_authority().incarnation()
    );
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.source_authority().epoch().get() + 1
    );
    assert!(
        !connection
            .configure_reviewed_native(
                2,
                ResourceMode::Selected,
                vec![resource("a", "A"), resource("b", "B")],
                "b".repeat(64),
            )
            .unwrap()
    );
    assert_eq!(connection.revision(), 2);
    let original = connection.clone();
    assert!(
        connection
            .configure_reviewed_native(
                2,
                ResourceMode::Selected,
                vec![resource("a", "Renamed"), resource("b", "B")],
                "b".repeat(64),
            )
            .unwrap()
    );
    original.validate_successor(&connection).unwrap();
    assert_eq!(connection.source_authority(), original.source_authority());
    let original = connection.clone();
    assert!(
        connection
            .configure_reviewed_native(
                3,
                ResourceMode::Selected,
                vec![resource("a", "Renamed"), resource("b", "B")],
                "c".repeat(64),
            )
            .unwrap()
    );
    original.validate_successor(&connection).unwrap();
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.source_authority().epoch().get() + 1
    );
    let original = connection.clone();
    assert!(
        connection
            .configure_reviewed_native(
                4,
                ResourceMode::Selected,
                vec![resource("a", "Renamed")],
                "c".repeat(64),
            )
            .unwrap()
    );
    original.validate_successor(&connection).unwrap();
    assert_eq!(
        connection.source_authority().epoch().get(),
        original.source_authority().epoch().get() + 1
    );
    assert!(connection.disconnect(5).unwrap());
    assert_eq!(
        connection.configure_reviewed_native(
            6,
            ResourceMode::Selected,
            vec![resource("a", "A")],
            "d".repeat(64)
        ),
        Err(SourceConnectionError::Disconnected)
    );
}
