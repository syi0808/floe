use base64::Engine as _;
use floe_kernel::AgentFailure;
use sha2::{Digest, Sha256};

/// The version a producer's pairing report must be stated in.
pub const PAIRING_REPORT_VERSION: u32 = 1;

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

/// Whether a producer's pairing report is one this device may act on.
///
/// The producer speaks for the pairing, not for the Person: a report that
/// renames the client, or whose issuer fingerprint disagrees with the issuer
/// key it carries, is refused rather than reconciled.
pub fn admit_pairing_status(status: PairingStatus) -> Result<PairingStatus, AgentFailure> {
    if status.schema_version != PAIRING_REPORT_VERSION
        || status.pairing_id.is_empty()
        || status.person_id.is_empty()
        || status.device_id.is_empty()
        || status
            .client_id
            .as_deref()
            .is_some_and(|client_id| client_id != status.pairing_id)
        || status.issuer.as_ref().is_some_and(|issuer| {
            status
                .issuer_fingerprint
                .as_deref()
                .is_some_and(|fingerprint| fingerprint != issuer.fingerprint)
        })
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    if let Some(issuer) = status.issuer.as_ref() {
        admit_pairing_issuer(issuer)?;
    }
    Ok(status)
}

/// Whether an issuer key is the key its own fingerprint names.
pub fn admit_pairing_issuer(issuer: &PairingIssuer) -> Result<(), AgentFailure> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(issuer.public_key.as_bytes())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    if decoded.len() != 32
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != issuer.public_key
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let fingerprint = Sha256::digest(&decoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if fingerprint != issuer.fingerprint {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok(())
}
