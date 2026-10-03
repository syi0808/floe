use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelPurpose(String);

impl ModelPurpose {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        matches!(value.as_str(), "quick_response" | "everyday_assistance" | "deep_work")
            .then_some(Self(value))
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
        (!value.is_empty()
            && value.len() <= 128
            && value.trim() == value
            && !value.chars().any(char::is_control))
            .then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
