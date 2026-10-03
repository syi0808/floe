//! Task admission, owned execution lifetime and authenticated terminal receipts.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, BoxFuture, Cancellation, DelegationPort, DelegationRequest,
    DependencyCoverage, EndpointInvocation, ExecutionScope, ExpertExecutionOutcome, TaskBlockage,
    TaskExecutionEvidence, TaskExecutionReceipt, TaskExecutionReceiptRef, TaskId, TaskReceipt,
    TaskSnapshot, TaskState, delegation_request_digest,
};
use floe_execution::budget::ModelReservationCeiling;
use tokio::sync::watch;

use crate::task_record::{
    TaskRecord, missing_requirement_keys, project_task_journal, settle_task_execution, terminal,
};
use crate::task_repository::{TaskAdmission, TaskExecutionCommit, TaskRepository};
use crate::{Directory, DirectoryQuery};

const CANCEL_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
type TaskOutcome = Result<TaskExecutionReceipt, AgentFailure>;
type ActiveTasks = Arc<Mutex<HashMap<TaskId, Arc<ActiveTask>>>>;

struct ActiveTask {
    proposed: TaskRecord,
    cancellation: Cancellation,
    completion: watch::Receiver<Option<TaskOutcome>>,
}

/// Admission, endpoint execution and settlement all belong to this retained
/// driver. Removing an observer cannot drop an in-flight storage mutation.
struct ActiveCompletion {
    task: Arc<ActiveTask>,
    active: ActiveTasks,
    sender: Option<watch::Sender<Option<TaskOutcome>>>,
}

impl ActiveCompletion {
    fn finish(mut self, outcome: TaskOutcome) {
        if let Some(sender) = self.sender.take() {
            sender.send_replace(Some(outcome));
        }
    }
}

impl Drop for ActiveCompletion {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            // Driver loss is not a persisted terminal outcome. Fenced executor
            // recovery still sees the unfinished journal.
            sender.send_replace(Some(Err(AgentFailure::Interrupted)));
        }
        if let Ok(mut active) = self.active.lock() {
            if active
                .get(&self.task.proposed.snapshot.task_id)
                .is_some_and(|task| Arc::ptr_eq(task, &self.task))
            {
                active.remove(&self.task.proposed.snapshot.task_id);
            }
        }
    }
}

pub struct TaskCoordinator<Repository> {
    directory: Directory,
    repository: Arc<Repository>,
    purpose: String,
    maximum_output_bytes: usize,
    executor_generation: u64,
    active: ActiveTasks,
    closing: std::sync::atomic::AtomicBool,
    admissions: tokio::sync::RwLock<()>,
}

impl<Repository> TaskCoordinator<Repository> {
    pub fn close_admission(&self) {
        self.closing
            .store(true, std::sync::atomic::Ordering::Release);
        let active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for task in active.values() {
            task.cancellation
                .cancel_with_reason(floe_agent_contract::CancelReason::OwnerDropped);
        }
    }
}

pub struct RunExpertEnvironment<Repository> {
    coordinator: Arc<TaskCoordinator<Repository>>,
    principal: String,
    snapshot: crate::directory::DirectorySnapshot,
}

impl<Repository> RunExpertEnvironment<Repository> {
    pub fn identity(&self) -> crate::RunExpertEnvironmentIdentity {
        self.snapshot.identity()
    }
    pub fn catalog(&self) -> AllowedCatalog {
        self.snapshot.catalog()
    }
}

