use crate::conversion;
use floe_protocol::wire::{WireResult, conversion_error, parse_date, parse_id, parse_time};
use floe_protocol::{DayMutationDto, DayQueryDto};

pub(crate) fn read(day: DayQueryDto) -> WireResult<floe_app::DayRead> {
    Ok(floe_app::DayRead {
        date: parse_date(&day.date)?,
        timezone_offset_seconds: day.timezone_offset_seconds,
        end_timezone_offset_seconds: day.end_timezone_offset_seconds,
        now: parse_time(&day.now, "day.now")?,
    })
}

pub(crate) fn mutation(mutation: DayMutationDto) -> WireResult<floe_app::DayMutation> {
    Ok(match mutation {
        DayMutationDto::SubmitCapture { input, occurred_at } => {
            floe_app::DayMutation::SubmitCapture {
                input,
                occurred_at: parse_time(&occurred_at, "occurred_at")?,
            }
        }
        DayMutationDto::ClassifyCapture {
            capture_id,
            expected_revision,
            classification,
            occurred_at,
        } => floe_app::DayMutation::ClassifyCapture {
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
        } => floe_app::DayMutation::CreateEvent {
            title,
            schedule: conversion::event_schedule_from_dto(schedule).map_err(conversion_error)?,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::CreateTask {
            title,
            deadline,
            priority,
            occurred_at,
        } => floe_app::DayMutation::CreateTask {
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
        } => floe_app::DayMutation::CreateNote {
            content,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::UpdateEvent {
            event_id,
            expected_revision,
            title,
            schedule,
            occurred_at,
        } => floe_app::DayMutation::UpdateEvent {
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
        } => floe_app::DayMutation::UpdateTask {
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
        } => floe_app::DayMutation::UpdateNote {
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
        } => floe_app::DayMutation::SetTaskCompletion {
            task_id: parse_id(&task_id, "task_id", floe_app::TaskId)?,
            expected_revision,
            completed,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
        DayMutationDto::DeleteItem {
            target,
            expected_revision,
            occurred_at,
        } => floe_app::DayMutation::DeleteItem {
            target: conversion::domain_ref_from_dto(target).map_err(conversion_error)?,
            expected_revision,
            occurred_at: parse_time(&occurred_at, "occurred_at")?,
        },
    })
}
