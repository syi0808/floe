//! The app wire for the bundled host, converted once.
//!
//! Everything the host sends arrives as a DTO and leaves here as the app's own
//! acquisition command; everything it polls for leaves as a DTO again. The
//! bounds checked here are the wire's own — lengths, hex digits, id parsing,
//! which fields a domain may carry. What an answer *means* is decided past this
//! boundary.

use floe_app::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, CalendarAcquisitionMode,
    CalendarAcquisitionRequest, CalendarProvider, CalendarSourceFailure, NativeCalendarBatch,
    NativeCalendarFailure, NativeCalendarRecord, NativeEventSchedule, NativeHostOutcome,
    PersonalAcquisitionMode, PersonalAcquisitionRequest, PersonalDomain,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::{
    AppCommandResultDto, AppQueryResultDto, CalendarFailureDto, CalendarProviderDto,
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionModeDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalDomainDto, NativeCalendarBatchDto, NativeEventScheduleDto,
    NativeHostRegistrationDto, UuidRefDto,
};

pub fn calendar_provider(value: CalendarProviderDto) -> CalendarProvider {
    floe_protocol::conversion::calendar_provider_from_dto(value)
}

pub fn calendar_provider_dto(value: CalendarProvider) -> CalendarProviderDto {
    floe_protocol::conversion::calendar_provider_to_dto(value)
}

pub(crate) fn calendar_source_failure(value: CalendarFailureDto) -> CalendarSourceFailure {
    match value {
        CalendarFailureDto::PermissionDenied => CalendarSourceFailure::PermissionDenied,
        CalendarFailureDto::CalendarUnavailable => CalendarSourceFailure::CalendarUnavailable,
        CalendarFailureDto::ProviderUnavailable => CalendarSourceFailure::ProviderUnavailable,
    }
}

fn native_failure(value: CalendarFailureDto) -> NativeCalendarFailure {
    match value {
        CalendarFailureDto::PermissionDenied => NativeCalendarFailure::PermissionDenied,
        CalendarFailureDto::CalendarUnavailable => NativeCalendarFailure::CalendarUnavailable,
        CalendarFailureDto::ProviderUnavailable => NativeCalendarFailure::ProviderUnavailable,
    }
}

fn native_schedule(value: NativeEventScheduleDto) -> NativeEventSchedule {
    match value {
        NativeEventScheduleDto::Timed {
            starts_at,
            ends_at,
            timezone,
        } => NativeEventSchedule::Timed {
            starts_at,
            ends_at,
            timezone,
        },
        NativeEventScheduleDto::AllDay {
            start_date,
            end_date_exclusive,
        } => NativeEventSchedule::AllDay {
            start_date,
            end_date_exclusive,
        },
    }
}

pub(crate) fn native_batch(value: NativeCalendarBatchDto) -> NativeCalendarBatch {
    NativeCalendarBatch {
        calendar_id: value.calendar_id,
        records: value
            .records
            .into_iter()
            .map(|record| NativeCalendarRecord {
                can_modify: record.can_modify,
                calendar_id: record.calendar_id,
                external_id: record.external_id,
                external_revision: record.external_revision,
                title: record.title,
                schedule: native_schedule(record.schedule),
            })
            .collect(),
        failure: value.failure.map(native_failure),
    }
}

pub(crate) fn acquisition_mode(value: LocalContextAcquisitionModeDto) -> CalendarAcquisitionMode {
    match value {
        LocalContextAcquisitionModeDto::InspectSubject => CalendarAcquisitionMode::InspectSubject,
        LocalContextAcquisitionModeDto::InspectCatalog => CalendarAcquisitionMode::InspectCatalog,
        LocalContextAcquisitionModeDto::RequestPermission => {
            CalendarAcquisitionMode::RequestPermission
        }
        LocalContextAcquisitionModeDto::ReadEvents => CalendarAcquisitionMode::ReadEvents,
    }
}

fn acquisition_mode_dto(value: CalendarAcquisitionMode) -> LocalContextAcquisitionModeDto {
    match value {
        CalendarAcquisitionMode::InspectSubject => LocalContextAcquisitionModeDto::InspectSubject,
        CalendarAcquisitionMode::InspectCatalog => LocalContextAcquisitionModeDto::InspectCatalog,
        CalendarAcquisitionMode::RequestPermission => {
            LocalContextAcquisitionModeDto::RequestPermission
        }
        CalendarAcquisitionMode::ReadEvents => LocalContextAcquisitionModeDto::ReadEvents,
    }
}

pub(crate) fn attention_mode(
    value: LocalContextAttentionAcquisitionModeDto,
) -> AttentionAcquisitionMode {
    match value {
        LocalContextAttentionAcquisitionModeDto::InspectSubject => {
            AttentionAcquisitionMode::InspectSubject
        }
        LocalContextAttentionAcquisitionModeDto::ReadProjection => {
            AttentionAcquisitionMode::ReadProjection
        }
    }
}

fn attention_mode_dto(value: AttentionAcquisitionMode) -> LocalContextAttentionAcquisitionModeDto {
    match value {
        AttentionAcquisitionMode::InspectSubject => {
            LocalContextAttentionAcquisitionModeDto::InspectSubject
        }
        AttentionAcquisitionMode::ReadProjection => {
            LocalContextAttentionAcquisitionModeDto::ReadProjection
        }
    }
}

