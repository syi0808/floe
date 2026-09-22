//! The app wire for the bundled host, converted once.
//!
//! Everything the host sends arrives as a DTO and leaves here as the app's own
//! acquisition command; everything it polls for leaves as a DTO again. The
//! bounds checked here are the wire's own — lengths, hex digits, id parsing,
//! which fields a domain may carry. What an answer *means* is decided past this
//! boundary.

use floe_app::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    CalendarObservationPublication, CalendarProvider, CalendarSourceFailure, LocalContextCommand,
    LocalContextOutcome, MAX_ACQUISITION_DEADLINE_MS, NativeCalendarBatch, NativeCalendarFailure,
    NativeCalendarRecord, NativeEventSchedule, PersonId, PersonalAcquisitionRequest,
    PersonalAcquisitionResult, PersonalDomain, attention_failure, personal_failure,
    valid_native_subject_fingerprint,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::{
    CalendarBatchDto, CalendarFailureDto, CalendarProviderDto, CalendarRecordDto, EventScheduleDto,
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAcquisitionResultDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextAttentionAcquisitionResultDto,
    LocalContextOperationDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalAcquisitionResultDto, LocalContextPersonalDomainDto, LocalContextResultDto,
};
use serde_json::Value;
use uuid::Uuid;

const MAX_ACQUISITION_CALENDARS: usize = 4;
const MAX_ACQUISITION_ITEMS: usize = 128;
const MAX_ACQUISITION_BYTES: usize = 65_536;

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

