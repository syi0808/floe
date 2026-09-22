//! The app wire for the bundled host, converted once.
//!
//! Everything the host sends arrives as a DTO and leaves here as the app's own
//! acquisition command; everything it polls for leaves as a DTO again. The
//! bounds checked here are the wire's own — lengths, hex digits, id parsing,
//! which fields a domain may carry. What an answer *means* is decided past this
//! boundary.

use floe_app::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, CalendarAcquisitionMode,
    CalendarAcquisitionRequest, CalendarProvider, CalendarSourceFailure, LocalContextOutcome,
    NativeCalendarBatch, NativeCalendarFailure, NativeCalendarRecord, NativeEventSchedule,
    PersonId, PersonalAcquisitionRequest, PersonalDomain, valid_native_subject_fingerprint,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::{
    CalendarBatchDto, CalendarFailureDto, CalendarProviderDto, EventScheduleDto,
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionRequestDto, LocalContextPersonalDomainDto,
    LocalContextResultDto,
};
use uuid::Uuid;

const ALLOWED_VIEW_IDS: [&str; 5] = [
    "people.identity",
    "schedule.feasibility",
    "attention.coarse",
    "wellbeing.derived",
    "calendar.timeline",
];

pub(crate) fn now_unix_ms() -> WireResult<i64> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| invalid("now", "system clock is before the epoch"))?
            .as_millis(),
    )
    .map_err(|_| invalid("now", "system clock is out of range"))
}

pub(crate) fn validate_handle(value: &str, field: &'static str) -> WireResult<()> {
    if !value.trim().is_empty() && value.len() <= 128 {
        Ok(())
    } else {
        Err(invalid(field, "must contain 1 to 128 characters"))
    }
}

pub(crate) fn validate_host_epoch(value: &str) -> WireResult<()> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("operation.host_epoch", "invalid host epoch"));
    }
    Ok(())
}

pub(crate) fn validate_view_id(view_id: &str) -> WireResult<()> {
    if ALLOWED_VIEW_IDS.contains(&view_id) {
        Ok(())
    } else {
        Err(invalid(
            "operation.view_id",
            "unsupported local context View",
        ))
    }
}

pub(crate) fn validate_fingerprint(value: &str, field: &'static str) -> WireResult<()> {
    if valid_native_subject_fingerprint(value) {
        Ok(())
    } else {
        Err(invalid(field, "invalid fingerprint"))
    }
}

pub(crate) fn parse_request_id(value: &str, field: &'static str) -> WireResult<Uuid> {
    validate_handle(value, field)?;
    Uuid::parse_str(value).map_err(|_| invalid(field, "must be a UUID"))
}

pub fn calendar_provider(value: CalendarProviderDto) -> CalendarProvider {
    super::day::calendar_provider_from_dto(value)
}

pub fn calendar_provider_dto(value: CalendarProvider) -> CalendarProviderDto {
    super::day::calendar_provider_to_dto(value)
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

fn native_schedule(value: EventScheduleDto) -> NativeEventSchedule {
    match value {
        EventScheduleDto::Timed {
            starts_at,
            ends_at,
            timezone,
        } => NativeEventSchedule::Timed {
            starts_at,
            ends_at,
            timezone,
        },
        EventScheduleDto::AllDay {
            start_date,
            end_date_exclusive,
        } => NativeEventSchedule::AllDay {
            start_date,
            end_date_exclusive,
        },
    }
}

pub(crate) fn native_batch(value: CalendarBatchDto) -> NativeCalendarBatch {
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
        LocalContextAcquisitionModeDto::ReadEvents => CalendarAcquisitionMode::ReadEvents,
    }
}

fn acquisition_mode_dto(value: CalendarAcquisitionMode) -> LocalContextAcquisitionModeDto {
    match value {
        CalendarAcquisitionMode::InspectSubject => LocalContextAcquisitionModeDto::InspectSubject,
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
        LocalContextPersonalDomainDto::Feasibility => PersonalDomain::Feasibility,
    }
}

fn personal_domain_dto(value: PersonalDomain) -> LocalContextPersonalDomainDto {
    match value {
        PersonalDomain::People => LocalContextPersonalDomainDto::People,
        PersonalDomain::Wellbeing => LocalContextPersonalDomainDto::Wellbeing,
        PersonalDomain::Feasibility => LocalContextPersonalDomainDto::Feasibility,
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
        selected_handles: request.selected_handles,
        event_handle: request.event_handle,
        evidence_handles: request.evidence_handles,
        destination_latitude: request.destination_latitude,
        destination_longitude: request.destination_longitude,
        event_start_unix_ms: request.event_start_unix_ms,
        event_end_unix_ms: request.event_end_unix_ms,
        travel_mode: request.travel_mode,
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint,
    }
}

/// Report one command's outcome back on the wire.
pub fn local_context_result(
    person_id: PersonId,
    outcome: LocalContextOutcome,
) -> LocalContextResultDto {
    LocalContextResultDto {
        person_id: person_id.to_string(),
        device_id: outcome.device_id,
        view_id: outcome.view_id,
        removed_count: outcome.removed_count,
        view: outcome.view,
        acquisitions: outcome
            .acquisitions
            .into_iter()
            .map(acquisition_request_dto)
            .collect(),
        attention_acquisitions: outcome
            .attention_acquisitions
            .into_iter()
            .map(attention_request_dto)
            .collect(),
        personal_acquisitions: outcome
            .personal_acquisitions
            .into_iter()
            .map(personal_request_dto)
            .collect(),
    }
}
