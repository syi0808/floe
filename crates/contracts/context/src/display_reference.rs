//! Person-scoped correlation for product source/resource displays, never authority.
use crate::{ConnectionId, PersonId};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn source_display_ref(person: PersonId, connection: &ConnectionId) -> Uuid {
    reference(
        b"floe.source.ref.v1",
        person,
        Value::String(connection.as_str().into()),
    )
}

/// The resource is the owner's opaque handle, not its mutable display label.
pub fn resource_display_ref(person: PersonId, connection: &ConnectionId, resource: &str) -> Uuid {
    reference(
        b"floe.resource.ref.v1",
        person,
        Value::Array(vec![
            Value::String(connection.as_str().into()),
            Value::String(resource.into()),
        ]),
    )
}

fn reference(domain: &[u8], person: PersonId, value: Value) -> Uuid {
    // The string/array JSON encoding preserves the existing Connections reference
    // bytes. No caller-supplied Serialize implementation or fallible payload exists.
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update([0]);
    hash.update(person.to_string().as_bytes());
    hash.update(value.to_string().as_bytes());
    let digest = hash.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Uuid::from_bytes(bytes)
}
