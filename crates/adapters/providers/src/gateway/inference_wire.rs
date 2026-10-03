//! Strict paired Gateway schema 2. Provider/account routing never crosses this wire.
use floe_agent_contract::AgentFailure;
use floe_inference::ModelObservationError;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub(crate) const MAX_RESPONSE_BYTES: usize = 65_536;
pub(crate) const MAX_REQUEST_BYTES: usize = 98_304;
pub(crate) const MAX_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inventory {
    pub schema_version: u32,
    pub purposes: Purposes,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Purposes {
    pub quick_response: PurposeCapability,
    pub everyday_assistance: PurposeCapability,
    pub deep_work: PurposeCapability,
}
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PurposeCapability {
    Available {
        capability_revision: String,
        capabilities: Vec<String>,
    },
    NotConfigured,
    Disabled,
}
impl Inventory {
    pub fn selected(self, purpose: &str) -> Result<PurposeCapability, ModelObservationError> {
        if self.schema_version != 2 {
            return Err(ModelObservationError::InvalidInventory);
        }
        for capability in [
            &self.purposes.quick_response,
            &self.purposes.everyday_assistance,
            &self.purposes.deep_work,
        ] {
            if let PurposeCapability::Available {
                capability_revision,
                capabilities,
            } = capability
            {
                if !valid_hex(capability_revision, 64) || capabilities.as_slice() != ["chat"] {
                    return Err(ModelObservationError::InvalidInventory);
                }
            }
        }
        match purpose {
            "quick_response" => Ok(self.purposes.quick_response),
            "everyday_assistance" => Ok(self.purposes.everyday_assistance),
            "deep_work" => Ok(self.purposes.deep_work),
            _ => Err(ModelObservationError::InvalidIdentity),
        }
    }
}

// Output remains an opaque JSON value until the complete metadata/usage envelope
// is validated, so catalog/output rejection cannot erase reported accounting.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentResponse {
    pub schema_version: u32,
    pub purpose: String,
    pub capability_revision: String,
    pub attempt_id: String,
    pub trace_id: String,
    pub output: Value,
    pub call_ids: Value,
    pub usage: Usage,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Usage {
    #[serde(deserialize_with = "required_nullable")]
    pub tokens: Option<u64>,
    #[serde(deserialize_with = "required_nullable")]
    pub cost_micros: Option<u64>,
}
fn required_nullable<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    let value = Option::<u64>::deserialize(deserializer)?;
    if value.is_some_and(|n| n > MAX_INTEGER) {
        return Err(serde::de::Error::custom("integer outside wire bound"));
    }
    Ok(value)
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum WireStep {
    Preamble {
        text: String,
    },
    Answer {
        text: String,
    },
    Call {
        capability_id: String,
        input: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorBody {
    schema_version: u32,
    error: ErrorCode,
    #[serde(deserialize_with = "required_trace")]
    trace_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorCode {
    code: String,
}
fn required_trace<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

pub(crate) fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, AgentFailure> {
    super::json::strict_json_bytes(bytes, MAX_RESPONSE_BYTES)
        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    serde_json::from_slice(bytes).map_err(|_| AgentFailure::ServerModelInvalidOutput)
}
pub(crate) fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn valid_call_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
pub(crate) fn valid_alias(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub(crate) fn failure(
    status: u16,
    bytes: &[u8],
) -> Result<(ModelObservationError, AgentFailure), AgentFailure> {
    use AgentFailure as F;
    use ModelObservationError as O;
    let body: ErrorBody = decode(bytes)?;
    if body.schema_version != 2 || body.trace_id.as_ref().is_some_and(|id| !valid_hex(id, 32)) {
        return Err(F::ServerModelInvalidOutput);
    }
    let mapped = match (status, body.error.code.as_str()) {
        (400, "validation" | "unsupported_schema")
        | (404, "not_found")
        | (405, "method_not_allowed")
        | (415, "content_type_unsupported") => (O::InvalidInventory, F::ServerModelRequestRejected),
        (401, "unauthorized") => (O::CredentialRejected, F::CredentialExpired),
        (403, "permission_denied") => (O::PermissionDenied, F::PolicyDenied),
        (403, "identity_mismatch") => (O::InvalidIdentity, F::PolicyDenied),
        (409, "capability_changed" | "purpose_not_configured" | "purpose_disabled") => {
            (O::InvalidInventory, F::PolicyDenied)
        }
        (413, "body_too_large") => (O::InvalidInventory, F::BudgetExceeded),
        (429, "model_busy" | "quota_exceeded") => (O::QuotaExceeded, F::QuotaExceeded),
        (502, "invalid_output") => (O::InvalidInventory, F::ServerModelInvalidOutput),
        (502, "request_rejected") => (O::TransportUnavailable, F::ServerModelRequestRejected),
        (503, "provider_credentials_unavailable" | "model_unavailable") => {
            (O::TransportUnavailable, F::ServerModelUnavailable)
        }
        (504, "model_timeout") => (O::Timeout, F::ServerModelTimeout),
        _ => return Err(F::ServerModelInvalidOutput),
    };
    Ok(mapped)
}
