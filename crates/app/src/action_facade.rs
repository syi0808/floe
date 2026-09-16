//! The Action services this host composes, and the observation they fence on.
//!
//! Actions owns the review, the durable intent and the dispatch. What the host
//! supplies is the repository behind them and the live observation an external
//! write must still match.

use chrono::{DateTime, Utc};

use floe_actions::{
    ActionAuthority, ActionAuthorityMode, ActionError, ActionErrorCode, ActionService,
    CalendarAction, CalendarActionPolicy, ExpertActionService, ExpertActionStore,
    ExpertCalendarInspection, ExpertCalendarRequest, ObservationFence,
};
use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_day::{CalendarProvider, TimedSchedule};
use floe_kernel::{EventId, PersonId, Revision};
use floe_provider_adapters::sources::native_calendar::{LOCAL_PERSON, NativeCalendar};
use floe_vault::TursoStore;
use uuid::Uuid;

use crate::{CalendarActionsResult, CoreError, ErrorCode, FloeCore};

/// One calendar-action request, as the caller's boundary states it.
///
/// The request names what the Person asked for. Which provider carries it out,
/// and whether this device may create at all, are the host's to resolve.
pub enum CalendarActionCommand {
    Capabilities,
    GetAuthority,
    SetAuthority,
    List,
    Get {
        action_id: Uuid,
    },
    Execute {
        action_id: Uuid,
    },
    Recover {
        action_id: Uuid,
    },
    Propose {
        calendar_id: String,
        title: String,
        schedule: TimedSchedule,
    },
    Direct {
        calendar_id: String,
        title: String,
        schedule: TimedSchedule,
        target: Option<(EventId, Revision)>,
        delete: bool,
    },
    Decide {
        action_id: Uuid,
        approve: bool,
    },
}

/// Restate an Action failure as the failure this host reports.
pub fn action_error(value: ActionError) -> CoreError {
    CoreError {
        code: match value.code {
            ActionErrorCode::Validation => ErrorCode::Validation,
            ActionErrorCode::NotFound => ErrorCode::NotFound,
            ActionErrorCode::Conflict => ErrorCode::Conflict,
            ActionErrorCode::Storage => ErrorCode::Storage,
        },
        message: value.message,
        metadata: value.metadata,
    }
}

fn device_bound(person_id: PersonId, what: &str) -> Result<(), CoreError> {
    if person_id.to_string() == LOCAL_PERSON {
        Ok(())
    } else {
        Err(CoreError::new(
            ErrorCode::Validation,
            format!("{what} Calendar is bound to this device's Person"),
        ))
    }
}

impl ObservationFence for FloeCore {
    fn observation(&self, dependency: &ContextDependency) -> Result<String, AgentFailure> {
        self.lease_registry
            .observation(dependency)
            .map(|(_, subject)| subject)
    }
}

impl FloeCore {
    /// Carry out one calendar-action request for this Person.
    ///
    /// Actions owns the review, the authority and the dispatch; what the host
    /// adds is the provider this device actually writes through and the
    /// connection that says which calendars it may touch.
    pub async fn calendar_action_command(
        &self,
        person_id: PersonId,
        command: CalendarActionCommand,
        now: DateTime<Utc>,
    ) -> Result<CalendarActionsResult, CoreError> {
        let actions = self.actions();
        let single = |action: CalendarAction| CalendarActionsResult {
            actions: vec![action],
            writes_enabled: None,
            authority: None,
        };
        match command {
            CalendarActionCommand::Capabilities => Ok(CalendarActionsResult {
                actions: vec![],
                writes_enabled: Some(
                    person_id.to_string() == LOCAL_PERSON && NativeCalendar::enabled(),
                ),
                authority: None,
            }),
            CalendarActionCommand::GetAuthority => Ok(CalendarActionsResult {
                actions: vec![],
                writes_enabled: None,
                authority: Some(
                    actions
                        .action_authority(person_id)
                        .await
                        .map_err(action_error)?,
                ),
            }),
            // Raising this Person's own authority is not something a caller
            // may ask for through the action path.
            CalendarActionCommand::SetAuthority => Err(CoreError::new(
                ErrorCode::Validation,
                "action authority cannot be set through this request",
            )),
            CalendarActionCommand::List => Ok(CalendarActionsResult {
                actions: actions
                    .calendar_actions(person_id)
                    .await
                    .map_err(action_error)?,
                writes_enabled: None,
                authority: None,
            }),
            CalendarActionCommand::Get { action_id } => actions
                .calendar_action(person_id, action_id)
                .await
                .map(single)
                .map_err(action_error),
            CalendarActionCommand::Propose {
                calendar_id,
                title,
                schedule,
            } => actions
                .propose_calendar_action(person_id, calendar_id, title, schedule, now)
                .await
                .map(single)
                .map_err(action_error),
            CalendarActionCommand::Direct {
                calendar_id,
                title,
                schedule,
                target,
                delete,
            } => {
                device_bound(person_id, "direct")?;
                actions
                    .direct_calendar_action(
                        person_id,
                        calendar_id,
                        title,
                        schedule,
                        target,
                        delete,
                        now,
                    )
                    .await
                    .map(single)
                    .map_err(action_error)
            }
            CalendarActionCommand::Decide { action_id, approve } => actions
                .decide_calendar_action(person_id, action_id, approve, now)
                .await
                .map(single)
                .map_err(action_error),
            CalendarActionCommand::Recover { action_id } => {
                device_bound(person_id, "native")?;
                let provider = self.native_calendar(person_id).await?;
                actions
                    .recover_calendar_action(person_id, action_id, &provider)
                    .await
                    .map(single)
                    .map_err(action_error)
            }
            CalendarActionCommand::Execute { action_id } => {
                device_bound(person_id, "native")?;
                let provider = self.native_calendar(person_id).await?;
                let authority = actions
                    .action_authority(person_id)
                    .await
                    .map_err(action_error)?;
                let action = actions
                    .calendar_action(person_id, action_id)
                    .await
                    .map_err(action_error)?;
                let policy = CalendarActionPolicy::for_execution(
                    &action,
                    &authority,
                    CalendarProvider::EventKit,
                    provider.calendar_ids.clone(),
                    NativeCalendar::enabled(),
                );
                actions
                    .execute_calendar_action(person_id, action_id, &policy, &provider, || now)
                    .await
                    .map(single)
                    .map_err(action_error)
            }
        }
    }

