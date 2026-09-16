//! Durable projection for Actions: proposals, approvals and settled outcomes.

use floe_actions::{
    ActionAuthority, ActionError, ActionErrorCode, ActionRepository, CalendarAction,
};
use floe_agent_contract::AgentFailure;
use floe_day::{CalendarConnection, CalendarMirror, Event, PersonId};
use serde_json::to_string;
use uuid::Uuid;

use crate::engine::storage_error;
use crate::{StoreError, StoreErrorCode, TursoStore};

impl TursoStore {
    pub(crate) async fn action_authority(
        &self,
        person_id: PersonId,
    ) -> Result<Option<floe_actions::ActionAuthority>, StoreError> {
        self.get("action_authorities", person_id.to_string()).await
    }

    pub(crate) async fn put_action_authority(
        &self,
        authority: &floe_actions::ActionAuthority,
    ) -> Result<(), StoreError> {
        self.put(
            "action_authorities",
            authority.person_id.to_string(),
            authority.person_id,
            authority,
        )
        .await
    }

    pub(crate) async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<floe_actions::CalendarAction>, StoreError> {
        let mut actions: Vec<floe_actions::CalendarAction> =
            self.list("calendar_actions", person_id).await?;
        actions.sort_by_key(|action| (std::cmp::Reverse(action.created_at), action.id));
        Ok(actions)
    }

    pub(crate) async fn calendar_action(
        &self,
        person_id: PersonId,
        id: uuid::Uuid,
    ) -> Result<floe_actions::CalendarAction, StoreError> {
        let action: floe_actions::CalendarAction = self
            .get("calendar_actions", id.to_string())
            .await?
            .ok_or_else(|| StoreError::new(StoreErrorCode::NotFound, "calendar action not found"))?;
        if action.person_id != person_id {
            return Err(StoreError::new(
                StoreErrorCode::NotFound,
                "calendar action not found",
            ));
        }
        Ok(action)
    }

    pub(crate) async fn bounded_expert_calendar_action(
        &self,
        person_id: PersonId,
        id: uuid::Uuid,
    ) -> Result<Option<floe_actions::CalendarAction>, floe_agent_contract::AgentFailure> {
        use floe_agent_contract::AgentFailure;
        let connection = self
            .connection()
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let mut rows = connection.query(
            "SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE NULL END FROM calendar_actions WHERE id = ? AND person_id = ?",
            (id.to_string(), person_id.to_string()),
        ).await.map_err(|_| AgentFailure::StorageUnavailable)?;
        let Some(row) = rows
            .next()
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        else {
            return Ok(None);
        };
        if row
            .get::<i64>(0)
            .map_err(|_| AgentFailure::StorageUnavailable)?
            > 65_536
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let action: floe_actions::CalendarAction = serde_json::from_str(
            &row.get::<String>(1)
                .map_err(|_| AgentFailure::StorageUnavailable)?,
        )
        .map_err(|_| AgentFailure::StorageUnavailable)?;
        if action.person_id != person_id || action.id != id {
            return Err(AgentFailure::Conflict);
        }
        Ok(Some(action))
    }

    pub(crate) async fn save_calendar_action(
        &self,
        action: &floe_actions::CalendarAction,
        previous: Option<&floe_actions::CalendarAction>,
    ) -> Result<(), StoreError> {
        let connection = self.connection().await?;
        let payload = to_string(action).map_err(storage_error)?;
        let changed = if let Some(previous) = previous {
            connection.execute(
                "UPDATE calendar_actions SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?",
                (payload, action.id.to_string(), action.person_id.to_string(), to_string(previous).map_err(storage_error)?),
            ).await.map_err(storage_error)?
        } else {
            connection.execute(
                "INSERT OR IGNORE INTO calendar_actions(id, person_id, payload) VALUES (?, ?, ?)",
                (action.id.to_string(), action.person_id.to_string(), payload),
            ).await.map_err(storage_error)?
        };
        if changed != 1 {
            return Err(StoreError::new(
                StoreErrorCode::Conflict,
                "calendar action changed; reload its status",
            ));
        }
        Ok(())
    }
}

impl ActionRepository for TursoStore {
    async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<CalendarAction>, ActionError> {
        TursoStore::calendar_actions(self, person_id)
            .await
            .map_err(action_error)
    }

    async fn calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
    ) -> Result<CalendarAction, ActionError> {
        TursoStore::calendar_action(self, person_id, id)
            .await
            .map_err(action_error)
    }

    async fn save_calendar_action(
        &self,
        action: &CalendarAction,
        previous: Option<&CalendarAction>,
    ) -> Result<(), ActionError> {
        TursoStore::save_calendar_action(self, action, previous)
            .await
            .map_err(action_error)
    }

    async fn bounded_expert_calendar_action(
        &self,
        person_id: PersonId,
        invocation_id: Uuid,
    ) -> Result<Option<CalendarAction>, ActionError> {
        TursoStore::bounded_expert_calendar_action(self, person_id, invocation_id)
            .await
            .map_err(|failure| match failure {
                AgentFailure::Conflict => ActionError::conflict("calendar action changed"),
                AgentFailure::BudgetExceeded => {
                    ActionError::validation("calendar action exceeds the bounded read")
                }
                _ => ActionError::storage("calendar action is unavailable"),
            })
    }

    async fn action_authority(
        &self,
        person_id: PersonId,
    ) -> Result<Option<ActionAuthority>, ActionError> {
        TursoStore::action_authority(self, person_id)
            .await
            .map_err(action_error)
    }

    async fn put_action_authority(&self, authority: &ActionAuthority) -> Result<(), ActionError> {
        TursoStore::put_action_authority(self, authority)
            .await
            .map_err(action_error)
    }

    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, ActionError> {
        TursoStore::calendar_mirror(self, person_id)
            .await
            .map_err(action_error)
    }

    async fn calendar_connection(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarConnection>, ActionError> {
        Ok(TursoStore::calendar_mirror(self, person_id)
            .await
            .map_err(action_error)?
            .map(|mirror| mirror.connection))
    }

    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, ActionError> {
        TursoStore::list_events(self, person_id)
            .await
            .map_err(action_error)
    }
}

fn action_error(error: StoreError) -> ActionError {
    let code = match error.code {
        StoreErrorCode::Validation | StoreErrorCode::NoFocusSlot => ActionErrorCode::Validation,
        StoreErrorCode::NotFound => ActionErrorCode::NotFound,
        StoreErrorCode::Conflict => ActionErrorCode::Conflict,
        StoreErrorCode::Storage => ActionErrorCode::Storage,
    };
    let mut result = ActionError::new(code, error.message);
    result.metadata = error.metadata;
    result
}
