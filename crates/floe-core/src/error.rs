use std::collections::BTreeMap;

use floe_domain::DomainError;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Validation,
    NotFound,
    Conflict,
    Storage,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CoreError {
    pub code: ErrorCode,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
}

impl CoreError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
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

impl From<DomainError> for CoreError {
    fn from(error: DomainError) -> Self {
        Self::new(ErrorCode::Validation, error.to_string())
    }
}
