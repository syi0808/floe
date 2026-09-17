//! The stored state an action decision is made against.
//!
//! Actions owns proposal, approval, durable pre-dispatch intent and the
//! settlement that follows an external write. It does not own storage, so these
//! tests give it a map: the timeline Day writes, plus the action records and
//! authority the decision reads and writes. Transactions and atomicity are the
//! Vault adapter's tests to make.

// Each integration test binary compiles this module on its own, so a helper only
// some of them need would otherwise read as dead code.
#![allow(dead_code)]

pub mod timeline;

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use floe_actions::{
    ActionAuthority, ActionError, ActionRepository, CalendarAction, CalendarActionState,
};
use floe_day::{
    CalendarConnection, CalendarMirror, DayService, Event, PersonId, TimelineRepository,
};
use uuid::Uuid;

pub use timeline::TestTimelineRepository;

#[derive(Default)]
struct Actions {
    records: HashMap<(PersonId, Uuid), CalendarAction>,
    authority: Option<ActionAuthority>,
    /// Every state an action was durably left in, in order, so a test can ask
    /// what was on record before an external write was attempted.
    history: Vec<(Uuid, CalendarActionState)>,
}

/// A timeline and an action record set that share one person's state.
#[derive(Default)]
pub struct TestActionStore {
    pub timeline: TestTimelineRepository,
    actions: Mutex<Actions>,
}

impl TestActionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn day(&self) -> DayService<'_, TestTimelineRepository> {
        DayService::new(&self.timeline)
    }

    fn actions(&self) -> Result<MutexGuard<'_, Actions>, ActionError> {
        self.actions
            .lock()
            .map_err(|_| ActionError::storage("test action store poisoned"))
    }

    /// The states this action was durably left in, oldest first.
    pub fn recorded_states(&self, id: Uuid) -> Vec<CalendarActionState> {
        self.actions()
            .expect("actions")
            .history
            .iter()
            .filter(|(recorded, _)| *recorded == id)
            .map(|(_, state)| state.clone())
            .collect()
    }
}

impl ActionRepository for TestActionStore {
    async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<CalendarAction>, ActionError> {
        Ok(self
            .actions()?
            .records
            .iter()
            .filter(|((owner, _), _)| *owner == person_id)
            .map(|(_, action)| action.clone())
            .collect())
    }

    async fn calendar_action(
        &self,
        person_id: PersonId,
        id: Uuid,
    ) -> Result<CalendarAction, ActionError> {
        self.actions()?
            .records
            .get(&(person_id, id))
            .cloned()
            .ok_or_else(|| ActionError::not_found("calendar action not found"))
    }

    async fn save_calendar_action(
        &self,
        action: &CalendarAction,
        previous: Option<&CalendarAction>,
    ) -> Result<(), ActionError> {
        let mut actions = self.actions()?;
        let key = (action.person_id, action.id);
        // The store admits the write only against the record the caller read.
        match (actions.records.get(&key), previous) {
            (Some(stored), Some(previous)) if stored != previous => {
                return Err(ActionError::conflict("stale calendar action"));
            }
            (Some(_), None) => {
                return Err(ActionError::conflict("calendar action already exists"));
            }
            (None, Some(_)) => {
                return Err(ActionError::not_found("calendar action not found"));
            }
            _ => {}
        }
        actions.records.insert(key, action.clone());
        actions.history.push((action.id, action.state.clone()));
        Ok(())
    }

    async fn bounded_expert_calendar_action(
        &self,
        person_id: PersonId,
        invocation_id: Uuid,
    ) -> Result<Option<CalendarAction>, ActionError> {
        Ok(self
            .actions()?
            .records
            .iter()
            .find(|((owner, _), action)| {
                *owner == person_id
                    && action
                        .agent_origin
                        .as_ref()
                        .is_some_and(|origin| origin.invocation_id == invocation_id)
            })
            .map(|(_, action)| action.clone()))
    }

    async fn action_authority(
        &self,
        _person_id: PersonId,
    ) -> Result<Option<ActionAuthority>, ActionError> {
        Ok(self.actions()?.authority.clone())
    }

    async fn put_action_authority(&self, authority: &ActionAuthority) -> Result<(), ActionError> {
        self.actions()?.authority = Some(authority.clone());
        Ok(())
    }

    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, ActionError> {
        self.timeline
            .calendar_mirror(person_id)
            .await
            .map_err(|error| ActionError::storage(error.message))
    }

    async fn calendar_connection(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarConnection>, ActionError> {
        Ok(self
            .calendar_mirror(person_id)
            .await?
            .map(|mirror| mirror.connection))
    }

    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, ActionError> {
        self.timeline
            .list_events(person_id)
            .await
            .map_err(|error| ActionError::storage(error.message))
    }
}
