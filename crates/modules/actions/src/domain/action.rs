use chrono::{DateTime, Utc};
use floe_day::{Event, PersonId, SourceRef, TimedSchedule};
use floe_context_contract::CalendarProvider;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{ActionAuthority, ActionAuthorityMode, AgentActionOrigin};

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

impl CalendarAction {
    /// That this stored action is one the Person may review as an Expert's
    /// proposal.
    ///
    /// Only an Expert's own proposal, raised for this Person, is a proposal to
    /// decide on; what the Person did directly is already theirs.
    pub fn admit_expert_proposal(
        &self,
        person_id: PersonId,
    ) -> Result<(), floe_agent_contract::AgentFailure> {
        if self.person_id != person_id || self.agent_origin.is_none() || self.direct {
            return Err(floe_agent_contract::AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    /// The policy this Expert proposal executes under.
    ///
    /// A proposal may touch only the calendar it named, on the provider it named
    /// and for the Person it was raised for.
    pub fn expert_proposal_policy(&self, provider_can_write: bool) -> CalendarActionPolicy {
        CalendarActionPolicy {
            person_id: self.person_id,
            provider: self.provider,
            allowed_calendar_ids: vec![self.calendar_id.clone()],
            allow_create: provider_can_write,
        }
    }
}

impl CalendarActionPolicy {
    /// The policy one action executes under.
    ///
    /// Creating on the Person's calendar needs a provider that can write at
    /// all, and then either the Person's standing authority or their own direct
    /// instruction for this very action. A proposal an Expert raised carries no
    /// such instruction, so it stands or falls on the standing authority alone.
    pub fn for_execution(
        action: &CalendarAction,
        authority: &ActionAuthority,
        provider: CalendarProvider,
        allowed_calendar_ids: Vec<String>,
        provider_can_write: bool,
    ) -> Self {
        Self {
            person_id: action.person_id,
            provider,
            allowed_calendar_ids,
            allow_create: provider_can_write
                && (action.direct || authority.calendar_create != ActionAuthorityMode::Deny),
        }
    }
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
