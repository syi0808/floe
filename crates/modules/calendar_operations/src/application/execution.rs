use super::owner::day_failure;
use crate::*;
use floe_execution::{Cancellation, ExecutionScope};
use floe_kernel::AgentFailure;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};
use tokio::time::Instant;
use uuid::Uuid;

impl CalendarOperationsService {
    /// A committed Approved record must not be stranded if the local worker
    /// cannot start. Record a durable pre-dispatch Blocked state so the UI and
    /// recovery path never mistake a failed spawn for accepted execution.
    pub(super) async fn spawn_or_block(
        &self,
        record: ActionRecord,
        recovery: bool,
        parent: &ExecutionScope,
    ) -> Result<ActionRecord, AgentFailure> {
        match self.spawn(record.id, recovery, parent) {
            Ok(()) => Ok(record),
            Err(error) if recovery => Err(error),
            Err(_) if record.state == ActionState::Approved => {
                self.repository
                    .stop_before_dispatch(PreDispatchStop {
                        person_id: record.person_id,
                        action_id: record.id,
                        expected_revision: record.revision,
                        state: PreDispatchState::Blocked {
                            reason: ActionBlockedReason::ExecutorUnavailable,
                        },
                    })
                    .await?;
                self.repository
                    .get(record.person_id, record.id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn spawn(
        &self,
        id: Uuid,
        recovery: bool,
        parent: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        if self.closed.load(Ordering::Acquire) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let cancellation = Cancellation::new();
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(AgentFailure::VaultUnavailable);
        }
        if jobs.contains_key(&id) {
            return Ok(());
        }
        if jobs.len() >= 64 {
            return Err(AgentFailure::BudgetExceeded);
        }
        jobs.insert(id, cancellation.clone());
        drop(jobs);
        let scope = ExecutionScope::root(
            cancellation,
            Instant::now() + Duration::from_secs(45),
            parent.budget().child(0, 0),
            parent.trace_context(),
        );
        let service = self.clone();
        let lease = ActionJobLease {
            id,
            jobs: self.jobs.clone(),
        };
        tokio::spawn(async move {
            let _lease = lease;
            let _ = if recovery {
                service.recover(id, &scope).await
            } else {
                service.drive(id, &scope).await
            };
        });
        Ok(())
    }
    async fn stop(
        &self,
        record: &ActionRecord,
        state: PreDispatchState,
    ) -> Result<(), AgentFailure> {
        const MAX_STOP_RETRIES: usize = 4;

        for attempt in 0..MAX_STOP_RETRIES {
            match self
                .repository
                .stop_before_dispatch(PreDispatchStop {
                    person_id: record.person_id,
                    action_id: record.id,
                    expected_revision: record.revision,
                    state,
                })
                .await
            {
                Ok(_) => return Ok(()),
                Err(crate::CalendarOperationStoreError::StorageBusy)
                    if attempt + 1 < MAX_STOP_RETRIES =>
                {
                    tokio::time::sleep(Duration::from_millis(20 << attempt)).await;
                }
                Err(crate::CalendarOperationStoreError::StorageBusy) => {
                    // Do not restart preflight after an already-known block/expiry/cancellation.
                    // Let the driver settle this Approved record as Blocked instead.
                    return Err(AgentFailure::StorageUnavailable);
                }
                Err(failure) => return Err(failure.into()),
            }
        }

        unreachable!("bounded pre-dispatch stop retry loop must return")
    }
    async fn drive(&self, id: Uuid, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        let mut delay = Duration::from_millis(20);
        loop {
            match self.drive_once(id, scope).await {
                Ok(None) => return Ok(()),
                Ok(Some(pending)) => {
                    // A received outcome must not be replaced by another provider
                    // lookup merely because its local custody was busy.
                    return self
                        .settle(&pending.intent, pending.revision, pending.outcome, scope)
                        .await;
                }
                Err(AgentFailure::StorageBusy) => {
                    // Observe the durable fence before deciding whether another
                    // preflight is allowed. A Busy read proves neither absence
                    // nor that the effect remains undispatched.
                    let record = match self.observe_driver_record(id, scope).await {
                        Ok(record) => record,
                        Err(failure) => {
                            return self.settle_driver_failure(id, failure, scope).await;
                        }
                    };
                    match record.state {
                        ActionState::Approved => {
                            if self.closed.load(Ordering::Acquire)
                                || scope.cancellation().is_cancelled()
                            {
                                return self.stop(&record, PreDispatchState::Cancelled).await;
                            }
                            if self.clock.now() >= record.expires_at {
                                return self.stop(&record, PreDispatchState::Expired).await;
                            }
                            if Instant::now() >= scope.deadline() {
                                return self
                                    .stop(
                                        &record,
                                        PreDispatchState::Blocked {
                                            reason: ActionBlockedReason::ExecutorUnavailable,
                                        },
                                    )
                                    .await;
                            }
                            // Approved proves no dispatch-intent CAS committed.
                            // Re-run all source/event/policy/native preparation
                            // fences, retaining the same durable operation.
                            let wake = (Instant::now() + delay).min(scope.deadline());
                            tokio::select! {
                                _ = scope.cancellation().cancelled() => {},
                                _ = tokio::time::sleep_until(wake) => {},
                            }
                            delay = (delay * 2).min(Duration::from_millis(500));
                        }
                        ActionState::Executing { .. }
                        | ActionState::Unknown { .. }
                        | ActionState::Succeeded { .. } => {
                            // Intent may exist: lookup/receipt recovery only.
                            return self.recover(id, scope).await;
                        }
                        _ => return Ok(()),
                    }
                }
                Err(failure) => return self.settle_driver_failure(id, failure, scope).await,
            }
        }
    }

    async fn observe_driver_record(
        &self,
        id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<ActionRecord, AgentFailure> {
        loop {
            match scope.run(self.record(&self.actor, id)).await {
                Err(AgentFailure::StorageBusy) => {
                    scope
                        .run(async {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                            Ok(())
                        })
                        .await?;
                }
                result => return result,
            }
        }
    }

    async fn drive_once(
        &self,
        id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<Option<PendingSettlement>, AgentFailure> {
        let actor = &self.actor;
        let record = self.record(actor, id).await?;
        if record.state != ActionState::Approved {
            return Ok(None);
        }
        if self.closed.load(Ordering::Acquire) || scope.cancellation().is_cancelled() {
            return self
                .stop(&record, PreDispatchState::Cancelled)
                .await
                .map(|_| None);
        }
        if self.clock.now() >= record.expires_at {
            return self
                .stop(&record, PreDispatchState::Expired)
                .await
                .map(|_| None);
        }
        if Instant::now() >= scope.deadline() {
            return self
                .stop(
                    &record,
                    PreDispatchState::Blocked {
                        reason: ActionBlockedReason::ExecutorUnavailable,
                    },
                )
                .await
                .map(|_| None);
        }
        let dependencies = match self.prepare_dependency_sources(actor, &record, scope).await {
            Ok(value) => value,
            Err(AgentFailure::StorageBusy) => return Err(AgentFailure::StorageBusy),
            Err(_) => {
                return self
                    .stop(
                        &record,
                        PreDispatchState::Blocked {
                            reason: ActionBlockedReason::SourceChanged,
                        },
                    )
                    .await
                    .map(|_| None);
            }
        };
        let source = match self.observe_source(actor, &record.effect).await {
            Ok(source) if source == record.source => source,
            Err(AgentFailure::StorageBusy) => return Err(AgentFailure::StorageBusy),
            _ => {
                return self
                    .stop(
                        &record,
                        PreDispatchState::Blocked {
                            reason: ActionBlockedReason::SourceChanged,
                        },
                    )
                    .await
                    .map(|_| None);
            }
        };
        let events = match self.current_events(actor, &record.effect, scope).await {
            Ok(events) => events,
            Err(AgentFailure::StorageBusy) => return Err(AgentFailure::StorageBusy),
            Err(_) => {
                return self
                    .stop(
                        &record,
                        PreDispatchState::Blocked {
                            reason: ActionBlockedReason::SourceChanged,
                        },
                    )
                    .await
                    .map(|_| None);
            }
        };
        let prepared = match self
            .executor
            .prepare(actor, &record, &dependencies, &events, scope)
            .await
        {
            Ok(prepared) => prepared,
            Err(reason) => {
                return self
                    .stop(&record, PreDispatchState::Blocked { reason })
                    .await
                    .map(|_| None);
            }
        };
        if self.closed.load(Ordering::Acquire) || scope.cancellation().is_cancelled() {
            return self
                .stop(&record, PreDispatchState::Cancelled)
                .await
                .map(|_| None);
        }
        if Instant::now() >= scope.deadline() {
            return self
                .stop(
                    &record,
                    PreDispatchState::Blocked {
                        reason: ActionBlockedReason::ExecutorUnavailable,
                    },
                )
                .await
                .map(|_| None);
        }
        let unchanged_source = observation_matches(
            self.observe_source(actor, &record.effect).await,
            |current| current == source,
        )?;
        let unchanged_events = observation_matches(
            self.current_events(actor, &record.effect, scope).await,
            |current| current == events,
        )?;
        let unchanged_dependencies = observation_matches(
            self.revalidate_dependency_sources(actor, &dependencies, scope)
                .await,
            |_| true,
        )?;
        // These asynchronous observations may overlap shutdown or expiry.
        // Fence the actual owner/job again before durable dispatch admission.
        if self.closed.load(Ordering::Acquire) || scope.cancellation().is_cancelled() {
            return self
                .stop(&record, PreDispatchState::Cancelled)
                .await
                .map(|_| None);
        }
        if Instant::now() >= scope.deadline() {
            return self
                .stop(
                    &record,
                    PreDispatchState::Blocked {
                        reason: ActionBlockedReason::ExecutorUnavailable,
                    },
                )
                .await
                .map(|_| None);
        }
        if !unchanged_source || !unchanged_events || !unchanged_dependencies {
            return self
                .stop(
                    &record,
                    PreDispatchState::Blocked {
                        reason: ActionBlockedReason::SourceChanged,
                    },
                )
                .await
                .map(|_| None);
        }
        if self.clock.now() >= record.expires_at {
            return self
                .stop(&record, PreDispatchState::Expired)
                .await
                .map(|_| None);
        }
        #[cfg(feature = "qa-fixtures")]
        if self
            .busy_dispatch_admissions
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(AgentFailure::StorageBusy);
        }
        let admission = self
            .repository
            .prepare_dispatch(DispatchIntent {
                action_id: record.id,
                person_id: record.person_id,
                device_id: record.device_id.clone(),
                expected_revision: record.revision,
                execution_id: record.execution_id,
                effect_digest: record.effect_digest,
                authorization: record
                    .authorization
                    .clone()
                    .ok_or(AgentFailure::PolicyDenied)?,
                current_source_fence: source,
                executor_generation: prepared.executor_generation(),
                now: self.clock.now(),
            })
            .await?;
        if !admission.dispatch_required {
            return Ok(None);
        }
        let intent = admission.intent.clone();
        let revision = admission.record_revision;
        let outcome = prepared.dispatch(admission, scope.clone()).await;
        Ok(Some(PendingSettlement {
            intent,
            revision,
            outcome,
        }))
    }

    /// A worker error is reconciled against the durable operation state before
    /// it is reported. Approved proves the dispatch-intent CAS did not commit,
    /// so it can be durably blocked. Executing means the CAS may have committed;
    /// only receipt lookup is safe from that point onward.
    async fn settle_driver_failure(
        &self,
        id: Uuid,
        failure: AgentFailure,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        use crate::CalendarOperationStoreError as StoreError;

        let actor = &self.actor;
        let reason = match failure {
            AgentFailure::PolicyDenied | AgentFailure::CapabilityDenied => {
                ActionBlockedReason::PolicyDenied
            }
            AgentFailure::StaleContext | AgentFailure::Conflict => {
                ActionBlockedReason::SourceChanged
            }
            _ => ActionBlockedReason::ExecutorUnavailable,
        };
        for attempt in 0..4 {
            let record = match self.record(actor, id).await {
                Ok(record) => record,
                Err(AgentFailure::StorageBusy) if attempt < 3 => {
                    tokio::time::sleep(Duration::from_millis(20 << attempt)).await;
                    continue;
                }
                Err(failure) => return Err(failure),
            };
            match record.state {
                ActionState::Approved => {
                    match self
                        .repository
                        .stop_before_dispatch(PreDispatchStop {
                            person_id: record.person_id,
                            action_id: record.id,
                            expected_revision: record.revision,
                            state: if failure == AgentFailure::Cancelled {
                                PreDispatchState::Cancelled
                            } else {
                                PreDispatchState::Blocked { reason }
                            },
                        })
                        .await
                    {
                        Ok(_) => return Ok(()),
                        Err(StoreError::StorageBusy | StoreError::Conflict) if attempt < 3 => {
                            tokio::time::sleep(Duration::from_millis(20 << attempt)).await;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                ActionState::Executing { .. } | ActionState::Unknown { .. } => {
                    return self.recover(id, scope).await;
                }
                _ => return Ok(()),
            }
        }
        Err(AgentFailure::StorageBusy)
    }

    async fn recover(&self, id: Uuid, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        let actor = &self.actor;
        self.admit_actor(actor, scope)?;
        let record = self.record(actor, id).await?;
        if matches!(record.state, ActionState::Succeeded { .. }) {
            return self.collect(&record, scope).await;
        }
        if !matches!(
            record.state,
            ActionState::Executing { .. } | ActionState::Unknown { .. }
        ) {
            return Err(AgentFailure::Conflict);
        }
        let admission = self
            .repository
            .load_execution(actor.person_id, record.execution_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if admission.dispatch_required || record.execution.as_ref() != Some(&admission.intent) {
            return Err(AgentFailure::StorageUnavailable);
        }
        let outcome = self.executor.recover(actor, &admission.intent, scope).await;
        self.settle(&admission.intent, record.revision, outcome, scope)
            .await
    }
    async fn settle(
        &self,
        intent: &ExecutionIntent,
        revision: u64,
        outcome: CalendarEffectOutcome,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let outcome = if outcome.validate_for(intent).is_ok() {
            outcome
        } else {
            CalendarEffectOutcome::Unknown {
                identity: intent.identity(),
                reason: ActionUnknownReason::InvalidReceipt,
            }
        };
        let settlement = ExecutionSettlement {
            person_id: intent.person_id,
            execution_id: intent.execution_id,
            effect_digest: intent.effect_digest,
            expected_revision: revision,
            outcome,
        };
        let record = self.persist_known_settlement(settlement, scope).await?;
        if matches!(record.state, ActionState::Succeeded { .. }) {
            self.collect(&record, scope).await?;
        }
        Ok(())
    }
    async fn persist_known_settlement(
        &self,
        settlement: ExecutionSettlement,
        scope: &ExecutionScope,
    ) -> Result<ActionRecord, AgentFailure> {
        self.retry_known_local_custody(scope, || self.settle_receipt_once(settlement.clone()))
            .await
    }

    async fn retry_known_local_custody<T, F, Fut>(
        &self,
        scope: &ExecutionScope,
        mut write: F,
    ) -> Result<T, AgentFailure>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, CalendarOperationStoreError>>,
    {
        // Retain the exact known local payload, including its CAS identity.
        // This retries neither the provider effect nor Day collection. Only
        // local Busy is retried within the owner's existing bounded lifetime.
        let mut delay = Duration::from_millis(20);
        loop {
            match write().await {
                Ok(record) => return Ok(record),
                Err(CalendarOperationStoreError::StorageBusy) => {
                    if self.closed.load(Ordering::Acquire)
                        || scope.cancellation().is_cancelled()
                        || Instant::now() >= scope.deadline()
                    {
                        return Err(AgentFailure::StorageBusy);
                    }
                    let wake = (Instant::now() + delay).min(scope.deadline());
                    tokio::select! {
                        biased;
                        _ = scope.cancellation().cancelled() => return Err(AgentFailure::StorageBusy),
                        _ = tokio::time::sleep_until(wake) => {},
                    }
                    if Instant::now() >= scope.deadline() {
                        return Err(AgentFailure::StorageBusy);
                    }
                    delay = (delay * 2).min(Duration::from_millis(100));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    async fn settle_receipt_once(
        &self,
        settlement: ExecutionSettlement,
    ) -> Result<ActionRecord, CalendarOperationStoreError> {
        #[cfg(feature = "qa-fixtures")]
        self.receipt_settlement_attempts
            .fetch_add(1, Ordering::AcqRel);
        #[cfg(feature = "qa-fixtures")]
        if self
            .busy_receipt_settlements
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(CalendarOperationStoreError::StorageBusy);
        }
        let record = self.repository.settle_execution(settlement).await?;
        #[cfg(feature = "qa-fixtures")]
        if self
            .lose_receipt_settlement_ack
            .swap(false, Ordering::AcqRel)
        {
            return Err(CalendarOperationStoreError::StorageBusy);
        }
        Ok(record)
    }

    async fn ack_collection_once(
        &self,
        ack: CollectionAck,
    ) -> Result<CollectionTicket, CalendarOperationStoreError> {
        #[cfg(feature = "qa-fixtures")]
        self.collection_ack_attempts.fetch_add(1, Ordering::AcqRel);
        #[cfg(feature = "qa-fixtures")]
        if self
            .busy_collection_acks
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(CalendarOperationStoreError::StorageBusy);
        }
        let result = self.repository.ack_collection(ack).await?;
        #[cfg(feature = "qa-fixtures")]
        if self.lose_collection_ack.swap(false, Ordering::AcqRel) {
            return Err(CalendarOperationStoreError::StorageBusy);
        }
        Ok(result)
    }

    async fn collect(
        &self,
        record: &ActionRecord,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let ActionState::Succeeded {
            receipt,
            collection: ActionCollectionState::Pending { .. },
        } = &record.state
        else {
            return Ok(());
        };
        let ticket = record
            .collection
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let destination = record.effect.destination();
        let source = floe_day::ActionCollectionSource {
            connection_id: destination.connection_id.clone(),
            connection_revision: destination.connection_revision,
            source_authority: record.source.authority,
            provider: destination.provider,
            calendar_id: destination.calendar_id.clone(),
            calendar_name: destination.calendar_name.clone(),
        };
        let calendar_record = |event: &CalendarWriteResult| floe_day::CalendarRecord {
            can_modify: event.can_modify,
            calendar_id: destination.calendar_id.clone(),
            external_id: event.external_id.clone(),
            external_revision: event.external_revision.clone(),
            title: event.title.clone(),
            schedule: floe_day::EventSchedule::Timed(event.schedule.clone()),
        };
        let collection = match &receipt.effect {
            CommittedCalendarEffect::Created { event } => {
                floe_day::CalendarActionCollection::Created {
                    source,
                    record: calendar_record(event),
                }
            }
            CommittedCalendarEffect::Updated { target, event } => {
                let original = target.source()?;
                floe_day::CalendarActionCollection::Updated {
                    source,
                    expected_external_id: original.external_id.clone(),
                    expected_external_revision: original.external_revision.clone(),
                    record: calendar_record(event),
                }
            }
            CommittedCalendarEffect::Deleted { target } => {
                let original = target.source()?;
                floe_day::CalendarActionCollection::Deleted {
                    source,
                    external_id: original.external_id.clone(),
                    expected_external_revision: original.external_revision.clone(),
                }
            }
        };
        let receipt_digest = receipt.digest()?;
        let result = self
            .day
            .collect_action(
                &self.actor,
                record.execution_id,
                receipt_digest,
                collection,
                scope,
            )
            .await
            .map_err(day_failure)?;
        if result.execution_id != record.execution_id || result.receipt_digest != receipt_digest {
            return Err(AgentFailure::Conflict);
        }
        let ack = CollectionAck {
            person_id: record.person_id,
            execution_id: record.execution_id,
            receipt_digest,
            expected_ticket_revision: ticket.revision,
            day_projection_ref: result.day_projection_ref,
        };
        self.retry_known_local_custody(scope, || self.ack_collection_once(ack.clone()))
            .await?;
        Ok(())
    }
}

/// A received external outcome has a separate local custody phase.
struct PendingSettlement {
    intent: ExecutionIntent,
    revision: u64,
    outcome: CalendarEffectOutcome,
}

struct ActionJobLease {
    id: Uuid,
    jobs: Arc<Mutex<HashMap<Uuid, Cancellation>>>,
}
impl Drop for ActionJobLease {
    fn drop(&mut self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(&self.id);
        }
    }
}

// Busy is not proof of changed source evidence. It remains retryable before
// dispatch; every other failed observation still blocks closed.
fn observation_matches<T>(
    result: Result<T, AgentFailure>,
    matches: impl FnOnce(T) -> bool,
) -> Result<bool, AgentFailure> {
    match result {
        Ok(value) => Ok(matches(value)),
        Err(AgentFailure::StorageBusy) => Err(AgentFailure::StorageBusy),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    #[test]
    fn busy_is_not_source_change_or_permission() {
        assert_eq!(observation_matches(Ok(7), |value| value == 7), Ok(true));
        assert_eq!(observation_matches(Ok(8), |value| value == 7), Ok(false));
        for failure in [
            AgentFailure::StorageBusy,
            AgentFailure::StorageUnavailable,
            AgentFailure::PolicyDenied,
            AgentFailure::StaleContext,
            AgentFailure::AccessReviewRequired,
        ] {
            assert_eq!(
                observation_matches::<()>(Err(failure), |_| panic!(
                    "failed evidence cannot authorize"
                )),
                if failure == AgentFailure::StorageBusy {
                    Err(failure)
                } else {
                    Ok(false)
                }
            );
        }
    }
}
