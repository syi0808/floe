use serde::{Deserialize, Serialize};

use super::calendar::CalendarProviderDto;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextAcquisitionRequestDto {
    pub request_id: String,
    pub host_epoch: String,
    pub person_id: String,
    pub device_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub provider: CalendarProviderDto,
    pub mode: LocalContextAcquisitionModeDto,
    pub calendar_ids: Vec<String>,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub deadline_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalContextAcquisitionModeDto {
    InspectSubject,
    ReadEvents,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextAttentionAcquisitionRequestDto {
    pub request_id: String,
    pub host_epoch: String,
    pub person_id: String,
    pub device_id: String,
    pub mode: LocalContextAttentionAcquisitionModeDto,
    pub deadline_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalContextAttentionAcquisitionModeDto {
    InspectSubject,
    ReadProjection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalContextPersonalDomainDto {
    People,
    Wellbeing,
    Feasibility,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextPersonalAcquisitionRequestDto {
    pub request_id: String,
    pub host_epoch: String,
    pub person_id: String,
    pub device_id: String,
    pub domain: LocalContextPersonalDomainDto,
    pub selected_handles: Vec<String>,
    pub event_handle: Option<String>,
    pub evidence_handles: Vec<String>,
    pub destination_latitude: Option<f64>,
    pub destination_longitude: Option<f64>,
    pub event_start_unix_ms: Option<i64>,
    pub event_end_unix_ms: Option<i64>,
    pub travel_mode: Option<String>,
    pub deadline_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_native_subject_fingerprint: Option<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acquisitions: Vec<LocalContextAcquisitionRequestDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attention_acquisitions: Vec<LocalContextAttentionAcquisitionRequestDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub personal_acquisitions: Vec<LocalContextPersonalAcquisitionRequestDto>,
}
