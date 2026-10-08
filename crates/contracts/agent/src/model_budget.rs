use serde::{Deserialize, Deserializer, Serialize};

use crate::AgentFailure;

pub const MODEL_BUDGET_PROFILE_VERSION: u32 = 1;
pub const MODEL_BUDGET_OVERRIDE_VERSION: u32 = 1;
pub const LEGACY_AGENT_INPUT_BYTES: u32 = 32_768;
pub const GATEWAY_INSTRUCTION_BYTES: u32 = 9_216;
pub const GATEWAY_MESSAGE_COUNT: u32 = 256;
pub const GATEWAY_TOOL_COUNT: u32 = 64;
pub const AGENT_CONVERSATION_BYTES: u32 = 128 * 1024;
pub const GATEWAY_REQUEST_BYTES: u32 = 98_304;
pub const GATEWAY_OUTPUT_BYTES: u32 = 16_384;
pub const MAX_KNOWN_MODEL_TOKENS: u32 = 10_000_000;

pub const MODEL_INPUT_ESTIMATE_METHOD: &str = "utf8_model_input_bytes_upper_estimate_v1";
pub const MODEL_INPUT_ESTIMATE_UNCERTAINTY: &str = "exact_tokenizer_unavailable";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenLimitStatus {
    Unknown,
    Known,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenLimitSource {
    Unknown,
    OperatorConfiguration,
    ProviderConfirmed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTokenLimit {
    pub status: TokenLimitStatus,
    #[serde(deserialize_with = "required_nullable_u32")]
    pub tokens: Option<u32>,
    pub source: TokenLimitSource,
}

impl ModelTokenLimit {
    pub fn unknown() -> Self {
        Self {
            status: TokenLimitStatus::Unknown,
            tokens: None,
            source: TokenLimitSource::Unknown,
        }
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        let valid = match (self.status, self.tokens, self.source) {
            (TokenLimitStatus::Unknown, None, TokenLimitSource::Unknown) => true,
            (TokenLimitStatus::Known, Some(value), TokenLimitSource::OperatorConfiguration)
            | (TokenLimitStatus::Known, Some(value), TokenLimitSource::ProviderConfirmed) => {
                value > 0 && value <= MAX_KNOWN_MODEL_TOKENS
            }
            _ => false,
        };
        valid.then_some(()).ok_or(AgentFailure::InvalidInput)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogMetadataStatus {
    Unavailable,
    ModelNotListed,
    MetadataUnknown,
    Available,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetProvenance {
    pub source: String,
    pub verified_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogBudgetFacts {
    pub status: CatalogMetadataStatus,
    #[serde(deserialize_with = "required_nullable_u64")]
    pub revision: Option<u64>,
    #[serde(deserialize_with = "required_nullable_u32")]
    pub context_window_tokens: Option<u32>,
    #[serde(deserialize_with = "required_nullable_u32")]
    pub max_output_tokens: Option<u32>,
    #[serde(deserialize_with = "required_nullable_provenance")]
    pub provenance: Option<BudgetProvenance>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorConfigurationStatus {
    Unconfigured,
    Configured,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderConfirmedStatus {
    Unknown,
    Available,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelBudgetSources {
    pub catalog: CatalogBudgetFacts,
    pub operator_configuration: OperatorConfigurationStatus,
    pub provider_confirmed: ProviderConfirmedStatus,
    #[serde(deserialize_with = "required_nullable_u32")]
    pub operator_configuration_version: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTokenEstimatePolicy {
    pub version: u32,
    pub method: String,
    pub uncertainty: String,
    #[serde(deserialize_with = "required_nullable_u32")]
    pub provider_overhead_tokens: Option<u32>,
    #[serde(deserialize_with = "required_nullable_u32")]
    pub safety_margin_tokens: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFramingGuards {
    #[serde(deserialize_with = "required_nullable_u32")]
    pub configured_input_json_bytes: Option<u32>,
    pub max_input_json_bytes: u32,
    pub max_instruction_bytes: u32,
    pub max_messages: u32,
    pub max_tools: u32,
    pub max_conversation_bytes: u32,
    pub max_request_bytes: u32,
    pub max_output_bytes: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelBudgetProfile {
    pub schema_version: u32,
    pub context_window: ModelTokenLimit,
    pub max_output: ModelTokenLimit,
    pub selected_output_reservation: ModelTokenLimit,
    pub estimator: ModelTokenEstimatePolicy,
    pub framing: ModelFramingGuards,
    pub sources: ModelBudgetSources,
}

impl ModelBudgetProfile {
    pub fn unknown() -> Self {
        Self {
            schema_version: MODEL_BUDGET_PROFILE_VERSION,
            context_window: ModelTokenLimit::unknown(),
            max_output: ModelTokenLimit::unknown(),
            selected_output_reservation: ModelTokenLimit::unknown(),
            estimator: ModelTokenEstimatePolicy {
                version: MODEL_BUDGET_PROFILE_VERSION,
                method: MODEL_INPUT_ESTIMATE_METHOD.to_owned(),
                uncertainty: MODEL_INPUT_ESTIMATE_UNCERTAINTY.to_owned(),
                provider_overhead_tokens: None,
                safety_margin_tokens: None,
            },
            framing: ModelFramingGuards {
                configured_input_json_bytes: None,
                max_input_json_bytes: LEGACY_AGENT_INPUT_BYTES,
                max_instruction_bytes: GATEWAY_INSTRUCTION_BYTES,
                max_messages: GATEWAY_MESSAGE_COUNT,
                max_tools: GATEWAY_TOOL_COUNT,
                max_conversation_bytes: AGENT_CONVERSATION_BYTES,
                max_request_bytes: GATEWAY_REQUEST_BYTES,
                max_output_bytes: GATEWAY_OUTPUT_BYTES,
            },
            sources: ModelBudgetSources {
                catalog: CatalogBudgetFacts {
                    status: CatalogMetadataStatus::Unavailable,
                    revision: None,
                    context_window_tokens: None,
                    max_output_tokens: None,
                    provenance: None,
                },
                operator_configuration: OperatorConfigurationStatus::Unconfigured,
                provider_confirmed: ProviderConfirmedStatus::Unknown,
                operator_configuration_version: None,
            },
        }
    }

    pub fn context_estimate_available(&self) -> bool {
        self.context_window.tokens.is_some()
            && self.selected_output_reservation.tokens.is_some()
            && self.estimator.provider_overhead_tokens.is_some()
            && self.estimator.safety_margin_tokens.is_some()
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != MODEL_BUDGET_PROFILE_VERSION
            || self.estimator.version != MODEL_BUDGET_PROFILE_VERSION
            || self.estimator.method != MODEL_INPUT_ESTIMATE_METHOD
            || self.estimator.uncertainty != MODEL_INPUT_ESTIMATE_UNCERTAINTY
            || self.framing.max_input_json_bytes == 0
            || self.framing.max_instruction_bytes != GATEWAY_INSTRUCTION_BYTES
            || self.framing.max_messages != GATEWAY_MESSAGE_COUNT
            || self.framing.max_tools != GATEWAY_TOOL_COUNT
            || self.framing.max_conversation_bytes != AGENT_CONVERSATION_BYTES
            || self.framing.max_request_bytes != GATEWAY_REQUEST_BYTES
            || self.framing.max_output_bytes != GATEWAY_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        let requested_input = self
            .framing
            .configured_input_json_bytes
            .unwrap_or(LEGACY_AGENT_INPUT_BYTES);
        let expected_input = requested_input
            .min(LEGACY_AGENT_INPUT_BYTES)
            .min(GATEWAY_REQUEST_BYTES);
        if self.framing.max_input_json_bytes != expected_input
            || self
                .framing
                .configured_input_json_bytes
                .is_some_and(|value| value == 0 || value > GATEWAY_REQUEST_BYTES)
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.context_window.validate()?;
        self.max_output.validate()?;
        self.selected_output_reservation.validate()?;
        if self
            .context_window
            .tokens
            .zip(self.max_output.tokens)
            .is_some_and(|(context, output)| output > context)
            || self
                .selected_output_reservation
                .tokens
                .zip(self.max_output.tokens)
                .is_some_and(|(reserve, output)| reserve > output)
            || self
                .selected_output_reservation
                .tokens
                .zip(self.context_window.tokens)
                .is_some_and(|(reserve, context)| reserve > context)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self
            .estimator
            .provider_overhead_tokens
            .is_some_and(|value| value == 0 || value > MAX_KNOWN_MODEL_TOKENS)
            || self
                .estimator
                .safety_margin_tokens
                .is_some_and(|value| value == 0 || value > MAX_KNOWN_MODEL_TOKENS)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let configured =
            self.sources.operator_configuration == OperatorConfigurationStatus::Configured;
        let has_provider_confirmed_limit = [
            &self.context_window,
            &self.max_output,
            &self.selected_output_reservation,
        ]
        .iter()
        .any(|limit| limit.source == TokenLimitSource::ProviderConfirmed);
        if configured != self.sources.operator_configuration_version.is_some()
            || self
                .sources
                .operator_configuration_version
                .is_some_and(|value| value != MODEL_BUDGET_OVERRIDE_VERSION)
            || (self.sources.provider_confirmed == ProviderConfirmedStatus::Available)
                != has_provider_confirmed_limit
            || !configured
                && (self.context_window.source == TokenLimitSource::OperatorConfiguration
                    || self.max_output.source == TokenLimitSource::OperatorConfiguration
                    || self.selected_output_reservation.source
                        == TokenLimitSource::OperatorConfiguration
                    || self.estimator.provider_overhead_tokens.is_some()
                    || self.estimator.safety_margin_tokens.is_some()
                    || self.framing.configured_input_json_bytes.is_some())
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self
            .sources
            .catalog
            .revision
            .is_some_and(|revision| revision == 0)
            || self
                .sources
                .catalog
                .context_window_tokens
                .is_some_and(|value| value == 0 || value > MAX_KNOWN_MODEL_TOKENS)
            || self
                .sources
                .catalog
                .max_output_tokens
                .is_some_and(|value| value == 0 || value > MAX_KNOWN_MODEL_TOKENS)
            || self
                .sources
                .catalog
                .context_window_tokens
                .zip(self.sources.catalog.max_output_tokens)
                .is_some_and(|(context, output)| output > context)
            || self
                .sources
                .catalog
                .provenance
                .as_ref()
                .is_some_and(|value| value.source.is_empty() || value.verified_at.is_empty())
        {
            return Err(AgentFailure::InvalidInput);
        }
        match self.sources.catalog.status {
            CatalogMetadataStatus::Unavailable => {
                if self.sources.catalog.revision.is_some()
                    || self.sources.catalog.context_window_tokens.is_some()
                    || self.sources.catalog.max_output_tokens.is_some()
                    || self.sources.catalog.provenance.is_some()
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            CatalogMetadataStatus::ModelNotListed | CatalogMetadataStatus::MetadataUnknown => {
                if self.sources.catalog.revision.is_none()
                    || self.sources.catalog.status == CatalogMetadataStatus::ModelNotListed
                        && self.sources.catalog.provenance.is_some()
                    || self.sources.catalog.context_window_tokens.is_some()
                    || self.sources.catalog.max_output_tokens.is_some()
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            CatalogMetadataStatus::Available => {
                if self.sources.catalog.revision.is_none()
                    || self.sources.catalog.provenance.is_none()
                    || self.sources.catalog.context_window_tokens.is_none()
                        && self.sources.catalog.max_output_tokens.is_none()
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        Ok(())
    }
}

fn required_nullable_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u32>::deserialize(deserializer)
}

fn required_nullable_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u64>::deserialize(deserializer)
}

fn required_nullable_provenance<'de, D>(
    deserializer: D,
) -> Result<Option<BudgetProvenance>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<BudgetProvenance>::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_confirmed_limits_require_matching_source_status() {
        let mut profile = ModelBudgetProfile::unknown();
        profile.context_window = ModelTokenLimit {
            status: TokenLimitStatus::Known,
            tokens: Some(32_000),
            source: TokenLimitSource::ProviderConfirmed,
        };
        assert_eq!(profile.validate(), Err(AgentFailure::InvalidInput));
        profile.sources.provider_confirmed = ProviderConfirmedStatus::Available;
        assert!(profile.validate().is_ok());
        profile.context_window.source = TokenLimitSource::Unknown;
        profile.context_window.status = TokenLimitStatus::Unknown;
        profile.context_window.tokens = None;
        assert_eq!(profile.validate(), Err(AgentFailure::InvalidInput));
    }
}
