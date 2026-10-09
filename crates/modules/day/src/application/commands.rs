use crate::{
    DayError, DayRepository, Event, ExternalCalendarOperationPort, ManualCalendarDestination,
    ManualCalendarOperation, ManualCalendarOperationPage, ManualCalendarOperationReceipt, Note,
    Task, TimelineItem,
};
use floe_execution::ExecutionScope;
use floe_kernel::{CommandFailure, OwnerActor, PersonId};
use std::sync::{Arc, RwLock, Weak};

#[derive(Clone)]
pub struct DayService {
    pub(crate) repository: std::sync::Arc<dyn DayRepository>,
    pub(crate) acquisition: std::sync::Arc<dyn crate::CalendarAcquisitionPort>,
    pub(crate) clock: std::sync::Arc<dyn crate::DayClock>,
    pub(crate) lifecycle: std::sync::Arc<super::refresh::DayLifecycle>,
    pub(crate) external_calendar_operations:
        Arc<RwLock<Option<Weak<dyn ExternalCalendarOperationPort>>>>,
}

impl DayService {
    pub fn new(
        repository: std::sync::Arc<dyn DayRepository>,
        acquisition: std::sync::Arc<dyn crate::CalendarAcquisitionPort>,
        clock: std::sync::Arc<dyn crate::DayClock>,
    ) -> Self {
        Self {
            repository,
            acquisition,
            clock,
            lifecycle: std::sync::Arc::new(super::refresh::DayLifecycle::new()),
            external_calendar_operations: Arc::new(RwLock::new(None)),
        }
    }

    /// App composition wires the inward Day port to Calendar Operations. A
    /// weak reference avoids an Arc cycle because Calendar Operations also
    /// reads Day observations while preparing external effects. Replacing the
    /// weak reference lets a newly activated owner generation take over.
    pub fn set_external_calendar_operations(
        &self,
        operations: &Arc<dyn ExternalCalendarOperationPort>,
    ) -> Result<(), DayError> {
        *self
            .external_calendar_operations
            .write()
            .map_err(|_| DayError::storage("Calendar Operations port unavailable"))? =
            Some(Arc::downgrade(operations));
        Ok(())
    }

    /// Execute a direct user instruction through Calendar Operations without
    /// retaining Day's mutation admission lock across an external call.
    pub async fn operate_external_calendar(
        &self,
        actor: &OwnerActor,
        command_id: uuid::Uuid,
        operation: ManualCalendarOperation,
        scope: &ExecutionScope,
    ) -> Result<ManualCalendarOperationReceipt, CommandFailure<DayError>> {
        self.admit_actor(actor)
            .map_err(CommandFailure::NotAdmitted)?;
        if command_id.is_nil() {
            return Err(CommandFailure::NotApplied(DayError::validation(
                "invalid external calendar command identity",
            )));
        }
        let operations = self
            .external_calendar_operations
            .read()
            .map_err(|_| {
                CommandFailure::NotAdmitted(DayError::storage(
                    "Calendar Operations port unavailable",
                ))
            })?
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                CommandFailure::NotAdmitted(DayError::storage("Calendar Operations is unavailable"))
            })?;
        operations
            .execute_manual(actor, command_id, operation, scope)
            .await
    }

    pub async fn external_calendar_destinations(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<Vec<ManualCalendarDestination>, DayError> {
        self.admit_actor(actor)?;
        let operations = self
            .external_calendar_operations
            .read()
            .map_err(|_| DayError::storage("Calendar Operations port unavailable"))?
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| DayError::storage("Calendar Operations is unavailable"))?;
        operations.list_destinations(actor, scope).await
    }

    pub async fn inspect_external_calendar_operation(
        &self,
        actor: &OwnerActor,
        operation_id: uuid::Uuid,
        scope: &ExecutionScope,
    ) -> Result<ManualCalendarOperationReceipt, DayError> {
        self.admit_actor(actor)?;
        let operations = self
            .external_calendar_operations
            .read()
            .map_err(|_| DayError::storage("Calendar Operations port unavailable"))?
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| DayError::storage("Calendar Operations is unavailable"))?;
        operations.inspect_manual(actor, operation_id, scope).await
    }

    pub async fn list_external_calendar_operations(
        &self,
        actor: &OwnerActor,
        cursor: Option<uuid::Uuid>,
        limit: u16,
        scope: &ExecutionScope,
    ) -> Result<ManualCalendarOperationPage, DayError> {
        self.admit_actor(actor)?;
        if !(1..=100).contains(&limit) {
            return Err(DayError::validation("invalid operation limit"));
        }
        let operations = self
            .external_calendar_operations
            .read()
            .map_err(|_| DayError::storage("Calendar Operations port unavailable"))?
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| DayError::storage("Calendar Operations is unavailable"))?;
        operations.list_manual(actor, cursor, limit, scope).await
    }

    pub async fn reconcile_external_calendar_operation(
        &self,
        actor: &OwnerActor,
        command_id: uuid::Uuid,
        operation_id: uuid::Uuid,
        expected_revision: u64,
        scope: &ExecutionScope,
    ) -> Result<ManualCalendarOperationReceipt, CommandFailure<DayError>> {
        self.admit_actor(actor)
            .map_err(CommandFailure::NotAdmitted)?;
        let operations = self
            .external_calendar_operations
            .read()
            .map_err(|_| {
                CommandFailure::NotAdmitted(DayError::storage(
                    "Calendar Operations port unavailable",
                ))
            })?
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                CommandFailure::NotAdmitted(DayError::storage("Calendar Operations is unavailable"))
            })?;
        operations
            .reconcile_manual(actor, command_id, operation_id, expected_revision, scope)
            .await
    }

    pub(crate) async fn selected_day_items(
        &self,
        person_id: PersonId,
        query: &crate::DayQuery,
    ) -> Result<(Vec<Event>, Vec<Task>, Vec<Note>), DayError> {
        let values = self
            .repository
            .read_items(crate::DayReadQuery::display(person_id, query)?)
            .await?;
        let mut events = Vec::new();
        let mut tasks = Vec::new();
        let mut notes = Vec::new();
        for item in values {
            match item {
                TimelineItem::Event(value) => events.push(value),
                TimelineItem::Task(value) => tasks.push(value),
                TimelineItem::Note(value) => notes.push(value),
            }
        }
        Ok((events, tasks, notes))
    }
}
