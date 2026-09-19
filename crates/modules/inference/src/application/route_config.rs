//! Route selection judgment: which model endpoint may run a purpose, and who is
//! allowed to receive the input data.
//!
//! Nothing here performs I/O. The provider adapter supplies observed server
//! facts; this module decides whether they may be used.

use floe_agent_contract::AgentFailure;

use crate::{
    DataRecipient, ExecutionLocation, InferenceRouter, ModelCapabilities, ModelConsumer,
    ModelProfile, ModelPurpose, RecipientConstraint, RoutePlanError, RouteRequest,
};

pub const LEGACY_INFERENCE_CONSUMER: &str = "legacy.inference";
pub const MODEL_GENERATION_CAPABILITY: &str = "agent_steps";
pub const EVERYDAY_ASSISTANCE_PURPOSE: &str = "everyday_assistance";

/// A paired local-server connection the person already consented to.
#[derive(Clone, Eq, PartialEq)]
pub struct RemoteModelConnection {
    pub base_url: String,
    pub bearer_token: String,
    pub client_id: String,
    pub person_id: String,
    pub device_id: String,
    pub allow_external: bool,
    pub external_recipients: Vec<String>,
}

/// The route value owned by Inference. Connection identifiers observed for other
/// owners travel separately; a route never carries a source catalog.
#[derive(Clone, Eq, PartialEq)]
pub struct RemoteRoute {
    pub base_url: String,
    pub bearer_token: String,
    pub purpose: String,
    pub external: bool,
    pub allow_external: bool,
    pub recipient: Option<String>,
    pub pairing: Option<RoutePairing>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePairing {
    pub client_id: String,
    pub person_id: String,
    pub device_id: String,
}

impl std::fmt::Debug for RemoteRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteRoute")
            .field("base_url", &self.base_url)
            // The credential is named but never rendered, so a reader can see
            // that something was withheld rather than that nothing was there.
            .field("bearer_token", &"[REDACTED]")
            .field("purpose", &self.purpose)
            .field("external", &self.external)
            .field("allow_external", &self.allow_external)
            .field("recipient", &self.recipient)
            .field("pairing", &self.pairing)
            .finish()
    }
}

/// What the server reported for one inference purpose.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PurposeAvailability {
    pub available: bool,
    pub requires_external_consent: bool,
    pub placement: Option<String>,
    pub recipient: Option<String>,
}

/// Validated placement of a route request: address shape, credential shape and
/// the recipient constraint that follows from the purpose.
pub struct ModelRouteConfig {
    pub base_url: String,
    pub bearer_token: String,
    pub purpose: String,
    pub external: bool,
    pub allow_external: bool,
    pub recipient: Option<String>,
}

impl ModelRouteConfig {
    pub fn from_route(route: &RemoteRoute) -> Result<Self, AgentFailure> {
        if !valid_loopback_gateway(&route.base_url)
            || route.bearer_token.len() < 32
            || route.bearer_token.len() > 256
            || !route
                .bearer_token
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
            || route.purpose != EVERYDAY_ASSISTANCE_PURPOSE
            || (!route.external && route.recipient.is_some())
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            base_url: route.base_url.clone(),
            bearer_token: route.bearer_token.clone(),
            purpose: route.purpose.clone(),
            external: route.external,
            allow_external: route.allow_external,
            recipient: route.recipient.clone(),
        })
    }

    pub fn admit(&self) -> Result<(), AgentFailure> {
        let purpose = ModelPurpose::new(self.purpose.clone()).ok_or(AgentFailure::InvalidInput)?;
        let consumer =
            ModelConsumer::new(LEGACY_INFERENCE_CONSUMER).ok_or(AgentFailure::InvalidInput)?;
        let data_recipient = if self.external {
            DataRecipient::external(
                self.recipient
                    .as_deref()
                    .ok_or(AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?
        } else {
            DataRecipient::Device
        };
        let profile = ModelProfile {
            id: "configured-route".into(),
            purpose: purpose.clone(),
            consumer: consumer.clone(),
            execution_location: if self.external {
                ExecutionLocation::Remote
            } else {
                ExecutionLocation::Gateway
            },
            data_recipient,
            capabilities: ModelCapabilities(vec![MODEL_GENERATION_CAPABILITY.into()]),
            available: true,
        };
        let recipient = if self.external {
            RecipientConstraint::External {
                recipient: self.recipient.clone().ok_or(AgentFailure::InvalidInput)?,
                consent: self.allow_external,
            }
        } else {
            RecipientConstraint::DeviceOnly
        };
        let request = RouteRequest {
            purpose,
            requested_capabilities: ModelCapabilities(vec![MODEL_GENERATION_CAPABILITY.into()]),
            consumer,
            recipient,
            preferred_profile_id: Some("configured-route".into()),
        };
        InferenceRouter::new([profile])
            .map_err(|_| AgentFailure::PolicyDenied)?
            .plan(&request)
            .map(|_| ())
            .map_err(|error| match error {
                RoutePlanError::NotConfigured => AgentFailure::ModelUnavailable,
                RoutePlanError::ConsentRequired => AgentFailure::ConsentRequired,
                RoutePlanError::Unavailable => AgentFailure::ServerModelUnavailable,
                RoutePlanError::Denied => AgentFailure::PolicyDenied,
            })
    }
}

fn valid_loopback_gateway(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    let port = rest.strip_suffix('/').unwrap_or(rest);
    !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|port| port > 0)
}

/// The route candidate before any server fact is observed.
pub fn candidate_route(connection: &RemoteModelConnection) -> Result<RemoteRoute, AgentFailure> {
    let candidate = RemoteRoute {
        base_url: connection.base_url.clone(),
        bearer_token: connection.bearer_token.clone(),
        purpose: EVERYDAY_ASSISTANCE_PURPOSE.into(),
        external: false,
        allow_external: false,
        recipient: None,
        pairing: Some(RoutePairing {
            client_id: connection.client_id.clone(),
            person_id: connection.person_id.clone(),
            device_id: connection.device_id.clone(),
        }),
    };
    ModelRouteConfig::from_route(&candidate)?;
    Ok(candidate)
}

/// Decide the final route from the purpose the server reported. Consent for an
/// external recipient is required here and is never inferred from the server.
pub fn plan_remote_route(
    connection: &RemoteModelConnection,
    candidate: RemoteRoute,
    availability: &PurposeAvailability,
) -> Result<RemoteRoute, AgentFailure> {
    if !availability.available {
        return Err(AgentFailure::ServerModelUnavailable);
    }
    let (external, recipient) = match (
        availability.placement.as_deref(),
        availability.requires_external_consent,
        availability.recipient.as_deref(),
    ) {
        (Some("server_local"), false, None) => (false, None),
        (Some("external"), true, Some(recipient)) if valid_external_recipient(recipient) => {
            (true, Some(recipient.to_owned()))
        }
        _ => return Err(AgentFailure::ServerModelInvalidOutput),
    };
    let allow_external = if let Some(recipient) = recipient.as_deref() {
        if !connection.allow_external
            || !connection
                .external_recipients
                .iter()
                .any(|allowed| allowed == recipient)
        {
            return Err(AgentFailure::ConsentRequired);
        }
        true
    } else {
        false
    };
    let route = RemoteRoute {
        external,
        allow_external,
        recipient,
        ..candidate
    };
    ModelRouteConfig::from_route(&route)?.admit()?;
    Ok(route)
}

pub fn valid_external_recipient(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= 253
        && !value.chars().any(char::is_control)
}
