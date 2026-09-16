use chrono::{DateTime, Duration, Utc};
use floe_day::{EventId, EventSchedule, PersonId, Revision, SourceRef, TimedSchedule};
use uuid::Uuid;

use crate::{ActionAuthority, ActionAuthorityMode, ActionBlockReason, ActionError, ActionFailure, ActionRepository, CalendarAction, CalendarActionPolicy, CalendarActionProvider, CalendarActionState, CalendarMutation};

/// Owner of proposal admission, approval decisions, external dispatch and the
/// uncertain-result recovery path for calendar writes.
pub struct ActionService<'a, Repository: ActionRepository + ?Sized> {
    pub(crate) repository: &'a Repository,
}

impl<'a, Repository: ActionRepository + ?Sized> ActionService<'a, Repository> {
    pub fn new(repository: &'a Repository) -> Self {
        Self { repository }
    }

    pub async fn action_authority(
        &self,
        person_id: PersonId,
    ) -> Result<ActionAuthority, ActionError> {
        Ok(self
            .repository
            .action_authority(person_id)
            .await?
            .unwrap_or_else(|| ActionAuthority::default_for(person_id)))
    }

    pub async fn set_action_authority(
        &self,
        person_id: PersonId,
        calendar_create: ActionAuthorityMode,
    ) -> Result<ActionAuthority, ActionError> {
        let authority = ActionAuthority {
            person_id,
            calendar_create,
        };
        self.repository.put_action_authority(&authority).await?;
        Ok(authority)
    }

    pub async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<CalendarAction>, ActionError> {
        self.repository.calendar_actions(person_id).await
    }

