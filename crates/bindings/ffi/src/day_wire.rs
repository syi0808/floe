use crate::conversion;
use floe_protocol::wire::{WireResult, conversion_error, parse_date, parse_id, parse_time};
use floe_protocol::{
    DayMutationDto, DayQueryDto, ManualCalendarOperationDto, ManualCalendarOperationReceiptDto,
    ManualCalendarOperationStatusDto,
};
use uuid::Uuid;

pub(crate) fn read(day: DayQueryDto) -> WireResult<floe_day::DayQuery> {
    day.validate()
        .map_err(|field| floe_protocol::wire::invalid(field, "invalid Day query"))?;
    Ok(floe_day::DayQuery {
        date: parse_date(&day.date)?,
        timezone_offset_seconds: day.timezone_offset_seconds,
        end_timezone_offset_seconds: day.end_timezone_offset_seconds,
        now: parse_time(&day.now, "day.now")?,
    })
}

pub(crate) fn mutation(mutation: DayMutationDto) -> WireResult<floe_day::DayMutation> {
    Ok(match mutation {
        DayMutationDto::SubmitCapture { input, occurred_at } => {
            floe_day::DayMutation::SubmitCapture {
                input,
                occurred_at: parse_time(&occurred_at, "occurred_at")?,
            }
        }
        DayMutationDto::ClassifyCapture {
            capture_id,
            expected_revision,
            classification,
            occurred_at,
        } => floe_day::DayMutation::ClassifyCapture {
            capture_id: parse_id(&capture_id, "capture_id", floe_app::CaptureId)?,
            expected_revision,
            classification: conversion::classification_from_dto(classification)
                .map_err(conversion_error)?,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::CreateEvent {
            title,
            schedule,
            occurred_at,
        } => floe_day::DayMutation::CreateEvent {
            title,
            schedule: conversion::event_schedule_from_dto(schedule).map_err(conversion_error)?,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::CreateTask {
            title,
            deadline,
            priority,
            occurred_at,
        } => floe_day::DayMutation::CreateTask {
            title,
            deadline: deadline
                .as_deref()
                .map(|value| parse_time(value, "deadline"))
                .transpose()?,
            priority: conversion::priority_from_dto(priority),
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::CreateNote {
            content,
            occurred_at,
        } => floe_day::DayMutation::CreateNote {
            content,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::UpdateEvent {
            event_id,
            expected_revision,
            title,
            schedule,
            occurred_at,
        } => floe_day::DayMutation::UpdateEvent {
            event_id: parse_id(&event_id, "event_id", floe_app::EventId)?,
            expected_revision,
            title,
            schedule: conversion::event_schedule_from_dto(schedule).map_err(conversion_error)?,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::UpdateTask {
            task_id,
            expected_revision,
            title,
            deadline,
            priority,
            occurred_at,
        } => floe_day::DayMutation::UpdateTask {
            task_id: parse_id(&task_id, "task_id", floe_app::TaskId)?,
            expected_revision,
            title,
            deadline: deadline
                .as_deref()
                .map(|value| parse_time(value, "deadline"))
                .transpose()?,
            priority: conversion::priority_from_dto(priority),
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::UpdateNote {
            note_id,
            expected_revision,
            content,
            occurred_at,
        } => floe_day::DayMutation::UpdateNote {
            note_id: parse_id(&note_id, "note_id", floe_app::NoteId)?,
            expected_revision,
            content,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::SetTaskCompletion {
            task_id,
            expected_revision,
            completed,
            occurred_at,
        } => floe_day::DayMutation::SetTaskCompletion {
            task_id: parse_id(&task_id, "task_id", floe_app::TaskId)?,
            expected_revision,
            completed,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::DeleteItem {
            target,
            expected_revision,
            occurred_at,
        } => floe_day::DayMutation::DeleteItem {
            target: conversion::domain_ref_from_dto(target).map_err(conversion_error)?,
            expected_revision,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
    })
}

pub(crate) fn manual_operation(
    operation: ManualCalendarOperationDto,
) -> WireResult<floe_day::ManualCalendarOperation> {
    let timed = |schedule: floe_protocol::TimedScheduleDto| -> WireResult<floe_day::TimedSchedule> {
        match conversion::event_schedule_from_dto(floe_protocol::EventScheduleDto::Timed {
            starts_at: schedule.starts_at,
            ends_at: schedule.ends_at,
            timezone: schedule.timezone,
        })
        .map_err(conversion_error)?
        {
            floe_day::EventSchedule::Timed(value) => Ok(value),
            floe_day::EventSchedule::AllDay(_) => Err(floe_protocol::wire::invalid(
                "schedule",
                "external calendar operations require a timed schedule",
            )),
        }
    };
    Ok(match operation {
        ManualCalendarOperationDto::Create {
            destination_ref,
            title,
            schedule,
        } => floe_day::ManualCalendarOperation::Create {
            destination_ref: Uuid::parse_str(&destination_ref)
                .map_err(|_| floe_protocol::wire::invalid("destination_ref", "invalid id"))?,
            title,
            schedule: timed(schedule)?,
        },
        ManualCalendarOperationDto::Update {
            event_ref,
            expected_revision,
            title,
            schedule,
        } => floe_day::ManualCalendarOperation::Update {
            event_ref: parse_id(&event_ref, "event_ref", floe_app::EventId)?,
            expected_revision: floe_app::Revision(expected_revision),
            title,
            schedule: timed(schedule)?,
        },
        ManualCalendarOperationDto::Delete {
            event_ref,
            expected_revision,
        } => floe_day::ManualCalendarOperation::Delete {
            event_ref: parse_id(&event_ref, "event_ref", floe_app::EventId)?,
            expected_revision: floe_app::Revision(expected_revision),
        },
    })
}

pub(crate) fn manual_operation_receipt(
    value: floe_day::ManualCalendarOperationReceipt,
) -> WireResult<ManualCalendarOperationReceiptDto> {
    if value.operation_id.is_nil() || value.revision == 0 {
        return Err(floe_protocol::wire::invalid(
            "operation",
            "invalid calendar operation receipt",
        ));
    }
    Ok(ManualCalendarOperationReceiptDto {
        operation_ref: floe_protocol::OperationRefDto::new(value.operation_id)
            .ok_or_else(|| floe_protocol::wire::invalid("operation_ref", "invalid id"))?,
        revision: value.revision,
        status: match value.status {
            floe_day::ManualCalendarOperationStatus::Pending => {
                ManualCalendarOperationStatusDto::Pending
            }
            floe_day::ManualCalendarOperationStatus::Executing => {
                ManualCalendarOperationStatusDto::Executing
            }
            floe_day::ManualCalendarOperationStatus::Blocked => {
                ManualCalendarOperationStatusDto::Blocked
            }
            floe_day::ManualCalendarOperationStatus::NotApplied => {
                ManualCalendarOperationStatusDto::NotApplied
            }
            floe_day::ManualCalendarOperationStatus::Unknown => {
                ManualCalendarOperationStatusDto::Unknown
            }
            floe_day::ManualCalendarOperationStatus::Succeeded => {
                ManualCalendarOperationStatusDto::Succeeded
            }
        },
        collection_pending: value.collection_pending,
    })
}

pub(crate) fn manual_destinations(
    values: Vec<floe_day::ManualCalendarDestination>,
) -> WireResult<Vec<floe_protocol::ManualCalendarDestinationDto>> {
    if values.len() > 64 {
        return Err(floe_protocol::wire::invalid(
            "destinations",
            "too many Calendar destinations",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .map(|value| {
            if value.destination_ref.is_nil()
                || value.label.is_empty()
                || value.label.trim() != value.label
                || value.label.len() > 512
                || value.label.chars().any(char::is_control)
                || !seen.insert(value.destination_ref)
            {
                return Err(floe_protocol::wire::invalid(
                    "destinations",
                    "invalid Calendar destination list",
                ));
            }
            Ok(floe_protocol::ManualCalendarDestinationDto {
                destination_ref: floe_protocol::UuidRefDto::new(value.destination_ref)
                    .ok_or_else(|| floe_protocol::wire::invalid("destination_ref", "invalid id"))?,
                label: value.label,
            })
        })
        .collect()
}

pub(crate) fn manual_operation_receipts(
    value: floe_day::ManualCalendarOperationPage,
) -> WireResult<floe_protocol::ManualCalendarOperationsDto> {
    value
        .validate(100)
        .map_err(floe_protocol::wire::agent_failure)?;
    let values = value.operations;
    if values.len() > 100 {
        return Err(floe_protocol::wire::invalid(
            "operations",
            "too many Calendar operations",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let operations = values
        .into_iter()
        .map(|value| {
            if !seen.insert(value.operation_id) {
                return Err(floe_protocol::wire::invalid(
                    "operations",
                    "duplicate Calendar operation",
                ));
            }
            manual_operation_receipt(value)
        })
        .collect::<WireResult<Vec<_>>>()?;
    let dto = floe_protocol::ManualCalendarOperationsDto {
        operations,
        next_cursor: value
            .next_cursor
            .and_then(floe_protocol::OperationRefDto::new),
    };
    dto.validate()
        .map_err(|field| floe_protocol::wire::invalid(field, "invalid operation list"))?;
    Ok(dto)
}

pub(crate) fn refresh(
    value: floe_day::DayRefreshSnapshot,
) -> WireResult<floe_protocol::DayRefreshStateDto> {
    use floe_day::DayRefreshState;
    use floe_protocol::DayRefreshStateDto;
    let operation_ref = floe_protocol::OperationRefDto::new(value.operation_ref)
        .ok_or_else(|| floe_protocol::wire::invalid("operation_ref", "nil refresh identity"))?;
    let revision = value.revision.0;
    let result = match value.state {
        DayRefreshState::Pending => DayRefreshStateDto::Pending {
            operation_ref,
            revision,
        },
        DayRefreshState::Running => DayRefreshStateDto::Running {
            operation_ref,
            revision,
        },
        DayRefreshState::Completed { day } => DayRefreshStateDto::Completed {
            operation_ref,
            revision,
            day: conversion::day_snapshot_to_dto(day).map_err(conversion_error)?,
        },
        DayRefreshState::Failed { failure } => DayRefreshStateDto::Failed {
            operation_ref,
            revision,
            failure: refresh_failure(failure),
        },
        DayRefreshState::Interrupted { failure } => DayRefreshStateDto::Interrupted {
            operation_ref,
            revision,
            failure: refresh_failure(failure),
        },
    };
    result
        .validate()
        .map_err(|field| floe_protocol::wire::invalid(field, "invalid refresh projection"))?;
    Ok(result)
}
fn refresh_failure(value: floe_day::DayRefreshFailure) -> floe_protocol::DayRefreshFailureDto {
    use floe_day::DayRefreshFailure as Owner;
    use floe_protocol::DayRefreshFailureDto as Wire;
    match value {
        Owner::SourceChanged => Wire::SourceChanged,
        Owner::PermissionDenied => Wire::PermissionDenied,
        Owner::Unavailable => Wire::Unavailable,
        Owner::VaultLocked => Wire::VaultLocked,
        Owner::BudgetExceeded => Wire::BudgetExceeded,
        Owner::DeadlineExceeded => Wire::DeadlineExceeded,
        Owner::Cancelled => Wire::Cancelled,
        Owner::HostInterrupted => Wire::HostInterrupted,
        Owner::StorageUnavailable => Wire::StorageUnavailable,
        Owner::InvalidAcquisition => Wire::InvalidAcquisition,
    }
}
