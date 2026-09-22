use crate::conversion::native::{
    acquisition_mode, attention_mode, calendar_provider, calendar_source_failure, native_batch,
    now_unix_ms, parse_request_id, personal_domain, validate_fingerprint, validate_handle,
    validate_host_epoch, validate_view_id,
};
use floe_app::{
    AttentionCompletion, CalendarCompletion, CalendarObservationPublication, ContextCommand,
    ContextQuery, MAX_ACQUISITION_DEADLINE_MS, PersonalCompletion, attention_failure,
    personal_failure, valid_native_subject_fingerprint,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::{
    AttentionCompletionDto, CalendarCompletionDto, CalendarProviderDto, ContextCommandDto,
    ContextQueryDto, LocalContextAcquisitionModeDto, PersonalCompletionDto,
};
use serde_json::Value;
use uuid::Uuid;

const MAX_ACQUISITION_CALENDARS: usize = 4;
const MAX_ACQUISITION_ITEMS: usize = 128;
const MAX_ACQUISITION_BYTES: usize = 65_536;

fn request_id(value: &str) -> WireResult<Uuid> {
    let identifier = parse_request_id(value, "command.request_id")?;
    if identifier.is_nil() {
        return Err(invalid("command.request_id", "must not be nil"));
    }
    Ok(identifier)
}

fn validate_calendar_completion(result: &CalendarCompletionDto) -> WireResult<()> {
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
    validate_host_epoch(&result.host_epoch)?;
    request_id(&result.request_id)?;
    validate_handle(&result.connection_id, "operation.connection_id")?;
    let now = now_unix_ms()?;
    if result.connection_revision == 0
        || !matches!(
            result.provider,
            CalendarProviderDto::EventKit | CalendarProviderDto::Android
        )
        || result.calendar_ids.is_empty()
        || result.calendar_ids.len() > MAX_ACQUISITION_CALENDARS
        || result
            .calendar_ids
            .iter()
            .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
        || result
            .calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || result.range_start_unix_ms < 0
        || result.range_end_unix_ms <= result.range_start_unix_ms
        || result.range_end_unix_ms - result.range_start_unix_ms > 32 * 86_400_000
        || now.saturating_add(MAX_ACQUISITION_DEADLINE_MS - 1_000) <= result.range_start_unix_ms
    {
        return Err(invalid(
            "operation.result",
            "acquisition result is outside bounds",
        ));
    }

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
fn validate_attention_completion(result: &AttentionCompletionDto) -> WireResult<()> {
    validate_host_epoch(&result.host_epoch)?;
    validate_handle(&result.request_id, "operation.result.request_id")?;
    validate_fingerprint(
        &result.native_subject_fingerprint_before,
        "operation.result.native_subject_fingerprint_before",
    )?;
    validate_fingerprint(
        &result.native_subject_fingerprint_after,
        "operation.result.native_subject_fingerprint_after",
    )?;
    if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after
        || result.permission_class.is_empty()
        || result.permission_class.len() > 64
    {
        return Err(invalid("operation.result", "invalid attention evidence"));
    }
    Ok(())
}
fn validate_personal_completion(result: &PersonalCompletionDto) -> WireResult<()> {
    validate_host_epoch(&result.host_epoch)?;
    validate_handle(&result.request_id, "operation.result.request_id")?;
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
fn acquisition_result(result: CalendarCompletionDto) -> WireResult<CalendarCompletion> {
    validate_calendar_completion(&result)?;
    Ok(CalendarCompletion {
        request_id: request_id(&result.request_id)?,
        host_epoch: result.host_epoch,
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

fn attention_result(result: AttentionCompletionDto) -> WireResult<AttentionCompletion> {
    validate_attention_completion(&result)?;
    Ok(AttentionCompletion {
        request_id: request_id(&result.request_id)?,
        host_epoch: result.host_epoch,
        mode: attention_mode(result.mode),
        native_subject_fingerprint_before: result.native_subject_fingerprint_before,
        native_subject_fingerprint_after: result.native_subject_fingerprint_after,
        permission_class: result.permission_class,
        view: result.view,
    })
}

fn personal_result(result: PersonalCompletionDto) -> WireResult<PersonalCompletion> {
    validate_personal_completion(&result)?;
    Ok(PersonalCompletion {
        request_id: request_id(&result.request_id)?,
        host_epoch: result.host_epoch,
        domain: personal_domain(result.domain),
        native_subject_fingerprint_before: result.native_subject_fingerprint_before,
        native_subject_fingerprint_after: result.native_subject_fingerprint_after,
        permission_class: result.permission_class,
        provider: result.provider,
        view: result.view,
    })
}

pub(crate) fn command(command: ContextCommandDto) -> WireResult<ContextCommand> {
    Ok(match command {
        ContextCommandDto::RegisterAcquisitionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::RegisterAcquisitionHost { host_epoch }
        }
        ContextCommandDto::DisposeAcquisitionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::DisposeAcquisitionHost { host_epoch }
        }
        ContextCommandDto::CompleteAcquisition { host_epoch, result } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::CompleteAcquisition {
                host_epoch,
                result: Box::new(acquisition_result(result)?),
            }
        }
        ContextCommandDto::FailAcquisition {
            host_epoch,
            request_id: identifier,
            failure,
        } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::FailAcquisition {
                host_epoch,
                request_id: request_id(&identifier)?,
                failure: calendar_source_failure(failure),
            }
        }
        ContextCommandDto::RegisterAttentionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::RegisterAttentionHost { host_epoch }
        }
        ContextCommandDto::DisposeAttentionHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::DisposeAttentionHost { host_epoch }
        }
        ContextCommandDto::CompleteAttentionAcquisition { host_epoch, result } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::CompleteAttentionAcquisition {
                host_epoch,
                result: Box::new(attention_result(result)?),
            }
        }
        ContextCommandDto::FailAttentionAcquisition {
            host_epoch,
            request_id: identifier,
            failure,
        } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::FailAttentionAcquisition {
                host_epoch,
                request_id: request_id(&identifier)?,
                failure: attention_failure(&failure)
                    .ok_or_else(|| invalid("command.failure", "unsupported failure"))?,
            }
        }
        ContextCommandDto::RegisterPersonalHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::RegisterPersonalHost { host_epoch }
        }
        ContextCommandDto::DisposePersonalHost { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::DisposePersonalHost { host_epoch }
        }
        ContextCommandDto::CompletePersonalAcquisition { host_epoch, result } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::CompletePersonalAcquisition {
                host_epoch,
                result: Box::new(personal_result(result)?),
            }
        }
        ContextCommandDto::FailPersonalAcquisition {
            host_epoch,
            request_id: identifier,
            failure,
        } => {
            validate_host_epoch(&host_epoch)?;
            ContextCommand::FailPersonalAcquisition {
                host_epoch,
                request_id: request_id(&identifier)?,
                failure: personal_failure(&failure),
            }
        }
        ContextCommandDto::Publish { view } => {
            let view_id = view
                .get("view_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("command.view.view_id", "must be a string"))?
                .to_owned();
            ContextCommand::Publish { view_id, view }
        }
        ContextCommandDto::PublishCalendarObservation {
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
            validate_handle(&connection_id, "command.connection_id")?;
            ContextCommand::PublishCalendarObservation {
                observation: Box::new(CalendarObservationPublication {
                    connection_id,
                    connection_revision,
                    provider: calendar_provider(provider),
                    calendar_ids,
                    observed_at_unix_ms,
                    expires_at_unix_ms,
                    range_start_unix_ms,
                    range_end_unix_ms,
                    batches: batches
                        .into_iter()
                        .map(crate::conversion::calendar_batch_from_dto)
                        .collect(),
                }),
            }
        }
        ContextCommandDto::Revoke { view_id } => {
            if let Some(view_id) = view_id.as_deref() {
                validate_view_id(view_id)?;
            }
            ContextCommand::Revoke { view_id }
        }
    })
}
pub(crate) fn query(query: ContextQueryDto) -> WireResult<ContextQuery> {
    Ok(match query {
        ContextQueryDto::PollAcquisitions { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextQuery::PollAcquisitions { host_epoch }
        }
        ContextQueryDto::PollAttentionAcquisitions { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextQuery::PollAttentionAcquisitions { host_epoch }
        }
        ContextQueryDto::PollPersonalAcquisitions { host_epoch } => {
            validate_host_epoch(&host_epoch)?;
            ContextQuery::PollPersonalAcquisitions { host_epoch }
        }
        ContextQueryDto::Read { view_id } => {
            validate_view_id(&view_id)?;
            ContextQuery::Read { view_id }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_completion_rejects_nil_request_and_changed_subject() {
        let completion = |identifier: Uuid, after: String| AttentionCompletionDto {
            request_id: identifier.to_string(),
            host_epoch: "native-host".into(),
            mode: floe_protocol::LocalContextAttentionAcquisitionModeDto::InspectSubject,
            native_subject_fingerprint_before: "a".repeat(64),
            native_subject_fingerprint_after: after,
            permission_class: "authorized".into(),
            view: None,
        };
        assert!(attention_result(completion(Uuid::nil(), "a".repeat(64))).is_err());
        assert!(attention_result(completion(Uuid::new_v4(), "b".repeat(64))).is_err());
        let identifier = Uuid::new_v4();
        let converted = attention_result(completion(identifier, "a".repeat(64))).unwrap();
        assert_eq!(converted.request_id, identifier);
        assert_eq!(converted.host_epoch, "native-host");
        assert!(
            query(ContextQueryDto::PollAttentionAcquisitions {
                host_epoch: String::new()
            })
            .is_err()
        );
    }
}
