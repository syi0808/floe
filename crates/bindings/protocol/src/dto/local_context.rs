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
    InspectCatalog,
    RequestPermission,
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
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextPersonalAcquisitionRequestDto {
    pub request_id: String,
    pub host_epoch: String,
    pub person_id: String,
    pub device_id: String,
    pub domain: LocalContextPersonalDomainDto,
    pub mode: LocalContextPersonalAcquisitionModeDto,
    pub selected_handles: Vec<String>,
    pub deadline_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalContextPersonalAcquisitionModeDto {
    ReadProjection,
    InspectSubject,
    InspectCatalog,
    RequestPermission,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSourceResourceDto {
    pub handle: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_label: Option<String>,
}
