//! Encrypted custody for Conversation's one bounded compaction journal.
use super::conversation_core::owner_store_failure;
use super::*;
use floe_agent_contract::{JournalEntry, JournalEvent};
use floe_conversation::{
    CompactionAdmission, CompactionClaim, CompactionDisposition, CompactionExecution,
    CompactionJournalHead, RecoveryPage, advance_compaction_journal, validate_compaction_journal,
};
use floe_kernel::OwnerActor;
use turso::Connection;

const MAX_PAYLOAD_BYTES: usize = 640 * 1024;

fn state(job: &CompactionExecution) -> &'static str {
    match job.disposition {
        CompactionDisposition::Pending => "pending",
        CompactionDisposition::Retired { .. } => "retired",
        CompactionDisposition::Committed { .. } => "committed",
    }
}
fn encode(job: &CompactionExecution) -> Result<String, AgentFailure> {
    job.validate()?;
    let payload = serde_json::to_string(job).map_err(|_| AgentFailure::StorageUnavailable)?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}
fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::StorageUnavailable)
}
async fn family(connection: &Connection) -> Result<bool, AgentFailure> {
    crate::schema::conversation_compaction_family_v1_present(connection)
        .await
        .map_err(|failure| owner_store_failure(super::conversation_core::schema_error(failure)))
}
async fn read_on(
    connection: &Connection,
    person_id: PersonId,
    execution_id: Uuid,
) -> Result<Option<CompactionExecution>, AgentFailure> {
    if !family(connection).await? {
        return Ok(None);
    }
    let mut rows = connection.query(
        "SELECT session_id, executor_generation, revision, state, length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= ? THEN payload ELSE NULL END FROM agent_conversation_compactions_v1 WHERE person_id = ? AND execution_id = ? LIMIT 2",
        (MAX_PAYLOAD_BYTES as i64, person_id.to_string(), execution_id.to_string()),
    ).await.map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let bytes: i64 = row.get(4).map_err(storage)?;
    if bytes <= 0 || bytes > MAX_PAYLOAD_BYTES as i64 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let payload: String = row.get(5).map_err(storage)?;
    let job: CompactionExecution =
        serde_json::from_str(&payload).map_err(|_| AgentFailure::StorageUnavailable)?;
    job.validate()?;
    if payload.len() != bytes as usize
        || job.head.person_id != person_id
        || job.claim.execution_id != execution_id
        || row.get::<String>(0).map_err(storage)? != job.source.session_id.to_string()
        || row.get::<i64>(1).map_err(storage)? != integer(job.claim.executor_generation)?
        || row.get::<i64>(2).map_err(storage)? != integer(job.revision)?
        || row.get::<String>(3).map_err(storage)? != state(&job)
        || rows.next().await.map_err(database_failure)?.is_some()
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(job))
}
async fn update_on(
    connection: &Connection,
    previous: &CompactionExecution,
    next: &CompactionExecution,
) -> Result<(), AgentFailure> {
    let payload = encode(next)?;
    if next.revision
        != previous
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?
        || next.claim.execution_id != previous.claim.execution_id
        || next.source != previous.source
        || next.head.person_id != previous.head.person_id
    {
        return Err(AgentFailure::Conflict);
    }
    let changed = connection.execute(
        "UPDATE agent_conversation_compactions_v1 SET executor_generation = ?, revision = ?, state = ?, payload = ? WHERE person_id = ? AND execution_id = ? AND revision = ? AND executor_generation = ?",
        (integer(next.claim.executor_generation)?, integer(next.revision)?, state(next), payload,
            previous.head.person_id.to_string(), previous.claim.execution_id.to_string(),
            integer(previous.revision)?, integer(previous.claim.executor_generation)?),
    ).await.map_err(database_failure)?;
    if changed != 1 {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Validate every retained record with a bounded keyset scan. Opening the
    /// Vault never creates the optional family or rewrites historical state.
    pub(super) async fn validate_manager_compaction_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = async {
            if !family(&transaction).await? { return Ok(()); }
            let mut after: Option<String> = None;
            loop {
                let mut rows = transaction.query(
                    "SELECT person_id, execution_id FROM agent_conversation_compactions_v1 WHERE (? IS NULL OR execution_id > ?) ORDER BY execution_id LIMIT 64",
                    (after.clone(), after.clone()),
                ).await.map_err(database_failure)?;
                let mut ids = vec![];
                while let Some(row) = rows.next().await.map_err(database_failure)? {
                    if row.get::<String>(0).map_err(storage)? != self.person_id.to_string() {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    let text: String = row.get(1).map_err(storage)?;
                    let id = Uuid::parse_str(&text).map_err(|_| AgentFailure::StorageUnavailable)?;
                    if id.is_nil() || id.to_string() != text { return Err(AgentFailure::StorageUnavailable); }
                    ids.push(id);
                }
                drop(rows);
                if ids.is_empty() { break; }
                for id in &ids {
                    read_on(&transaction, self.person_id, *id).await?.ok_or(AgentFailure::StorageUnavailable)?;
                }
                after = Some(ids.last().unwrap().to_string());
            }
            Ok(())
        }.await;
        match result {
            Ok(()) => {
                transaction.commit().await.map_err(database_failure)?;
                self.check_access()
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    fn validate_compaction_actor(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.check_access()?;
        actor.validate()?;
        if actor.person_id != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    pub(crate) async fn admit_manager_compaction(
        &self,
        actor: &OwnerActor,
        request: CompactionAdmission,
    ) -> Result<Option<CompactionExecution>, AgentFailure> {
        self.validate_compaction_actor(actor)?;
        request.validate()?;
        let mut connection = self.connection()?;
        let (mut guard, transaction) = self.journal_transaction(&mut connection).await?;
        let result = async {
            let generation = self.active_conversation_executor_generation(&transaction).await?;
            let Some(source) = self.prepare_manager_compaction_source_on(&transaction,
                request.session_id, request.expected_session_revision, request.source_byte_limit).await? else {
                return Ok(None);
            };
            let head = CompactionJournalHead::new(actor, &source, request.max_tokens,
                request.max_cost_micros, request.max_output_bytes)?;
            if let Some(existing) = read_on(&transaction, self.person_id, head.execution_id).await? {
                existing.authorize_actor(actor)?;
                if existing.claim.executor_generation != generation
                    || existing.source != source || existing.source_byte_limit != request.source_byte_limit
                    || existing.head.max_tokens != head.max_tokens
                    || existing.head.max_cost_micros != head.max_cost_micros
                    || existing.head.max_output_bytes != head.max_output_bytes {
                    return Err(AgentFailure::Conflict);
                }
                return Ok(Some(existing));
            }
            crate::schema::ensure_conversation_compaction_family_v1(&transaction).await
                .map_err(|failure| owner_store_failure(super::conversation_core::schema_error(failure)))?;
            let mut pending = transaction.query(
                "SELECT execution_id FROM agent_conversation_compactions_v1 WHERE person_id = ? AND session_id = ? AND state = 'pending' LIMIT 1",
                (self.person_id.to_string(), source.session_id.to_string()),
            ).await.map_err(database_failure)?;
            if pending.next().await.map_err(database_failure)?.is_some() { return Err(AgentFailure::Conflict); }
            drop(pending);
            let job = CompactionExecution { claim: CompactionClaim { execution_id: head.execution_id,
                executor_generation: generation }, revision: 1, source_byte_limit: request.source_byte_limit,
                source, head, entries: vec![], disposition: CompactionDisposition::Pending };
            transaction.execute(
                "INSERT INTO agent_conversation_compactions_v1 (execution_id, person_id, session_id, executor_generation, revision, state, payload) VALUES (?, ?, ?, ?, 1, 'pending', ?)",
                (job.claim.execution_id.to_string(), self.person_id.to_string(), job.source.session_id.to_string(), integer(generation)?, encode(&job)?),
            ).await.map_err(database_failure)?;
            self.check_access()?;
            Ok(Some(job))
        }.await;
        let settled = self
            .finish_registry_transaction_checked(transaction, result)
            .await;
        guard.settled();
        settled
    }

    pub(crate) async fn load_manager_compaction(
        &self,
        actor: &OwnerActor,
        claim: CompactionClaim,
    ) -> Result<CompactionExecution, AgentFailure> {
        self.validate_compaction_actor(actor)?;
        claim.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = async {
            if self
                .active_conversation_executor_generation(&transaction)
                .await?
                != claim.executor_generation
            {
                return Err(AgentFailure::Conflict);
            }
            let job = read_on(&transaction, self.person_id, claim.execution_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            job.authorize_actor(actor)?;
            if job.claim != claim {
                return Err(AgentFailure::Conflict);
            }
            Ok(job)
        }
        .await;
        match result {
            Ok(job) => {
                transaction.commit().await.map_err(database_failure)?;
                self.check_access()?;
                Ok(job)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    pub(crate) async fn pending_manager_compactions(
        &self,
        actor: &OwnerActor,
        after: Option<Uuid>,
        limit: usize,
    ) -> Result<RecoveryPage<Uuid, Uuid>, AgentFailure> {
        self.validate_compaction_actor(actor)?;
        if !(1..=64).contains(&limit) || after.is_some_and(|id| id.is_nil()) {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        if !family(&connection).await? {
            return Ok(RecoveryPage {
                items: vec![],
                next_cursor: None,
            });
        }
        let mut rows = connection.query(
            "SELECT execution_id FROM agent_conversation_compactions_v1 WHERE person_id = ? AND state = 'pending' AND (? IS NULL OR execution_id > ?) ORDER BY execution_id LIMIT ?",
            (self.person_id.to_string(), after.map(|id| id.to_string()), after.map(|id| id.to_string()), (limit + 1) as i64),
        ).await.map_err(database_failure)?;
        let mut ids = vec![];
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            let id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            if id.is_nil() {
                return Err(AgentFailure::StorageUnavailable);
            }
            ids.push(id);
        }
        let more = ids.len() > limit;
        ids.truncate(limit);
        let next_cursor = if more { ids.last().copied() } else { None };
        self.check_access()?;
        Ok(RecoveryPage {
            items: ids,
            next_cursor,
        })
    }

    pub(crate) async fn recover_manager_compaction(
        &self,
        actor: &OwnerActor,
        execution_id: Uuid,
    ) -> Result<CompactionExecution, AgentFailure> {
        self.validate_compaction_actor(actor)?;
        if execution_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let (mut guard, transaction) = self.journal_transaction(&mut connection).await?;
        let result = async {
            let generation = self
                .active_conversation_executor_generation(&transaction)
                .await?;
            let previous = read_on(&transaction, self.person_id, execution_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            previous.validate()?;
            if previous.disposition != CompactionDisposition::Pending {
                return Ok(previous);
            }
            // Exact replay after a lost recovery acknowledgement.
            if previous.claim.executor_generation == generation {
                if previous.disposition == CompactionDisposition::Pending {
                    previous.authorize_actor(actor)?;
                }
                return Ok(previous);
            }
            let next = previous.recover(actor, generation)?;
            update_on(&transaction, &previous, &next).await?;
            self.check_access()?;
            Ok(next)
        }
        .await;
        let settled = self
            .finish_registry_transaction_checked(transaction, result)
            .await;
        guard.settled();
        settled
    }

    pub(crate) async fn retire_manager_compaction(
        &self,
        actor: &OwnerActor,
        claim: CompactionClaim,
        failure: AgentFailure,
    ) -> Result<(), AgentFailure> {
        self.validate_compaction_actor(actor)?;
        claim.validate()?;
        let mut connection = self.connection()?;
        let (mut guard, transaction) = self.journal_transaction(&mut connection).await?;
        let result = async {
            if self
                .active_conversation_executor_generation(&transaction)
                .await?
                != claim.executor_generation
            {
                return Err(AgentFailure::Conflict);
            }
            let previous = read_on(&transaction, self.person_id, claim.execution_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            previous.authorize_actor(actor)?;
            if previous.claim != claim {
                return Err(AgentFailure::Conflict);
            }
            if previous.disposition == (CompactionDisposition::Retired { failure }) {
                return Ok(());
            }
            if !validate_compaction_journal(&previous.head, &previous.source, &previous.entries)?
                .unresolved_attempts
                .is_empty()
            {
                return Err(AgentFailure::Conflict);
            }
            let next = previous.retire(actor, failure)?;
            update_on(&transaction, &previous, &next).await?;
            self.check_access()?;
            Ok(())
        }
        .await;
        let settled = self
            .finish_registry_transaction_checked(transaction, result)
            .await;
        guard.settled();
        settled
    }

    pub(crate) async fn append_manager_compaction(
        &self,
        actor: &OwnerActor,
        claim: CompactionClaim,
        event: JournalEvent,
    ) -> Result<u64, AgentFailure> {
        self.validate_compaction_actor(actor)?;
        claim.validate()?;
        let encoded = serde_json::to_vec(&event).map_err(|_| AgentFailure::InvalidInput)?;
        if encoded.len() > MAX_PAYLOAD_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut connection = self.connection()?;
        let (mut guard, transaction) = self.journal_transaction(&mut connection).await?;
        let result = async {
            if self
                .active_conversation_executor_generation(&transaction)
                .await?
                != claim.executor_generation
            {
                return Err(AgentFailure::Conflict);
            }
            let previous = read_on(&transaction, self.person_id, claim.execution_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            previous.authorize_actor(actor)?;
            if previous.claim != claim {
                return Err(AgentFailure::Conflict);
            }
            for entry in &previous.entries {
                if serde_json::to_vec(&entry.event).map_err(|_| AgentFailure::StorageUnavailable)?
                    == encoded
                {
                    return Ok(entry.revision);
                }
            }
            if previous.disposition != CompactionDisposition::Pending {
                return Err(AgentFailure::Conflict);
            }
            if matches!(&event, JournalEvent::ModelIntent { .. }) {
                let current = self
                    .prepare_manager_compaction_source_on(
                        &transaction,
                        previous.source.session_id,
                        previous.source.session_revision,
                        previous.source_byte_limit,
                    )
                    .await?;
                if current.as_ref() != Some(&previous.source) {
                    return Err(AgentFailure::StaleContext);
                }
            }
            let mut next = previous.clone();
            next.entries.push(JournalEntry {
                revision: next
                    .head
                    .journal_revision
                    .checked_add(1)
                    .ok_or(AgentFailure::Conflict)?,
                event,
            });
            next.head =
                advance_compaction_journal(&previous.head, &previous.source, &next.entries)?;
            next.revision = next.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
            update_on(&transaction, &previous, &next).await?;
            self.check_access()?;
            Ok(next.head.journal_revision)
        }
        .await;
        let settled = self
            .finish_registry_transaction_checked(transaction, result)
            .await;
        guard.settled();
        settled
    }
}
