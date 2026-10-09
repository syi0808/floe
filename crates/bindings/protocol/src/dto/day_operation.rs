use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::OperationRefDto;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCalendarDestinationDto {
    pub destination_ref: super::UuidRefDto,
    pub label: String,
}

impl ManualCalendarDestinationDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.label.is_empty()
            || self.label.trim() != self.label
            || self.label.len() > 512
            || self.label.chars().any(char::is_control)
        {
            Err("day.calendar_destination.label")
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManualCalendarOperationDto {
    Create {
        destination_ref: String,
        title: String,
        schedule: super::TimedScheduleDto,
    },
    Update {
        event_ref: String,
        expected_revision: u64,
        title: String,
        schedule: super::TimedScheduleDto,
    },
    Delete {
        event_ref: String,
        expected_revision: u64,
    },
}

impl ManualCalendarOperationDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let valid_id = |value: &str| Uuid::parse_str(value).is_ok_and(|parsed| !parsed.is_nil());
        match self {
            Self::Create {
                destination_ref,
                title,
                schedule,
            } => {
                if !valid_id(destination_ref) {
                    return Err("command.destination_ref");
                }
                validate_title(title)?;
                schedule.validate_new_action()
            }
            Self::Update {
                event_ref,
                expected_revision,
                title,
                schedule,
            } => {
                if !valid_id(event_ref) {
                    return Err("command.event_ref");
                }
                validate_revision(*expected_revision)?;
                validate_title(title)?;
                schedule.validate_new_action()
            }
            Self::Delete {
                event_ref,
                expected_revision,
            } => {
                if !valid_id(event_ref) {
                    return Err("command.event_ref");
                }
                validate_revision(*expected_revision)
            }
        }
    }
}

fn validate_revision(revision: u64) -> Result<(), &'static str> {
    if revision == 0 || revision > i64::MAX as u64 {
        Err("command.expected_revision")
    } else {
        Ok(())
    }
}

fn validate_title(title: &str) -> Result<(), &'static str> {
    if title.trim().is_empty()
        || title.trim() != title
        || title.len() > 1024
        || title.chars().any(char::is_control)
    {
        Err("command.title")
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualCalendarOperationStatusDto {
    Pending,
    Executing,
    Blocked,
    NotApplied,
    Unknown,
    Succeeded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCalendarOperationReceiptDto {
    pub operation_ref: OperationRefDto,
    pub revision: u64,
    pub status: ManualCalendarOperationStatusDto,
    pub collection_pending: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCalendarOperationsDto {
    pub operations: Vec<ManualCalendarOperationReceiptDto>,
    pub next_cursor: Option<OperationRefDto>,
}

impl ManualCalendarOperationsDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.operations.len() > 100 {
            return Err("day.calendar_operations.limit");
        }
        let mut seen = std::collections::HashSet::new();
        for operation in &self.operations {
            if operation.revision == 0 || !seen.insert(operation.operation_ref) {
                return Err("day.calendar_operations.operation");
            }
        }
        if self.next_cursor.is_some_and(|cursor| {
            self.operations
                .last()
                .is_none_or(|last| last.operation_ref != cursor)
        }) {
            return Err("day.calendar_operations.cursor");
        }
        Ok(())
    }
}