fn parse_person(value: &str, field: &'static str) -> WireResult<PersonId> {
    value
        .parse::<Uuid>()
        .map(PersonId)
        .map_err(|_| invalid(field, "must be a UUID"))
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

fn native_failure_dto(value: NativeCalendarFailure) -> CalendarFailureDto {
    match value {
        NativeCalendarFailure::PermissionDenied => CalendarFailureDto::PermissionDenied,
        NativeCalendarFailure::CalendarUnavailable => CalendarFailureDto::CalendarUnavailable,
        NativeCalendarFailure::ProviderUnavailable => CalendarFailureDto::ProviderUnavailable,
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

fn native_schedule_dto(value: NativeEventSchedule) -> EventScheduleDto {
    match value {
        NativeEventSchedule::Timed {
            starts_at,
            ends_at,
            timezone,
        } => EventScheduleDto::Timed {
            starts_at,
            ends_at,
            timezone,
        },
        NativeEventSchedule::AllDay {
            start_date,
            end_date_exclusive,
        } => EventScheduleDto::AllDay {
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

fn native_batch_dto(value: NativeCalendarBatch) -> CalendarBatchDto {
    CalendarBatchDto {
        calendar_id: value.calendar_id,
        records: value
            .records
            .into_iter()
            .map(|record| CalendarRecordDto {
                can_modify: record.can_modify,
                calendar_id: record.calendar_id,
                external_id: record.external_id,
                external_revision: record.external_revision,
                title: record.title,
                schedule: native_schedule_dto(record.schedule),
            })
            .collect(),
        failure: value.failure.map(native_failure_dto),
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

pub fn validate_acquisition_request(request: &LocalContextAcquisitionRequestDto) -> WireResult<()> {
    validate_host_epoch(&request.host_epoch)?;
    validate_handle(&request.request_id, "operation.request_id")?;
    validate_handle(&request.person_id, "operation.person_id")?;
    validate_handle(&request.device_id, "operation.device_id")?;
    validate_handle(&request.connection_id, "operation.connection_id")?;
    let now = now_unix_ms()?;
    let fingerprint_valid = request
        .expected_native_subject_fingerprint
        .as_deref()
        .is_some_and(valid_native_subject_fingerprint);
    if request.connection_revision == 0
        || !matches!(
            request.provider,
            CalendarProviderDto::EventKit | CalendarProviderDto::Android
        )
        || request.calendar_ids.is_empty()
        || request.calendar_ids.len() > MAX_ACQUISITION_CALENDARS
        || request
            .calendar_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 512)
        || request
            .calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || request.range_start_unix_ms < 0
        || request.range_end_unix_ms <= request.range_start_unix_ms
        || request.range_end_unix_ms - request.range_start_unix_ms > 32 * 86_400_000
        || request.deadline_unix_ms <= request.range_start_unix_ms
        || request.deadline_unix_ms <= now
        || request.deadline_unix_ms > now.saturating_add(MAX_ACQUISITION_DEADLINE_MS)
        || (request.mode == LocalContextAcquisitionModeDto::ReadEvents && !fingerprint_valid)
        || (request.mode == LocalContextAcquisitionModeDto::InspectSubject
            && request.expected_native_subject_fingerprint.is_some())
    {
        return Err(invalid(
            "operation",
            "acquisition request is outside bounds",
        ));
    }
    Ok(())
}

pub fn validate_acquisition_result(result: &LocalContextAcquisitionResultDto) -> WireResult<()> {
    if !valid_native_subject_fingerprint(&result.native_subject_fingerprint_before)
        || !valid_native_subject_fingerprint(&result.native_subject_fingerprint_after)
        || result.native_subject_fingerprint_before != result.native_subject_fingerprint_after
        || result.permission_class.trim().is_empty()
        || result.permission_class.len() > 64
        || result.permission_class.chars().any(char::is_control)
        || result.available_calendar_ids.is_empty()
        || result.available_calendar_ids.len() > 128
        || result
            .available_calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || result
            .available_calendar_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 512)
    {
        return Err(invalid(
            "operation.result",
            "native subject evidence is outside bounds",
        ));
    }
    let request = LocalContextAcquisitionRequestDto {
        request_id: result.request_id.clone(),
        host_epoch: result.host_epoch.clone(),
        person_id: result.person_id.clone(),
        device_id: result.device_id.clone(),
        connection_id: result.connection_id.clone(),
        connection_revision: result.connection_revision,
        provider: result.provider,
        mode: result.mode,
        calendar_ids: result.calendar_ids.clone(),
        range_start_unix_ms: result.range_start_unix_ms,
        range_end_unix_ms: result.range_end_unix_ms,
        deadline_unix_ms: now_unix_ms()?.saturating_add(MAX_ACQUISITION_DEADLINE_MS - 1_000),
        expected_native_subject_fingerprint: (result.mode
            == LocalContextAcquisitionModeDto::ReadEvents)
            .then(|| result.native_subject_fingerprint_before.clone()),
    };
    validate_acquisition_request(&request)?;
    if (result.mode == LocalContextAcquisitionModeDto::InspectSubject && !result.batches.is_empty())
        || (result.mode == LocalContextAcquisitionModeDto::ReadEvents
            && result.batches.len() != result.calendar_ids.len())
        || result
            .calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || result.batches.iter().enumerate().any(|(index, batch)| {
            batch.calendar_id != result.calendar_ids[index]
                || (batch.failure.is_some() && !batch.records.is_empty())
                || batch.records.iter().any(|record| {
                    record.calendar_id != batch.calendar_id
                        || record.external_id.trim().is_empty()
                        || record.external_id.len() > 512
                        || record.external_revision.trim().is_empty()
                        || record.external_revision.len() > 512
                        || record.title.len() > 4096
                })
        })
        || result
            .batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>()
            > MAX_ACQUISITION_ITEMS
        || serde_json::to_vec(result)
            .map_err(|_| invalid("operation.result", "invalid acquisition result"))?
            .len()
            > MAX_ACQUISITION_BYTES
    {
        return Err(invalid(
            "operation.result",
            "acquisition result is outside bounds",
        ));
    }
    Ok(())
}

pub fn validate_attention_acquisition_request(
    request: &LocalContextAttentionAcquisitionRequestDto,
) -> WireResult<()> {
    validate_host_epoch(&request.host_epoch)?;
    validate_handle(&request.request_id, "operation.request_id")?;
    validate_handle(&request.device_id, "operation.device_id")?;
    if request.person_id.parse::<Uuid>().is_err()
        || request.deadline_unix_ms <= 0
        || request.mode == LocalContextAttentionAcquisitionModeDto::ReadProjection
            && !request
                .expected_native_subject_fingerprint
                .as_deref()
                .is_some_and(valid_native_subject_fingerprint)
    {
        return Err(invalid(
            "operation",
            "invalid attention acquisition request",
        ));
    }
    Ok(())
}

pub fn validate_attention_acquisition_result(
    result: &LocalContextAttentionAcquisitionResultDto,
) -> WireResult<()> {
    validate_host_epoch(&result.host_epoch)?;
    validate_handle(&result.request_id, "operation.result.request_id")?;
    validate_handle(&result.device_id, "operation.result.device_id")?;
    validate_fingerprint(
        &result.native_subject_fingerprint_before,
        "operation.result.native_subject_fingerprint_before",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_after,
        "operation.result.native_subject_fingerprint_after",
    )?;
    if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after
        || result.person_id.parse::<Uuid>().is_err()
        || result.permission_class.is_empty()
        || result.permission_class.len() > 64
    {
        return Err(invalid("operation.result", "invalid attention evidence"));
    }
    Ok(())
}

pub fn validate_personal_acquisition_request(
    request: &LocalContextPersonalAcquisitionRequestDto,
) -> WireResult<()> {
    validate_host_epoch(&request.host_epoch)?;
    validate_handle(&request.request_id, "operation.request_id")?;
    validate_handle(&request.device_id, "operation.device_id")?;
    if request.person_id.parse::<Uuid>().is_err()
        || request.deadline_unix_ms <= 0
        || request
            .expected_native_subject_fingerprint
            .as_deref()
            .is_none_or(|value| !valid_native_subject_fingerprint(value))
    {
        return Err(invalid("operation", "invalid personal acquisition request"));
    }
    match request.domain {
        LocalContextPersonalDomainDto::People => {
            if request.selected_handles.is_empty()
                || request.selected_handles.len() > 64
                || request
                    .selected_handles
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
                || request
                    .selected_handles
                    .iter()
                    .any(|value| validate_handle(value, "operation.selected_handles").is_err())
                || request.event_handle.is_some()
                || !request.evidence_handles.is_empty()
                || request.destination_latitude.is_some()
                || request.destination_longitude.is_some()
                || request.event_start_unix_ms.is_some()
                || request.event_end_unix_ms.is_some()
                || request.travel_mode.is_some()
            {
                return Err(invalid("operation", "invalid People acquisition request"));
            }
        }
        LocalContextPersonalDomainDto::Wellbeing => {
            if !request.selected_handles.is_empty()
                || request.event_handle.is_some()
                || !request.evidence_handles.is_empty()
                || request.destination_latitude.is_some()
                || request.destination_longitude.is_some()
                || request.event_start_unix_ms.is_some()
                || request.event_end_unix_ms.is_some()
                || request.travel_mode.is_some()
            {
                return Err(invalid(
                    "operation",
                    "invalid Wellbeing acquisition request",
                ));
            }
        }
        LocalContextPersonalDomainDto::Feasibility => {
            if !request.selected_handles.is_empty()
                || request
                    .event_handle
                    .as_deref()
                    .is_none_or(|value| validate_handle(value, "operation.event_handle").is_err())
                || request.evidence_handles.is_empty()
                || request.evidence_handles.len() > 8
                || request
                    .evidence_handles
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
                || request
                    .evidence_handles
                    .iter()
                    .any(|value| validate_handle(value, "operation.evidence_handles").is_err())
                || request.destination_latitude.is_none()
                || request.destination_longitude.is_none()
                || request.event_start_unix_ms.is_none()
                || request.event_end_unix_ms.is_none()
                || request
                    .travel_mode
                    .as_deref()
                    .is_none_or(|value| !matches!(value, "automobile" | "transit" | "walking"))
            {
                return Err(invalid(
                    "operation",
                    "invalid Feasibility acquisition request",
                ));
            }
            let latitude = request.destination_latitude.unwrap();
            let longitude = request.destination_longitude.unwrap();
            let start = request.event_start_unix_ms.unwrap();
            let end = request.event_end_unix_ms.unwrap();
            if !latitude.is_finite()
                || !longitude.is_finite()
                || !(-90.0..=90.0).contains(&latitude)
                || !(-180.0..=180.0).contains(&longitude)
                || start < 0
                || end <= start
                || end - start > 86_400_000
            {
                return Err(invalid("operation", "invalid Feasibility window"));
            }
        }
    }
    Ok(())
}

pub fn validate_personal_acquisition_result(
    result: &LocalContextPersonalAcquisitionResultDto,
) -> WireResult<()> {
    validate_host_epoch(&result.host_epoch)?;
    validate_handle(&result.request_id, "operation.result.request_id")?;
    validate_handle(&result.device_id, "operation.result.device_id")?;
    validate_handle(&result.provider, "operation.result.provider")?;
    validate_handle(
        &result.permission_class,
        "operation.result.permission_class",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_before,
        "operation.result.native_subject_fingerprint_before",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_after,
        "operation.result.native_subject_fingerprint_after",
    )?;
    if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after {
        return Err(invalid("operation.result", "personal subject changed"));
    }
    if result.view.is_none() {
        return Err(invalid("operation.result.view", "missing personal view"));
    }
    Ok(())
}

pub fn acquisition_request(
    request: &LocalContextAcquisitionRequestDto,
) -> WireResult<CalendarAcquisitionRequest> {
    validate_acquisition_request(request)?;
    Ok(CalendarAcquisitionRequest {
        request_id: parse_request_id(&request.request_id, "operation.request_id")?,
        host_epoch: request.host_epoch.clone(),
        person_id: parse_person(&request.person_id, "operation.person_id")?,
        device_id: request.device_id.clone(),
        connection_id: request.connection_id.clone(),
        connection_revision: request.connection_revision,
        provider: calendar_provider(request.provider),
        mode: acquisition_mode(request.mode),
        calendar_ids: request.calendar_ids.clone(),
        range_start_unix_ms: request.range_start_unix_ms,
        range_end_unix_ms: request.range_end_unix_ms,
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint.clone(),
    })
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

fn acquisition_result(
    result: LocalContextAcquisitionResultDto,
) -> WireResult<CalendarAcquisitionResult> {
    validate_acquisition_result(&result)?;
    Ok(CalendarAcquisitionResult {
        request_id: parse_request_id(&result.request_id, "operation.result.request_id")?,
        host_epoch: result.host_epoch,
        person_id: parse_person(&result.person_id, "operation.result.person_id")?,
        device_id: result.device_id,
        connection_id: result.connection_id,
        connection_revision: result.connection_revision,
        provider: calendar_provider(result.provider),
        mode: acquisition_mode(result.mode),
        calendar_ids: result.calendar_ids,
        range_start_unix_ms: result.range_start_unix_ms,
        range_end_unix_ms: result.range_end_unix_ms,
        native_subject_fingerprint_before: result.native_subject_fingerprint_before,
        native_subject_fingerprint_after: result.native_subject_fingerprint_after,
        available_calendar_ids: result.available_calendar_ids,
        permission_class: result.permission_class,
        batches: result.batches.into_iter().map(native_batch).collect(),
    })
}

pub fn acquisition_result_dto(
    result: CalendarAcquisitionResult,
) -> LocalContextAcquisitionResultDto {
    LocalContextAcquisitionResultDto {
        request_id: result.request_id.to_string(),
        host_epoch: result.host_epoch,
        person_id: result.person_id.to_string(),
        device_id: result.device_id,
        connection_id: result.connection_id,
        connection_revision: result.connection_revision,
        provider: calendar_provider_dto(result.provider),
        mode: acquisition_mode_dto(result.mode),
        calendar_ids: result.calendar_ids,
        range_start_unix_ms: result.range_start_unix_ms,
        range_end_unix_ms: result.range_end_unix_ms,
        native_subject_fingerprint_before: result.native_subject_fingerprint_before,
        native_subject_fingerprint_after: result.native_subject_fingerprint_after,
        available_calendar_ids: result.available_calendar_ids,
        permission_class: result.permission_class,
        batches: result.batches.into_iter().map(native_batch_dto).collect(),
    }
}

pub fn attention_request(
    request: &LocalContextAttentionAcquisitionRequestDto,
) -> WireResult<AttentionAcquisitionRequest> {
    validate_attention_acquisition_request(request)?;
    Ok(AttentionAcquisitionRequest {
        request_id: parse_request_id(&request.request_id, "operation.request_id")?,
        host_epoch: request.host_epoch.clone(),
        person_id: parse_person(&request.person_id, "operation.person_id")?,
        device_id: request.device_id.clone(),
        mode: attention_mode(request.mode),
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint.clone(),
    })
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

fn attention_result(
    result: LocalContextAttentionAcquisitionResultDto,
) -> WireResult<AttentionAcquisitionResult> {
    validate_attention_acquisition_result(&result)?;
    Ok(AttentionAcquisitionResult {
        request_id: parse_request_id(&result.request_id, "operation.result.request_id")?,
        host_epoch: result.host_epoch,
        person_id: parse_person(&result.person_id, "operation.result.person_id")?,
        device_id: result.device_id,
        mode: attention_mode(result.mode),
        native_subject_fingerprint_before: result.native_subject_fingerprint_before,
        native_subject_fingerprint_after: result.native_subject_fingerprint_after,
        permission_class: result.permission_class,
        view: result.view,
    })
}

pub fn personal_request(
    request: &LocalContextPersonalAcquisitionRequestDto,
) -> WireResult<PersonalAcquisitionRequest> {
    validate_personal_acquisition_request(request)?;
    Ok(PersonalAcquisitionRequest {
        request_id: parse_request_id(&request.request_id, "operation.request_id")?,
        host_epoch: request.host_epoch.clone(),
        person_id: parse_person(&request.person_id, "operation.person_id")?,
        device_id: request.device_id.clone(),
        domain: personal_domain(request.domain),
        selected_handles: request.selected_handles.clone(),
        event_handle: request.event_handle.clone(),
        evidence_handles: request.evidence_handles.clone(),
        destination_latitude: request.destination_latitude,
        destination_longitude: request.destination_longitude,
        event_start_unix_ms: request.event_start_unix_ms,
        event_end_unix_ms: request.event_end_unix_ms,
        travel_mode: request.travel_mode.clone(),
        deadline_unix_ms: request.deadline_unix_ms,
        expected_native_subject_fingerprint: request.expected_native_subject_fingerprint.clone(),
    })
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

fn personal_result(
    result: LocalContextPersonalAcquisitionResultDto,
) -> WireResult<PersonalAcquisitionResult> {
    validate_personal_acquisition_result(&result)?;
    Ok(PersonalAcquisitionResult {
        request_id: parse_request_id(&result.request_id, "operation.result.request_id")?,
        host_epoch: result.host_epoch,
        person_id: parse_person(&result.person_id, "operation.result.person_id")?,
        device_id: result.device_id,
        domain: personal_domain(result.domain),
        native_subject_fingerprint_before: result.native_subject_fingerprint_before,
        native_subject_fingerprint_after: result.native_subject_fingerprint_after,
        permission_class: result.permission_class,
        provider: result.provider,
        view: result.view,
    })
}

/// Read one native operation off the wire as the app's own command.
pub fn local_context_command(
    operation: LocalContextOperationDto,
) -> WireResult<LocalContextCommand> {
    Ok(match operation {
        LocalContextOperationDto::RegisterAcquisitionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::RegisterAcquisitionHost { host_epoch }
        }
        LocalContextOperationDto::PollAcquisitions { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::PollAcquisitions { host_epoch }
        }
        LocalContextOperationDto::CompleteAcquisition { host_epoch, result } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::CompleteAcquisition {
                host_epoch,
                result: Box::new(acquisition_result(result)?),
            }
        }
        LocalContextOperationDto::FailAcquisition {
            host_epoch,
            request_id,
            failure,
        } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::FailAcquisition {
                host_epoch,
                request_id: parse_request_id(&request_id, "operation.request_id")?,
                failure: calendar_source_failure(failure),
            }
        }
        LocalContextOperationDto::DisposeAcquisitionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::DisposeAcquisitionHost { host_epoch }
        }
        LocalContextOperationDto::RegisterAttentionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::RegisterAttentionHost { host_epoch }
        }
        LocalContextOperationDto::PollAttentionAcquisitions { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::PollAttentionAcquisitions { host_epoch }
        }
        LocalContextOperationDto::CompleteAttentionAcquisition { host_epoch, result } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::CompleteAttentionAcquisition {
                host_epoch,
                result: Box::new(attention_result(result)?),
            }
        }
        LocalContextOperationDto::FailAttentionAcquisition {
            host_epoch,
            request_id,
            failure,
        } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::FailAttentionAcquisition {
                host_epoch,
                request_id: parse_request_id(&request_id, "operation.request_id")?,
                failure: attention_failure(&failure)
                    .ok_or_else(|| invalid("operation.failure", "unsupported failure"))?,
            }
        }
        LocalContextOperationDto::DisposeAttentionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::DisposeAttentionHost { host_epoch }
        }
        LocalContextOperationDto::RegisterPersonalHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::RegisterPersonalHost { host_epoch }
        }
        LocalContextOperationDto::PollPersonalAcquisitions { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::PollPersonalAcquisitions { host_epoch }
        }
        LocalContextOperationDto::CompletePersonalAcquisition { host_epoch, result } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::CompletePersonalAcquisition {
                host_epoch,
                result: Box::new(personal_result(result)?),
            }
        }
        LocalContextOperationDto::FailPersonalAcquisition {
            host_epoch,
            request_id,
            failure,
        } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::FailPersonalAcquisition {
                host_epoch,
                request_id: parse_request_id(&request_id, "operation.request_id")?,
                failure: personal_failure(&failure),
            }
        }
        LocalContextOperationDto::DisposePersonalHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            LocalContextCommand::DisposePersonalHost { host_epoch }
        }
        LocalContextOperationDto::Publish { device_id, view } => {
            validate_handle(&device_id, "operation.device_id")?;
            let view_id = view
                .get("view_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("operation.view.view_id", "must be a string"))?
                .to_owned();
            LocalContextCommand::Publish {
                device_id,
                view_id,
                view,
            }
        }
        LocalContextOperationDto::PublishCalendarObservation {
            device_id,
            connection_id,
            connection_revision,
            provider,
            calendar_ids,
            observed_at_unix_ms,
            expires_at_unix_ms,
            range_start_unix_ms,
            range_end_unix_ms,
            batches,
        } => {
            validate_handle(&device_id, "operation.device_id")?;
            validate_handle(&connection_id, "operation.connection_id")?;
            let provider = calendar_provider(provider);
            let batches = batches
                .into_iter()
                .map(|batch| super::day::calendar_batch_from_dto(batch))
                .collect();
            LocalContextCommand::PublishCalendarObservation {
                device_id,
                observation: Box::new(CalendarObservationPublication {
                    connection_id,
                    connection_revision,
                    provider,
                    calendar_ids,
                    observed_at_unix_ms,
                    expires_at_unix_ms,
                    range_start_unix_ms,
                    range_end_unix_ms,
                    batches,
                }),
            }
        }
        LocalContextOperationDto::Read { view_id, device_id } => {
            validate_view_id(&view_id)?;
            if let Some(device_id) = device_id.as_deref() {
                validate_handle(device_id, "operation.device_id")?;
            }
            LocalContextCommand::Read { view_id, device_id }
        }
        LocalContextOperationDto::Revoke { device_id, view_id } => {
            validate_handle(&device_id, "operation.device_id")?;
            if let Some(view_id) = view_id.as_deref() {
                validate_view_id(view_id)?;
            }
            LocalContextCommand::Revoke { device_id, view_id }
        }
    })
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
