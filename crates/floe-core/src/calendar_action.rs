use std::future::Future;

use chrono::{DateTime, Duration, Utc};
use floe_domain::{CalendarProvider, Event, PersonId, TimedSchedule};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CoreError, ErrorCode, FloeCore};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarAction {
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

pub trait CalendarActionProvider {
    fn preflight(
        &self,
        action: &CalendarAction,
        local_events: &[Event],
    ) -> impl Future<Output = Result<CalendarPreflight, ActionFailure>> + Send;

    fn create(
        &self,
        action: &CalendarAction,
    ) -> impl Future<Output = Result<CalendarCreateReceipt, ActionFailure>> + Send;

    fn lookup(
        &self,
        action: &CalendarAction,
    ) -> impl Future<Output = Result<Vec<CalendarCreateReceipt>, ActionFailure>> + Send;
}

impl FloeCore {
    pub async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<CalendarAction>, CoreError> {
        self.store.calendar_actions(person_id).await
    }

    pub async fn propose_calendar_action(
        &self,
        person_id: PersonId,
        calendar_id: String,
        title: String,
        schedule: TimedSchedule,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, CoreError> {
        TimedSchedule::new(schedule.starts_at, schedule.ends_at, &schedule.timezone)?;
        if title.trim().is_empty() || schedule.starts_at <= now {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "provide a title and future interval",
            ));
        }
        let mirror = self
            .store
            .calendar_mirror(person_id)
            .await?
            .ok_or_else(|| CoreError::new(ErrorCode::NotFound, "connect a calendar first"))?;
        let calendar = mirror
            .connection
            .selected_calendars()
            .into_iter()
            .find(|calendar| calendar.calendar_id == calendar_id)
            .ok_or_else(|| CoreError::new(ErrorCode::Validation, "calendar is not connected"))?;
        let action = CalendarAction {
            id: Uuid::new_v4(),
            person_id,
            provider: mirror.connection.provider,
            calendar_id,
            calendar_name: calendar.calendar_name,
            title: title.trim().to_owned(),
            expires_at: (now + Duration::minutes(15)).min(schedule.starts_at),
            schedule,
            connection_revision: mirror.connection.revision,
            created_at: now,
            approved_at: None,
            execution_id: Uuid::new_v4(),
            state: CalendarActionState::Pending,
        };
        self.store.save_calendar_action(&action, None).await?;
        Ok(action)
    }

    pub async fn calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
    ) -> Result<CalendarAction, CoreError> {
        self.store.calendar_action(person_id, id).await
    }

    pub async fn decide_calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, CoreError> {
        let action = self.calendar_action(person_id, id).await?;
        if action.state != CalendarActionState::Pending {
            return Err(conflict());
        }
        let mut updated = action.clone();
        updated.state = if !approve {
            CalendarActionState::Rejected
        } else if now < action.created_at || now >= action.expires_at {
            CalendarActionState::Blocked {
                reason: ActionBlockReason::Expired,
            }
        } else {
            updated.approved_at = Some(now);
            CalendarActionState::Approved
        };
        self.store
            .save_calendar_action(&updated, Some(&action))
            .await?;
        Ok(updated)
    }

    pub async fn execute_calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
        policy: &CalendarActionPolicy,
        provider: &impl CalendarActionProvider,
        clock: impl Fn() -> DateTime<Utc>,
    ) -> Result<CalendarAction, CoreError> {
        let action = self.calendar_action(person_id, id).await?;
        if action.state != CalendarActionState::Approved {
            return Err(conflict());
        }
        let executing = self
            .transition(&action, CalendarActionState::Executing)
            .await?;
        if let Some(reason) = self
            .action_block_reason(&executing, policy, clock())
            .await?
        {
            return self
                .transition(&executing, CalendarActionState::Blocked { reason })
                .await;
        }
        let local_events = self.store.list_events(person_id).await?;
        let preflight = provider.preflight(&executing, &local_events).await;
        let reason = match preflight {
            Err(ActionFailure::PermissionDenied) => Some(ActionBlockReason::PermissionDenied),
            Err(_) => Some(ActionBlockReason::ProviderUnavailable),
            Ok(check)
                if check.person_id != person_id
                    || check.provider != executing.provider
                    || check.calendar_id != executing.calendar_id =>
            {
                Some(ActionBlockReason::CalendarChanged)
            }
            Ok(check) if !check.permission_granted => Some(ActionBlockReason::PermissionDenied),
            Ok(check) if !check.can_create => Some(ActionBlockReason::CapabilityUnavailable),
            Ok(check) if !check.timezone_valid => Some(ActionBlockReason::InvalidTimezone),
            Ok(check) if check.has_conflict => Some(ActionBlockReason::ScheduleConflict),
            Ok(_) => {
                self.action_block_reason(&executing, policy, clock())
                    .await?
            }
        };
        if let Some(reason) = reason {
            return self
                .transition(&executing, CalendarActionState::Blocked { reason })
                .await;
        }
        if self.store.list_events(person_id).await? != local_events {
            return self
                .transition(
                    &executing,
                    CalendarActionState::Blocked {
                        reason: ActionBlockReason::CalendarChanged,
                    },
                )
                .await;
        }
        let dispatch_time = clock();
        if dispatch_time < executing.approved_at.unwrap_or(executing.created_at)
            || dispatch_time >= executing.expires_at
        {
            return self
                .transition(
                    &executing,
                    CalendarActionState::Blocked {
                        reason: ActionBlockReason::Expired,
                    },
                )
                .await;
        }
        let state = match provider.create(&executing).await {
            Ok(receipt) if receipt.matches(&executing) => CalendarActionState::Succeeded {
                external_id: receipt.external_id,
            },
            Ok(_) => CalendarActionState::Unknown {
                reason: ActionFailure::UncertainResult,
            },
            Err(reason) => CalendarActionState::Unknown { reason },
        };
        self.transition(&executing, state).await
    }

    pub async fn recover_calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
        provider: &impl CalendarActionProvider,
    ) -> Result<CalendarAction, CoreError> {
        let action = self.calendar_action(person_id, id).await?;
        if !matches!(
            action.state,
            CalendarActionState::Executing | CalendarActionState::Unknown { .. }
        ) {
            return Err(conflict());
        }
        let state = match provider.lookup(&action).await {
            Ok(receipts) if receipts.len() == 1 && receipts[0].matches(&action) => {
                CalendarActionState::Succeeded {
                    external_id: receipts[0].external_id.clone(),
                }
            }
            Ok(_) => CalendarActionState::Unknown {
                reason: ActionFailure::UncertainResult,
            },
            Err(reason) => CalendarActionState::Unknown { reason },
        };
        self.transition(&action, state).await
    }

    async fn action_block_reason(
        &self,
        action: &CalendarAction,
        policy: &CalendarActionPolicy,
        now: DateTime<Utc>,
    ) -> Result<Option<ActionBlockReason>, CoreError> {
        if now < action.approved_at.unwrap_or(action.created_at) || now >= action.expires_at {
            return Ok(Some(ActionBlockReason::Expired));
        }
        if !policy.allow_create
            || policy.person_id != action.person_id
            || policy.provider != action.provider
            || !policy.allowed_calendar_ids.contains(&action.calendar_id)
        {
            return Ok(Some(ActionBlockReason::PolicyDenied));
        }
        let mirror = self.store.calendar_mirror(action.person_id).await?;
        if mirror.is_none_or(|mirror| {
            mirror.connection.revision != action.connection_revision
                || mirror.connection.disconnected
                || mirror
                    .connection
                    .source_statuses
                    .get(&action.calendar_id)
                    .is_some_and(|status| status.error.is_some())
                || mirror.connection.provider != action.provider
                || mirror.connection.error.is_some()
                || !mirror
                    .connection
                    .selected_calendars()
                    .iter()
                    .any(|calendar| calendar.calendar_id == action.calendar_id)
        }) {
            return Ok(Some(ActionBlockReason::CalendarChanged));
        }
        Ok(None)
    }

    async fn transition(
        &self,
        previous: &CalendarAction,
        state: CalendarActionState,
    ) -> Result<CalendarAction, CoreError> {
        let mut updated = previous.clone();
        updated.state = state;
        self.store
            .save_calendar_action(&updated, Some(previous))
            .await?;
        Ok(updated)
    }
}

impl CalendarCreateReceipt {
    fn matches(&self, action: &CalendarAction) -> bool {
        self.execution_id == action.execution_id
            && self.person_id == action.person_id
            && self.provider == action.provider
            && self.calendar_id == action.calendar_id
            && !self.external_id.trim().is_empty()
            && self.title == action.title
            && self.schedule == action.schedule
    }
}

fn conflict() -> CoreError {
    CoreError::new(
        ErrorCode::Conflict,
        "action cannot transition; reload status or propose again",
    )
}
