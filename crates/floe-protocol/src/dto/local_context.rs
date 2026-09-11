use floe_domain::CalendarProvider;
use serde::{Deserialize, Serialize};

use crate::CalendarBatchDto;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub operation: LocalContextOperationDto,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalContextOperationDto {
    Publish {
        device_id: String,
        view: serde_json::Value,
    },
    PublishCalendarObservation {
        device_id: String,
        connection_revision: u64,
        provider: CalendarProvider,
        calendar_ids: Vec<String>,
        observed_at_unix_ms: i64,
        expires_at_unix_ms: i64,
        range_start_unix_ms: i64,
        range_end_unix_ms: i64,
        batches: Vec<CalendarBatchDto>,
    },
    Read {
        view_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device_id: Option<String>,
    },
    Revoke {
        device_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view_id: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextResultDto {
    pub person_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_id: Option<String>,
    pub removed_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<serde_json::Value>,
}
