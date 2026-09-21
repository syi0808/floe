use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelPurpose(String);

impl ModelPurpose {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty() && value.len() <= 128).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelConsumer(String);

impl ModelConsumer {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty() && value.len() <= 128).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataRecipient {
    Device,
    External(String),
}

impl DataRecipient {
    pub fn external(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty() && value.len() <= 256).then_some(Self::External(value))
    }

    pub fn is_external(&self) -> bool {
        matches!(self, Self::External(_))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Device => None,
            Self::External(value) => Some(value),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLocation {
    Device,
    Gateway,
    Remote,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelCapabilities(pub Vec<String>);

impl ModelCapabilities {
    pub fn contains(&self, capability: &str) -> bool {
        self.0.iter().any(|value| value == capability)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub purpose: ModelPurpose,
    pub consumer: ModelConsumer,
    pub execution_location: ExecutionLocation,
    pub data_recipient: DataRecipient,
    pub capabilities: ModelCapabilities,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientConstraint {
    DeviceOnly,
    External { recipient: String, consent: bool },
}

/// What execution class a domain caller requires.
///
/// Inference still selects the profile; the caller only constrains the class.
/// `DeviceOnly` admits device execution with a device recipient only;
/// `RemoteOnly` admits off-device execution (gateway or remote).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceExecutionConstraint {
    Any,
    DeviceOnly,
    RemoteOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteRequest {
    pub purpose: ModelPurpose,
    pub requested_capabilities: ModelCapabilities,
    pub consumer: ModelConsumer,
    pub recipient: RecipientConstraint,
    pub preferred_profile_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlannedRoute {
    pub profile_id: String,
    pub purpose: ModelPurpose,
    pub consumer: ModelConsumer,
    pub execution_location: ExecutionLocation,
    pub data_recipient: DataRecipient,
}
