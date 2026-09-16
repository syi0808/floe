//! The failure one composed Day operation reports to its caller.
//!
//! Modules report their own typed failures; this is the shape the app host
//! hands back, with whatever the module attached to it preserved.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Validation,
    NotFound,
    Conflict,
    Storage,
    NoFocusSlot,
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
