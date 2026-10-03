use std::{collections::HashMap, sync::{Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use floe_execution::{BudgetConfig, BudgetLedger, CancelReason, Cancellation, ExecutionScope, ModelUsage};
use floe_kernel::{OwnerActor, PersonId};
use tokio::task::JoinHandle;
use uuid::Uuid;
use crate::{CalendarMirror, CalendarRefreshRequest, DayError, DayErrorCode, DayQuery, DayRefreshFailure, DayRefreshSnapshot, DayRefreshState, DayService, Event, EventSchedule, RefreshAdmission, RefreshAdmissionResult, RefreshCommit, RefreshExecutorReplacement, RefreshLookup, RefreshRecord, RefreshTransition, REFRESH_DEADLINE_SECONDS};

pub(crate) struct DayLifecycle {
    actor: Mutex<Option<OwnerActor>>,
    pub(super) generation: Uuid,
    closing: AtomicBool,
    pub(super) cancellation: Cancellation,
    pub(super) admission: tokio::sync::Mutex<()>,
    tasks: Mutex<HashMap<Uuid, JoinHandle<()>>>,
}
impl DayLifecycle {
    pub(crate) fn new() -> Self { Self { actor: Mutex::new(None), generation: Uuid::new_v4(), closing: AtomicBool::new(false), cancellation: Cancellation::new(), admission: tokio::sync::Mutex::new(()), tasks: Mutex::new(HashMap::new()) } }
}
impl DayService {
    pub async fn activate(&self, actor: &OwnerActor, scope: &ExecutionScope) -> Result<(), DayError> {
        actor.validate().map_err(|_| DayError::validation("invalid Day actor"))?; check_scope(scope)?;
        let _admission = self.lifecycle.admission.lock().await;
        if self.lifecycle.closing.load(Ordering::Acquire) { return Err(DayError::storage("Day host is closed")); }
        { let current = self.lifecycle.actor.lock().map_err(|_| DayError::storage("Day admission unavailable"))?; if let Some(current) = current.as_ref() { return if current == actor { Ok(()) } else { Err(DayError::conflict("Day host actor changed")) }; } }
        self.repository.interrupt_refreshes(RefreshExecutorReplacement { person_id: actor.person_id, device_id: actor.device_id.clone(), executor_generation: self.lifecycle.generation, now: self.clock.now() }).await?;
        check_scope(scope)?;
        if self.lifecycle.closing.load(Ordering::Acquire) { return Err(DayError::storage("Day host is closed")); }
        *self.lifecycle.actor.lock().map_err(|_| DayError::storage("Day admission unavailable"))? = Some(actor.clone()); Ok(())
    }
    pub(crate) fn admit_actor(&self, actor: &OwnerActor) -> Result<(), DayError> {
        actor.validate().map_err(|_| DayError::validation("invalid Day actor"))?;
        if self.lifecycle.closing.load(Ordering::Acquire) || self.lifecycle.actor.lock().map_err(|_| DayError::storage("Day admission unavailable"))?.as_ref() != Some(actor) { return Err(DayError::storage("Day host is unavailable")); } Ok(())
    }
    pub(crate) fn admit_person(&self, person_id: PersonId) -> Result<(), DayError> {
        if self.lifecycle.closing.load(Ordering::Acquire) || self.lifecycle.actor.lock().map_err(|_| DayError::storage("Day admission unavailable"))?.as_ref().is_none_or(|actor| actor.person_id != person_id) { return Err(DayError::storage("Day host is unavailable")); } Ok(())
    }
    pub async fn refresh_day(&self, actor: &OwnerActor, command_id: Uuid, query: DayQuery, scope: &ExecutionScope) -> Result<DayRefreshSnapshot, DayError> {
        self.admit_actor(actor)?; check_scope(scope)?; if command_id.is_nil() { return Err(DayError::validation("invalid refresh command")); }
        let intent_digest = query.refresh_intent_digest(actor.person_id, &actor.device_id, command_id)?;
        let _admission = self.lifecycle.admission.lock().await; self.admit_actor(actor)?;
        let admitted = self.repository.admit_refresh(RefreshAdmission { operation_id: Uuid::new_v4(), person_id: actor.person_id, device_id: actor.device_id.clone(), command_id, intent_digest, query, executor_generation: self.lifecycle.generation, admitted_at: self.clock.now() }).await?;
        let record = match admitted { RefreshAdmissionResult::Existing(record) => { record.validate()?; return Ok(record.snapshot()); }, RefreshAdmissionResult::New(record) => record };
        record.validate()?;
        if self.admit_actor(actor).is_err() { self.finish_failure(&record, DayRefreshFailure::HostInterrupted, true).await?; return Err(DayError::storage("Day host closed during admission")); }
        let owned_scope = ExecutionScope::root(self.lifecycle.cancellation.child_scope(), tokio::time::Instant::now() + Duration::from_secs(REFRESH_DEADLINE_SECONDS), BudgetLedger::new(BudgetConfig::new(0, 0), ModelUsage::default()).root_lease(), scope.trace_context());
        let service = self.clone(); let owner = actor.clone(); let task_record = record.clone();
        let handle = tokio::spawn(async move {
            let worker = service.clone(); let working_record = task_record.clone();
            let result = tokio::spawn(async move { worker.drive_refresh(owner, working_record, owned_scope).await }).await;
            if !matches!(result, Ok(Ok(()))) { let _ = service.finish_current_failure(&task_record, DayRefreshFailure::HostInterrupted, true).await; }
        });
        let mut tasks = self.lifecycle.tasks.lock().map_err(|_| DayError::storage("Day task registry unavailable"))?;
        tasks.retain(|_, task| !task.is_finished()); tasks.insert(record.operation_id, handle); Ok(record.snapshot())
    }
    pub async fn get_refresh(&self, actor: &OwnerActor, operation_ref: Uuid, scope: &ExecutionScope) -> Result<DayRefreshSnapshot, DayError> {
        self.admit_actor(actor)?; check_scope(scope)?;
        let record = self.repository.read_refresh(RefreshLookup { operation_id: operation_ref, person_id: actor.person_id, device_id: actor.device_id.clone() }).await?.ok_or_else(|| DayError::not_found("Day refresh", operation_ref))?; record.validate()?; Ok(record.snapshot())
    }
    pub async fn snapshot(&self, actor: &OwnerActor, query: DayQuery, scope: &ExecutionScope) -> Result<crate::DaySnapshot, DayError> {
        self.admit_actor(actor)?; check_scope(scope)?; let range = query.range()?;
        let mirror = self.repository.calendar_mirror(actor.person_id).await?;
        let (mut events, tasks, notes) = self.selected_day_items(actor.person_id, &query).await?;
        let inspection = tokio::select! {
            _ = scope.cancellation().cancelled() => return Err(DayError::storage("Day cache inspection cancelled")),
            _ = tokio::time::sleep_until(scope.deadline()) => return Err(DayError::storage("Day cache inspection expired")),
            result = self.acquisition.inspect_sources(actor, scope) => result.map_err(cache_inspection_error)?,
        };
        inspection.validate(actor, self.clock.now())?;
        self.admit_actor(actor)?; check_scope(scope)?;
        if self.repository.calendar_mirror(actor.person_id).await? != mirror { return Err(DayError::conflict("Calendar cache changed during snapshot")); }
        if let Some(mirror) = &mirror { events.extend(mirror.events.clone()); }
        let mut snapshot = crate::project_day_with_end_offset(actor.person_id, query.date, query.timezone_offset_seconds, query.end_timezone_offset_seconds, query.now, events, tasks, notes)?;
        let state = inspection.project_state(mirror.as_ref().map(|mirror| &mirror.state));
        snapshot.calendar = Some(crate::project_calendar_coverage(&state, &range, self.clock.now()));
        snapshot.calendar_mirror_revision = mirror.as_ref().map(|mirror| mirror.mirror_revision);
        snapshot.validate_bounds()?; Ok(snapshot)
    }
    async fn drive_refresh(&self, actor: OwnerActor, record: RefreshRecord, scope: ExecutionScope) -> Result<(), DayError> {
        let running = record.transition(DayRefreshState::Running, self.clock.now())?;
        let running = self.repository.transition_refresh(RefreshTransition { previous: record, next: running }).await?;
        let request = CalendarRefreshRequest { actor: actor.clone(), refresh_operation_id: running.operation_id, query: running.query.clone(), expected_mirror_revision: running.expected_mirror_revision };
        let acquired = tokio::select! {
            _ = scope.cancellation().cancelled() => Err(DayRefreshFailure::HostInterrupted),
            _ = tokio::time::sleep_until(scope.deadline()) => { scope.cancellation().cancel_with_reason(CancelReason::Deadline); Err(DayRefreshFailure::DeadlineExceeded) },
            result = self.acquisition.acquire(request.clone(), &scope) => result,
        };
        let acquisition = match acquired { Ok(value) => value, Err(reason) => { self.finish_failure(&running, reason, matches!(reason, DayRefreshFailure::HostInterrupted)).await?; return Ok(()); } };
        let result = async {
            self.admit_actor(&actor)?; check_scope(&scope)?;
            let previous = self.repository.calendar_mirror(actor.person_id).await?;
            let mirror = super::observations::reconcile_refresh(&request, &acquisition, previous.as_ref(), self.clock.now())?;
            let (mut events, tasks, notes) = self.selected_day_items(actor.person_id, &running.query).await?; events.extend(mirror.events.clone());
            let mut day = crate::project_day_with_end_offset(actor.person_id, running.query.date, running.query.timezone_offset_seconds, running.query.end_timezone_offset_seconds, running.query.now, events, tasks, notes)?;
            day.calendar = Some(crate::project_calendar_coverage(&mirror.state, &running.query.range()?, acquisition.completed_at)); day.calendar_mirror_revision = Some(mirror.mirror_revision); day.validate_bounds()?;
            self.admit_actor(&actor)?; check_scope(&scope)?;
            let next = running.transition(DayRefreshState::Completed { day }, self.clock.now())?;
            self.repository.commit_refresh(RefreshCommit { previous: running.clone(), next, acquisition, mirror }).await?; Ok::<(), DayError>(())
        }.await;
        if let Err(error) = result { let failure = if self.lifecycle.closing.load(Ordering::Acquire) { DayRefreshFailure::HostInterrupted } else if error.metadata.get("reason_code").map(String::as_str) == Some("budget_exceeded") { DayRefreshFailure::BudgetExceeded } else { match error.code { DayErrorCode::Conflict => DayRefreshFailure::SourceChanged, DayErrorCode::Storage => DayRefreshFailure::StorageUnavailable, _ => DayRefreshFailure::InvalidAcquisition } }; self.finish_current_failure(&running, failure, failure == DayRefreshFailure::HostInterrupted).await?; }
        Ok(())
    }
    async fn finish_current_failure(&self, record: &RefreshRecord, failure: DayRefreshFailure, interrupted: bool) -> Result<(), DayError> {
        let current = self.repository.read_refresh(RefreshLookup { operation_id: record.operation_id, person_id: record.person_id, device_id: record.device_id.clone() }).await?.ok_or_else(|| DayError::not_found("Day refresh", record.operation_id))?;
        if current.state.terminal() { return Ok(()); }
        if current.executor_generation != record.executor_generation { return Err(DayError::conflict("Day executor changed")); }
        self.finish_failure(&current, failure, interrupted).await
    }
    async fn finish_failure(&self, record: &RefreshRecord, failure: DayRefreshFailure, interrupted: bool) -> Result<(), DayError> {
        let state = if interrupted { DayRefreshState::Interrupted { failure } } else { DayRefreshState::Failed { failure } };
        self.repository.transition_refresh(RefreshTransition { previous: record.clone(), next: record.transition(state, self.clock.now())? }).await?; Ok(())
    }
    pub fn close_admission(&self) { self.lifecycle.closing.store(true, Ordering::Release); self.lifecycle.cancellation.cancel_with_reason(CancelReason::OwnerDropped); }
    pub async fn shutdown(&self, scope: &ExecutionScope) -> Result<(), DayError> {
        self.close_admission();
        let _admission = tokio::time::timeout_at(scope.deadline(), self.lifecycle.admission.lock()).await.map_err(|_| DayError::storage("Day shutdown deadline"))?;
        let actor = self.lifecycle.actor.lock().map_err(|_| DayError::storage("Day admission unavailable"))?.clone();
        if let Some(actor) = actor { tokio::time::timeout_at(scope.deadline(), self.repository.retire_refresh_executor(RefreshExecutorReplacement { person_id: actor.person_id, device_id: actor.device_id, executor_generation: self.lifecycle.generation, now: self.clock.now() })).await.map_err(|_| DayError::storage("Day retirement deadline"))??; }
        let tasks = std::mem::take(&mut *self.lifecycle.tasks.lock().map_err(|_| DayError::storage("Day task registry unavailable"))?);
        for (_, task) in tasks { tokio::time::timeout_at(scope.deadline(), task).await.map_err(|_| DayError::storage("Day shutdown deadline"))?.map_err(|_| DayError::storage("Day task interrupted"))?; }
        Ok(())
    }
    pub async fn calendar_mirror(&self, actor: &OwnerActor, scope: &ExecutionScope) -> Result<Option<CalendarMirror>, DayError> { self.admit_actor(actor)?; check_scope(scope)?; self.repository.calendar_mirror(actor.person_id).await }
    pub async fn collect_action(&self, actor: &OwnerActor, execution_id: Uuid, receipt_digest: [u8; 32], collection: crate::CalendarActionCollection, scope: &ExecutionScope) -> Result<crate::DayCollectionReceipt, DayError> {
        self.admit_actor(actor)?; check_scope(scope)?;
        let _admission = tokio::time::timeout_at(scope.deadline(), self.lifecycle.admission.lock()).await.map_err(|_| DayError::storage("Day collection admission expired"))?;
        self.admit_actor(actor)?; check_scope(scope)?;
        let intent_digest = crate::digest(&(actor.person_id, &actor.device_id, execution_id, receipt_digest, &collection))?;
        let commit = crate::DayCollectionCommit { person_id: actor.person_id, device_id: actor.device_id.clone(), executor_generation: self.lifecycle.generation, execution_id, receipt_digest, intent_digest, collection, collected_at: self.clock.now() };
        let fence = crate::DayWriteFence::new(actor.clone(), self.lifecycle.generation, self.lifecycle.cancellation.clone(), scope);
        commit.validate()?; self.repository.collect_action(commit, &fence).await
    }
    pub async fn calendar_event(&self, actor: &OwnerActor, event_id: floe_kernel::EventId, scope: &ExecutionScope) -> Result<Option<Event>, DayError> { Ok(self.calendar_mirror(actor, scope).await?.and_then(|mirror| mirror.events.into_iter().find(|event| event.id == event_id && event.person_id == actor.person_id && event.deleted_at.is_none()))) }
    pub async fn events_for_action(&self, actor: &OwnerActor, starts_at: chrono::DateTime<chrono::Utc>, ends_at: chrono::DateTime<chrono::Utc>, scope: &ExecutionScope) -> Result<Vec<Event>, DayError> {
        self.admit_actor(actor)?; check_scope(scope)?;
        if starts_at >= ends_at || ends_at.signed_duration_since(starts_at) > chrono::Duration::days(32) { return Err(DayError::validation("invalid action event range")); }
        let query = crate::DayReadQuery { person_id: actor.person_id, selection: crate::DayReadSelection::ActionWindow { starts_at, ends_at }, max_items: crate::MAX_DAY_SNAPSHOT_ITEMS, max_bytes: crate::MAX_DAY_SNAPSHOT_BYTES };
        let mut events = Vec::new();
        for item in self.repository.read_items(query.clone()).await? { if let crate::TimelineItem::Event(value) = item { events.push(value); } else { return Err(DayError::storage("invalid Action event selection")); } }
        if let Some(mirror) = self.repository.calendar_mirror(actor.person_id).await? { for event in mirror.events { if query.selects(&crate::TimelineItem::Event(event.clone()))? { events.push(event); } } }

        if events.len() > crate::MAX_REFRESH_RECORDS { return Err(DayError::validation("action event range budget")); } Ok(events)
    }
}
fn check_scope(scope: &ExecutionScope) -> Result<(), DayError> { if scope.cancellation().is_cancelled() || tokio::time::Instant::now() >= scope.deadline() { Err(DayError::storage("Day operation cancelled or expired")) } else { Ok(()) } }

fn cache_inspection_error(failure: DayRefreshFailure) -> DayError {
    let code = match failure { DayRefreshFailure::SourceChanged => "source_changed", DayRefreshFailure::PermissionDenied => "permission_denied", DayRefreshFailure::Unavailable => "unavailable", DayRefreshFailure::VaultLocked => "vault_locked", DayRefreshFailure::BudgetExceeded => "budget_exceeded", DayRefreshFailure::DeadlineExceeded => "deadline_exceeded", DayRefreshFailure::Cancelled => "cancelled", DayRefreshFailure::HostInterrupted => "host_interrupted", DayRefreshFailure::StorageUnavailable => "storage_unavailable", DayRefreshFailure::InvalidAcquisition => "invalid_acquisition" };
    let error = match failure { DayRefreshFailure::SourceChanged => DayError::conflict("Calendar metadata changed during snapshot"), DayRefreshFailure::BudgetExceeded => DayError::budget("Calendar metadata budget"), _ => DayError::storage("Calendar metadata is unavailable") };
    error.with_metadata("reason_code", code)
}
