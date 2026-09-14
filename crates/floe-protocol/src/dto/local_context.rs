use serde::{Deserialize, Serialize};

use super::calendar::{CalendarFailureDto, CalendarProviderDto};
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
    RegisterAcquisitionHost {
        host_epoch: String,
    },
    PollAcquisitions {
        host_epoch: String,
    },
    CompleteAcquisition {
        host_epoch: String,
        result: LocalContextAcquisitionResultDto,
    },
    FailAcquisition {
        host_epoch: String,
        request_id: String,
        failure: CalendarFailureDto,
    },
    DisposeAcquisitionHost {
        host_epoch: String,
    },
    RegisterAttentionHost {
        host_epoch: String,
    },
    PollAttentionAcquisitions {
        host_epoch: String,
    },
    CompleteAttentionAcquisition {
        host_epoch: String,
        result: LocalContextAttentionAcquisitionResultDto,
    },
    FailAttentionAcquisition {
        host_epoch: String,
        request_id: String,
        failure: String,
    },
    DisposeAttentionHost {
        host_epoch: String,
    },
    RegisterPersonalHost {
        host_epoch: String,
    },
    PollPersonalAcquisitions {
        host_epoch: String,
    },
    CompletePersonalAcquisition {
        host_epoch: String,
        result: LocalContextPersonalAcquisitionResultDto,
    },
    FailPersonalAcquisition {
        host_epoch: String,
        request_id: String,
        failure: String,
    },
    DisposePersonalHost {
        host_epoch: String,
    },
    Publish {
        device_id: String,
        view: serde_json::Value,
    },
    PublishCalendarObservation {
        device_id: String,
        connection_id: String,
        connection_revision: u64,
        provider: CalendarProviderDto,
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
pub struct LocalContextAcquisitionResultDto {
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
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub available_calendar_ids: Vec<String>,
    pub permission_class: String,
    pub batches: Vec<CalendarBatchDto>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalContextAttentionAcquisitionResultDto {
    pub request_id: String,
    pub host_epoch: String,
    pub person_id: String,
    pub device_id: String,
    pub mode: LocalContextAttentionAcquisitionModeDto,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<serde_json::Value>,
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
pub struct LocalContextPersonalAcquisitionResultDto {
    pub request_id: String,
    pub host_epoch: String,
    pub person_id: String,
    pub device_id: String,
    pub domain: LocalContextPersonalDomainDto,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
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
