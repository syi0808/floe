use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::APP_WIRE_VERSION;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppWireErrorCodeDto {
    Validation,
    UnsupportedVersion,
    CommandIdConflict,
    SessionBusy,
    NotFound,
    AccessDenied,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppWireErrorDto {
    pub code: AppWireErrorCodeDto,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppResponseDto<T> {
    pub schema_version: u32,
    pub request_id: Uuid,
    #[serde(flatten)]
    pub outcome: AppResponseOutcomeDto<T>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AppResponseOutcomeDto<T> {
    Ok { result: T },
    Error { error: AppWireErrorDto },
}

impl<T> AppResponseDto<T> {
    pub fn ok(request_id: Uuid, result: T) -> Self {
        Self {
            schema_version: APP_WIRE_VERSION,
            request_id,
            outcome: AppResponseOutcomeDto::Ok { result },
        }
    }

    pub fn error(request_id: Uuid, error: AppWireErrorDto) -> Self {
        Self {
            schema_version: APP_WIRE_VERSION,
            request_id,
            outcome: AppResponseOutcomeDto::Error { error },
        }
    }
}
