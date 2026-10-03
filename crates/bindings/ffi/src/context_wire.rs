//! Mechanical native callback decoding. Authority comes from the host registration
//! and outstanding acquisition in App and the native broker.
use crate::conversion::native::{
    acquisition_mode, attention_mode, calendar_provider, calendar_source_failure, native_batch,
    personal_domain, personal_mode,
};
use floe_app::{
    AttentionCompletion, CalendarCompletion, NativeHostCommand, NativeHostKind, NativeHostQuery,
    NativeHostRegistrationRef, NativeSourceResource, PersonalCompletion, attention_failure,
    personal_failure,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::{
    AttentionCompletionDto, CalendarCompletionDto, NativeHostCommandDto, NativeHostQueryDto,
    NativeHostRegistrationDto, NativeSourceResourceDto, PersonalCompletionDto,
};
use uuid::Uuid;

fn request_id(value: &str) -> WireResult<Uuid> {
    let id = Uuid::parse_str(value).map_err(|_| invalid("request_id", "must be a UUID"))?;
    if id.is_nil() || id.to_string() != value {
        return Err(invalid("request_id", "must be canonical and non-nil"));
    }
    Ok(id)
}
fn text(value: &str, max: usize, field: &'static str) -> WireResult<()> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(field, "text is outside bounds"));
    }
    Ok(())
}
fn bounded<T: serde::Serialize>(value: &T) -> WireResult<()> {
    if serde_json::to_vec(value)
        .map_err(|_| invalid("result", "invalid result"))?
        .len()
        > 65_536
    {
        return Err(invalid("result", "result is outside bounds"));
    }
    Ok(())
}
fn registration(value: NativeHostRegistrationDto) -> WireResult<NativeHostRegistrationRef> {
    value
        .validate()
        .map_err(|field| invalid(field, "invalid host registration"))?;
    Ok(NativeHostRegistrationRef {
        registration_id: value.registration_id.get(),
        host_epoch: value.host_epoch,
        runtime_epoch: value.runtime_epoch,
    })
}
fn resources(values: Vec<NativeSourceResourceDto>) -> WireResult<Vec<NativeSourceResource>> {
    if values.len() > 256 {
        return Err(invalid("resources", "too many resources"));
    }
    values
        .into_iter()
        .map(|value| {
            text(&value.handle, 512, "resources.handle")?;
            text(&value.label, 256, "resources.label")?;
            Ok(NativeSourceResource {
                handle: value.handle,
                label: value.label,
            })
        })
        .collect()
}
fn calendar_result(value: CalendarCompletionDto) -> WireResult<CalendarCompletion> {
    bounded(&value)?;
    if value.connection_revision == 0
        || value.connection_revision > i64::MAX as u64
        || value.calendar_ids.len() > 256
        || value.available_calendar_ids.len() > 256
        || value.batches.len() > 256
        || value
            .batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>()
            > 128
    {
        return Err(invalid("result", "calendar result is outside bounds"));
    }
    text(&value.connection_id, 512, "connection_id")?;
    for id in value
        .calendar_ids
        .iter()
        .chain(value.available_calendar_ids.iter())
    {
        text(id, 512, "calendar_ids")?;
    }
    text(&value.permission_class, 64, "permission_class")?;
    for batch in &value.batches {
        text(&batch.calendar_id, 512, "calendar_id")?;
        for record in &batch.records {
            text(&record.calendar_id, 512, "calendar_id")?;
            text(&record.external_id, 512, "external_id")?;
            text(&record.external_revision, 512, "external_revision")?;
            if record.title.len() > 4096 {
                return Err(invalid("title", "title is outside bounds"));
            }
        }
    }
    Ok(CalendarCompletion {
        request_id: request_id(&value.request_id)?,
        host_epoch: value.host_epoch,
        connection_id: value.connection_id,
        connection_revision: value.connection_revision,
        provider: calendar_provider(value.provider),
        mode: acquisition_mode(value.mode),
        calendar_ids: value.calendar_ids,
        range_start_unix_ms: value.range_start_unix_ms,
        range_end_unix_ms: value.range_end_unix_ms,
        native_subject_fingerprint_before: value.native_subject_fingerprint_before,
        native_subject_fingerprint_after: value.native_subject_fingerprint_after,
        available_calendar_ids: value.available_calendar_ids,
        available_calendars: resources(value.available_calendars)?,
        permission_class: value.permission_class,
        batches: value.batches.into_iter().map(native_batch).collect(),
    })
}
fn attention_result(value: AttentionCompletionDto) -> WireResult<AttentionCompletion> {
    bounded(&value)?;
    text(&value.permission_class, 64, "permission_class")?;
    Ok(AttentionCompletion {
        request_id: request_id(&value.request_id)?,
        host_epoch: value.host_epoch,
        mode: attention_mode(value.mode),
        native_subject_fingerprint_before: value.native_subject_fingerprint_before,
        native_subject_fingerprint_after: value.native_subject_fingerprint_after,
        permission_class: value.permission_class,
        view: value.view,
    })
}
fn personal_result(value: PersonalCompletionDto) -> WireResult<PersonalCompletion> {
    bounded(&value)?;
    text(&value.permission_class, 64, "permission_class")?;
    text(&value.provider, 128, "provider")?;
    Ok(PersonalCompletion {
        request_id: request_id(&value.request_id)?,
        host_epoch: value.host_epoch,
        domain: personal_domain(value.domain),
        mode: personal_mode(value.mode),
        native_subject_fingerprint_before: value.native_subject_fingerprint_before,
        native_subject_fingerprint_after: value.native_subject_fingerprint_after,
        permission_class: value.permission_class,
        provider: value.provider,
        view: value.view,
        transform_operation_id: value.transform_operation_id.map(|id| id.get()),
        resources: resources(value.resources)?,
        catalog_complete: value.catalog_complete,
    })
}
pub(crate) fn command(value: NativeHostCommandDto) -> WireResult<NativeHostCommand> {
    value
        .validate()
        .map_err(|field| invalid(field, "invalid native host command"))?;
    Ok(match value {
        NativeHostCommandDto::CalendarRegister {} => NativeHostCommand::Register {
            kind: NativeHostKind::Calendar,
        },
        NativeHostCommandDto::AttentionRegister {} => NativeHostCommand::Register {
            kind: NativeHostKind::Attention,
        },
        NativeHostCommandDto::PersonalRegister {} => NativeHostCommand::Register {
            kind: NativeHostKind::Personal,
        },
        NativeHostCommandDto::CalendarDispose { registration: r } => NativeHostCommand::Dispose {
            kind: NativeHostKind::Calendar,
            registration: registration(r)?,
        },
        NativeHostCommandDto::AttentionDispose { registration: r } => NativeHostCommand::Dispose {
            kind: NativeHostKind::Attention,
            registration: registration(r)?,
        },
        NativeHostCommandDto::PersonalDispose { registration: r } => NativeHostCommand::Dispose {
            kind: NativeHostKind::Personal,
            registration: registration(r)?,
        },
        NativeHostCommandDto::CalendarComplete {
            registration: r,
            result,
        } => NativeHostCommand::CompleteCalendar {
            registration: registration(r)?,
            result: Box::new(calendar_result(result)?),
        },
        NativeHostCommandDto::AttentionComplete {
            registration: r,
            result,
        } => NativeHostCommand::CompleteAttention {
            registration: registration(r)?,
            result: Box::new(attention_result(result)?),
        },
        NativeHostCommandDto::PersonalComplete {
            registration: r,
            result,
        } => NativeHostCommand::CompletePersonal {
            registration: registration(r)?,
            result: Box::new(personal_result(result)?),
        },
        NativeHostCommandDto::CalendarFail {
            registration: r,
            request_id: id,
            failure,
        } => NativeHostCommand::FailCalendar {
            registration: registration(r)?,
            request_id: request_id(&id)?,
            failure: calendar_source_failure(failure),
        },
        NativeHostCommandDto::AttentionFail {
            registration: r,
            request_id: id,
            failure,
        } => NativeHostCommand::FailAttention {
            registration: registration(r)?,
            request_id: request_id(&id)?,
            failure: attention_failure(&failure)
                .ok_or_else(|| invalid("failure", "unsupported attention failure"))?,
        },
        NativeHostCommandDto::PersonalFail {
            registration: r,
            request_id: id,
            failure,
        } => {
            text(&failure, 128, "failure")?;
            NativeHostCommand::FailPersonal {
                registration: registration(r)?,
                request_id: request_id(&id)?,
                failure: personal_failure(&failure),
            }
        }
    })
}
pub(crate) fn query(value: NativeHostQueryDto) -> WireResult<NativeHostQuery> {
    value
        .validate()
        .map_err(|field| invalid(field, "invalid native host query"))?;
    let (kind, r) = match value {
        NativeHostQueryDto::CalendarPoll { registration } => {
            (NativeHostKind::Calendar, registration)
        }
        NativeHostQueryDto::AttentionPoll { registration } => {
            (NativeHostKind::Attention, registration)
        }
        NativeHostQueryDto::PersonalPoll { registration } => {
            (NativeHostKind::Personal, registration)
        }
    };
    Ok(NativeHostQuery::Poll {
        kind,
        registration: registration(r)?,
    })
}
