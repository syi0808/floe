//! Strict paired Gateway schema 3. Provider/account routing never crosses this wire.
use floe_agent_contract::AgentFailure;
use floe_agent_contract::ModelBudgetProfile;
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
        budget_profile: ModelBudgetProfile,
    },
    NotConfigured,
    Disabled,
}
impl Inventory {
    pub fn selected(self, purpose: &str) -> Result<PurposeCapability, ModelObservationError> {
        if self.schema_version != 3 {
            return Err(ModelObservationError::InvalidInventory);
        }
        for capability in [
            &self.purposes.quick_response,
            &self.purposes.everyday_assistance,
            &self.purposes.deep_work,
        ] {
            match capability {
                PurposeCapability::Available {
                    capability_revision,
                    capabilities,
                    budget_profile,
                } => {
                    if !valid_hex(capability_revision, 64)
                        || model_capabilities(capabilities).is_err()
                        || budget_profile.validate().is_err()
                    {
                        return Err(ModelObservationError::InvalidInventory);
                    }
                }
                PurposeCapability::NotConfigured | PurposeCapability::Disabled => {}
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

pub(crate) fn selection_commitment(
    revision: &str,
) -> Result<floe_agent_contract::ModelSelectionCommitment, ModelObservationError> {
    if !valid_hex(revision, 64) {
        return Err(ModelObservationError::InvalidInventory);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in revision.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or(ModelObservationError::InvalidInventory)?;
        let low = hex_nibble(pair[1]).ok_or(ModelObservationError::InvalidInventory)?;
        bytes[index] = (high << 4) | low;
    }
    let commitment = floe_agent_contract::ModelSelectionCommitment(bytes);
    commitment
        .validate()
        .map_err(|_| ModelObservationError::InvalidInventory)?;
    Ok(commitment)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}
pub(crate) fn failure(
    status: u16,
    bytes: &[u8],
) -> Result<(ModelObservationError, AgentFailure), AgentFailure> {
    use AgentFailure as F;
    let body: InferenceErrorBody = decode(bytes)?;
    if body.schema_version != 3
        || body.trace_id.is_some()
        || body.attempt_id.is_some()
        || body.purpose.is_some()
        || body.capability_revision.is_some()
        || body.usage.tokens.is_some()
        || body.usage.cost_micros.is_some()
    {
        return Err(F::ServerModelInvalidOutput);
    }
    map_failure(status, &body.error.code)
}

fn map_failure(
    status: u16,
    code: &str,
) -> Result<(ModelObservationError, AgentFailure), AgentFailure> {
    use AgentFailure as F;
    use ModelObservationError as O;
    let mapped = match (status, code) {
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
        (413, "body_too_large") => (O::InvalidInventory, F::ModelInputCapacityExceeded),
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

pub(crate) fn model_capabilities(
    values: &[String],
) -> Result<floe_agent_contract::ModelCapabilities, AgentFailure> {
    use floe_agent_contract::{ModelCapabilities, ModelCapability};
    let capabilities = ModelCapabilities(
        values
            .iter()
            .map(|value| match value.as_str() {
                "chat" => Ok(ModelCapability::Chat),
                "structured_output" => Ok(ModelCapability::StructuredOutput),
                "tool_proposals" => Ok(ModelCapability::ToolProposals),
                _ => Err(AgentFailure::ServerModelInvalidOutput),
            })
            .collect::<Result<_, _>>()?,
    );
    capabilities
        .validate()
        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    Ok(capabilities)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InferenceErrorBody {
    schema_version: u32,
    error: ErrorCode,
    #[serde(deserialize_with = "required_trace")]
    trace_id: Option<String>,
    #[serde(deserialize_with = "required_trace")]
    attempt_id: Option<String>,
    #[serde(deserialize_with = "required_trace")]
    purpose: Option<String>,
    #[serde(deserialize_with = "required_trace")]
    capability_revision: Option<String>,
    usage: Usage,
}
/// Only the exact admitted response correlation can carry observed failure usage.
pub(crate) fn inference_failure(
    status: u16,
    bytes: &[u8],
    attempt_id: uuid::Uuid,
    purpose: &str,
    revision: &str,
) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
    let body: InferenceErrorBody = decode(bytes)?;
    if body.schema_version != 3
        || body
            .trace_id
            .as_ref()
            .is_some_and(|trace| !valid_hex(trace, 32))
    {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    let failure = map_failure(status, &body.error.code)?.1;
    let correlation = (
        body.attempt_id.as_deref(),
        body.purpose.as_deref(),
        body.capability_revision.as_deref(),
    );
    if correlation == (None, None, None)
        && body.trace_id.is_none()
        && body.usage.tokens.is_none()
        && body.usage.cost_micros.is_none()
    {
        return Err(failure);
    }
    if body.trace_id.is_none()
        || body.attempt_id.as_deref() != Some(attempt_id.to_string().as_str())
        || body.purpose.as_deref() != Some(purpose)
        || body.capability_revision.as_deref() != Some(revision)
    {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    Ok(floe_inference::CanonicalModelResponse {
        output: Err(failure),
        usage: floe_inference::ProviderUsageObservation {
            tokens: body.usage.tokens,
            cost_micros: body.usage.cost_micros,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{AgentFailure, Inventory, PurposeCapability};

    fn fixture(name: &str) -> &'static str {
        match name {
            "unknown" => {
                include_str!("../../../../../testdata/inference-budget-schema3-unknown.json")
            }
            "unavailable" => {
                include_str!("../../../../../testdata/inference-budget-schema3-unavailable.json")
            }
            "configured" => {
                include_str!("../../../../../testdata/inference-budget-schema3-configured.json")
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn shared_schema3_budget_fixtures_preserve_unknown_and_descriptive_sources() {
        for name in ["unknown", "unavailable", "configured"] {
            let inventory: Inventory =
                serde_json::from_str(fixture(name)).expect("valid shared inventory fixture");
            let selected = inventory
                .selected("quick_response")
                .expect("valid selected capability");
            let PurposeCapability::Available {
                capabilities,
                budget_profile,
                ..
            } = selected
            else {
                panic!("shared fixture must have an available purpose");
            };
            assert_eq!(capabilities, vec!["chat".to_owned()]);
            budget_profile.validate().expect("valid v1 profile");
            if name == "configured" {
                assert_eq!(budget_profile.context_window.tokens, Some(10_000));
                assert_eq!(
                    budget_profile.sources.catalog.context_window_tokens,
                    Some(8_192)
                );
                assert_eq!(budget_profile.framing.max_input_json_bytes, 16_000);
            } else {
                assert_eq!(budget_profile.context_window.tokens, None);
                assert_eq!(budget_profile.max_output.tokens, None);
            }
        }
    }

    #[test]
    fn schema3_capability_drift_and_oversize_errors_keep_typed_rust_outcomes() {
        let response = |code: &str| {
            serde_json::json!({
                "schema_version": 3,
                "error": {"code": code},
                "trace_id": null,
                "attempt_id": null,
                "purpose": null,
                "capability_revision": null,
                "usage": {"tokens": null, "cost_micros": null}
            })
            .to_string()
        };
        let drift = response("capability_changed");
        let (observation, failure) =
            super::failure(409, drift.as_bytes()).expect("typed drift response");
        assert_eq!(
            observation,
            floe_inference::ModelObservationError::InvalidInventory
        );
        assert_eq!(failure, AgentFailure::PolicyDenied);

        let oversize = response("body_too_large");
        let (observation, failure) =
            super::failure(413, oversize.as_bytes()).expect("typed capacity response");
        assert_eq!(
            observation,
            floe_inference::ModelObservationError::InvalidInventory
        );
        assert_eq!(failure, AgentFailure::ModelInputCapacityExceeded);
    }

    #[test]
    fn go_compatible_model_estimate_counts_korean_and_escaped_html_bytes() {
        let encoded = serde_json::to_vec(&serde_json::json!({"input": "서울 <>&\u{2028}"}))
            .expect("UTF-8 JSON");
        let estimate = crate::gateway::inference::go_compatible_json_len(&encoded);
        assert_eq!(estimate, encoded.len() + 5 * 3 + 3);
        assert!(encoded.iter().any(|byte| *byte >= 0x80));
    }
}
