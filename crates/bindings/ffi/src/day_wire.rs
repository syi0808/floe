use crate::conversion;
use floe_protocol::wire::{WireResult, conversion_error, parse_date, parse_id, parse_time};
use floe_protocol::{DayMutationDto, DayQueryDto};

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
