use chrono::{DateTime, Utc};
use floe_day::{CalendarProvider, Event, PersonId, SourceRef, TimedSchedule};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::AgentActionOrigin;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_origin: Option<AgentActionOrigin>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub direct: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation: Option<CalendarMutation>,
    pub id: Uuid,
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub calendar_name: String,
    pub title: String,
    pub schedule: TimedSchedule,
    pub connection_revision: u64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub execution_id: Uuid,
    pub state: CalendarActionState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarMutation {
    pub original: Event,
    pub delete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum CalendarActionState {
    Pending,
    Approved,
    Rejected,
    Executing,
    Blocked { reason: ActionBlockReason },
    Unknown { reason: ActionFailure },
    Succeeded { external_id: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionBlockReason {
    Expired,
    PolicyDenied,
    CalendarChanged,
    PermissionDenied,
    CapabilityUnavailable,
    InvalidTimezone,
    ScheduleConflict,
    ProviderUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionFailure {
    PermissionDenied,
    ProviderUnavailable,
    Timeout,
    UncertainResult,
}

pub struct CalendarActionPolicy {
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub allowed_calendar_ids: Vec<String>,
    pub allow_create: bool,
}

#[derive(Deserialize)]
pub struct CalendarPreflight {
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub can_create: bool,
    pub permission_granted: bool,
    pub timezone_valid: bool,
    pub has_conflict: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CalendarCreateReceipt {
    pub execution_id: Uuid,
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub external_id: String,
    pub title: String,
    pub schedule: TimedSchedule,
}

impl CalendarCreateReceipt {
    pub fn matches(&self, action: &CalendarAction) -> bool {
        self.execution_id == action.execution_id
            && self.person_id == action.person_id
            && self.provider == action.provider
            && self.calendar_id == action.calendar_id
            && !self.external_id.trim().is_empty()
            && action.mutation.as_ref().is_none_or(|mutation| {
                matches!(&mutation.original.source, SourceRef::Calendar(source)
                    if source.external_id == self.external_id)
            })
            && self.title == action.title
            && self.schedule == action.schedule
    }
}
