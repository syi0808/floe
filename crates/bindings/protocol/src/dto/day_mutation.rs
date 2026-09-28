use super::{
    CalendarBatchDto, CalendarFailureDto, CalendarRangeDto, CalendarRecordDto, ClassificationDto,
    DomainRefDto, EventScheduleDto, PriorityDto,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayMutationDto {
    ImportCalendarSources {
        connection_id: String,
        expected_mirror_revision: Option<u64>,
        range: CalendarRangeDto,
        batches: Vec<CalendarBatchDto>,
        occurred_at: String,
    },
    ImportCalendar {
        connection_id: String,
        expected_mirror_revision: Option<u64>,
        range: CalendarRangeDto,
        records: Vec<CalendarRecordDto>,
        occurred_at: String,
    },
    CalendarFailed {
        connection_id: String,
        expected_mirror_revision: Option<u64>,
        failure: CalendarFailureDto,
    },
    SubmitCapture {
        input: String,
        occurred_at: String,
    },
    ClassifyCapture {
        capture_id: String,
        expected_revision: u64,
        classification: ClassificationDto,
        occurred_at: String,
    },
    CreateEvent {
        title: String,
        schedule: EventScheduleDto,
        occurred_at: String,
    },
    CreateTask {
        title: String,
        deadline: Option<String>,
        priority: PriorityDto,
        occurred_at: String,
    },
    CreateNote {
        content: String,
        occurred_at: String,
    },
    UpdateEvent {
        event_id: String,
        expected_revision: u64,
        title: String,
        schedule: EventScheduleDto,
        occurred_at: String,
    },
    UpdateTask {
        task_id: String,
        expected_revision: u64,
        title: String,
        deadline: Option<String>,
        priority: PriorityDto,
        occurred_at: String,
    },
    UpdateNote {
        note_id: String,
        expected_revision: u64,
        content: String,
        occurred_at: String,
    },
    SetTaskCompletion {
        task_id: String,
        expected_revision: u64,
        completed: bool,
        occurred_at: String,
    },
    DeleteItem {
        target: DomainRefDto,
        expected_revision: u64,
        occurred_at: String,
    },
}

impl DayMutationDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let invalid_id =
            |value: &str| uuid::Uuid::parse_str(value).map_or(true, |value| value.is_nil());
        match self {
            Self::ImportCalendarSources { connection_id, .. }
            | Self::ImportCalendar { connection_id, .. }
            | Self::CalendarFailed { connection_id, .. }
                if connection_id.trim().is_empty() =>
            {
                return Err("command.connection_id");
            }
            Self::ClassifyCapture { capture_id, .. } if invalid_id(capture_id) => {
                return Err("command.capture_id");
            }
            Self::UpdateEvent { event_id, .. } if invalid_id(event_id) => {
                return Err("command.event_id");
            }
            Self::UpdateTask { task_id, .. } | Self::SetTaskCompletion { task_id, .. }
                if invalid_id(task_id) =>
            {
                return Err("command.task_id");
            }
            Self::UpdateNote { note_id, .. } if invalid_id(note_id) => {
                return Err("command.note_id");
            }
            Self::DeleteItem { target, .. } => {
                let (DomainRefDto::Event { id }
                | DomainRefDto::Task { id }
                | DomainRefDto::Note { id }) = target;
                if invalid_id(id) {
                    return Err("command.target");
                }
            }
            _ => {}
        }
        let revision = match self {
            Self::ClassifyCapture {
                expected_revision, ..
            }
            | Self::UpdateEvent {
                expected_revision, ..
            }
            | Self::UpdateTask {
                expected_revision, ..
            }
            | Self::UpdateNote {
                expected_revision, ..
            }
            | Self::SetTaskCompletion {
                expected_revision, ..
            }
            | Self::DeleteItem {
                expected_revision, ..
            } => Some(*expected_revision),
            _ => None,
        };
        if revision.is_some_and(|revision| revision > i64::MAX as u64) {
            return Err("command.revision");
        }
        if serde_json::to_vec(self).map_or(true, |value| value.len() > 1_048_576) {
            return Err("command.mutation");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DayMutationDto;
    use serde_json::json;

    #[test]
    fn day_wire_rejects_removed_source_configuration_commands() {
        for mutation in [
            json!({"type": "disconnect_calendar", "expected_revision": 1}),
            json!({
                "type": "set_calendar_scope",
                "connection_id": "old",
                "connection_revision": 1,
                "provider": "event_kit",
                "calendars": [],
                "scope": "selected"
            }),
            json!({"type": "discover_calendars", "expected_revision": 1, "calendars": []}),
        ] {
            assert!(serde_json::from_value::<DayMutationDto>(mutation).is_err());
        }
    }

    #[test]
    fn day_import_carries_source_identity_and_mirror_cas_only() {
        let mutation = json!({
            "type": "import_calendar_sources",
            "connection_id": "calendar-source",
            "expected_mirror_revision": null,
            "range": {
                "start_date": "2026-09-05",
                "end_date_exclusive": "2026-09-06",
                "timezone_offset_seconds": 0,
                "end_timezone_offset_seconds": null
            },
            "batches": [],
            "occurred_at": "2026-09-05T00:00:00Z"
        });
        let decoded: DayMutationDto = serde_json::from_value(mutation).unwrap();
        assert!(decoded.validate().is_ok());
        assert!(matches!(
            decoded,
            DayMutationDto::ImportCalendarSources {
                connection_id,
                expected_mirror_revision: None,
                ..
            } if connection_id == "calendar-source"
        ));
    }
}