    /// The device calendar this Person writes through, scoped to the calendars
    /// their connection actually selected.
    async fn native_calendar(&self, person_id: PersonId) -> Result<NativeCalendar, CoreError> {
        let connection = self.calendar_connection(person_id).await?;
        Ok(NativeCalendar::new(
            connection
                .map(|connection| {
                    connection
                        .calendars
                        .into_iter()
                        .map(|calendar| calendar.calendar_id)
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }

    pub fn actions(&self) -> ActionService<'_, TursoStore> {
        ActionService::new(&self.store)
    }

    pub fn expert_actions(&self) -> ExpertActionService<'_, TursoStore, Self> {
        ExpertActionService::new(&self.store, self)
    }

    pub async fn inspect_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        request: ExpertCalendarInspection,
    ) -> Result<Option<CalendarAction>, AgentFailure> {
        self.expert_actions()
            .inspect_expert_calendar_action(store, request)
            .await
    }

    pub async fn prepare_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        request: ExpertCalendarRequest,
        clock: impl Fn() -> DateTime<Utc>,
    ) -> Result<CalendarAction, AgentFailure> {
        self.expert_actions()
            .prepare_expert_calendar_action(store, request, clock)
            .await
    }

    pub async fn decide_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: uuid::Uuid,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, AgentFailure> {
        self.expert_actions()
            .decide_expert_calendar_action(store, person_id, execution_id, approve, now)
            .await
    }

    pub async fn recover_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: uuid::Uuid,
        provider: &impl floe_actions::CalendarActionProvider,
    ) -> Result<CalendarAction, AgentFailure> {
        self.expert_actions()
            .recover_expert_calendar_action(store, person_id, execution_id, provider)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_expert_calendar_action_with_cancellation(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: uuid::Uuid,
        policy: &floe_actions::CalendarActionPolicy,
        provider: &impl floe_actions::CalendarActionProvider,
        clock: impl Fn() -> DateTime<Utc>,
        cancellation: floe_execution::Cancellation,
    ) -> Result<CalendarAction, AgentFailure> {
        self.expert_actions()
            .execute_expert_calendar_action_with_cancellation(
                store,
                person_id,
                execution_id,
                policy,
                provider,
                clock,
                cancellation,
            )
            .await
    }

    pub async fn set_action_authority(
        &self,
        person_id: PersonId,
        calendar_create: ActionAuthorityMode,
    ) -> Result<ActionAuthority, ActionError> {
        self.actions()
            .set_action_authority(person_id, calendar_create)
            .await
    }

    /// The connector view of this Person's calendar, as this device sees it.
    pub async fn calendar_connector_snapshot(
        &self,
        person_id: PersonId,
        device_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<floe_connections::ConnectorSnapshot>, AgentFailure> {
        floe_connections::validate_connector_device(device_id)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let Some(mirror) = floe_day::TimelineRepository::calendar_mirror(&self.store, person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        else {
            return Ok(None);
        };
        floe_connections::project_calendar_connector(&mirror, device_id, now)
            .map(Some)
            .map_err(|_| AgentFailure::StorageUnavailable)
    }
}
