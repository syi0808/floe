//! The Action services this host composes, and the observation they fence on.
//!
//! Actions owns the review, the durable intent and the dispatch. What the host
//! supplies is the repository behind them and the live observation an external
//! write must still match.

use chrono::{DateTime, Utc};

use floe_actions::{
    ActionAuthority, ActionAuthorityMode, ActionError, ActionService, CalendarAction,
    ExpertActionService, ExpertActionStore, ExpertCalendarInspection, ExpertCalendarRequest,
    ObservationFence,
};
use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_kernel::PersonId;
use floe_vault::TursoStore;

use crate::FloeCore;

impl ObservationFence for FloeCore {
    fn observation(&self, dependency: &ContextDependency) -> Result<String, AgentFailure> {
        self.lease_registry
            .observation(dependency)
            .map(|(_, subject)| subject)
    }
}

impl FloeCore {
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
