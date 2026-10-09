//! Manual commands for calendars owned by an external provider.
//!
//! Day defines this inward port contract. The Calendar Operations owner
//! implements it and retains responsibility for effect identity, authorization,
//! dispatch, receipts and reconciliation.
use floe_kernel::{EventId, Revision};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManualCalendarOperation {
    Create {
        destination_ref: Uuid,
        title: String,
        schedule: crate::TimedSchedule,
    },
    Update {
        event_ref: EventId,
        expected_revision: Revision,
        title: String,
        schedule: crate::TimedSchedule,
    },
    Delete {
        event_ref: EventId,
        expected_revision: Revision,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualCalendarOperationStatus {
    Pending,
    Executing,
    Blocked,
    NotApplied,
    Unknown,
    Succeeded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCalendarDestination {
    pub destination_ref: Uuid,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCalendarOperationReceipt {
    pub operation_id: Uuid,
    pub revision: u64,
    pub status: ManualCalendarOperationStatus,
    pub collection_pending: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCalendarOperationPage {
    pub operations: Vec<ManualCalendarOperationReceipt>,
    pub next_cursor: Option<Uuid>,
}

impl ManualCalendarOperationPage {
    pub fn validate(&self, limit: u16) -> Result<(), floe_kernel::AgentFailure> {
        if !(1..=100).contains(&limit)
            || self.operations.len() > usize::from(limit)
            || self.next_cursor.is_some_and(|cursor| cursor.is_nil())
        {
            return Err(floe_kernel::AgentFailure::InvalidInput);
        }
        let mut seen = std::collections::HashSet::new();
        for operation in &self.operations {
            operation.validate()?;
            if !seen.insert(operation.operation_id) {
                return Err(floe_kernel::AgentFailure::InvalidInput);
            }
        }
        if self.next_cursor.is_some_and(|cursor| {
            self.operations
                .last()
                .is_none_or(|last| last.operation_id != cursor)
        }) {
            return Err(floe_kernel::AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

impl ManualCalendarOperationReceipt {
    pub fn validate(&self) -> Result<(), floe_kernel::AgentFailure> {
        if self.operation_id.is_nil() || self.revision == 0 {
            return Err(floe_kernel::AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            ManualCalendarOperationStatus::Blocked
                | ManualCalendarOperationStatus::NotApplied
                | ManualCalendarOperationStatus::Succeeded
        ) && !(self.status == ManualCalendarOperationStatus::Succeeded && self.collection_pending)
    }
}
