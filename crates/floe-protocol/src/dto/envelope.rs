use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::PROTOCOL_VERSION;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCodeDto {
    Validation,
    NotFound,
    Conflict,
    Storage,
    Internal,
    UnsupportedVersion,
    NoFocusSlot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorDto {
    pub code: ErrorCodeDto,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResponseEnvelopeDto<T> {
    pub schema_version: u32,
    #[serde(flatten)]
    pub outcome: ResponseOutcomeDto<T>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResponseOutcomeDto<T> {
    Ok { data: T },
    Error { error: ErrorDto },
}

impl<T> ResponseEnvelopeDto<T> {
    pub fn ok(data: T) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            outcome: ResponseOutcomeDto::Ok { data },
        }
    }

    pub fn error(error: ErrorDto) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            outcome: ResponseOutcomeDto::Error { error },
        }
    }
}
