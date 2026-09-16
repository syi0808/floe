use std::collections::BTreeMap;

use floe_day::DomainError;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreErrorCode {
    Validation,
    NotFound,
    Conflict,
    Storage,
    NoFocusSlot,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct StoreError {
    pub code: StoreErrorCode,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
}

impl StoreError {
    pub fn new(code: StoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

impl From<DomainError> for StoreError {
    fn from(error: DomainError) -> Self {
        Self::new(StoreErrorCode::Validation, error.to_string())
    }
}
