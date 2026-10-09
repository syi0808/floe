//! One generation-owned recovery driver. Discovery pages and observations are
//! bounded; durable rows, never an in-memory cursor, own unfinished work.
use super::*;
use floe_agent_contract::{DelegationRequest, ReplayReceipt, TaskId};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

const PAGE_SIZE: usize = 16;
const PASS_DEADLINE: Duration = Duration::from_secs(5);
const REOBSERVE_DELAY: Duration = Duration::from_secs(1);

pub(super) struct WakeOnDrop<'a>(pub(super) &'a Notify);
impl Drop for WakeOnDrop<'_> {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

impl<R, S, T> ServiceState<R, S, T>
where
    R: ConversationRepository + InteractionRepository + SessionRepository + 'static,
    S: SessionStore + Send + Sync + 'static,
    T: floe_experts::TaskRepository + 'static,
{
    pub(super) async fn start_recovery(
        self: &Arc<Self>,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        self.check(actor)?;
        let mut tasks = scope.run(async { Ok(self.tasks.lock().await) }).await?;
        self.check(actor)?;
        if self.recovery_started.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let state = self.clone();
        let actor = actor.clone();
        tasks.spawn(async move {
            state.drive_recovery(actor).await;
        });
        Ok(())
    }

    fn recovery_scope(&self) -> ExecutionScope {
        ExecutionScope::root(
            self.shutdown.child_scope(),
            tokio::time::Instant::now() + PASS_DEADLINE,
            BudgetLedger::new(
                floe_execution::budget::BudgetConfig::new(0, 0),
                Default::default(),
            )
            .root_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }

    async fn drive_recovery(self: Arc<Self>, actor: OwnerActor) {
        loop {
            #[cfg(feature = "qa-fixtures")]
            if self.recovery_pause_requested.load(Ordering::Acquire) {
                self.recovery_pause_acknowledged
                    .store(true, Ordering::Release);
                self.recovery_pause_ack.notify_one();
                tokio::select! {
                    _ = self.shutdown.cancelled() => return,
                    _ = self.recovery_wake.notified() => {},
                    _ = tokio::time::sleep(REOBSERVE_DELAY) => {},
                }
                continue;
            } else {
                #[cfg(feature = "qa-fixtures")]
                self.recovery_pause_acknowledged
                    .store(false, Ordering::Release);
            }
            let mut run_after = None;
            let mut interaction_after = None;
            let mut resume_after = None;
            let mut runs_done = false;
            let mut interactions_done = false;
            let mut resumes_done = false;
            let mut reobserve = false;
            while !(runs_done && interactions_done && resumes_done) {
                if self.closing.load(Ordering::Acquire) || self.shutdown.is_cancelled() {
                    return;
                }
                if !runs_done {
                    let scope = self.recovery_scope();
                    match scope
                        .run(
                            self.dependencies
                                .repository
                                .recovery_runs(&actor, run_after, PAGE_SIZE),
                        )
                        .await
                    {
                        Ok(page) => {
                            run_after = page.next_cursor;
                            runs_done = run_after.is_none();
                            for id in page.items {
                                match scope.run(self.recover_run(&actor, id, &scope)).await {
                                    Ok(pending) => reobserve |= pending,
                                    Err(failure) => {
                                        reobserve = true;
                                        report(failure);
                                    }
                                }
                            }
                        }
                        Err(failure) => {
                            runs_done = true;
                            reobserve = true;
                            report(failure);
                        }
                    }
                }
                if !interactions_done {
                    let scope = self.recovery_scope();
                    match scope
                        .run(self.dependencies.repository.resolving_interactions(
                            &actor,
                            interaction_after,
                            PAGE_SIZE,
                        ))
                        .await
                    {
                        Ok(page) => {
                            interaction_after = page.next_cursor;
                            interactions_done = interaction_after.is_none();
                            for record in page.items {
                                let result = scope
                                    .run(recover_source_interaction(
                                        self.dependencies.repository.as_ref(),
                                        self.dependencies.connections.as_ref(),
                                        self.dependencies.calendar_operations.as_ref(),
                                        &actor,
                                        record.id,
                                        chrono::Utc::now().timestamp_millis(),
                                        &scope,
                                    ))
                                    .await;
                                if let Err(failure) = result {
                                    reobserve = true;
                                    report(failure);
                                }
                            }
                        }
                        Err(failure) => {
                            interactions_done = true;
                            reobserve = true;
                            report(failure);
                        }
                    }
                }
                if !resumes_done {
                    let scope = self.recovery_scope();
                    match scope
                        .run(self.dependencies.repository.pending_resume_requests(
                            &actor,
                            resume_after,
                            PAGE_SIZE,
                        ))
                        .await
                    {
                        Ok(page) => {
                            resume_after = page.next_cursor;
                            resumes_done = resume_after.is_none();
                            for request in page.items {
                                match scope
                                    .run(self.resume_one(&actor, request.origin_run_id, &scope))
                                    .await
                                {
                                    Ok(pending) => reobserve |= pending,
                                    Err(failure) => {
                                        reobserve = true;
                                        report(failure);
                                    }
                                }
                            }
                        }
                        Err(failure) => {
                            resumes_done = true;
                            reobserve = true;
                            report(failure);
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
            if reobserve {
                tokio::select! {
                    _ = self.shutdown.cancelled() => return,
                    _ = self.recovery_wake.notified() => {},
                    _ = tokio::time::sleep(REOBSERVE_DELAY) => {},
                }
            } else {
                tokio::select! {
                    _ = self.shutdown.cancelled() => return,
                    _ = self.recovery_wake.notified() => {},
                }
            }
        }
    }

    async fn recover_run(
        &self,
        actor: &OwnerActor,
        run_id: RunId,
        scope: &ExecutionScope,
    ) -> Result<bool, AgentFailure> {
        let repository = self.dependencies.repository.as_ref();
        let mut before = repository
            .load_receipt(run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let initial_revision = before.aggregate_revision;
        if before.principal != actor.person_id.to_string() || before.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        if before.state == RunState::Working {
            // A submit holds admission through durable admission and driver
            // registration. This barrier rules out that in-between window.
            let _admission = self.admission.write().await;
            if self.cancellations.is_active(run_id, &before.principal)? {
                return Ok(false);
            }
            if before.pending_terminal.is_none() {
                before = match repository
                    .finish_run(
                        run_id,
                        before.aggregate_revision,
                        RunTerminal::from_failure(AgentFailure::Interrupted),
                    )
                    .await
                {
                    Ok(record) => record,
                    // A strictly older executor can be retired only through the
                    // explicit current-executor recovery transaction.
                    Err(AgentFailure::Conflict) => {
                        repository.settle_pending_terminal(actor, run_id).await?
                    }
                    Err(failure) => return Err(failure),
                };
            }
        }
        let mut chain = vec![before.clone()];
        let mut seen = HashSet::from([run_id]);
        while let Some(parent) = chain.last().and_then(|run| run.continuation_of) {
            if chain.len() >= 4 || !seen.insert(parent) {
                return Err(AgentFailure::StorageUnavailable);
            }
            chain.push(
                repository
                    .load_receipt(parent)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?,
            );
        }
        chain.reverse();
        let mut originals: BTreeMap<TaskId, DelegationRequest> = BTreeMap::new();
        let mut replays: BTreeMap<TaskId, ReplayReceipt> = BTreeMap::new();
        let mut waiting = false;
        for source in chain {
            if source.principal != actor.person_id.to_string()
                || source.device_id != actor.device_id
                || source.session_id != before.session_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut entries = repository.load_journal(source.run_id).await?;
            crate::validate_run_journal(&source, &entries)?;
            for entry in &entries {
                if let floe_agent_contract::JournalEvent::DelegationIntent { request } =
                    &entry.event
                {
                    let original = originals
                        .entry(request.task_id)
                        .or_insert_with(|| request.clone());
                    let mut normalized = request.clone();
                    normalized.parent_run_id = original.parent_run_id;
                    if normalized != *original {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
            }
            for request in crate::unresolved_run_delegations(&source, &entries)? {
                let original = originals
                    .get(&request.task_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let mut receipt = match self
                    .dependencies
                    .experts_owner
                    .recover_delegation(actor, original, scope)
                    .await
                {
                    Ok(receipt) => receipt,
                    Err(AgentFailure::Conflict) => {
                        waiting = true;
                        continue;
                    }
                    Err(failure) => return Err(failure),
                };
                if request.parent_run_id != original.parent_run_id {
                    receipt.replay = Some(
                        replays
                            .get(&request.task_id)
                            .cloned()
                            .ok_or(AgentFailure::Conflict)?,
                    );
                }
                repository
                    .reconcile_delegation(source.run_id, receipt)
                    .await?;
            }
            entries = repository.load_journal(source.run_id).await?;
            let projection = floe_agent_runtime::project_execution_journal(
                &super::super::recovery::journal_binding(&source, &entries),
                &entries,
                floe_agent_runtime::JournalProjectionMode::DurablePrefix,
            )?;
            for replay in projection.replay {
                if let Some(task) = replay.task_id {
                    replays.insert(task, replay);
                }
            }
        }
        let after = repository.settle_pending_terminal(actor, run_id).await?;
        waiting |= after.pending_terminal.is_some() || !after.unresolved_delegations.is_empty();
        if after.aggregate_revision != initial_revision {
            self.events.publish_run(&after)?;
        }
        Ok(waiting)
    }

    async fn resume_one(
        self: &Arc<Self>,
        actor: &OwnerActor,
        origin: RunId,
        scope: &ExecutionScope,
    ) -> Result<bool, AgentFailure> {
        let repository = self.dependencies.repository.as_ref();
        let Some(pending) = repository.reconcile_resume_request(actor, origin).await? else {
            return Ok(false);
        };
        if pending.person_id != actor.person_id || pending.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let prepared = match prepare_resume(
            repository,
            self.dependencies.sessions.as_ref(),
            ResumePreparationRequest {
                principal: actor.person_id.to_string(),
                person_id: actor.person_id,
                session_id: pending.session_id,
                resume: InteractionResumeRef {
                    origin_run_id: origin,
                    lineage: pending.lineage,
                },
            },
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(AgentFailure::Conflict) => {
                return Ok(repository
                    .reconcile_resume_request(actor, origin)
                    .await?
                    .is_some());
            }
            Err(failure) => return Err(failure),
        };
        let intent = CanonicalTurnIntent {
            session_id: pending.session_id,
            expected_revision: pending.expected_session_revision,
            text: prepared.text,
            mode: prepared.mode,
            retry_of: None,
        };
        let child_command_id = resume_command_id(origin)?;
        let _command = self.lock_command(child_command_id).await?;
        match self
            .submit(actor, child_command_id, intent, prepared.session, scope)
            .await
        {
            Ok(_) => Ok(false),
            Err(floe_kernel::CommandFailure::NotApplied(AgentFailure::Conflict)) => Ok(repository
                .reconcile_resume_request(actor, origin)
                .await?
                .is_some()),
            Err(failure) => Err(failure.into_failure()),
        }
    }
}

fn report(failure: AgentFailure) {
    if !matches!(failure, AgentFailure::Cancelled | AgentFailure::Interrupted) {
        tracing::warn!(?failure, "conversation_recovery_deferred");
    }
}