impl<Repository: TaskRepository + 'static> TaskCoordinator<Repository> {
    pub async fn activate(
        directory: Directory,
        repository: Arc<Repository>,
        purpose: impl Into<String>,
        maximum_output_bytes: usize,
    ) -> Result<(Self, Vec<TaskReceipt>), AgentFailure> {
        let purpose = purpose.into();
        if purpose.trim().is_empty()
            || maximum_output_bytes == 0
            || maximum_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        let activation = repository.activate().await?;
        if activation.executor_generation == 0 {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut task_ids = HashSet::new();
        let mut interrupted = Vec::with_capacity(activation.interrupted.len());
        for record in activation.interrupted {
            record.validate(maximum_output_bytes)?;
            if record.snapshot.state != TaskState::Interrupted
                || record.snapshot.issue != Some(AgentFailure::Interrupted)
                || record.executor_generation >= activation.executor_generation
                || !task_ids.insert(record.snapshot.task_id)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let execution =
                verified_receipt(repository.as_ref(), &record, maximum_output_bytes).await?;
            interrupted.push(receipt(execution, maximum_output_bytes)?);
        }
        Ok((
            Self {
                directory,
                repository,
                purpose,
                maximum_output_bytes,
                executor_generation: activation.executor_generation,
                active: Arc::new(Mutex::new(HashMap::new())),
                closing: std::sync::atomic::AtomicBool::new(false),
                admissions: tokio::sync::RwLock::new(()),
            },
            interrupted,
        ))
    }

    pub fn environment(
        self: &Arc<Self>,
        principal: &str,
    ) -> Result<RunExpertEnvironment<Repository>, AgentFailure> {
        if self.closing.load(std::sync::atomic::Ordering::Acquire) {
            return Err(AgentFailure::Interrupted);
        }
        let snapshot = self.directory.snapshot(DirectoryQuery {
            principal,
            purpose: &self.purpose,
        })?;
        Ok(RunExpertEnvironment {
            coordinator: Arc::clone(self),
            principal: principal.to_owned(),
            snapshot,
        })
    }

    /// Settings and execution share this exact Directory instance.
    pub fn directory(&self) -> Directory {
        self.directory.clone()
    }

    pub async fn shutdown(&self) -> Result<(), AgentFailure> {
        self.close_admission();
        tokio::time::timeout(CANCEL_DRAIN_TIMEOUT, async {
            let admission_guard = self.admissions.write().await;
            self.close_admission();
            let active = self
                .active
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            drop(admission_guard);
            let mut failure = None;
            for task in active {
                if let Err(error) = completed(&task).await {
                    failure.get_or_insert(error);
                }
            }
            failure.map_or(Ok(()), Err)
        })
        .await
        .map_err(|_| AgentFailure::DeadlineExceeded)?
    }

    /// Observation may return Working, but never manufactures a delegation receipt.
    pub async fn get_task(
        &self,
        principal: &str,
        parent_run_id: Option<uuid::Uuid>,
        task_id: TaskId,
        scope: &ExecutionScope,
    ) -> Result<Option<TaskSnapshot>, AgentFailure> {
        validate_query(principal, parent_run_id, scope)?;
        let record = scope.run(self.repository.get(task_id)).await?;
        match record {
            Some(record) => {
                authorize_task(&record, principal, parent_run_id)?;
                record.validate(self.maximum_output_bytes)?;
                Ok(Some(record.snapshot))
            }
            None => Ok(None),
        }
    }

    /// Load through the stored Task. Explicit continuation lineage remains the
    /// observing owner's check; this never rewrites the receipt's original Run.
    pub async fn read_execution_receipt(
        &self,
        actor: &floe_kernel::OwnerActor,
        reference: &TaskExecutionReceiptRef,
        scope: &ExecutionScope,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        self.read_execution_record(actor, reference, scope)
            .await?
            .receipt
            .ok_or(AgentFailure::StorageUnavailable)
    }

    /// Recover only acknowledged owner state. This path never resolves a live
    /// endpoint, admits a Task, waits for a driver or dispatches model work.
    pub async fn recover_delegation(
        &self,
        actor: &floe_kernel::OwnerActor,
        request: &DelegationRequest,
        scope: &ExecutionScope,
    ) -> Result<TaskReceipt, AgentFailure> {
        actor.validate()?;
        validate_request_fields(request)?;
        if request.principal != actor.person_id.to_string()
            || request.execution_context.device_id != actor.device_id
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        // An admission may be owned but not yet visible in storage. Fence
        // new reservations while proving absence, and never call that phase
        // Unadmitted merely because its first write has not acknowledged.
        let admission_guard = scope
            .run(async { Ok(self.admissions.write().await) })
            .await?;
        if let Some(active) = self.active_task(request.task_id)? {
            validate_replay(
                request,
                delegation_request_digest(request),
                &active.proposed,
            )?;
            return Err(AgentFailure::Conflict);
        }
        let stored = scope.run(self.repository.get(request.task_id)).await?;
        drop(admission_guard);
        let Some(record) = stored else {
            // Durable admission precedes every endpoint handoff. Only a real
            // successful absence read can attest that no Task was admitted.
            let snapshot = TaskSnapshot {
                coverage: DependencyCoverage::Independent,
                ..snapshot(
                    request,
                    TaskState::Rejected,
                    Some(AgentFailure::Interrupted),
                )
            };
            let rejected = TaskReceipt {
                task_id: request.task_id,
                snapshot,
                replay: None,
                execution: TaskExecutionEvidence::Unadmitted,
            };
            rejected.validate(self.maximum_output_bytes)?;
            return Ok(rejected);
        };
        validate_replay(request, delegation_request_digest(request), &record)?;
        record.validate(self.maximum_output_bytes)?;
        if !terminal(record.snapshot.state) {
            return Err(AgentFailure::Conflict);
        }
        let execution = scope
            .run(verified_receipt(
                self.repository.as_ref(),
                &record,
                self.maximum_output_bytes,
            ))
            .await?;
        receipt(execution, self.maximum_output_bytes)
    }

    /// Binding review needs the exact admitted selection as well as its receipt.
    pub(crate) async fn read_execution_record(
        &self,
        actor: &floe_kernel::OwnerActor,
        reference: &TaskExecutionReceiptRef,
        scope: &ExecutionScope,
    ) -> Result<TaskRecord, AgentFailure> {
        actor.validate()?;
        reference.validate()?;
        let record = scope
            .run(self.repository.get(reference.execution.task_id))
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.snapshot.principal != actor.person_id.to_string()
            || record.device_id != actor.device_id
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let value = scope
            .run(verified_receipt(
                self.repository.as_ref(),
                &record,
                self.maximum_output_bytes,
            ))
            .await?;
        if value.reference != *reference {
            return Err(AgentFailure::Conflict);
        }
        Ok(record)
    }

    pub async fn cancel_task(
        &self,
        principal: &str,
        parent_run_id: Option<uuid::Uuid>,
        task_id: TaskId,
        scope: &ExecutionScope,
    ) -> Result<TaskSnapshot, AgentFailure> {
        validate_query(principal, parent_run_id, scope)?;
        // The reservation exposes cancellation even before the admission write
        // is acknowledged. It carries the same immutable identity as that write.
        let active = if let Some(active) = self.active_task(task_id)? {
            authorize_task(&active.proposed, principal, parent_run_id)?;
            active
        } else {
            let record = scope
                .run(self.repository.get(task_id))
                .await?
                .ok_or(AgentFailure::NotFound)?;
            authorize_task(&record, principal, parent_run_id)?;
            record.validate(self.maximum_output_bytes)?;
            if terminal(record.snapshot.state) {
                return Ok(record.snapshot);
            }
            return Err(AgentFailure::Conflict);
        };
        active.cancellation.cancel();
        // Timeout leaves Working intact; only the retained driver may settle.
        let outcome = tokio::time::timeout(CANCEL_DRAIN_TIMEOUT, completed(&active))
            .await
            .map_err(|_| AgentFailure::DeadlineExceeded)??;
        outcome.validate(self.maximum_output_bytes)?;
        Ok(outcome.snapshot)
    }

    fn active_task(&self, task_id: TaskId) -> Result<Option<Arc<ActiveTask>>, AgentFailure> {
        Ok(self
            .active
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .get(&task_id)
            .cloned())
    }

    async fn existing(
        &self,
        record: TaskRecord,
        request: &DelegationRequest,
        endpoint: &crate::directory::ResolvedDirectoryEntry,
        scope: &ExecutionScope,
    ) -> Result<TaskReceipt, AgentFailure> {
        record.validate(self.maximum_output_bytes)?;
        let execution = if terminal(record.snapshot.state) {
            scope
                .run(verified_receipt(
                    self.repository.as_ref(),
                    &record,
                    self.maximum_output_bytes,
                ))
                .await?
        } else if let Some(active) = self.active_task(record.snapshot.task_id)? {
            validate_replay(
                request,
                delegation_request_digest(request),
                &active.proposed,
            )?;
            validate_endpoint(&active.proposed, endpoint)?;
            if active.proposed.execution() != record.execution() {
                return Err(AgentFailure::Conflict);
            }
            wait_for_execution(&active, scope).await?
        } else {
            // Completion can remove the driver after the observer read Working.
            // Rejoin only the exact terminal execution; never restart it.
            let current = scope
                .run(self.repository.get(record.snapshot.task_id))
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            validate_replay(request, delegation_request_digest(request), &current)?;
            validate_endpoint(&current, endpoint)?;
            if current.execution() != record.execution() || !terminal(current.snapshot.state) {
                return Err(AgentFailure::Conflict);
            }
            scope
                .run(verified_receipt(
                    self.repository.as_ref(),
                    &current,
                    self.maximum_output_bytes,
                ))
                .await?
        };
        receipt(execution, self.maximum_output_bytes)
    }

    async fn execute(
        self: &Arc<Self>,
        request: DelegationRequest,
        scope: &ExecutionScope,
        endpoint: crate::directory::ResolvedDirectoryEntry,
    ) -> Result<TaskReceipt, AgentFailure> {
        let admission_guard = self.admissions.read().await;
        if self.closing.load(std::sync::atomic::Ordering::Acquire) {
            return Err(AgentFailure::Interrupted);
        }
        validate_request(&request, scope)?;
        let request_digest = delegation_request_digest(&request);
        if let Some(active) = self.active_task(request.task_id)? {
            validate_replay(&request, request_digest, &active.proposed)?;
            validate_endpoint(&active.proposed, &endpoint)?;
            drop(admission_guard);
            return receipt(
                wait_for_execution(&active, scope).await?,
                self.maximum_output_bytes,
            );
        }
        if let Some(record) = scope.run(self.repository.get(request.task_id)).await? {
            validate_replay(&request, request_digest, &record)?;
            validate_endpoint(&record, &endpoint)?;
            drop(admission_guard);
            return self.existing(record, &request, &endpoint, scope).await;
        }
        let proposed = TaskRecord {
            snapshot: snapshot(&request, TaskState::Submitted, None),
            admission: endpoint.admission.clone(),
            selection: endpoint.selection.clone(),
            invocation_key: request.invocation_key,
            request_digest,
            aggregate_revision: 1,
            executor_generation: self.executor_generation,
            execution_id: uuid::Uuid::new_v4(),
            device_id: request.execution_context.device_id.clone(),
            catalog_revision: request.selected_definition_revision,
            model_allowance: ModelReservationCeiling::for_lease(scope.budget()),
            receipt: None,
            maximum_output_bytes: self
                .maximum_output_bytes
                .min(request.execution_context.max_output_bytes),
            journal_revision: 0,
            journal_digest: floe_agent_runtime::journal_digest(&[])?,
        };
        proposed.validate_initial(self.maximum_output_bytes)?;
        let run_scope = scope.child_scope(
            scope.deadline(),
            proposed.model_allowance.tokens,
            proposed.model_allowance.cost_micros,
            Some(proposed.snapshot.task_id),
        );
        let (sender, completion) = watch::channel(None);
        let candidate = Arc::new(ActiveTask {
            proposed: proposed.clone(),
            cancellation: run_scope.cancellation().clone(),
            completion,
        });
        let (active, start) = {
            let mut tasks = self
                .active
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            if let Some(existing) = tasks.get(&request.task_id) {
                validate_replay(&request, request_digest, &existing.proposed)?;
                validate_endpoint(&existing.proposed, &endpoint)?;
                (Arc::clone(existing), false)
            } else {
                tasks.insert(request.task_id, Arc::clone(&candidate));
                (candidate, true)
            }
        };
        if start {
            if self.closing.load(std::sync::atomic::Ordering::Acquire) {
                active
                    .cancellation
                    .cancel_with_reason(floe_agent_contract::CancelReason::OwnerDropped);
            }
            let completion = ActiveCompletion {
                task: Arc::clone(&active),
                active: Arc::clone(&self.active),
                sender: Some(sender),
            };
            let coordinator = Arc::clone(self);
            // No await separates active registration from owner handoff. The
            // first storage mutation runs only after this driver owns it.
            tokio::spawn(async move {
                let result = coordinator
                    .admit_and_drive(proposed, request, endpoint, run_scope)
                    .await;
                completion.finish(result);
            });
        }
        drop(admission_guard);
        receipt(
            wait_for_execution(&active, scope).await?,
            self.maximum_output_bytes,
        )
    }

    async fn admit_and_drive(
        &self,
        proposed: TaskRecord,
        request: DelegationRequest,
        endpoint: crate::directory::ResolvedDirectoryEntry,
        scope: ExecutionScope,
    ) -> TaskOutcome {
        if let Some(failure) = running_failure(&scope) {
            return Err(failure);
        }
        let admitted = match self.repository.admit(proposed.clone()).await {
            Ok(TaskAdmission::Created(record)) => {
                if record != proposed {
                    return Err(AgentFailure::StorageUnavailable);
                }
                record
            }
            Ok(TaskAdmission::Existing(record)) => {
                validate_replay(&request, proposed.request_digest, &record)?;
                validate_endpoint(&record, &endpoint)?;
                record.validate(self.maximum_output_bytes)?;
                if terminal(record.snapshot.state) {
                    return verified_receipt(
                        self.repository.as_ref(),
                        &record,
                        self.maximum_output_bytes,
                    )
                    .await;
                }
                // A competing or abandoned execution is never dispatched by
                // this reservation, even when it has the same public Task ID.
                if record != proposed {
                    return Err(AgentFailure::Conflict);
                }
                record
            }
            Err(failure) => match self.repository.get(proposed.snapshot.task_id).await? {
                Some(record) if record == proposed => record,
                Some(_) => return Err(AgentFailure::Conflict),
                None => return Err(failure),
            },
        };
        admitted.validate_initial(self.maximum_output_bytes)?;
        let expected_working = admitted.transition(
            admitted.aggregate_revision,
            admitted.executor_generation,
            snapshot(&request, TaskState::Working, None),
            self.maximum_output_bytes,
        )?;
        let mut working = None;
        for attempt in 0..2 {
            match self
                .repository
                .compare_and_swap(
                    admitted.snapshot.task_id,
                    admitted.aggregate_revision,
                    admitted.executor_generation,
                    expected_working.snapshot.clone(),
                )
                .await
            {
                Ok(record) => {
                    if record != expected_working {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    working = Some(record);
                    break;
                }
                Err(failure) => {
                    let current = self
                        .repository
                        .get(admitted.snapshot.task_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if current == expected_working {
                        working = Some(current);
                        break;
                    }
                    if current != admitted {
                        return Err(AgentFailure::Conflict);
                    }
                    // Only an exact unchanged Submitted readback permits one
                    // same-CAS retry. Endpoint execution has not begun.
                    if attempt != 0 {
                        return Err(failure);
                    }
                }
            }
        }
        let working = working.ok_or(AgentFailure::StorageUnavailable)?;
        let invocation = EndpointInvocation {
            request,
            request_digest: working.request_digest,
            execution: working.execution(),
            journal: self.repository.journal(working.execution())?,
            resources: Arc::new(floe_agent_contract::EndpointResources::default()),
        };
        self.drive(working, invocation, endpoint, scope).await
    }

    async fn drive(
        &self,
        working: TaskRecord,
        invocation: EndpointInvocation,
        endpoint: crate::directory::ResolvedDirectoryEntry,
        scope: ExecutionScope,
    ) -> TaskOutcome {
        let retained_resources = Arc::clone(&invocation.resources);
        let missing = missing_requirement_keys(&working.selection);
        let mut settlement = None;
        let terminal_snapshot = if let Some(failure) = running_failure(&scope) {
            snapshot(&invocation.request, failure_state(failure), Some(failure))
        } else if !missing.is_empty() {
            TaskSnapshot {
                state: TaskState::Blocked,
                coverage: DependencyCoverage::Independent,
                blockage: Some(TaskBlockage::Binding {
                    requirement_keys: missing,
                }),
                ..snapshot(&invocation.request, TaskState::Working, None)
            }
        } else {
            // Do not drop a live endpoint on timeout. Signal its scope and wait
            // for endpoint return before deriving or committing terminal evidence.
            let future = endpoint.endpoint.execute(invocation.clone(), &scope);
            tokio::pin!(future);
            let outcome = tokio::select! {
                biased;
                outcome = &mut future => outcome,
                _ = scope.cancellation().cancelled() => future.await,
                _ = tokio::time::sleep_until(scope.deadline()) => {
                    scope.cancellation().cancel_with_reason(floe_agent_contract::CancelReason::Deadline);
                    future.await
                }
            };
            let outcome = outcome.and_then(|outcome| {
                match &outcome {
                    ExpertExecutionOutcome::Completed(report) => {
                        report.validate(&invocation, self.maximum_output_bytes)?;
                        if let Some(value) = &report.settlement {
                            self.repository.validate_settlement(value)?;
                        }
                    }
                    ExpertExecutionOutcome::Blocked(report) => report.validate(&invocation)?,
                }
                Ok(outcome)
            });
            // A validated endpoint completion is acknowledged work. A later
            // cancellation cannot erase its Output or private-state settlement.
            // Storage still checks the exact journal and live source fences.
            match outcome {
                Ok(ExpertExecutionOutcome::Completed(report)) => {
                    settlement = report.settlement;
                    TaskSnapshot {
                        state: TaskState::Completed,
                        result: Some(report.result),
                        artifacts: report.artifacts,
                        coverage: report.coverage,
                        ..snapshot(&invocation.request, TaskState::Working, None)
                    }
                }
                Ok(ExpertExecutionOutcome::Blocked(report)) => TaskSnapshot {
                    state: TaskState::Blocked,
                    coverage: report.coverage,
                    blockage: Some(report.blockage),
                    ..snapshot(&invocation.request, TaskState::Working, None)
                },
                Err(failure) => {
                    snapshot(&invocation.request, failure_state(failure), Some(failure))
                }
            }
        };
        // The Task's envelope has its own admitted bound. A typed endpoint
        // report that cannot fit is a terminal failure, never an unpersistable
        // Working Task or an oversized successful receipt.
        let terminal_snapshot = match terminal_snapshot.validate(working.maximum_output_bytes) {
            Ok(()) => terminal_snapshot,
            Err(failure) => {
                settlement = None;
                snapshot(&invocation.request, failure_state(failure), Some(failure))
            }
        };
        // Recording already completed work survives execution cancellation. It
        // performs no source/model I/O and never replenishes the Task allowance.
        let current = self
            .repository
            .get(working.snapshot.task_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let mut expected_working = working.clone();
        expected_working.journal_revision = current.journal_revision;
        expected_working.journal_digest = current.journal_digest;
        if current != expected_working {
            return Err(AgentFailure::Conflict);
        }
        let journal = self.repository.load_journal(current.execution()).await?;
        let commit = TaskExecutionCommit {
            execution: current.execution(),
            expected_task_revision: current.aggregate_revision,
            expected_journal_revision: current.journal_revision,
            terminal: terminal_snapshot,
            settlement,
        };
        let expected =
            settle_task_execution(&current, &commit, &journal, self.maximum_output_bytes)?;
        let expected_receipt = expected
            .receipt
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let mut saved = None;
        for attempt in 0..2 {
            match self.repository.settle_execution(commit.clone()).await {
                Ok(value) => {
                    if value != *expected_receipt {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    saved = Some(value);
                    break;
                }
                Err(failure) => {
                    let readback = self
                        .repository
                        .get(current.snapshot.task_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if readback == expected {
                        saved = Some(
                            verified_receipt(
                                self.repository.as_ref(),
                                &readback,
                                self.maximum_output_bytes,
                            )
                            .await?,
                        );
                        break;
                    }
                    if readback != current {
                        return Err(AgentFailure::Conflict);
                    }
                    // The same atomic commit either rejoins its receipt or
                    // rechecks all source/private-state fences before mutation.
                    if attempt != 0 {
                        return Err(failure);
                    }
                }
            }
        }
        let saved = saved.ok_or(AgentFailure::StorageUnavailable)?;
        let readback = self
            .repository
            .read_execution_receipt(saved.reference.clone())
            .await?;
        if readback != saved {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(retained_resources);
        Ok(saved)
    }
}

async fn completed(active: &ActiveTask) -> TaskOutcome {
    let mut receiver = active.completion.clone();
    loop {
        if let Some(outcome) = receiver.borrow().clone() {
            return outcome;
        }
        receiver
            .changed()
            .await
            .map_err(|_| AgentFailure::Interrupted)?;
    }
}

async fn wait_for_execution(active: &ActiveTask, scope: &ExecutionScope) -> TaskOutcome {
    tokio::select! {
        biased;
        outcome = completed(active) => outcome,
        _ = scope.cancellation().cancelled() => {
            active.cancellation.cancel_with_reason(scope.cancellation().reason().unwrap_or(floe_agent_contract::CancelReason::User));
            tokio::time::timeout(CANCEL_DRAIN_TIMEOUT, completed(active)).await
                .map_err(|_| running_failure(scope).unwrap_or(AgentFailure::Cancelled))?
        }
        _ = tokio::time::sleep_until(scope.deadline()) => {
            active.cancellation.cancel_with_reason(floe_agent_contract::CancelReason::Deadline);
            tokio::time::timeout(CANCEL_DRAIN_TIMEOUT, completed(active)).await
                .map_err(|_| AgentFailure::DeadlineExceeded)?
        }
    }
}

async fn verified_receipt<Repository: TaskRepository>(
    repository: &Repository,
    record: &TaskRecord,
    maximum_bytes: usize,
) -> TaskOutcome {
    record.validate(maximum_bytes)?;
    let receipt = record.receipt.as_ref().ok_or(AgentFailure::Conflict)?;
    let journal = repository.load_journal(record.execution()).await?;
    let projection = project_task_journal(record, &journal)?;
    if projection.journal_revision != receipt.reference.journal_revision
        || projection.journal_digest != receipt.journal_digest
        || projection.own_accounting != receipt.accounting
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    settle_task_execution(
        record,
        &TaskExecutionCommit {
            execution: record.execution(),
            expected_task_revision: record
                .aggregate_revision
                .checked_sub(1)
                .ok_or(AgentFailure::StorageUnavailable)?,
            expected_journal_revision: receipt.reference.journal_revision,
            terminal: record.snapshot.clone(),
            settlement: None,
        },
        &journal,
        maximum_bytes,
    )?;
    let saved = repository
        .read_execution_receipt(receipt.reference.clone())
        .await?;
    if saved != *receipt {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(saved)
}

fn receipt(
    execution: TaskExecutionReceipt,
    maximum_bytes: usize,
) -> Result<TaskReceipt, AgentFailure> {
    let receipt = TaskReceipt {
        task_id: execution.snapshot.task_id,
        snapshot: execution.snapshot.clone(),
        replay: None,
        execution: TaskExecutionEvidence::Admitted(execution),
    };
    receipt.validate(maximum_bytes)?;
    Ok(receipt)
}

fn authorize_task(
    record: &TaskRecord,
    principal: &str,
    parent_run_id: Option<uuid::Uuid>,
) -> Result<(), AgentFailure> {
    (record.snapshot.principal == principal && record.snapshot.parent_run_id == parent_run_id)
        .then_some(())
        .ok_or(AgentFailure::CapabilityDenied)
}

fn validate_query(
    principal: &str,
    parent_run_id: Option<uuid::Uuid>,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    if principal.trim().is_empty() || scope.root_run_id().map(|id| id.as_uuid()) != parent_run_id {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn validate_request(
    request: &DelegationRequest,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    validate_request_fields(request)?;
    if scope.task_id() != Some(request.task_id)
        || scope.root_run_id().map(|id| id.as_uuid()) != request.parent_run_id
    {
        return Err(AgentFailure::InvalidInput);
    }
    ModelReservationCeiling::for_lease(scope.budget()).validate()
}

fn validate_request_fields(request: &DelegationRequest) -> Result<(), AgentFailure> {
    if !request.task_id.is_valid()
        || request.invocation_key.as_uuid().is_nil()
        || request.principal.trim().is_empty()
        || request.selected_agent_id.trim().is_empty()
        || request.selected_definition_revision == 0
        || request.message.trim().is_empty()
        || request.message.len() > floe_agent_contract::MAX_OUTPUT_BYTES
        || request.context_refs.len() > 32
        || request
            .context_refs
            .iter()
            .any(|reference| reference.trim().is_empty() || reference.len() > 512)
        || request.parent_run_id.is_some_and(|id| id.is_nil())
    {
        return Err(AgentFailure::InvalidInput);
    }
    request.execution_context.validate()
}

fn validate_replay(
    request: &DelegationRequest,
    digest: [u8; 32],
    record: &TaskRecord,
) -> Result<(), AgentFailure> {
    if record.snapshot.task_id != request.task_id
        || record.snapshot.parent_run_id != request.parent_run_id
        || record.snapshot.principal != request.principal
        || record.snapshot.agent_id != request.selected_agent_id
        || record.snapshot.definition_revision != request.selected_definition_revision
        || record.invocation_key != request.invocation_key
        || record.request_digest != digest
        || record.device_id != request.execution_context.device_id
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

fn validate_endpoint(
    record: &TaskRecord,
    endpoint: &crate::directory::ResolvedDirectoryEntry,
) -> Result<(), AgentFailure> {
    if record.admission != endpoint.admission || record.selection != endpoint.selection {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

fn snapshot(
    request: &DelegationRequest,
    state: TaskState,
    issue: Option<AgentFailure>,
) -> TaskSnapshot {
    TaskSnapshot {
        task_id: request.task_id,
        parent_run_id: request.parent_run_id,
        principal: request.principal.clone(),
        agent_id: request.selected_agent_id.clone(),
        definition_revision: request.selected_definition_revision,
        state,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        issue,
        blockage: None,
    }
}

fn running_failure(scope: &ExecutionScope) -> Option<AgentFailure> {
    if scope.deadline() <= tokio::time::Instant::now() {
        return Some(AgentFailure::DeadlineExceeded);
    }
    scope
        .cancellation()
        .is_cancelled()
        .then(|| match scope.cancellation().reason() {
            Some(floe_agent_contract::CancelReason::Deadline) => AgentFailure::DeadlineExceeded,
            Some(floe_agent_contract::CancelReason::OwnerDropped) => AgentFailure::Interrupted,
            Some(floe_agent_contract::CancelReason::User) | None => AgentFailure::Cancelled,
        })
}

fn failure_state(failure: AgentFailure) -> TaskState {
    match failure {
        AgentFailure::CapabilityDenied
        | AgentFailure::PolicyDenied
        | AgentFailure::ConsentRequired => TaskState::Rejected,
        AgentFailure::Cancelled => TaskState::Cancelled,
        AgentFailure::DeadlineExceeded => TaskState::TimedOut,
        AgentFailure::Interrupted => TaskState::Interrupted,
        _ => TaskState::Failed,
    }
}

impl<Repository: TaskRepository + 'static> DelegationPort for RunExpertEnvironment<Repository> {
    fn delegate<'a>(
        &'a self,
        request: DelegationRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        Box::pin(async move {
            if request.principal != self.principal {
                return Err(AgentFailure::CapabilityDenied);
            }
            validate_request(&request, scope)?;
            let endpoint = self.snapshot.resolve(
                &request.selected_agent_id,
                request.selected_definition_revision,
            )?;
            self.coordinator.execute(request, scope, endpoint).await
        })
    }
}