    pub async fn calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
    ) -> Result<CalendarAction, ActionError> {
        self.repository.calendar_action(person_id, id).await
    }

    pub async fn propose_calendar_action(
        &self,
        person_id: PersonId,
        calendar_id: String,
        title: String,
        schedule: TimedSchedule,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, ActionError> {
        let action = self
            .draft_calendar_action(person_id, calendar_id, title, schedule, now)
            .await?;
        self.repository.save_calendar_action(&action, None).await?;
        Ok(action)
    }

    pub async fn draft_calendar_action(
        &self,
        person_id: PersonId,
        calendar_id: String,
        title: String,
        schedule: TimedSchedule,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, ActionError> {
        TimedSchedule::new(schedule.starts_at, schedule.ends_at, &schedule.timezone)
            .map_err(|error| ActionError::validation(error.to_string()))?;
        if title.trim().is_empty() || schedule.starts_at <= now {
            return Err(ActionError::validation(
                "provide a title and future interval",
            ));
        }
        let mirror = self
            .repository
            .calendar_mirror(person_id)
            .await?
            .ok_or_else(|| ActionError::not_found("connect a calendar first"))?;
        let calendar = mirror
            .connection
            .calendars
            .into_iter()
            .find(|calendar| calendar.calendar_id == calendar_id)
            .ok_or_else(|| ActionError::validation("calendar is not connected"))?;
        let action = CalendarAction {
            agent_origin: None,
            direct: false,
            mutation: None,
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
        Ok(action)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn direct_calendar_action(
        &self,
        person_id: PersonId,
        calendar_id: String,
        title: String,
        schedule: TimedSchedule,
        event_id: Option<(EventId, Revision)>,
        delete: bool,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, ActionError> {
        TimedSchedule::new(schedule.starts_at, schedule.ends_at, &schedule.timezone)
            .map_err(|error| ActionError::validation(error.to_string()))?;
        if title.trim().is_empty() || schedule.ends_at - schedule.starts_at > Duration::hours(24) {
            return Err(ActionError::validation("invalid event interval or title"));
        }
        let mirror = self
            .repository
            .calendar_mirror(person_id)
            .await?
            .ok_or_else(|| ActionError::not_found("connect a calendar first"))?;
        let mutation = if let Some((event_id, revision)) = event_id {
            let original = mirror
                .events
                .iter()
                .find(|event| event.id == event_id && event.deleted_at.is_none())
                .cloned()
                .ok_or_else(|| ActionError::not_found("event not found"))?;
            if original.revision != revision {
                return Err(ActionError::conflict("event changed; reload before editing"));
            }
            if !matches!(&original.source, SourceRef::Calendar(source)
                if source.calendar_id == calendar_id && source.can_modify)
                || !matches!(original.schedule, EventSchedule::Timed(_))
            {
                return Err(ActionError::validation("unsupported event"));
            }
            Some(CalendarMutation { original, delete })
        } else {
            if delete {
                return Err(ActionError::validation("delete requires an event"));
            }
            None
        };
        let calendar = mirror
            .connection
            .calendars
            .into_iter()
            .find(|calendar| calendar.calendar_id == calendar_id)
            .ok_or_else(|| ActionError::validation("calendar is not connected"))?;
        let action = CalendarAction {
            agent_origin: None,
            direct: true,
            mutation,
            id: Uuid::new_v4(),
            person_id,
            provider: mirror.connection.provider,
            calendar_id,
            calendar_name: calendar.calendar_name,
            title: title.trim().to_owned(),
            schedule,
            connection_revision: mirror.connection.revision,
            created_at: now,
            expires_at: now + Duration::minutes(15),
            approved_at: Some(now),
            execution_id: Uuid::new_v4(),
            state: CalendarActionState::Approved,
        };
        self.repository.save_calendar_action(&action, None).await?;
        Ok(action)
    }

    pub async fn decide_calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, ActionError> {
        let action = self.calendar_action(person_id, id).await?;
        if action.agent_origin.is_some() || action.state != CalendarActionState::Pending {
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
        self.repository
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
    ) -> Result<CalendarAction, ActionError> {
        let action = self.calendar_action(person_id, id).await?;
        if action.agent_origin.is_some() || action.state != CalendarActionState::Approved {
            return Err(conflict());
        }
        let executing = self
            .transition(&action, CalendarActionState::Executing)
            .await?;
        if let Some(reason) = self.action_block_reason(&executing, policy, clock()).await? {
            return self
                .transition(&executing, CalendarActionState::Blocked { reason })
                .await;
        }
        let local_events = self.repository.list_events(person_id).await?;
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
            Ok(check)
                if check.has_conflict && !(executing.direct && executing.mutation.is_some()) =>
            {
                Some(ActionBlockReason::ScheduleConflict)
            }
            Ok(_) => self.action_block_reason(&executing, policy, clock()).await?,
        };
        if let Some(reason) = reason {
            return self
                .transition(&executing, CalendarActionState::Blocked { reason })
                .await;
        }
        if self.repository.list_events(person_id).await? != local_events {
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
    ) -> Result<CalendarAction, ActionError> {
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

    pub(crate) async fn action_block_reason(
        &self,
        action: &CalendarAction,
        policy: &CalendarActionPolicy,
        now: DateTime<Utc>,
    ) -> Result<Option<ActionBlockReason>, ActionError> {
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
        if let Some(origin) = &action.agent_origin {
            let authority = self
                .action_authority(action.person_id)
                .await?
                .calendar_create;
            if !origin.valid_for(action)
                || authority == ActionAuthorityMode::Deny
                || (origin.automatic && authority != ActionAuthorityMode::Allow)
            {
                return Ok(Some(ActionBlockReason::PolicyDenied));
            }
        }
        let mirror = self.repository.calendar_mirror(action.person_id).await?;
        if let Some(mutation) = &action.mutation
            && mirror
                .as_ref()
                .is_none_or(|mirror| !mirror.events.contains(&mutation.original))
        {
            return Ok(Some(ActionBlockReason::CalendarChanged));
        }
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
                    .calendars
                    .iter()
                    .any(|calendar| calendar.calendar_id == action.calendar_id)
        }) {
            return Ok(Some(ActionBlockReason::CalendarChanged));
        }
        Ok(None)
    }

    pub(crate) async fn transition(
        &self,
        previous: &CalendarAction,
        state: CalendarActionState,
    ) -> Result<CalendarAction, ActionError> {
        let mut updated = previous.clone();
        updated.state = state;
        self.repository
            .save_calendar_action(&updated, Some(previous))
            .await?;
        Ok(updated)
    }
}

pub(crate) fn conflict() -> ActionError {
    ActionError::conflict("action cannot transition; reload status or propose again")
}
