#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerIdentity {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingIssuer {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingConfirmation {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingStatus {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
    pub person_id: String,
    pub device_id: String,
    pub producer: Option<ProducerIdentity>,
    pub issuer: Option<PairingIssuer>,
    pub issuer_fingerprint: Option<String>,
    pub client_id: Option<String>,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingConfirmationRequest {
    pub pairing_id: String,
    pub polling_proof: String,
    pub challenge_id: String,
    pub key_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingStatusRequest {
    pub pairing_id: String,
    pub polling_proof: String,
}

/// A connector catalog exactly as a producer reported it.
///
/// Connections projects it; a model route never queries a source catalog.
pub struct ConnectorCatalogObservation {
    pub schema_version: u32,
    pub person_id: String,
    pub device_id: String,
    pub connectors: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarConnectionRef {
    pub connector_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
}

/// Project the connected calendar sources from an observed catalog.
///
/// The whole catalog is rejected when the producer identity does not match the
/// caller, when an identifier repeats, or when a connected entry is malformed.
pub fn project_calendar_connections(
    catalog: &ConnectorCatalogObservation,
    person_id: &str,
    device_id: &str,
) -> Option<Vec<CalendarConnectionRef>> {
    use std::collections::HashSet;

    if catalog.schema_version != 1
        || catalog.person_id != person_id
        || catalog.device_id != device_id
        || catalog.connectors.len() > 64
    {
        return None;
    }
    let mut identifiers = HashSet::new();
    let mut calendar = Vec::new();
    for raw in &catalog.connectors {
        let value = raw.as_object()?;
        let identifier = value.get("id")?.as_str()?;
        if !identifiers.insert(identifier.to_owned()) {
            return None;
        }
        if value.get("status")?.as_str()? != "connected"
            || !matches!(identifier, "calendar.google" | "calendar.microsoft")
        {
            continue;
        }
        let connection_id = value.get("connection_id")?.as_str()?;
        if uuid::Uuid::parse_str(connection_id).is_err() {
            return None;
        }
        let connection_revision = value.get("connection_revision")?.as_u64()?;
        if connection_revision == 0 {
            return None;
        }
        calendar.push(CalendarConnectionRef {
            connector_id: identifier.to_owned(),
            connection_id: connection_id.to_owned(),
            connection_revision,
        });
    }
    Some(calendar)
}
