//! Exact physical Gateway schema. Workflow and authority remain with their owners.
use super::SchemaObject;

pub(super) const GATEWAY_OBJECTS: &[SchemaObject] = &[
    SchemaObject::table(
        "connections_command_rejections",
        "CREATE TABLE connections_command_rejections(person_id TEXT NOT NULL,command_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(person_id,command_id))",
    ),
    SchemaObject::table(
        "gateway_pairing_command_receipts",
        "CREATE TABLE gateway_pairing_command_receipts(person_id TEXT NOT NULL,command_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(person_id,command_id))",
    ),
    SchemaObject::table(
        "gateway_pairing_generation",
        "CREATE TABLE gateway_pairing_generation(id INTEGER PRIMARY KEY CHECK(id=1),generation INTEGER NOT NULL CHECK(generation>=0))",
    ),
    SchemaObject::marker(
        "remote_authority_schema",
        2,
        "CREATE TABLE remote_authority_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL)",
    ),
    SchemaObject::table(
        "remote_authority_producer",
        "CREATE TABLE remote_authority_producer(id INTEGER PRIMARY KEY CHECK(id=1),identity_json TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0))",
    ),
    SchemaObject::table(
        "gateway_credential_expectation",
        "CREATE TABLE gateway_credential_expectation(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "remote_authority_clock",
        "CREATE TABLE remote_authority_clock(id INTEGER PRIMARY KEY CHECK(id=1),last_now_unix_ms INTEGER NOT NULL)",
    ),
    SchemaObject::table(
        "gateway_pin_receipts",
        "CREATE TABLE gateway_pin_receipts(operation_id TEXT PRIMARY KEY,payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "gateway_enrollment_receipts",
        "CREATE TABLE gateway_enrollment_receipts(operation_id TEXT PRIMARY KEY,challenge_id TEXT UNIQUE NOT NULL,command_json TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "gateway_authorization_receipts",
        "CREATE TABLE gateway_authorization_receipts(operation_id TEXT PRIMARY KEY,challenge_id TEXT UNIQUE NOT NULL,request_digest TEXT NOT NULL,operation TEXT NOT NULL,admission_id TEXT NOT NULL,expectation_json TEXT NOT NULL,expires_at_unix_ms INTEGER NOT NULL)",
    ),
    SchemaObject::table(
        "gateway_product_authorization_receipts",
        "CREATE TABLE gateway_product_authorization_receipts(challenge_id TEXT PRIMARY KEY,request_digest TEXT NOT NULL,operation TEXT NOT NULL CHECK(operation IN ('day_calendar_admission','day_calendar_release')),admission_id TEXT NOT NULL,person_id TEXT NOT NULL,device_id TEXT NOT NULL,owner_key_id TEXT NOT NULL,gateway_runtime_generation INTEGER NOT NULL CHECK(gateway_runtime_generation>0),expires_at_unix_ms INTEGER NOT NULL,payload TEXT NOT NULL)",
    ),
    SchemaObject::index(
        "gateway_product_one_release",
        "CREATE UNIQUE INDEX gateway_product_one_release ON gateway_product_authorization_receipts(admission_id) WHERE operation='day_calendar_release'",
    ),
    SchemaObject::table(
        "connections_product_records",
        "CREATE TABLE connections_product_records(record_ref TEXT PRIMARY KEY,person_id TEXT NOT NULL,command_id TEXT NOT NULL,revision INTEGER NOT NULL,payload TEXT NOT NULL,UNIQUE(person_id,command_id))",
    ),
    SchemaObject::table(
        "gateway_setup_receipts",
        "CREATE TABLE gateway_setup_receipts(target_ref TEXT PRIMARY KEY,person_id TEXT NOT NULL,command_id TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(person_id,command_id))",
    ),
    SchemaObject::table(
        "gateway_pairing_operations",
        "CREATE TABLE gateway_pairing_operations(operation_id TEXT PRIMARY KEY,person_id TEXT NOT NULL,command_id TEXT NOT NULL,revision INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(person_id,command_id))",
    ),
    SchemaObject::table(
        "gateway_pairing_private",
        "CREATE TABLE gateway_pairing_private(operation_id TEXT PRIMARY KEY REFERENCES gateway_pairing_operations(operation_id),proof BLOB NOT NULL CHECK(length(proof)=32),issuer_key_id TEXT UNIQUE NOT NULL,issuer_public_key TEXT UNIQUE NOT NULL,issuer_nonce TEXT NOT NULL,issuer_ciphertext TEXT NOT NULL,enrollment_json TEXT,credential BLOB CHECK(credential IS NULL OR (length(credential)>=32 AND length(credential)<=256)))",
    ),
];
