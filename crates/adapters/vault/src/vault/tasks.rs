use floe_agent_contract::{
    AgentFailure, DependencyCoverage, InvocationKey, MAX_OUTPUT_BYTES, TaskId, TaskSnapshot,
    TaskState,
};
use serde::{Deserialize, Serialize};
use turso::transaction::{Transaction, TransactionBehavior};

use super::*;

const SCHEMA_VERSION: i64 = 4;
const MAX_TASK_RECORD_BYTES: usize = 128 * 1024;
const MAX_TASK_ROWS: i64 = 4_096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VaultTaskRecord {
    pub snapshot: TaskSnapshot,
    pub admission: floe_experts::ExpertAdmissionIdentity,
    pub selection: floe_experts::ExpertExecutionSelection,
    pub invocation_key: InvocationKey,
    pub request_digest: [u8; 32],
    pub aggregate_revision: u64,
    pub executor_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VaultTaskAdmission {
    Created(VaultTaskRecord),
    Existing(VaultTaskRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultTaskActivation {
    pub executor_generation: u64,
    pub interrupted: Vec<VaultTaskRecord>,
}

impl VaultTaskRecord {
    fn validate(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        self.snapshot.validate(MAX_OUTPUT_BYTES)?;
        if self.snapshot.principal != person_id.to_string()
            || self.aggregate_revision == 0
            || self.executor_generation == 0
            || self.invocation_key.as_uuid().is_nil()
            || self.admission.validate_task(
                &self.snapshot.agent_id,
                self.snapshot.definition_revision,
            ).is_err()
            || self.selection.validate().is_err()
        {
            return Err(AgentFailure::CapabilityDenied);
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
        valid.then_some(()).ok_or(AgentFailure::VaultUnavailable)
    }

    fn validate_initial(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        self.validate(person_id)?;
        if self.snapshot.state != TaskState::Submitted || self.aggregate_revision != 1 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub(super) fn transition(
        &self,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        person_id: PersonId,
    ) -> Result<Self, AgentFailure> {
        self.validate(person_id)?;
        if self.aggregate_revision != expected_aggregate_revision
            || self.executor_generation != executor_generation
            || snapshot.task_id != self.snapshot.task_id
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
            selection: self.selection.clone(),
            invocation_key: self.invocation_key,
            request_digest: self.request_digest,
            aggregate_revision: self
                .aggregate_revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?,
            executor_generation: self.executor_generation,
        };
        next.validate(person_id)?;
        Ok(next)
    }

    fn interrupt(
        &self,
        executor_generation: u64,
        person_id: PersonId,
    ) -> Result<Self, AgentFailure> {
        if self.executor_generation >= executor_generation || terminal(self.snapshot.state) {
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
            person_id,
        )?;
        interrupted.executor_generation = executor_generation;
        Ok(interrupted)
    }

    fn exact_admission(&self, proposed: &Self) -> bool {
        self.snapshot.task_id == proposed.snapshot.task_id
            && self.snapshot.parent_run_id == proposed.snapshot.parent_run_id
            && self.snapshot.principal == proposed.snapshot.principal
            && self.snapshot.agent_id == proposed.snapshot.agent_id
            && self.snapshot.definition_revision == proposed.snapshot.definition_revision
            && self.admission == proposed.admission
            && self.selection == proposed.selection
            && self.invocation_key == proposed.invocation_key
            && self.request_digest == proposed.request_digest
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_task_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = initialize(&transaction).await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn activate_task_executor(&self) -> Result<VaultTaskActivation, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let current_generation = executor_generation(&transaction).await?;
            let next_generation = current_generation
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            let mut rows = transaction
                .query(
                    "SELECT task_id FROM agent_tasks WHERE state IN ('submitted', 'working') ORDER BY task_id LIMIT 4097",
                    (),
                )
                .await
                .map_err(storage)?;
            let mut task_ids = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                task_ids.push(parse_task_id(&row.get::<String>(0).map_err(storage)?)?);
            }
            drop(rows);
            if task_ids.len() > usize::try_from(MAX_TASK_ROWS).unwrap_or(usize::MAX) {
                return Err(AgentFailure::BudgetExceeded);
            }
            let mut interrupted = Vec::with_capacity(task_ids.len());
            for task_id in task_ids {
                let current = self
                    .task_on(&transaction, task_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                let next = current.interrupt(next_generation, self.person_id)?;
                let changed = write_task(
                    &transaction,
                    &next,
                    current.aggregate_revision,
                    current.executor_generation,
                )
                .await?;
                if changed != 1 {
                    return Err(AgentFailure::Conflict);
                }
                interrupted.push(next);
            }
            let changed = transaction
                .execute(
                    "UPDATE agent_task_executor SET generation = ? WHERE id = 1 AND generation = ?",
                    (integer(next_generation)?, integer(current_generation)?),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(VaultTaskActivation {
                executor_generation: next_generation,
                interrupted,
            })
        }
        .await;
        let activation = self
            .finish_registry_transaction_checked(transaction, result)
            .await?;
        self.task_executor_generation
            .store(activation.executor_generation, Ordering::Release);
        Ok(activation)
    }

    pub async fn admit_task(
        &self,
        proposed: VaultTaskRecord,
    ) -> Result<VaultTaskAdmission, AgentFailure> {
        proposed.validate_initial(self.person_id)?;
        let payload = encode(&proposed)?;
        let task_id = proposed.snapshot.task_id;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            if self.task_executor_generation.load(Ordering::Acquire) == 0 {
                return Err(AgentFailure::Conflict);
            }
            if let Some(existing) = self.task_on(&transaction, task_id).await? {
                return if existing.exact_admission(&proposed) {
                    Ok(VaultTaskAdmission::Existing(existing))
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
            if proposed.executor_generation != self.active_executor_generation(&transaction).await? {
                return Err(AgentFailure::Conflict);
            }
            let registry = self.registry_on(&transaction).await?.ok_or(AgentFailure::NotFound)?;
            floe_experts::AgentRegistry::restore(registry, self.registry_instance_id())?
                .validate_current_execution_selection(
                    self.person_id,
                    &proposed.admission,
                    &proposed.selection,
                    true,
                )?;
            let mut count = transaction
                .query("SELECT count(*) FROM agent_tasks", ())
                .await
                .map_err(storage)?;
            let rows = count
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            if rows >= MAX_TASK_ROWS {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction
                .execute(
                    "INSERT INTO agent_tasks (task_id, invocation_key, person_id, state, aggregate_revision, executor_generation, payload) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        task_id.as_uuid().to_string(),
                        proposed.invocation_key.as_uuid().to_string(),
                        self.person_id.to_string(),
                        state_name(proposed.snapshot.state),
                        integer(proposed.aggregate_revision)?,
                        integer(proposed.executor_generation)?,
                        payload,
                    ),
                )
                .await
                .map_err(|error| match error {
                    turso::Error::Constraint(_) => AgentFailure::Conflict,
                    other => storage(other),
                })?;
            self.check_access()?;
            Ok(VaultTaskAdmission::Created(proposed))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn compare_and_swap_task(
        &self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
    ) -> Result<VaultTaskRecord, AgentFailure> {
        if snapshot.task_id != task_id {
            return Err(AgentFailure::Conflict);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            if executor_generation != self.active_executor_generation(&transaction).await? {
                return Err(AgentFailure::Conflict);
            }
            let current = self
                .task_on(&transaction, task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let next = current.transition(
                expected_aggregate_revision,
                executor_generation,
                snapshot,
                self.person_id,
            )?;
            if write_task(
                &transaction,
                &next,
                expected_aggregate_revision,
                executor_generation,
            )
            .await?
                != 1
            {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(next)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn task(&self, task_id: TaskId) -> Result<Option<VaultTaskRecord>, AgentFailure> {
        if !task_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let record = self.task_on(&self.connection()?, task_id).await?;
        self.check_access()?;
        Ok(record)
    }

    pub(super) async fn task_on(
        &self,
        connection: &turso::Connection,
        task_id: TaskId,
    ) -> Result<Option<VaultTaskRecord>, AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT invocation_key, person_id, state, aggregate_revision, executor_generation, payload FROM agent_tasks WHERE task_id = ? AND length(CAST(payload AS BLOB)) <= 131072",
                [task_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Ok(None);
        };
        let record: VaultTaskRecord =
            serde_json::from_str(&row.get::<String>(5).map_err(storage)?).map_err(unavailable)?;
        record.validate(self.person_id).map_err(unavailable)?;
        if record.snapshot.task_id != task_id
            || row.get::<String>(0).map_err(storage)? != record.invocation_key.as_uuid().to_string()
            || row.get::<String>(1).map_err(storage)? != self.person_id.to_string()
            || row.get::<String>(2).map_err(storage)? != state_name(record.snapshot.state)
            || row.get::<i64>(3).map_err(storage)? != integer(record.aggregate_revision)?
            || row.get::<i64>(4).map_err(storage)? != integer(record.executor_generation)?
            || rows.next().await.map_err(storage)?.is_some()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(Some(record))
    }

    pub(super) async fn active_executor_generation(
        &self,
        connection: &turso::Connection,
    ) -> Result<u64, AgentFailure> {
        let active = self.task_executor_generation.load(Ordering::Acquire);
        if active == 0 || active != executor_generation_on(connection).await? {
            return Err(AgentFailure::Conflict);
        }
        Ok(active)
    }
}

async fn initialize(transaction: &Transaction<'_>) -> Result<(), AgentFailure> {
    let mut tables = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_task_schema', 'agent_task_executor', 'agent_tasks')",
            (),
        )
        .await
        .map_err(storage)?;
    let mut found = Vec::new();
    while let Some(row) = tables.next().await.map_err(storage)? {
        found.push(row.get::<String>(0).map_err(storage)?);
    }
    found.sort();
    if found.is_empty() {
        transaction
            .execute(
                "CREATE TABLE agent_task_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 4))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_task_executor (id INTEGER PRIMARY KEY CHECK (id = 1), generation INTEGER NOT NULL CHECK (generation >= 0))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_tasks (task_id TEXT PRIMARY KEY, invocation_key TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('submitted', 'working', 'completed', 'failed', 'rejected', 'cancelled', 'timed_out', 'interrupted')), aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_tasks_recovery ON agent_tasks (state, executor_generation, task_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_task_schema (id, version) VALUES (1, 4)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_task_executor (id, generation) VALUES (1, 0)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found
        != [
            "agent_task_executor".to_owned(),
            "agent_task_schema".to_owned(),
            "agent_tasks".to_owned(),
        ]
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut marker = transaction
        .query("SELECT id, version FROM agent_task_schema", ())
        .await
        .map_err(storage)?;
    let row = marker
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?;
    if row.get::<i64>(0).map_err(storage)? != 1
        || row.get::<i64>(1).map_err(storage)? != SCHEMA_VERSION
        || marker.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    transaction
        .query(
            "SELECT task_id, invocation_key, person_id, state, aggregate_revision, executor_generation, payload FROM agent_tasks LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    let mut index = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'agent_tasks_recovery' AND tbl_name = 'agent_tasks'",
            (),
        )
        .await
        .map_err(storage)?;
    if index.next().await.map_err(storage)?.is_none()
        || index.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    executor_generation(transaction).await?;
    Ok(())
}

async fn executor_generation(connection: &turso::Connection) -> Result<u64, AgentFailure> {
    let generation = executor_generation_on(connection).await?;
    if generation > i64::MAX as u64 {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(generation)
}

async fn executor_generation_on(connection: &turso::Connection) -> Result<u64, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT generation FROM agent_task_executor WHERE id = 1",
            (),
        )
        .await
        .map_err(storage)?;
    let value = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    if value < 0 || rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(value as u64)
}

pub(super) async fn write_task(
    transaction: &Transaction<'_>,
    next: &VaultTaskRecord,
    expected_revision: u64,
    expected_generation: u64,
) -> Result<u64, AgentFailure> {
    let payload = encode(next)?;
    transaction
        .execute(
            "UPDATE agent_tasks SET state = ?, aggregate_revision = ?, executor_generation = ?, payload = ? WHERE task_id = ? AND aggregate_revision = ? AND executor_generation = ?",
            (
                state_name(next.snapshot.state),
                integer(next.aggregate_revision)?,
                integer(next.executor_generation)?,
                payload,
                next.snapshot.task_id.as_uuid().to_string(),
                integer(expected_revision)?,
                integer(expected_generation)?,
            ),
        )
        .await
        .map_err(storage)
}

fn encode(record: &VaultTaskRecord) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_TASK_RECORD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

fn parse_task_id(value: &str) -> Result<TaskId, AgentFailure> {
    TaskId::from_uuid(uuid::Uuid::parse_str(value).map_err(unavailable)?)
        .ok_or(AgentFailure::VaultUnavailable)
}

fn terminal(state: TaskState) -> bool {
    !matches!(state, TaskState::Submitted | TaskState::Working)
}

fn valid_transition(previous: TaskState, next: TaskState) -> bool {
    match previous {
        TaskState::Submitted => matches!(
            next,
            TaskState::Working
                | TaskState::Failed
                | TaskState::Rejected
                | TaskState::Cancelled
                | TaskState::TimedOut
                | TaskState::Interrupted
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

fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Rejected => "rejected",
        TaskState::Cancelled => "cancelled",
        TaskState::TimedOut => "timed_out",
        TaskState::Interrupted => "interrupted",
    }
}

#[cfg(test)]
mod tests;
