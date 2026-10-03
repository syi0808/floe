use serde::{Deserialize, Serialize};

use super::{
    CalendarBatchDto, CalendarFailureDto, CalendarProviderDto, LocalContextAcquisitionModeDto,
    LocalContextAcquisitionRequestDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalDomainDto, LocalContextPersonalAcquisitionModeDto, NativeSourceResourceDto, UuidRefDto,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHostRegistrationDto {
    pub registration_id: UuidRefDto,
    pub host_epoch: String,
    pub runtime_epoch: u64,
}

impl NativeHostRegistrationDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_text(&self.host_epoch, 256)
            || self.runtime_epoch == 0
            || self.runtime_epoch > i64::MAX as u64
        {
            return Err("registration");
        }
        Ok(())
    }
}

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
    pub available_calendars: Vec<NativeSourceResourceDto>,
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
    pub mode: LocalContextPersonalAcquisitionModeDto,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
    #[serde(default)]
    pub transform_operation_id: Option<UuidRefDto>,
    pub resources: Vec<NativeSourceResourceDto>,
    pub catalog_complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum NativeHostCommandDto {
    #[serde(rename = "native_host.calendar.register")]
    CalendarRegister {},
    #[serde(rename = "native_host.calendar.complete")]
    CalendarComplete {
        registration: NativeHostRegistrationDto,
        result: CalendarCompletionDto,
    },
    #[serde(rename = "native_host.calendar.fail")]
    CalendarFail {
        registration: NativeHostRegistrationDto,
        request_id: String,
        failure: CalendarFailureDto,
    },
    #[serde(rename = "native_host.calendar.dispose")]
    CalendarDispose {
        registration: NativeHostRegistrationDto,
    },
    #[serde(rename = "native_host.attention.register")]
    AttentionRegister {},
    #[serde(rename = "native_host.attention.complete")]
    AttentionComplete {
        registration: NativeHostRegistrationDto,
        result: AttentionCompletionDto,
    },
    #[serde(rename = "native_host.attention.fail")]
    AttentionFail {
        registration: NativeHostRegistrationDto,
        request_id: String,
        failure: String,
    },
    #[serde(rename = "native_host.attention.dispose")]
    AttentionDispose {
        registration: NativeHostRegistrationDto,
    },
    #[serde(rename = "native_host.personal.register")]
    PersonalRegister {},
    #[serde(rename = "native_host.personal.complete")]
    PersonalComplete {
        registration: NativeHostRegistrationDto,
        result: PersonalCompletionDto,
    },
    #[serde(rename = "native_host.personal.fail")]
    PersonalFail {
        registration: NativeHostRegistrationDto,
        request_id: String,
        failure: String,
    },
    #[serde(rename = "native_host.personal.dispose")]
    PersonalDispose {
        registration: NativeHostRegistrationDto,
    },
}

impl NativeHostCommandDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::CalendarRegister {}
            | Self::AttentionRegister {}
            | Self::PersonalRegister {} => Ok(()),
            Self::CalendarComplete {
                registration,
                result,
            } => {
                validate_registration_result(registration, &result.host_epoch)?;
                validate_request_id(&result.request_id)
            }
            Self::CalendarFail {
                registration,
                request_id,
                ..
            }
            | Self::AttentionFail {
                registration,
                request_id,
                ..
            }
            | Self::PersonalFail {
                registration,
                request_id,
                ..
            } => {
                registration.validate()?;
                validate_request_id(request_id)
            }
            Self::AttentionComplete {
                registration,
                result,
            } => {
                validate_registration_result(registration, &result.host_epoch)?;
                validate_request_id(&result.request_id)
            }
            Self::PersonalComplete {
                registration,
                result,
            } => {
                validate_registration_result(registration, &result.host_epoch)?;
                validate_request_id(&result.request_id)?;
                match (result.mode, result.domain, result.transform_operation_id) {
                    (LocalContextPersonalAcquisitionModeDto::ReadProjection, LocalContextPersonalDomainDto::Wellbeing, Some(_)) => Ok(()),
                    (LocalContextPersonalAcquisitionModeDto::ReadProjection, LocalContextPersonalDomainDto::People, None) => Ok(()),
                    (LocalContextPersonalAcquisitionModeDto::InspectSubject | LocalContextPersonalAcquisitionModeDto::InspectCatalog | LocalContextPersonalAcquisitionModeDto::RequestPermission, _, None) => Ok(()),
                    _ => Err("completion.transform_operation_id"),
                }
            }
            Self::CalendarDispose { registration }
            | Self::AttentionDispose { registration }
            | Self::PersonalDispose { registration } => registration.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum NativeHostQueryDto {
    #[serde(rename = "native_host.calendar.poll")]
    CalendarPoll {
        registration: NativeHostRegistrationDto,
    },
    #[serde(rename = "native_host.attention.poll")]
    AttentionPoll {
        registration: NativeHostRegistrationDto,
    },
    #[serde(rename = "native_host.personal.poll")]
    PersonalPoll {
        registration: NativeHostRegistrationDto,
    },
}

impl NativeHostQueryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        let registration = match self {
            Self::CalendarPoll { registration }
            | Self::AttentionPoll { registration }
            | Self::PersonalPoll { registration } => registration,
        };
        registration.validate()
    }
}

fn validate_registration_result(
    registration: &NativeHostRegistrationDto,
    result_host_epoch: &str,
) -> Result<(), &'static str> {
    registration.validate()?;
    if result_host_epoch != registration.host_epoch {
        return Err("completion.host_epoch");
    }
    Ok(())
}

fn validate_request_id(value: &str) -> Result<(), &'static str> {
    let parsed = uuid::Uuid::parse_str(value).map_err(|_| "request_id")?;
    if parsed.is_nil() || parsed.to_string() != value {
        Err("request_id")
    } else {
        Ok(())
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
