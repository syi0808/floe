use super::{
    ClassificationDto,
    DomainRefDto, EventScheduleDto, PriorityDto,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayMutationDto {
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
