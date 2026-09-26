use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, BoxFuture, DelegationPort, DelegationRequest, DependencyCoverage,
    EndpointInvocation, EndpointSettlement, TaskId, TaskReceipt, TaskSnapshot, TaskState,
    delegation_request_digest,
};
use serde::{Deserialize, Serialize};

use crate::{Directory, DirectoryQuery, ExpertAdmissionIdentity};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    pub snapshot: TaskSnapshot,
    pub admission: ExpertAdmissionIdentity,
    pub invocation_key: floe_agent_contract::InvocationKey,
    pub request_digest: [u8; 32],
    pub aggregate_revision: u64,
    pub executor_generation: u64,
}

impl TaskRecord {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.snapshot.validate(maximum_bytes)?;
        if self.aggregate_revision == 0
            || self.executor_generation == 0
            || self.invocation_key.as_uuid().is_nil()
            || self.admission.validate_task(
                &self.snapshot.agent_id,
                self.snapshot.definition_revision,
            ).is_err()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let valid = match self.snapshot.state {
            TaskState::Submitted | TaskState::Working => {
                self.snapshot.result.is_none()
                    && self.snapshot.artifacts.is_empty()
                    && self.snapshot.issue.is_none()
                    && self.snapshot.coverage == DependencyCoverage::Unknown
            }
            TaskState::Completed => {
                self.snapshot
                    .result
                    .as_deref()
                    .is_some_and(|result| !result.trim().is_empty())
                    && self.snapshot.issue.is_none()
                    && self.snapshot.coverage != DependencyCoverage::Unknown
            }
            TaskState::Failed
            | TaskState::Rejected
            | TaskState::Cancelled
            | TaskState::TimedOut
            | TaskState::Interrupted => {
                self.snapshot.result.is_none()
                    && self.snapshot.artifacts.is_empty()
                    && self.snapshot.issue.is_some()
                    && self.snapshot.coverage == DependencyCoverage::Unknown
            }
        };
        valid.then_some(()).ok_or(AgentFailure::StorageUnavailable)
    }

    pub fn validate_initial(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.validate(maximum_bytes)?;
        if self.aggregate_revision != 1 || self.snapshot.state != TaskState::Submitted {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn transition(
        &self,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        maximum_bytes: usize,
    ) -> Result<Self, AgentFailure> {
        self.validate(maximum_bytes)?;
        if self.aggregate_revision != expected_aggregate_revision
            || self.executor_generation != executor_generation
        {
            return Err(AgentFailure::Conflict);
        }
        if snapshot.task_id != self.snapshot.task_id
            || snapshot.parent_run_id != self.snapshot.parent_run_id
            || snapshot.principal != self.snapshot.principal
            || snapshot.agent_id != self.snapshot.agent_id
            || snapshot.definition_revision != self.snapshot.definition_revision
            || !valid_transition(self.snapshot.state, snapshot.state)
        {
            return Err(AgentFailure::Conflict);
        }
        let next = Self {
            snapshot,
            admission: self.admission.clone(),
            invocation_key: self.invocation_key,
            request_digest: self.request_digest,
            aggregate_revision: self
                .aggregate_revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?,
            executor_generation: self.executor_generation,
        };
        next.validate(maximum_bytes)?;
        Ok(next)
    }

    pub fn interrupt_orphan(
        &self,
        executor_generation: u64,
        maximum_bytes: usize,
    ) -> Result<Option<Self>, AgentFailure> {
        self.validate(maximum_bytes)?;
        if executor_generation == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        if terminal(self.snapshot.state) || self.executor_generation == executor_generation {
            return Ok(None);
        }
        if self.executor_generation > executor_generation {
            return Err(AgentFailure::Conflict);
        }
        let snapshot = TaskSnapshot {
            state: TaskState::Interrupted,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: Some(AgentFailure::Interrupted),
            ..self.snapshot.clone()
        };
        let mut interrupted = self.transition(
            self.aggregate_revision,
            self.executor_generation,
            snapshot,
            maximum_bytes,
        )?;
        interrupted.executor_generation = executor_generation;
        Ok(Some(interrupted))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskAdmission {
    Created(TaskRecord),
    Existing(TaskRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskActivation {
    pub executor_generation: u64,
    pub interrupted: Vec<TaskRecord>,
}

pub trait TaskRepository: Send + Sync {
    fn activate<'a>(&'a self) -> BoxFuture<'a, Result<TaskActivation, AgentFailure>>;

    fn admit<'a>(
        &'a self,
        proposed: TaskRecord,
    ) -> BoxFuture<'a, Result<TaskAdmission, AgentFailure>>;

    fn compare_and_swap<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>>;

    fn validate_settlement(&self, settlement: &EndpointSettlement) -> Result<(), AgentFailure>;

    fn settle<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        settlement: Option<EndpointSettlement>,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>>;

    fn get<'a>(
        &'a self,
        task_id: TaskId,
    ) -> BoxFuture<'a, Result<Option<TaskRecord>, AgentFailure>>;
}

pub struct TaskCoordinator<Repository> {
    directory: Directory,
    repository: Arc<Repository>,
    purpose: String,
    maximum_output_bytes: usize,
    executor_generation: u64,
    active: Mutex<HashMap<TaskId, floe_agent_contract::Cancellation>>,
}

impl<Repository> TaskCoordinator<Repository> {
    pub async fn activate(
        directory: Directory,
        repository: Arc<Repository>,
        purpose: impl Into<String>,
        maximum_output_bytes: usize,
    ) -> Result<(Self, Vec<TaskReceipt>), AgentFailure>
    where
        Repository: TaskRepository,
    {
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
        let mut task_ids = std::collections::HashSet::new();
        let interrupted = activation
            .interrupted
            .into_iter()
            .map(|record| {
                record.validate(maximum_output_bytes)?;
                if record.snapshot.state != TaskState::Interrupted
                    || record.snapshot.issue != Some(AgentFailure::Interrupted)
                    || record.executor_generation != activation.executor_generation
                    || !task_ids.insert(record.snapshot.task_id)
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(receipt(record))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((
            Self {
                directory,
                repository,
                purpose,
                maximum_output_bytes,
                executor_generation: activation.executor_generation,
                active: Mutex::new(HashMap::new()),
            },
            interrupted,
        ))
    }
}

impl<Repository: TaskRepository> TaskCoordinator<Repository> {
    /// The Experts-owned catalog: cards admitted for this principal under the
    /// coordinator's purpose, with the definition revisions the Directory
    /// registered. Eligibility is the Directory's judgment alone; model
    /// placement never filters it.
    pub fn catalog(&self, principal: &str) -> Result<AllowedCatalog, AgentFailure> {
        self.directory.list_cards(DirectoryQuery {
            principal,
            purpose: &self.purpose,
        })
    }

    pub async fn get_task(
        &self,
        principal: &str,
        parent_run_id: Option<uuid::Uuid>,
        task_id: TaskId,
        scope: &floe_agent_contract::ExecutionScope,
    ) -> Result<Option<TaskReceipt>, AgentFailure> {
        if principal.trim().is_empty()
            || scope.root_run_id().map(|run_id| run_id.as_uuid()) != parent_run_id
        {
            return Err(AgentFailure::InvalidInput);
        }
        let record = scope.run(self.repository.get(task_id)).await?;
        record
            .map(|record| {
                authorize_task(&record, principal, parent_run_id)?;
                validate_owned_record(&record, self.maximum_output_bytes)?;
                Ok(TaskReceipt {
                    task_id,
                    snapshot: record.snapshot,
                    replay: None,
                })
            })
            .transpose()
    }

    pub async fn cancel_task(
        &self,
        principal: &str,
        parent_run_id: Option<uuid::Uuid>,
        task_id: TaskId,
        scope: &floe_agent_contract::ExecutionScope,
    ) -> Result<TaskReceipt, AgentFailure> {
        if principal.trim().is_empty()
            || scope.root_run_id().map(|run_id| run_id.as_uuid()) != parent_run_id
        {
            return Err(AgentFailure::InvalidInput);
        }
        let record = scope
            .run(async {
                self.repository
                    .get(task_id)
                    .await?
                    .ok_or(AgentFailure::NotFound)
            })
            .await?;
        authorize_task(&record, principal, parent_run_id)?;
        validate_owned_record(&record, self.maximum_output_bytes)?;
        if terminal(record.snapshot.state) {
            return Ok(receipt(record));
        }
        if let Some(cancellation) = self
            .active
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .get(&task_id)
            .cloned()
        {
            cancellation.cancel();
        }
        let cancelled =
            snapshot_from_record(&record, TaskState::Cancelled, Some(AgentFailure::Cancelled));
        let saved = match scope
            .run(self.repository.compare_and_swap(
                task_id,
                record.aggregate_revision,
                record.executor_generation,
                cancelled.clone(),
            ))
            .await
        {
            Ok(saved) => saved,
            Err(AgentFailure::Conflict) => {
                let current = scope
                    .run(self.repository.get(task_id))
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                authorize_task(&current, principal, parent_run_id)?;
                validate_owned_record(&current, self.maximum_output_bytes)?;
                if !terminal(current.snapshot.state) {
                    return Err(AgentFailure::Conflict);
                }
                return Ok(receipt(current));
            }
            Err(failure) => return Err(failure),
        };
        validate_saved_transition(&record, &saved, &cancelled, self.maximum_output_bytes)?;
        Ok(receipt(saved))
    }

    async fn execute(
        &self,
        request: DelegationRequest,
        scope: &floe_agent_contract::ExecutionScope,
    ) -> Result<TaskReceipt, AgentFailure> {
        validate_request(&request, scope)?;
        let request_digest = delegation_request_digest(&request);
        if let Some(record) = scope.run(self.repository.get(request.task_id)).await? {
            validate_replay(&request, request_digest, &record)?;
            validate_owned_record(&record, self.maximum_output_bytes)?;
            return Ok(TaskReceipt {
                task_id: request.task_id,
                snapshot: record.snapshot,
                replay: None,
            });
        }
        let query = DirectoryQuery {
            principal: &request.principal,
            purpose: &self.purpose,
        };
        let endpoint = self.directory.resolve(
            &request.selected_agent_id,
            request.selected_definition_revision,
            query.clone(),
        )?;
        let proposed = TaskRecord {
            snapshot: snapshot(
                &request,
                TaskState::Submitted,
                None,
                vec![],
                DependencyCoverage::Unknown,
                None,
            ),
            admission: endpoint.admission.clone(),
            invocation_key: request.invocation_key,
            request_digest,
            aggregate_revision: 1,
            executor_generation: self.executor_generation,
        };
        let admitted = scope.run(self.repository.admit(proposed)).await?;
        let admitted = match admitted {
            TaskAdmission::Created(record) => {
                validate_replay(&request, request_digest, &record)?;
                validate_owned_record(&record, self.maximum_output_bytes)?;
                if record.admission != endpoint.admission {
                    return Err(AgentFailure::StorageUnavailable);
                }
                record
            }
            TaskAdmission::Existing(record) => {
                validate_replay(&request, request_digest, &record)?;
                validate_owned_record(&record, self.maximum_output_bytes)?;
                if record.admission != endpoint.admission {
                    return Err(AgentFailure::Conflict);
                }
                return Ok(TaskReceipt {
                    task_id: request.task_id,
                    snapshot: record.snapshot,
                    replay: None,
                });
            }
        };
        let working_snapshot = snapshot(
            &request,
            TaskState::Working,
            None,
            vec![],
            DependencyCoverage::Unknown,
            None,
        );
        let working = scope
            .run(self.repository.compare_and_swap(
                request.task_id,
                admitted.aggregate_revision,
                admitted.executor_generation,
                working_snapshot.clone(),
            ))
            .await?;
        validate_saved_transition(
            &admitted,
            &working,
            &working_snapshot,
            self.maximum_output_bytes,
        )?;
        let invocation = EndpointInvocation {
            request: request.clone(),
            request_digest,
        };
        self.active
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .insert(request.task_id, scope.cancellation().clone());
        let current = match before_deadline(scope, self.repository.get(request.task_id)).await {
            Ok(current) => current,
            Err(failure) => {
                self.active
                    .lock()
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    .remove(&request.task_id);
                return Err(failure);
            }
        };
        if current
            .as_ref()
            .is_some_and(|record| terminal(record.snapshot.state))
        {
            self.active
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .remove(&request.task_id);
            return current.map(receipt).ok_or(AgentFailure::StorageUnavailable);
        }
        let outcome = scope
            .run(endpoint.endpoint.execute(invocation.clone(), scope))
            .await;
        self.active
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .remove(&request.task_id);
        let outcome = outcome.and_then(|report| {
            report.validate(&invocation, self.maximum_output_bytes)?;
            if let Some(settlement) = &report.settlement {
                self.repository.validate_settlement(settlement)?;
            }
            Ok(report)
        });
        let outcome = running_failure(scope).map_or(outcome, Err);
        let (terminal_snapshot, settlement) = match outcome {
            Ok(report) => (
                snapshot(
                    &request,
                    TaskState::Completed,
                    Some(report.result),
                    report.artifacts,
                    report.coverage,
                    None,
                ),
                report.settlement,
            ),
            Err(failure) => (
                snapshot(
                    &request,
                    failure_state(failure),
                    None,
                    vec![],
                    DependencyCoverage::Unknown,
                    Some(failure),
                ),
                None,
            ),
        };
        terminal_snapshot.validate(self.maximum_output_bytes)?;
        let completed = match before_deadline(
            scope,
            self.repository.settle(
                request.task_id,
                working.aggregate_revision,
                working.executor_generation,
                terminal_snapshot.clone(),
                settlement,
            ),
        )
        .await
        {
            Ok(completed) => completed,
            Err(AgentFailure::Conflict) => {
                let current = before_deadline(scope, self.repository.get(request.task_id))
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                validate_replay(&request, request_digest, &current)?;
                validate_owned_record(&current, self.maximum_output_bytes)?;
                if !terminal(current.snapshot.state) {
                    return Err(AgentFailure::Conflict);
                }
                return Ok(receipt(current));
            }
            Err(failure) => return Err(failure),
        };
        validate_saved_transition(
            &working,
            &completed,
            &terminal_snapshot,
            self.maximum_output_bytes,
        )?;
        Ok(TaskReceipt {
            task_id: request.task_id,
            snapshot: completed.snapshot,
            replay: None,
        })
    }
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

fn receipt(record: TaskRecord) -> TaskReceipt {
    TaskReceipt {
        task_id: record.snapshot.task_id,
        snapshot: record.snapshot,
        replay: None,
    }
}

fn terminal(state: TaskState) -> bool {
    !matches!(state, TaskState::Submitted | TaskState::Working)
}

fn snapshot_from_record(
    record: &TaskRecord,
    state: TaskState,
    issue: Option<AgentFailure>,
) -> TaskSnapshot {
    TaskSnapshot {
        state,
        result: None,
        artifacts: vec![],
        coverage: DependencyCoverage::Unknown,
        issue,
        ..record.snapshot.clone()
    }
}

fn running_failure(scope: &floe_agent_contract::ExecutionScope) -> Option<AgentFailure> {
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

async fn before_deadline<Value>(
    scope: &floe_agent_contract::ExecutionScope,
    future: impl Future<Output = Result<Value, AgentFailure>>,
) -> Result<Value, AgentFailure> {
    if scope.deadline() <= tokio::time::Instant::now() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    tokio::time::timeout_at(scope.deadline(), future)
        .await
        .unwrap_or(Err(AgentFailure::DeadlineExceeded))
}

impl<Repository: TaskRepository> DelegationPort for TaskCoordinator<Repository> {
    fn delegate<'a>(
        &'a self,
        request: DelegationRequest,
        scope: &'a floe_agent_contract::ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        Box::pin(self.execute(request, scope))
    }
}

fn validate_request(
    request: &DelegationRequest,
    scope: &floe_agent_contract::ExecutionScope,
) -> Result<(), AgentFailure> {
    if !request.task_id.is_valid()
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
        || scope.task_id() != Some(request.task_id)
        || scope.root_run_id().map(|run_id| run_id.as_uuid()) != request.parent_run_id
    {
        return Err(AgentFailure::InvalidInput);
    }
    request.execution_context.validate()?;
    Ok(())
}

fn validate_replay(
    request: &DelegationRequest,
    request_digest: [u8; 32],
    record: &TaskRecord,
) -> Result<(), AgentFailure> {
    if record.snapshot.task_id != request.task_id
        || record.snapshot.parent_run_id != request.parent_run_id
        || record.snapshot.principal != request.principal
        || record.snapshot.agent_id != request.selected_agent_id
        || record.snapshot.definition_revision != request.selected_definition_revision
        || record.invocation_key != request.invocation_key
        || record.request_digest != request_digest
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

fn validate_owned_record(record: &TaskRecord, maximum_bytes: usize) -> Result<(), AgentFailure> {
    record.validate(maximum_bytes)
}

fn validate_saved_transition(
    previous: &TaskRecord,
    saved: &TaskRecord,
    expected_snapshot: &TaskSnapshot,
    maximum_bytes: usize,
) -> Result<(), AgentFailure> {
    validate_owned_record(saved, maximum_bytes)?;
    if saved.snapshot != *expected_snapshot
        || saved.admission != previous.admission
        || saved.invocation_key != previous.invocation_key
        || saved.request_digest != previous.request_digest
        || saved.executor_generation != previous.executor_generation
        || saved.aggregate_revision
            != previous
                .aggregate_revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

fn snapshot(
    request: &DelegationRequest,
    state: TaskState,
    result: Option<String>,
    artifacts: Vec<floe_agent_contract::Artifact>,
    coverage: DependencyCoverage,
    issue: Option<AgentFailure>,
) -> TaskSnapshot {
    TaskSnapshot {
        task_id: request.task_id,
        parent_run_id: request.parent_run_id,
        principal: request.principal.clone(),
        agent_id: request.selected_agent_id.clone(),
        definition_revision: request.selected_definition_revision,
        state,
        result,
        artifacts,
        coverage,
        issue,
    }
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

fn valid_transition(previous: TaskState, next: TaskState) -> bool {
    match previous {
        TaskState::Submitted => matches!(
            next,
            TaskState::Working
                | TaskState::Rejected
                | TaskState::Cancelled
                | TaskState::TimedOut
                | TaskState::Interrupted
                | TaskState::Failed
        ),
        TaskState::Working => terminal(next),
        TaskState::Completed
        | TaskState::Failed
        | TaskState::Rejected
        | TaskState::Cancelled
        | TaskState::TimedOut
        | TaskState::Interrupted => false,
    }
}
