use super::{
    CalendarBatchDto, CalendarFailureDto, CalendarProviderDto, LocalContextAcquisitionModeDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextPersonalDomainDto,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarCompletionDto {
    pub request_id: String,
    pub host_epoch: String,
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
pub struct AttentionCompletionDto {
    pub request_id: String,
    pub host_epoch: String,
    pub mode: LocalContextAttentionAcquisitionModeDto,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalCompletionDto {
    pub request_id: String,
    pub host_epoch: String,
    pub domain: LocalContextPersonalDomainDto,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextCommandDto {
    RegisterAcquisitionHost {
        host_epoch: String,
    },
    DisposeAcquisitionHost {
        host_epoch: String,
    },
    CompleteAcquisition {
        host_epoch: String,
        result: CalendarCompletionDto,
    },
    FailAcquisition {
        host_epoch: String,
        request_id: String,
        failure: CalendarFailureDto,
    },
    RegisterAttentionHost {
        host_epoch: String,
    },
    DisposeAttentionHost {
        host_epoch: String,
    },
    CompleteAttentionAcquisition {
        host_epoch: String,
        result: AttentionCompletionDto,
    },
    FailAttentionAcquisition {
        host_epoch: String,
        request_id: String,
        failure: String,
    },
    RegisterPersonalHost {
        host_epoch: String,
    },
    DisposePersonalHost {
        host_epoch: String,
    },
    CompletePersonalAcquisition {
        host_epoch: String,
        result: PersonalCompletionDto,
    },
    FailPersonalAcquisition {
        host_epoch: String,
        request_id: String,
        failure: String,
    },
    Publish {
        view: serde_json::Value,
    },
    PublishCalendarObservation {
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
    Revoke {
        view_id: Option<String>,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextQueryDto {
    PollAcquisitions { host_epoch: String },
    PollAttentionAcquisitions { host_epoch: String },
    PollPersonalAcquisitions { host_epoch: String },
    Read { view_id: String },
}
