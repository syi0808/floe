use floe_agent_contract::{
    AgentFailure, DependencyCoverage, JournalEntry, JournalEvent,
    TaskExecutionKey, TaskExecutionReceipt, TaskExecutionReceiptRef, TaskId, TaskSnapshot,
    TaskState, MAX_OUTPUT_BYTES,
};
use floe_experts::{
    advance_task_journal, interrupt_task_execution, settle_task_execution as settle_task_record_execution,
    validate_task_journal, MAX_TASK_RECORD_BYTES,
    ExpertSettlement, TaskActivation, TaskAdmission, TaskExecutionCommit, TaskRecord,
};
use turso::transaction::{Transaction, TransactionBehavior};

use super::*;

const SCHEMA_VERSION: i64 = 5;
const MAX_TASK_JOURNAL_ENTRY_BYTES: usize = 128 * 1024;
const MAX_TASK_JOURNAL_ENTRIES: usize = 512;
const MAX_TASK_ROWS: i64 = 4_096;

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

    pub async fn activate_task_executor(&self) -> Result<TaskActivation, AgentFailure> {
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

            // The immediate transaction fences concurrent old-executor writes;
            // advance the durable fence before deriving any interruption.
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
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let journal = self.task_journal_on(&transaction, &current).await?;
                let next = interrupt_task_execution(
                    &current,
                    next_generation,
                    &journal,
                    MAX_OUTPUT_BYTES,
                )?
                .ok_or(AgentFailure::Conflict)?;
                if write_task(
                    &transaction,
                    &next,
                    current.aggregate_revision,
                    current.executor_generation,
                )
                .await?
                    != 1
                {
                    return Err(AgentFailure::Conflict);
                }
                interrupted.push(next);
            }
            self.check_access()?;
            Ok(TaskActivation {
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

    pub async fn admit_task(&self, proposed: TaskRecord) -> Result<TaskAdmission, AgentFailure> {
        proposed.validate_initial(MAX_OUTPUT_BYTES)?;
        if proposed.snapshot.principal != self.person_id.to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
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
                return if exact_admission(&existing, &proposed) {
                    Ok(TaskAdmission::Existing(existing))
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
            if proposed.executor_generation != self.active_executor_generation(&transaction).await? {
                return Err(AgentFailure::Conflict);
            }
            let mut count = transaction
                .query("SELECT count(*) FROM agent_tasks", ())
                .await
                .map_err(storage)?;
            let rows = count
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::StorageUnavailable)?
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
            Ok(TaskAdmission::Created(proposed))
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
    ) -> Result<TaskRecord, AgentFailure> {
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
                MAX_OUTPUT_BYTES,
            )?;
            if write_task(
                &transaction,
                &next,
                expected_aggregate_revision,
                current.executor_generation,
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

    pub async fn settle_task_execution(
        &self,
        commit: TaskExecutionCommit,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        commit.execution.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let current = self
                .task_on(&transaction, commit.execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.execution() != commit.execution {
                return Err(AgentFailure::Conflict);
            }
            let replay = terminal(current.snapshot.state);
            if !replay {
                let active = self.active_executor_generation(&transaction).await?;
                if commit.execution.executor_generation != active {
                    return Err(AgentFailure::Conflict);
                }
            }

            let journal = self.task_journal_on(&transaction, &current).await?;
            let next = settle_task_record_execution(
                &current,
                &commit,
                &journal,
                MAX_OUTPUT_BYTES,
            )?;
            if replay {
                if next != current {
                    return Err(AgentFailure::Conflict);
                }
                return current
                    .receipt
                    .ok_or(AgentFailure::StorageUnavailable);
            }

            self.validate_context_dependency_coverage_in_transaction(
                &transaction,
                &commit.terminal.coverage,
            )
            .await?;

            if let Some(endpoint_settlement) = commit.settlement.as_ref() {
                let settlement = ExpertSettlement::from_endpoint_settlement(
                    endpoint_settlement,
                    &current.snapshot.agent_id,
                )?;
                let coverage = if settlement.dependencies.is_empty() {
                    DependencyCoverage::Independent
                } else {
                    DependencyCoverage::Dependent {
                        dependencies: settlement.dependencies.clone(),
                    }
                };
                coverage
                    .validate()
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                if settlement.admission.assignment_id.is_nil()
                    || settlement.invocation_id.is_nil()
                    || settlement.owner() != commit.terminal.agent_id
                    || commit.terminal.task_id != commit.execution.task_id
                    || commit.terminal.principal != self.person_id.to_string()
                    || commit.terminal.state != TaskState::Completed
                    || commit.terminal.coverage != coverage
                    || commit.terminal.result.as_deref()
                        != Some(settlement.task_result.as_str())
                    || settlement.next_private_state.schema_version != 1
                    || settlement.next_private_state.last_invocation_id
                        != Some(settlement.invocation_id)
                    || settlement.expected_private_state_revision.checked_add(1)
                        != Some(settlement.next_private_state.revision)
                    || settlement.next_private_state.completed_invocations
                        != settlement.next_private_state.revision
                {
                    return Err(AgentFailure::Conflict);
                }

                let mut registry = self
                    .registry_on(&transaction)
                    .await?
                    .ok_or(AgentFailure::NotFound)?;
                if registry.instance_id != settlement.admission.registry_instance_id {
                    return Err(AgentFailure::Conflict);
                }
                let assignment = registry
                    .assignments
                    .iter_mut()
                    .find(|assignment| {
                        assignment.id == settlement.admission.assignment_id
                            && assignment.person_id == self.person_id
                    })
                    .ok_or(AgentFailure::Conflict)?;
                if assignment.installation_id != settlement.admission.installation_id
                    || assignment.private_state.revision
                        != settlement.expected_private_state_revision
                    || assignment.private_state.completed_invocations
                        != settlement.expected_private_state_revision
                    || assignment.private_state.last_invocation_id
                        == Some(settlement.invocation_id)
                    || !registry.installations.iter().any(|installation| {
                        installation.id == assignment.installation_id
                            && installation.package == settlement.admission.package
                    })
                    || settlement.admission.definition_revision
                        != commit.terminal.definition_revision
                    || current.admission != settlement.admission
                    || current.invocation_key.as_uuid() != settlement.invocation_id
                {
                    return Err(AgentFailure::Conflict);
                }
                assignment.private_state = settlement.next_private_state;
                let previous_revision = registry.revision;
                registry.revision = previous_revision
                    .checked_add(1)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let payload = self.registry_payload(&registry)?;
                self.update_registry(
                    &transaction,
                    previous_revision,
                    registry.revision,
                    payload,
                )
                .await?;
            }

            if write_task(
                &transaction,
                &next,
                current.aggregate_revision,
                current.executor_generation,
            )
            .await?
                != 1
            {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            next.receipt.ok_or(AgentFailure::StorageUnavailable)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn task(&self, task_id: TaskId) -> Result<Option<TaskRecord>, AgentFailure> {
        if !task_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred).await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = self.task_on(&transaction, task_id).await;
        self.finish_registry_transaction_checked(transaction, result).await
    }

    pub async fn load_task_journal(
        &self,
        execution: TaskExecutionKey,
    ) -> Result<Vec<JournalEntry>, AgentFailure> {
        execution.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred).await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
        let record = self
            .task_on(&transaction, execution.task_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.execution() != execution {
            return Err(AgentFailure::Conflict);
        }
        let entries = self.task_journal_on(&transaction, &record).await?;
        self.check_access()?;
        Ok(entries)
        }.await;
        self.finish_registry_transaction_checked(transaction, result).await
    }

    pub async fn read_task_execution_receipt(
        &self,
        reference: TaskExecutionReceiptRef,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred).await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = self.read_execution_receipt_on(&transaction, &reference).await;
        self.finish_registry_transaction_checked(transaction, result).await
    }

    pub async fn append_task_journal(
        &self,
        execution: TaskExecutionKey,
        phase: &str,
        event: JournalEvent,
    ) -> Result<u64, AgentFailure> {
        execution.validate()?;
        let kind = task_journal_kind(&event).ok_or(AgentFailure::CapabilityDenied)?;
        if phase != kind {
            return Err(AgentFailure::InvalidInput);
        }
        let payload = serde_json::to_string(&event).map_err(|_| AgentFailure::StorageUnavailable)?;
        if payload.is_empty() || payload.len() > MAX_TASK_JOURNAL_ENTRY_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let current = self
                .task_on(&transaction, execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.execution() != execution
                || current.snapshot.state != TaskState::Working
                || current.executor_generation
                    != self.active_executor_generation(&transaction).await?
            {
                return Err(AgentFailure::Conflict);
            }
            let mut entries = self.task_journal_on(&transaction, &current).await?;
            if entries.len() >= MAX_TASK_JOURNAL_ENTRIES {
                return Err(AgentFailure::BudgetExceeded);
            }
            let revision = (entries.len() as u64)
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?;
            entries.push(JournalEntry {
                revision,
                event,
            });
            let next = advance_task_journal(&current, &entries)?;
            transaction
                .execute(
                    "INSERT INTO agent_task_journal (task_id, execution_id, executor_generation, revision, kind, payload) VALUES (?, ?, ?, ?, ?, ?)",
                    (
                        execution.task_id.as_uuid().to_string(),
                        execution.execution_id.to_string(),
                        integer(execution.executor_generation)?,
                        integer(revision)?,
                        kind,
                        payload,
                    ),
                )
                .await
                .map_err(|error| match error {
                    turso::Error::Constraint(_) => AgentFailure::Conflict,
                    other => storage(other),
                })?;
            if write_task(&transaction, &next, current.aggregate_revision, current.executor_generation).await? != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(revision)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub(super) async fn task_on(
        &self,
        connection: &turso::Connection,
        task_id: TaskId,
    ) -> Result<Option<TaskRecord>, AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT invocation_key, person_id, state, aggregate_revision, executor_generation, payload FROM agent_tasks WHERE task_id = ?",
                [task_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            drop(rows);
            let mut orphan = connection.query("SELECT 1 FROM agent_task_journal WHERE task_id = ? LIMIT 1",
                [task_id.as_uuid().to_string()]).await.map_err(storage)?;
            if orphan.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
            return Ok(None);
        };
        let payload = row.get::<String>(5).map_err(storage)?;
        if payload.is_empty() || payload.len() > MAX_TASK_RECORD_BYTES {
            return Err(AgentFailure::StorageUnavailable);
        }
        let record: TaskRecord = serde_json::from_str(&payload).map_err(unavailable)?;
        record.validate(MAX_OUTPUT_BYTES).map_err(unavailable)?;
        if record.snapshot.task_id != task_id
            || record.snapshot.principal != self.person_id.to_string()
            || row.get::<String>(0).map_err(storage)?
                != record.invocation_key.as_uuid().to_string()
            || row.get::<String>(1).map_err(storage)? != self.person_id.to_string()
            || row.get::<String>(2).map_err(storage)? != state_name(record.snapshot.state)
            || row.get::<i64>(3).map_err(storage)? != integer(record.aggregate_revision)?
            || row.get::<i64>(4).map_err(storage)? != integer(record.executor_generation)?
            || rows.next().await.map_err(storage)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        self.task_journal_on(connection, &record).await?;
        Ok(Some(record))
    }

    pub(super) async fn read_execution_receipt_on(
        &self,
        connection: &turso::Connection,
        reference: &TaskExecutionReceiptRef,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        self.check_access()?;
        reference.validate()?;
        let record = self
            .task_on(connection, reference.execution.task_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.snapshot.principal != self.person_id.to_string()
            || record.execution() != reference.execution
        {
            return Err(AgentFailure::Conflict);
        }
        let stored = record.receipt.as_ref().ok_or(AgentFailure::Conflict)?;
        if stored.reference != *reference {
            return Err(AgentFailure::Conflict);
        }
        let journal = self.task_journal_on(connection, &record).await?;
        let expected_task_revision = record
            .aggregate_revision
            .checked_sub(1)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let identical_replay = TaskExecutionCommit {
            execution: reference.execution,
            expected_task_revision,
            expected_journal_revision: reference.journal_revision,
            terminal: record.snapshot.clone(),
            settlement: None,
        };
        let verified = settle_task_record_execution(
            &record,
            &identical_replay,
            &journal,
            MAX_OUTPUT_BYTES,
        )?;
        if verified != record {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.check_access()?;
        Ok(stored.clone())
    }

    async fn task_journal_on(
        &self,
        connection: &turso::Connection,
        record: &TaskRecord,
    ) -> Result<Vec<JournalEntry>, AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT task_id, execution_id, executor_generation, revision, kind, payload FROM agent_task_journal WHERE task_id = ? ORDER BY revision LIMIT 513",
                [record.snapshot.task_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        let execution = record.execution();
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let task_id = row.get::<String>(0).map_err(storage)?;
            let execution_id = row.get::<String>(1).map_err(storage)?;
            let executor_generation = row.get::<i64>(2).map_err(storage)?;
            let revision = u64::try_from(row.get::<i64>(3).map_err(storage)?)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let kind = row.get::<String>(4).map_err(storage)?;
            let payload = row.get::<String>(5).map_err(storage)?;
            if task_id != execution.task_id.as_uuid().to_string()
                || execution_id != execution.execution_id.to_string()
                || executor_generation != integer(execution.executor_generation)?
                || revision != entries.len() as u64 + 1
                || kind.is_empty()
                || kind.len() > 64
                || payload.is_empty()
                || payload.len() > MAX_TASK_JOURNAL_ENTRY_BYTES
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let event: JournalEvent =
                serde_json::from_str(&payload).map_err(|_| AgentFailure::StorageUnavailable)?;
            if task_journal_kind(&event) != Some(kind.as_str()) {
                return Err(AgentFailure::StorageUnavailable);
            }
            entries.push(JournalEntry { revision, event });
        }
        if entries.len() > MAX_TASK_JOURNAL_ENTRIES {
            return Err(AgentFailure::StorageUnavailable);
        }
        validate_task_journal(record, &entries)?;
        validate_terminal_journal(record, &entries)?;
        Ok(entries)
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
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_task_schema', 'agent_task_executor', 'agent_tasks', 'agent_task_journal')",
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
                "CREATE TABLE agent_task_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 5))",
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
                "CREATE TABLE agent_tasks (task_id TEXT PRIMARY KEY, invocation_key TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('submitted', 'working', 'completed', 'blocked', 'failed', 'rejected', 'cancelled', 'timed_out', 'interrupted')), aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 524288))",
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
                "CREATE TABLE agent_task_journal (task_id TEXT NOT NULL, execution_id TEXT NOT NULL, executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 512), kind TEXT NOT NULL CHECK (kind IN ('intent', 'result', 'output', 'checkpoint')), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072), PRIMARY KEY (task_id, execution_id, executor_generation, revision))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_task_journal_task_revision ON agent_task_journal (task_id, revision)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_task_schema (id, version) VALUES (1, 5)",
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

    let expected = [
        "agent_task_executor".to_owned(),
        "agent_task_journal".to_owned(),
        "agent_task_schema".to_owned(),
        "agent_tasks".to_owned(),
    ];
    if found != expected {
        return Err(if found == [
            "agent_task_executor".to_owned(),
            "agent_task_schema".to_owned(),
            "agent_tasks".to_owned(),
        ] {
            AgentFailure::UnsupportedVersion
        } else {
            AgentFailure::VaultUnavailable
        });
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
    let version = row.get::<i64>(1).map_err(storage)?;
    if row.get::<i64>(0).map_err(storage)? != 1
        || version != SCHEMA_VERSION
        || marker.next().await.map_err(storage)?.is_some()
    {
        return Err(if version < SCHEMA_VERSION {
            AgentFailure::UnsupportedVersion
        } else {
            AgentFailure::VaultUnavailable
        });
    }
    transaction
        .query(
            "SELECT task_id, invocation_key, person_id, state, aggregate_revision, executor_generation, payload FROM agent_tasks LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction
        .query(
            "SELECT task_id, execution_id, executor_generation, revision, kind, payload FROM agent_task_journal LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    for (name, table) in [
        ("agent_tasks_recovery", "agent_tasks"),
        ("agent_task_journal_task_revision", "agent_task_journal"),
    ] {
        let mut index = transaction
            .query(
                "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = ? AND tbl_name = ?",
                (name, table),
            )
            .await
            .map_err(storage)?;
        if index.next().await.map_err(storage)?.is_none()
            || index.next().await.map_err(storage)?.is_some()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
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

async fn write_task(
    transaction: &Transaction<'_>,
    next: &TaskRecord,
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

fn encode(record: &TaskRecord) -> Result<String, AgentFailure> {
    record.validate(MAX_OUTPUT_BYTES)?;
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_TASK_RECORD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

fn exact_admission(existing: &TaskRecord, proposed: &TaskRecord) -> bool {
    existing.snapshot.task_id == proposed.snapshot.task_id
        && existing.snapshot.parent_run_id == proposed.snapshot.parent_run_id
        && existing.snapshot.principal == proposed.snapshot.principal
        && existing.snapshot.agent_id == proposed.snapshot.agent_id
        && existing.snapshot.definition_revision == proposed.snapshot.definition_revision
        && existing.admission == proposed.admission
        && existing.selection == proposed.selection
        && existing.invocation_key == proposed.invocation_key
        && existing.request_digest == proposed.request_digest
        && existing.execution_id == proposed.execution_id
        && existing.device_id == proposed.device_id
        && existing.catalog_revision == proposed.catalog_revision
        && existing.model_allowance == proposed.model_allowance
        && existing.maximum_output_bytes == proposed.maximum_output_bytes
}

fn validate_terminal_journal(
    record: &TaskRecord,
    entries: &[JournalEntry],
) -> Result<(), AgentFailure> {
    if !terminal(record.snapshot.state) {
        return Ok(());
    }
    let receipt = record
        .receipt
        .as_ref()
        .ok_or(AgentFailure::StorageUnavailable)?;
    let expected_task_revision = record
        .aggregate_revision
        .checked_sub(1)
        .ok_or(AgentFailure::StorageUnavailable)?;
    let replay = TaskExecutionCommit {
        execution: record.execution(),
        expected_task_revision,
        expected_journal_revision: receipt.reference.journal_revision,
        terminal: record.snapshot.clone(),
        settlement: None,
    };
    let verified = settle_task_record_execution(record, &replay, entries, MAX_OUTPUT_BYTES)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if verified != *record {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

fn task_journal_kind(event: &JournalEvent) -> Option<&'static str> {
    match event {
        JournalEvent::ModelIntent { .. } | JournalEvent::ToolIntent { .. } => Some("intent"),
        JournalEvent::ModelResult { .. }
        | JournalEvent::ToolResult { .. }
        | JournalEvent::ToolReviewRequired { .. } => Some("result"),
        JournalEvent::Output { .. } => Some("output"),
        JournalEvent::Checkpoint { .. }
        | JournalEvent::ValidatedBatch { .. }
        | JournalEvent::BatchProgress { .. } => Some("checkpoint"),
        JournalEvent::DelegationIntent { .. }
        | JournalEvent::DelegationResult { .. }
        | JournalEvent::FinalizationStarted { .. } => None,
    }
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

fn parse_task_id(value: &str) -> Result<TaskId, AgentFailure> {
    TaskId::from_uuid(uuid::Uuid::parse_str(value).map_err(unavailable)?)
        .ok_or(AgentFailure::StorageUnavailable)
}

fn terminal(state: TaskState) -> bool {
    !matches!(state, TaskState::Submitted | TaskState::Working)
}

fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::Completed => "completed",
        TaskState::Blocked => "blocked",
        TaskState::Failed => "failed",
        TaskState::Rejected => "rejected",
        TaskState::Cancelled => "cancelled",
        TaskState::TimedOut => "timed_out",
        TaskState::Interrupted => "interrupted",
    }
}
