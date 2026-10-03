//! Fixed plain layout. No existing-store admission creates objects.
use super::SchemaObject;

pub(super) const OBJECTS: &[SchemaObject] = &[
    SchemaObject::marker(
        "floe_source_schema",
        1,
        "CREATE TABLE floe_source_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",
    ),
    SchemaObject::table(
        "source_connections",
        "CREATE TABLE source_connections (connection_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector_id TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "source_operations",
        "CREATE TABLE source_operations (operation_id TEXT PRIMARY KEY, command_id TEXT NOT NULL, person_id TEXT NOT NULL, connection_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), fence INTEGER NOT NULL CHECK(fence IN (0,1)), payload TEXT NOT NULL, UNIQUE(person_id,command_id))",
    ),
    SchemaObject::marker(
        "floe_day_schema",
        1,
        "CREATE TABLE floe_day_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",
    ),
    SchemaObject::table(
        "day_refreshes",
        "CREATE TABLE day_refreshes (operation_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_id TEXT NOT NULL, executor_generation TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), payload TEXT NOT NULL, UNIQUE(person_id, command_id))",
    ),
    SchemaObject::table(
        "day_executors",
        "CREATE TABLE day_executors (person_id TEXT NOT NULL, device_id TEXT NOT NULL, executor_generation TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), PRIMARY KEY(person_id,device_id))",
    ),
    SchemaObject::table(
        "day_action_collections",
        "CREATE TABLE day_action_collections (execution_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, receipt_digest TEXT NOT NULL, intent_digest TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "day_mutation_receipts",
        "CREATE TABLE day_mutation_receipts (person_id TEXT NOT NULL, command_id TEXT NOT NULL, device_id TEXT NOT NULL, intent_digest TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(person_id,command_id))",
    ),
    SchemaObject::table(
        "calendar_actions",
        "CREATE TABLE calendar_actions (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "action_authorities",
        "CREATE TABLE action_authorities (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "captures",
        "CREATE TABLE captures (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "events",
        "CREATE TABLE events (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "tasks",
        "CREATE TABLE tasks (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "notes",
        "CREATE TABLE notes (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "calendar_mirrors",
        "CREATE TABLE calendar_mirrors (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::index(
        "source_connections_person_connector",
        "CREATE INDEX source_connections_person_connector ON source_connections(person_id, connector_id)",
    ),
    SchemaObject::index(
        "source_operation_fence",
        "CREATE UNIQUE INDEX source_operation_fence ON source_operations(connection_id) WHERE fence = 1",
    ),
    SchemaObject::index(
        "source_operation_history",
        "CREATE INDEX source_operation_history ON source_operations(connection_id)",
    ),
    SchemaObject::index(
        "day_refreshes_person_device_generation",
        "CREATE INDEX day_refreshes_person_device_generation ON day_refreshes(person_id, device_id, executor_generation)",
    ),
    SchemaObject::index(
        "calendar_actions_person",
        "CREATE INDEX calendar_actions_person ON calendar_actions(person_id)",
    ),
    SchemaObject::index(
        "action_authorities_person",
        "CREATE INDEX action_authorities_person ON action_authorities(person_id)",
    ),
    SchemaObject::index(
        "captures_person",
        "CREATE INDEX captures_person ON captures(person_id)",
    ),
    SchemaObject::index(
        "events_person",
        "CREATE INDEX events_person ON events(person_id)",
    ),
    SchemaObject::index(
        "tasks_person",
        "CREATE INDEX tasks_person ON tasks(person_id)",
    ),
    SchemaObject::index(
        "notes_person",
        "CREATE INDEX notes_person ON notes(person_id)",
    ),
    SchemaObject::index(
        "calendar_mirrors_person",
        "CREATE INDEX calendar_mirrors_person ON calendar_mirrors(person_id)",
    ),
];