pub(crate) fn personal_domain(value: LocalContextPersonalDomainDto) -> PersonalDomain {
    match value {
        LocalContextPersonalDomainDto::People => PersonalDomain::People,
        LocalContextPersonalDomainDto::Wellbeing => PersonalDomain::Wellbeing,
    }
}

fn personal_domain_dto(value: PersonalDomain) -> LocalContextPersonalDomainDto {
    match value {
        PersonalDomain::People => LocalContextPersonalDomainDto::People,
        PersonalDomain::Wellbeing => LocalContextPersonalDomainDto::Wellbeing,
    }
}

pub fn acquisition_request_dto(
    request: CalendarAcquisitionRequest,
) -> LocalContextAcquisitionRequestDto {
    LocalContextAcquisitionRequestDto {
        request_id: request.request_id.to_string(),
        host_epoch: request.host_epoch,
        person_id: request.person_id.to_string(),
        device_id: request.device_id,
        connection_id: request.connection_id,
        connection_revision: request.connection_revision,
        provider: calendar_provider_dto(request.provider),
        mode: acquisition_mode_dto(request.mode),
        calendar_ids: request.calendar_ids,
        range_start_unix_ms: request.range_start_unix_ms,
        range_end_unix_ms: request.range_end_unix_ms,
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint,
    }
}

pub fn attention_request_dto(
    request: AttentionAcquisitionRequest,
) -> LocalContextAttentionAcquisitionRequestDto {
    LocalContextAttentionAcquisitionRequestDto {
        request_id: request.request_id.to_string(),
        host_epoch: request.host_epoch,
        person_id: request.person_id.to_string(),
        device_id: request.device_id,
        mode: attention_mode_dto(request.mode),
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint,
    }
}

pub fn personal_request_dto(
    request: PersonalAcquisitionRequest,
) -> LocalContextPersonalAcquisitionRequestDto {
    LocalContextPersonalAcquisitionRequestDto {
        request_id: request.request_id.to_string(),
        host_epoch: request.host_epoch,
        person_id: request.person_id.to_string(),
        device_id: request.device_id,
        domain: personal_domain_dto(request.domain),
        mode: personal_mode_dto(request.mode),
        selected_handles: request.selected_handles,
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint,
    }
}

pub(crate) fn personal_mode(
    value: LocalContextPersonalAcquisitionModeDto,
) -> PersonalAcquisitionMode {
    match value {
        LocalContextPersonalAcquisitionModeDto::ReadProjection => {
            PersonalAcquisitionMode::ReadProjection
        }
        LocalContextPersonalAcquisitionModeDto::InspectSubject => {
            PersonalAcquisitionMode::InspectSubject
        }
        LocalContextPersonalAcquisitionModeDto::InspectCatalog => {
            PersonalAcquisitionMode::InspectCatalog
        }
        LocalContextPersonalAcquisitionModeDto::RequestPermission => {
            PersonalAcquisitionMode::RequestPermission
        }
    }
}
fn personal_mode_dto(value: PersonalAcquisitionMode) -> LocalContextPersonalAcquisitionModeDto {
    match value {
        PersonalAcquisitionMode::ReadProjection => {
            LocalContextPersonalAcquisitionModeDto::ReadProjection
        }
        PersonalAcquisitionMode::InspectSubject => {
            LocalContextPersonalAcquisitionModeDto::InspectSubject
        }
        PersonalAcquisitionMode::InspectCatalog => {
            LocalContextPersonalAcquisitionModeDto::InspectCatalog
        }
        PersonalAcquisitionMode::RequestPermission => {
            LocalContextPersonalAcquisitionModeDto::RequestPermission
        }
    }
}
pub fn native_host_command_result(outcome: NativeHostOutcome) -> WireResult<AppCommandResultDto> {
    match outcome {
        NativeHostOutcome::Registered(value) => Ok(AppCommandResultDto::NativeHostRegistered {
            registration: NativeHostRegistrationDto {
                registration_id: UuidRefDto::new(value.registration_id)
                    .ok_or_else(|| invalid("registration_id", "nil host registration"))?,
                host_epoch: value.host_epoch,
                runtime_epoch: value.runtime_epoch,
            },
        }),
        NativeHostOutcome::Acknowledged => Ok(AppCommandResultDto::NativeHostAcknowledged {}),
        _ => Err(invalid("outcome", "unexpected native command outcome")),
    }
}
pub fn native_host_query_result(outcome: NativeHostOutcome) -> WireResult<AppQueryResultDto> {
    match outcome {
        NativeHostOutcome::CalendarAcquisitions(values) => {
            Ok(AppQueryResultDto::NativeHostCalendarAcquisitions {
                acquisitions: values.into_iter().map(acquisition_request_dto).collect(),
            })
        }
        NativeHostOutcome::AttentionAcquisitions(values) => {
            Ok(AppQueryResultDto::NativeHostAttentionAcquisitions {
                acquisitions: values.into_iter().map(attention_request_dto).collect(),
            })
        }
        NativeHostOutcome::PersonalAcquisitions(values) => {
            Ok(AppQueryResultDto::NativeHostPersonalAcquisitions {
                acquisitions: values.into_iter().map(personal_request_dto).collect(),
            })
        }
        _ => Err(invalid("outcome", "unexpected native query outcome")),
    }
}
