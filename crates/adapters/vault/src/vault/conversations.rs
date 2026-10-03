use floe_agent_contract::DataClass;
use floe_agent_contract::{CommandId, DependencyCoverage, RunId};
use floe_agent_contract::{EngineStep, JournalEvent};
use floe_conversation::{
    AgentMessage, AgentUsage, MAX_RESUME_LINEAGE, RunRecord, RunState, RunTerminal,
    TurnAdmissionRequest, TurnInput, TurnMode,
};
use floe_conversation::{BlockedRunCommit, JournalEntry, ResumeChildAdmission, ResumeRequired};
use sha2::{Digest, Sha256};
use turso::transaction::{Transaction, TransactionBehavior};

use super::*;

const SCHEMA_VERSION: i64 = 9;
const MAX_RUN_RECORD_BYTES: usize = 128 * 1024;
const MAX_JOURNAL_ENTRY_BYTES: usize = floe_agent_contract::MAX_TASK_RECEIPT_BYTES + 4096;
const MAX_RUN_ROWS: i64 = 4_096;
const MAX_COMMAND_ROWS: i64 = 4_096;
pub(super) const MAX_JOURNAL_ENTRIES: u64 = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VaultConversationAdmission {
    Created {
        record: RunRecord,
        session: AgentSession,
    },
    Existing(RunRecord),
    /// The origin slot already admitted this resume under another command.
    Resumed(RunRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationCancelRequest {
    pub command_id: CommandId,
    pub run_id: RunId,
    pub person_id: PersonId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationCancelReceipt {
    pub command_id: CommandId,
    pub run_id: RunId,
    pub person_id: PersonId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VaultConversationCancelAdmission {
    Created(VaultConversationCancelReceipt),
    Existing(VaultConversationCancelReceipt),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationActivation {
    pub executor_generation: u64,
    pub interrupted: Vec<RunRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationJournalEntry {
    pub revision: u64,
    pub kind: String,
    pub payload: String,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_conversation_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = initialize(&transaction).await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn activate_conversation_executor(
        &self,
    ) -> Result<VaultConversationActivation, AgentFailure> {
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
                    "SELECT run_id FROM agent_conversation_runs WHERE state = 'working' ORDER BY run_id LIMIT 4097",
                    (),
                )
                .await
                .map_err(storage)?;
            let mut run_ids = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                run_ids.push(parse_run_id(&row.get::<String>(0).map_err(storage)?)?);
            }
            drop(rows);
            if run_ids.len() > usize::try_from(MAX_RUN_ROWS).unwrap_or(usize::MAX) {
                return Err(AgentFailure::BudgetExceeded);
            }
            let mut interrupted = Vec::with_capacity(run_ids.len());
            for run_id in run_ids {
                let current = self
                    .conversation_run_on(&transaction, run_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                if current.state != RunState::Working
                    || current.executor_generation >= next_generation
                {
                    return Err(AgentFailure::Conflict);
                }
                let current_session = self.session_on(&transaction, current.session_id).await?;
                let previous_session_revision = current_session.revision;
                let journal = self.conversation_journal_on(&transaction, &current).await?;
                let (next, session) = floe_conversation::interrupt_for_activation(
                    &current, &current_session, &journal, next_generation)?;
                let changed = transaction
                    .execute(
                        "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                        (
                            integer(session.revision)?,
                            self.payload(&session)?,
                            session.id.to_string(),
                            integer(previous_session_revision)?,
                        ),
                    )
                    .await
                    .map_err(storage)?;
                if changed != 1 {
                    return Err(AgentFailure::Conflict);
                }
                let changed = write_run(
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
                    "UPDATE agent_conversation_executor SET generation = ? WHERE id = 1 AND generation = ?",
                    (integer(next_generation)?, integer(current_generation)?),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(VaultConversationActivation {
                executor_generation: next_generation,
                interrupted,
            })
        }
        .await;
        let activation = self
            .finish_registry_transaction_checked(transaction, result)
            .await?;
        self.conversation_executor_generation
            .store(activation.executor_generation, Ordering::Release);
        Ok(activation)
    }

    pub async fn admit_conversation_turn(
        &self,
        request: TurnAdmissionRequest,
    ) -> Result<VaultConversationAdmission, AgentFailure> {
        if matches!(request.mode, TurnMode::Resume(_)) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = self.admit_conversation_turn_on(&transaction, request).await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    async fn admit_conversation_turn_on(
        &self,
        transaction: &Transaction<'_>,
        request: TurnAdmissionRequest,
    ) -> Result<VaultConversationAdmission, AgentFailure> {
        request.validate()?;
        if request.principal != self.person_id.to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
        let continuation = match &request.mode {
            TurnMode::Continue(value) => Some(value.clone()),
            _ => None,
        };
        let resume = match &request.mode {
            TurnMode::Resume(value) => Some(value.clone()),
            _ => None,
        };
        let user_message_id = match &request.input {
            TurnInput::NewMessage(message) => message.message_id,
            TurnInput::ExistingMessage { message_id } => *message_id,
        };
        initialize(transaction).await?;
        if session_command_on(transaction, self.person_id, request.command_id)
            .await?
            .is_some()
        {
            return Err(AgentFailure::Conflict);
        }
        if let Some(existing) = self
            .conversation_run_by_command_on(transaction, request.command_id)
            .await?
        {
            return if existing.matches_admission(&request) {
                Ok(VaultConversationAdmission::Existing(existing))
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        if command_identity_used(transaction, request.command_id.as_uuid()).await? {
            return Err(AgentFailure::Conflict);
        }
        let mut conflicting_command = transaction
            .query(
                "SELECT 1 FROM agent_conversation_commands WHERE command_id = ?",
                [request.command_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        if conflicting_command.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::Conflict);
        }
        let executor_generation = self
            .active_conversation_executor_generation(transaction)
            .await?;
        // A claimed resume slot rejoins before any Session comparison:
        // the winner's child is canonical no matter which revision the
        // losing command carries.
        if let Some(reference) = &resume {
            let origin = self
                .conversation_run_on(transaction, reference.origin_run_id)
                .await?
                .ok_or(AgentFailure::Conflict)?;
            if origin.person_id != self.person_id
                || origin.session_id != request.session_id
                || !matches!(origin.state, RunState::Completed | RunState::Blocked)
                || origin
                    .resume_lineage
                    .checked_add(1)
                    .filter(|lineage| *lineage <= MAX_RESUME_LINEAGE)
                    != Some(reference.lineage)
                || origin.device_id != request.device_id
                || origin.user_message_id != user_message_id
            {
                return Err(AgentFailure::Conflict);
            }
            if let Some(child_run_id) = self
                .resume_slot_on(transaction, reference.origin_run_id)
                .await?
            {
                let child = self
                    .conversation_run_on(transaction, child_run_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                if child.person_id != self.person_id
                    || child.session_id != request.session_id
                    || child.resume_of != Some(reference.origin_run_id)
                    || child.resume_lineage != reference.lineage
                {
                    return Err(AgentFailure::VaultUnavailable);
                }
                return Ok(VaultConversationAdmission::Resumed(child));
            }
            self.check_resume_group(transaction, reference.origin_run_id)
                .await?;
        }
        let mut session = self.session_on(transaction, request.session_id).await?;
        if session.person_id != self.person_id
            || session.scope.is_some()
            || session.data_classes != [DataClass::Personal]
            || session.revision != request.expected_session_revision
            || session.active_turn.is_some()
        {
            return Err(AgentFailure::Conflict);
        }
        if let Some(reference) = &continuation {
            let source = self
                .conversation_run_on(transaction, reference.run_id)
                .await?
                .ok_or(AgentFailure::Conflict)?;
            let resumable = matches!(
                (source.state, source.issue),
                (RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
                    | (RunState::Failed, Some(AgentFailure::BudgetExceeded))
            );
            if !resumable
                || source.session_id != request.session_id
                || source.person_id != self.person_id
                || source.session_revision != request.expected_session_revision
                || source.device_id != request.device_id
                || source.user_message_id != user_message_id
                || source.executor_generation != reference.executor_generation
                || source
                    .continuation_level
                    .checked_add(1)
                    .filter(|level| *level <= 3)
                    != Some(reference.level)
            {
                return Err(AgentFailure::Conflict);
            }
        }
        if let Some(retry_of) = request.retry_of {
            let source = self
                .conversation_run_on(transaction, retry_of)
                .await?
                .ok_or(AgentFailure::Conflict)?;
            if source.session_id != request.session_id
                || source.person_id != self.person_id
                || !source.state.is_terminal()
                || source.session_revision != request.expected_session_revision
            {
                return Err(AgentFailure::Conflict);
            }
        }
        let mut count = transaction
            .query("SELECT count(*) FROM agent_conversation_runs", ())
            .await
            .map_err(storage)?;
        let rows = count
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        if rows >= MAX_RUN_ROWS {
            return Err(AgentFailure::BudgetExceeded);
        }
        if matches!(request.input, TurnInput::ExistingMessage { .. }) && !session.messages.iter().any(|message|
            matches!(message, AgentMessage::User { message_id, .. } if *message_id == user_message_id)) {
            return Err(AgentFailure::Conflict);
        }
        if matches!(request.mode, TurnMode::New) {
            transaction.execute("UPDATE agent_conversation_resume_requests SET state = 'superseded' WHERE session_id = ? AND person_id = ? AND state = 'pending'",
                (session.id.to_string(), self.person_id.to_string())).await.map_err(storage)?;
        }
        let previous_revision = session.revision;
        session.revision = session
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        if continuation.is_none() && resume.is_none() {
            let TurnInput::NewMessage(message) = &request.input else {
                return Err(AgentFailure::InvalidInput);
            };
            session.messages.push(AgentMessage::User {
                turn_id: request.run_id.as_uuid(),
                message_id: message.message_id,
                text: message.text.clone(),
            });
        }
        session.usage = AgentUsage::default();
        session.active_turn = Some(request.run_id.as_uuid());
        session.last_outcome = None;
        session.continuation = None;
        let payload = self.payload(&session)?;
        let changed = transaction
            .execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (
                    integer(session.revision)?,
                    payload,
                    session.id.to_string(),
                    integer(previous_revision)?,
                ),
            )
            .await
            .map_err(storage)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        context_dependencies::merge_context_dependency_coverage(
            transaction,
            self.person_id,
            session.id,
            request.run_id.as_uuid(),
            DependencyCoverage::Independent,
        )
        .await?;
        let record = RunRecord {
            expert_environment: request.expert_environment,
            run_id: request.run_id,
            command_id: request.command_id,
            session_id: request.session_id,
            person_id: self.person_id,
            device_id: request.device_id.clone(),
            user_message_id,
            initial_session_revision: request.expected_session_revision,
            request_digest: request.request_digest,
            state: RunState::Working,
            output: None,
            coverage: DependencyCoverage::Unknown,
            issue: None,
            blocked: None,
            session_revision: session.revision,
            aggregate_revision: 1,
            journal_revision: 0,
            executor_generation,
            continuation_of: continuation.as_ref().map(|value| value.run_id),
            continuation_executor_generation: continuation
                .as_ref()
                .map(|value| value.executor_generation),
            continuation_level: continuation.as_ref().map_or(0, |value| value.level),
            retry_of: request.retry_of,
            resume_of: resume.as_ref().map(|value| value.origin_run_id),
            resume_lineage: resume.as_ref().map_or(0, |value| value.lineage),
        };
        record.validate(self.person_id)?;
        transaction
            .execute(
                "INSERT INTO agent_conversation_runs (run_id, command_id, session_id, person_id, state, aggregate_revision, journal_revision, executor_generation, payload) VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)",
                (
                    record.run_id.as_uuid().to_string(),
                    record.command_id.as_uuid().to_string(),
                    record.session_id.to_string(),
                    record.person_id.to_string(),
                    state_name(record.state),
                    integer(record.executor_generation)?,
                    encode_record(&record)?,
                ),
            )
            .await
            .map_err(storage)?;
        if let Some(reference) = &resume {
            transaction
                .execute(
                    "INSERT INTO agent_conversation_resume_slots (origin_run_id, child_run_id, child_command_id, session_id, person_id) VALUES (?, ?, ?, ?, ?)",
                    (
                        reference.origin_run_id.as_uuid().to_string(),
                        record.run_id.as_uuid().to_string(),
                        record.command_id.as_uuid().to_string(),
                        record.session_id.to_string(),
                        record.person_id.to_string(),
                    ),
                )
                .await
                .map_err(storage)?;
        }
        self.check_access()?;
        Ok(VaultConversationAdmission::Created { record, session })
    }

    pub async fn admit_conversation_cancel(
        &self,
        request: VaultConversationCancelRequest,
    ) -> Result<VaultConversationCancelAdmission, AgentFailure> {
        if !request.command_id.is_valid() || !request.run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        if request.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            if session_command_on(&transaction, self.person_id, request.command_id).await?.is_some() { return Err(AgentFailure::Conflict); }
            let mut existing = transaction
                .query(
                    "SELECT person_id, target_id, kind FROM agent_conversation_commands WHERE command_id = ?",
                    [request.command_id.as_uuid().to_string()],
                )
                .await
                .map_err(storage)?;
            if let Some(row) = existing.next().await.map_err(storage)? {
                if row.get::<String>(2).map_err(storage)? != "cancel_run" {
                    return Err(AgentFailure::Conflict);
                }
                let receipt = VaultConversationCancelReceipt {
                    command_id: request.command_id,
                    person_id: PersonId(
                        Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                            .map_err(|_| AgentFailure::VaultUnavailable)?,
                    ),
                    run_id: RunId::from_uuid(
                        Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
                            .map_err(|_| AgentFailure::VaultUnavailable)?,
                    )
                    .ok_or(AgentFailure::VaultUnavailable)?,
                };
                return if receipt.person_id == self.person_id
                    && receipt.run_id == request.run_id
                {
                    Ok(VaultConversationCancelAdmission::Existing(receipt))
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
            if command_identity_used(&transaction, request.command_id.as_uuid()).await? { return Err(AgentFailure::Conflict); }
            if self
                .conversation_run_by_command_on(&transaction, request.command_id)
                .await?
                .is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            let run = self
                .conversation_run_on(&transaction, request.run_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if run.person_id != self.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            let mut count = transaction
                .query("SELECT count(*) FROM agent_conversation_commands", ())
                .await
                .map_err(storage)?;
            let rows = count
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            if rows >= MAX_COMMAND_ROWS {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction
                .execute(
                    "INSERT INTO agent_conversation_commands (command_id, person_id, kind, target_id) VALUES (?, ?, 'cancel_run', ?)",
                    (
                        request.command_id.as_uuid().to_string(),
                        self.person_id.to_string(),
                        request.run_id.as_uuid().to_string(),
                    ),
                )
                .await
                .map_err(storage)?;
            self.check_access()?;
            Ok(VaultConversationCancelAdmission::Created(
                VaultConversationCancelReceipt {
                    command_id: request.command_id,
                    run_id: request.run_id,
                    person_id: self.person_id,
                },
            ))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn append_conversation_journal(
        &self,
        run_id: RunId,
        kind: &str,
        payload: &str,
    ) -> Result<u64, AgentFailure> {
        if !run_id.is_valid()
            || kind.is_empty()
            || kind.len() > 64
            || !kind
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            || payload.is_empty()
            || payload.len() > MAX_JOURNAL_ENTRY_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let mut record = self
                .conversation_run_on(&transaction, run_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if record.executor_generation
                != self.active_conversation_executor_generation(&transaction).await?
            {
                return Err(AgentFailure::Conflict);
            }
            if record.state != RunState::Working { return Err(AgentFailure::Conflict); }
            if record.journal_revision >= MAX_JOURNAL_ENTRIES { return Err(AgentFailure::BudgetExceeded); }
            let event: JournalEvent = serde_json::from_str(payload).map_err(|_| AgentFailure::InvalidInput)?;
            if kind != journal_kind(&event) || payload.len() > journal_event_byte_limit(&event) { return Err(AgentFailure::InvalidInput); }
            let mut entries = self.conversation_journal_on(&transaction, &record).await?;
            entries.push(JournalEntry { revision: record.journal_revision + 1, event });
            validate_journal(&record, &entries)?;
            let previous = record.journal_revision;
            record.journal_revision += 1;
            let changed = transaction
                .execute(
                    "UPDATE agent_conversation_runs SET journal_revision = ?, payload = ? WHERE run_id = ? AND state = 'working' AND journal_revision = ?",
                    (
                        integer(record.journal_revision)?,
                        encode_record(&record)?,
                        run_id.as_uuid().to_string(),
                        integer(previous)?,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            transaction
                .execute(
                    "INSERT INTO agent_conversation_journal (run_id, revision, kind, payload) VALUES (?, ?, ?, ?)",
                    (
                        run_id.as_uuid().to_string(),
                        integer(record.journal_revision)?,
                        kind,
                        payload,
                    ),
                )
                .await
                .map_err(storage)?;
            self.check_access()?;
            Ok(record.journal_revision)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn finish_conversation_run(
        &self,
        run_id: RunId,
        expected_aggregate_revision: u64,
        terminal: RunTerminal,
    ) -> Result<RunRecord, AgentFailure> {
        if terminal.state == RunState::Blocked {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = self
            .finish_conversation_run_on(&transaction, run_id, expected_aggregate_revision, terminal)
            .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    async fn finish_conversation_run_on(
        &self,
        transaction: &Transaction<'_>,
        run_id: RunId,
        expected_aggregate_revision: u64,
        terminal: RunTerminal,
    ) -> Result<RunRecord, AgentFailure> {
        terminal.validate()?;
        let digest = terminal_digest(run_id, expected_aggregate_revision, &terminal)?;
        let current = self
            .conversation_run_on(transaction, run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if current.state.is_terminal() {
            let receipt = terminal_receipt_on(transaction, run_id).await?;
            return if receipt.as_deref() == Some(digest.as_str())
                && current.aggregate_revision
                    == expected_aggregate_revision
                        .checked_add(1)
                        .ok_or(AgentFailure::Conflict)?
            {
                Ok(current)
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        if current.executor_generation
            != self
                .active_conversation_executor_generation(transaction)
                .await?
            || current.aggregate_revision != expected_aggregate_revision
        {
            return Err(AgentFailure::Conflict);
        }
        let session = self.session_on(transaction, current.session_id).await?;
        let journal = self.conversation_journal_on(transaction, &current).await?;
        floe_conversation::validate_terminal_steps(&terminal, &journal)?;
        let (next, next_session) =
            floe_conversation::apply_terminal(&current, &session, &terminal, &journal)?;
        let changed = transaction
            .execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (
                    integer(next_session.revision)?,
                    self.payload(&next_session)?,
                    session.id.to_string(),
                    integer(session.revision)?,
                ),
            )
            .await
            .map_err(storage)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        context_dependencies::merge_context_dependency_coverage(
            transaction,
            self.person_id,
            session.id,
            run_id.as_uuid(),
            terminal.coverage.clone(),
        )
        .await?;
        if write_run(
            transaction,
            &next,
            current.aggregate_revision,
            current.executor_generation,
        )
        .await?
            != 1
        {
            return Err(AgentFailure::Conflict);
        }
        transaction
            .execute(
                "INSERT INTO agent_conversation_terminal_receipts (run_id, digest) VALUES (?, ?)",
                (run_id.as_uuid().to_string(), digest),
            )
            .await
            .map_err(storage)?;
        self.enqueue_resume_on(transaction, &next).await?;
        self.check_access()?;
        Ok(next)
    }

    pub async fn finish_blocked_conversation_run(
        &self,
        commit: BlockedRunCommit,
    ) -> Result<RunRecord, AgentFailure> {
        commit.validate()?;
        if commit.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            super::conversation_interactions::initialize(&transaction).await?;
            let current = self
                .conversation_run_on(&transaction, commit.run_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let replay = current.state == RunState::Blocked;
            if current.person_id != commit.person_id
                || current.session_id != commit.session_id
                || current.executor_generation != commit.executor_generation
                || current.journal_revision != commit.expected_journal_revision
                || (!replay
                    && (current.state != RunState::Working
                        || current.session_revision != commit.expected_session_revision
                        || current.aggregate_revision != commit.expected_aggregate_revision))
                || (replay
                    && (current.session_revision
                        != commit
                            .expected_session_revision
                            .checked_add(1)
                            .ok_or(AgentFailure::Conflict)?
                        || current.aggregate_revision
                            != commit
                                .expected_aggregate_revision
                                .checked_add(1)
                                .ok_or(AgentFailure::Conflict)?))
            {
                return Err(AgentFailure::Conflict);
            }
            let block = commit
                .terminal
                .blocked
                .as_ref()
                .ok_or(AgentFailure::InvalidInput)?;
            self.validate_blocked_task_audits_on(&transaction, &commit).await?;
            for publication in &commit.publications {
                if publication.record.device_id != current.device_id {
                    return Err(AgentFailure::Conflict);
                }
                self.store_review_audit_on(
                    &transaction,
                    &current,
                    &publication.record,
                    replay,
                )
                .await?;
                for interaction in &publication.interactions {
                    let link = block.interactions.iter()
                        .find(|link| link.interaction_id == interaction.id)
                        .ok_or(AgentFailure::Conflict)?;
                    let origin = &link.origin;
                    if origin.run_id != current.run_id
                        || origin.session_id != current.session_id
                        || origin.person_id != current.person_id
                        || origin.device_id != current.device_id
                        || origin.executor_generation != current.executor_generation
                        || origin.origin != interaction.origin
                        || link.target != interaction.target
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    if let Some(existing) = super::conversation_interactions::read_interaction(
                        &transaction,
                        self.person_id,
                        interaction.id,
                    )
                    .await?
                    {
                        if !same_publication(&existing, interaction) {
                            return Err(AgentFailure::Conflict);
                        }
                    } else {
                        if replay {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        self.check_interaction_origin_on(&transaction, interaction)
                            .await?;
                        let stored = super::conversation_interactions::count_interactions(
                            &transaction,
                            current.run_id,
                            false,
                        )
                        .await?;
                        let active = super::conversation_interactions::count_interactions(
                            &transaction,
                            current.run_id,
                            true,
                        )
                        .await?;
                        if stored >= floe_conversation::MAX_STORED_INTERACTIONS_PER_RUN as u64
                            || active >= floe_conversation::MAX_ACTIVE_INTERACTIONS_PER_RUN as u64
                        {
                            return Err(AgentFailure::BudgetExceeded);
                        }
                        super::conversation_interactions::insert_interaction(
                            &transaction,
                            interaction,
                        )
                        .await?;
                    }
                }
            }
            self.finish_conversation_run_on(
                &transaction,
                commit.run_id,
                commit.expected_aggregate_revision,
                commit.terminal,
            )
            .await
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub(super) async fn conversation_journal_on(
        &self,
        connection: &turso::Connection,
        record: &RunRecord,
    ) -> Result<Vec<JournalEntry>, AgentFailure> {
        let mut rows = connection.query("SELECT revision, kind, payload FROM agent_conversation_journal WHERE run_id = ? ORDER BY revision LIMIT 513", [record.run_id.as_uuid().to_string()]).await.map_err(storage)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let revision = u64::try_from(row.get::<i64>(0).map_err(storage)?)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let kind = row.get::<String>(1).map_err(storage)?;
            let payload = row.get::<String>(2).map_err(storage)?;
            if revision != entries.len() as u64 + 1
                || payload.is_empty()
                || payload.len() > MAX_JOURNAL_ENTRY_BYTES
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let event: JournalEvent =
                serde_json::from_str(&payload).map_err(|_| AgentFailure::StorageUnavailable)?;
            if kind != journal_kind(&event) || payload.len() > journal_event_byte_limit(&event) {
                return Err(AgentFailure::StorageUnavailable);
            }
            entries.push(JournalEntry { revision, event });
        }
        if entries.len() > MAX_JOURNAL_ENTRIES as usize
            || entries.len() as u64 != record.journal_revision
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        validate_journal(record, &entries)?;
        Ok(entries)
    }

    pub async fn conversation_run(&self, run_id: RunId) -> Result<Option<RunRecord>, AgentFailure> {
        if !run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let result = self
            .conversation_run_on(&self.connection()?, run_id)
            .await?;
        self.check_access()?;
        Ok(result)
    }

    pub async fn conversation_run_by_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<RunRecord>, AgentFailure> {
        if !command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let record = self
            .conversation_run_by_command_on(&self.connection()?, command_id)
            .await?;
        if let Some(record) = &record {
            record.validate(self.person_id)?;
        }
        self.check_access()?;
        Ok(record)
    }

    pub async fn conversation_journal(
        &self,
        run_id: RunId,
    ) -> Result<Vec<VaultConversationJournalEntry>, AgentFailure> {
        if !run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let record = self
            .conversation_run_on(&connection, run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        let mut rows = connection
            .query(
                "SELECT revision, kind, payload FROM agent_conversation_journal WHERE run_id = ? ORDER BY revision LIMIT 513",
                (run_id.as_uuid().to_string(),),
            )
            .await
            .map_err(storage)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let revision = row.get::<i64>(0).map_err(storage)?;
            let kind = row.get::<String>(1).map_err(storage)?;
            let payload = row.get::<String>(2).map_err(storage)?;
            if revision <= 0
                || revision as u64 != entries.len() as u64 + 1
                || kind.is_empty()
                || kind.len() > 64
                || payload.is_empty()
                || payload.len() > MAX_JOURNAL_ENTRY_BYTES
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            entries.push(VaultConversationJournalEntry {
                revision: revision as u64,
                kind,
                payload,
            });
        }
        if entries.len() > MAX_JOURNAL_ENTRIES as usize
            || entries.len() as u64 != record.journal_revision
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let typed = entries
            .iter()
            .map(|entry| {
                let event: JournalEvent =
                    serde_json::from_str(&entry.payload).map_err(unavailable)?;
                if entry.kind != journal_kind(&event) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(JournalEntry {
                    revision: entry.revision,
                    event,
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        validate_journal(&record, &typed)?;
        self.check_access()?;
        Ok(entries)
    }

    pub async fn recover_conversation_session(
        &self,
        command_id: CommandId,
        session_id: Uuid,
        person_id: PersonId,
        expected_session_revision: u64,
    ) -> Result<u64, AgentFailure> {
        if !command_id.is_valid() || session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let mut receipts = transaction.query("SELECT person_id, session_id, expected_revision, result_revision FROM agent_conversation_recovery_commands WHERE command_id = ?", [command_id.as_uuid().to_string()]).await.map_err(storage)?;
            if let Some(row) = receipts.next().await.map_err(storage)? {
                let revision = u64::try_from(row.get::<i64>(3).map_err(storage)?).map_err(|_| AgentFailure::StorageUnavailable)?;
                if row.get::<String>(0).map_err(storage)? != person_id.to_string() || row.get::<String>(1).map_err(storage)? != session_id.to_string()
                    || row.get::<i64>(2).map_err(storage)? != integer(expected_session_revision)? || receipts.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
                return Ok(revision);
            }
            drop(receipts);
            if command_identity_used(&transaction, command_id.as_uuid()).await? { return Err(AgentFailure::Conflict); }
            let session = self.session_on(&transaction, session_id).await?;
            if session.person_id != person_id
                || session.scope.is_some()
                || session.data_classes != [DataClass::Personal]
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            if session.revision != expected_session_revision {
                return Err(AgentFailure::Conflict);
            }
            if let Some(active_turn) = session.active_turn {
                let run_id = RunId::from_uuid(active_turn).ok_or(AgentFailure::VaultUnavailable)?;
                let run = self
                    .conversation_run_on(&transaction, run_id)
                    .await?
                    .ok_or(AgentFailure::VaultUnavailable)?;
                if run.state == RunState::Working
                    && run.executor_generation
                        == self
                            .active_conversation_executor_generation(&transaction)
                            .await?
                {
                    return Err(AgentFailure::Conflict);
                }
                return Err(AgentFailure::VaultUnavailable);
            }
            let mut count = transaction.query("SELECT count(*) FROM agent_conversation_recovery_commands WHERE person_id = ?", [person_id.to_string()]).await.map_err(storage)?;
            if count.next().await.map_err(storage)?.ok_or(AgentFailure::StorageUnavailable)?.get::<i64>(0).map_err(storage)? >= 4096 { return Err(AgentFailure::BudgetExceeded); }
            drop(count);
            transaction.execute("INSERT INTO agent_conversation_recovery_commands (command_id, person_id, session_id, expected_revision, result_revision) VALUES (?, ?, ?, ?, ?)",
                (command_id.as_uuid().to_string(), person_id.to_string(), session_id.to_string(), integer(expected_session_revision)?, integer(session.revision)?)).await.map_err(storage)?;
            self.check_access()?;
            Ok(session.revision)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    async fn conversation_run_by_command_on(
        &self,
        connection: &turso::Connection,
        command_id: CommandId,
    ) -> Result<Option<RunRecord>, AgentFailure> {
        read_record(
            self.person_id,
            connection,
            "SELECT run_id, command_id, session_id, person_id, state, aggregate_revision, journal_revision, executor_generation, payload FROM agent_conversation_runs WHERE command_id = ?",
            command_id.as_uuid().to_string(),
        )
        .await
    }

    pub(super) async fn conversation_run_on(
        &self,
        connection: &turso::Connection,
        run_id: RunId,
    ) -> Result<Option<RunRecord>, AgentFailure> {
        read_record(
            self.person_id,
            connection,
            "SELECT run_id, command_id, session_id, person_id, state, aggregate_revision, journal_revision, executor_generation, payload FROM agent_conversation_runs WHERE run_id = ?",
            run_id.as_uuid().to_string(),
        )
        .await
    }

    /// The child already admitted for one origin, if its slot is claimed.
    async fn resume_slot_on(
        &self,
        transaction: &Transaction<'_>,
        origin_run_id: RunId,
    ) -> Result<Option<RunId>, AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT child_run_id FROM agent_conversation_resume_slots WHERE origin_run_id = ?",
                [origin_run_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Ok(None);
        };
        let child = RunId::from_uuid(
            Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        )
        .ok_or(AgentFailure::VaultUnavailable)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(Some(child))
    }

    /// The origin group admits a child only once every card is terminal
    /// and at least one resolved. Missing tables or rows conflict: nothing
    /// was ever reviewed for this origin.
    async fn check_resume_group(
        &self,
        transaction: &Transaction<'_>,
        origin_run_id: RunId,
    ) -> Result<(), AgentFailure> {
        if !table_exists(transaction, "agent_conversation_interactions").await? {
            return Err(AgentFailure::Conflict);
        }
        let mut rows = transaction
            .query(
                "SELECT state FROM agent_conversation_interactions WHERE origin_run_id = ?",
                [origin_run_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        let mut count = 0u64;
        let mut resolved = false;
        while let Some(row) = rows.next().await.map_err(storage)? {
            count += 1;
            match row.get::<String>(0).map_err(storage)?.as_str() {
                "resolved" => resolved = true,
                "denied" | "cancelled" | "superseded" | "expired" => {}
                _ => return Err(AgentFailure::Conflict),
            }
        }
        if count == 0 || !resolved {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    async fn active_conversation_executor_generation(
        &self,
        connection: &turso::Connection,
    ) -> Result<u64, AgentFailure> {
        let active = self
            .conversation_executor_generation
            .load(Ordering::Acquire);
        if active == 0 || active != executor_generation_on(connection).await? {
            return Err(AgentFailure::Conflict);
        }
        Ok(active)
    }
}

async fn initialize(transaction: &Transaction<'_>) -> Result<(), AgentFailure> {
    let mut tables = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_conversation_schema', 'agent_conversation_executor', 'agent_conversation_runs', 'agent_conversation_journal', 'agent_conversation_commands', 'agent_conversation_resume_slots', 'agent_conversation_resume_requests', 'agent_conversation_review_audits', 'agent_conversation_terminal_receipts', 'agent_conversation_session_commands', 'agent_conversation_recovery_commands')",
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
                "CREATE TABLE agent_conversation_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 9))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_executor (id INTEGER PRIMARY KEY CHECK (id = 1), generation INTEGER NOT NULL CHECK (generation >= 0))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_runs (run_id TEXT PRIMARY KEY, command_id TEXT NOT NULL UNIQUE, session_id TEXT NOT NULL, person_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('working', 'blocked', 'completed', 'failed', 'cancelled', 'timed_out', 'interrupted')), aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0), journal_revision INTEGER NOT NULL CHECK (journal_revision >= 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_journal (run_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK (revision > 0), kind TEXT NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 1052672), PRIMARY KEY (run_id, revision), FOREIGN KEY (run_id) REFERENCES agent_conversation_runs(run_id))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_commands (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('cancel_run')), target_id TEXT NOT NULL, FOREIGN KEY (target_id) REFERENCES agent_conversation_runs(run_id))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_resume_slots (origin_run_id TEXT PRIMARY KEY, child_run_id TEXT NOT NULL, child_command_id TEXT NOT NULL, session_id TEXT NOT NULL, person_id TEXT NOT NULL, FOREIGN KEY (child_run_id) REFERENCES agent_conversation_runs(run_id))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction.execute(
            "CREATE TABLE agent_conversation_resume_requests (origin_run_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending','claimed','superseded')), child_run_id TEXT, payload TEXT NOT NULL)", (),
        ).await.map_err(storage)?;
        transaction.execute("CREATE TABLE agent_conversation_recovery_commands (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES agent_sessions(id), expected_revision INTEGER NOT NULL CHECK(expected_revision >= 0), result_revision INTEGER NOT NULL CHECK(result_revision >= 0))", ()).await.map_err(storage)?;
        transaction.execute("CREATE TABLE agent_conversation_session_commands (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES agent_sessions(id), initial_revision INTEGER NOT NULL CHECK(initial_revision = 0))", ()).await.map_err(storage)?;
        transaction.execute("CREATE TABLE agent_conversation_terminal_receipts (run_id TEXT PRIMARY KEY REFERENCES agent_conversation_runs(run_id), digest TEXT NOT NULL CHECK(length(digest) = 64))", ()).await.map_err(storage)?;
        transaction.execute(
            "CREATE TABLE agent_conversation_review_audits (operation_id TEXT NOT NULL, run_id TEXT NOT NULL, person_id TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(run_id, operation_id))", (),
        ).await.map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_conversation_active_session ON agent_conversation_runs (session_id, state, run_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_conversation_schema (id, version) VALUES (1, 9)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_conversation_executor (id, generation) VALUES (1, 0)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found
        != [
            "agent_conversation_commands".to_owned(),
            "agent_conversation_executor".to_owned(),
            "agent_conversation_journal".to_owned(),
            "agent_conversation_review_audits".to_owned(),
            "agent_conversation_recovery_commands".to_owned(),
            "agent_conversation_resume_requests".to_owned(),
            "agent_conversation_resume_slots".to_owned(),
            "agent_conversation_runs".to_owned(),
            "agent_conversation_schema".to_owned(),
            "agent_conversation_session_commands".to_owned(),
            "agent_conversation_terminal_receipts".to_owned(),
        ]
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut marker = transaction
        .query("SELECT id, version FROM agent_conversation_schema", ())
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
            "SELECT run_id, command_id, session_id, person_id, state, aggregate_revision, journal_revision, executor_generation, payload FROM agent_conversation_runs LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction
        .query(
            "SELECT run_id, revision, kind, payload FROM agent_conversation_journal LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction
        .query(
            "SELECT command_id, person_id, kind, target_id FROM agent_conversation_commands LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction
        .query(
            "SELECT origin_run_id, child_run_id, child_command_id, session_id, person_id FROM agent_conversation_resume_slots LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    let mut index = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'agent_conversation_active_session' AND tbl_name = 'agent_conversation_runs'",
            (),
        )
        .await
        .map_err(storage)?;
    if index.next().await.map_err(storage)?.is_none()
        || index.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    transaction.query("SELECT origin_run_id, person_id, session_id, state, child_run_id, payload FROM agent_conversation_resume_requests LIMIT 0", ()).await.map_err(storage)?;
    transaction.query("SELECT operation_id, run_id, person_id, payload FROM agent_conversation_review_audits LIMIT 0", ()).await.map_err(storage)?;
    transaction
        .query(
            "SELECT run_id, digest FROM agent_conversation_terminal_receipts LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction.query("SELECT command_id, person_id, session_id, initial_revision FROM agent_conversation_session_commands LIMIT 0", ()).await.map_err(storage)?;
    transaction.query("SELECT command_id, person_id, session_id, expected_revision, result_revision FROM agent_conversation_recovery_commands LIMIT 0", ()).await.map_err(storage)?;
    executor_generation(transaction).await?;
    Ok(())
}

async fn table_exists(transaction: &Transaction<'_>, table: &str) -> Result<bool, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?",
            [table],
        )
        .await
        .map_err(storage)?;
    Ok(rows.next().await.map_err(storage)?.is_some())
}

async fn read_record(
    person_id: PersonId,
    connection: &turso::Connection,
    query: &str,
    identifier: String,
) -> Result<Option<RunRecord>, AgentFailure> {
    let mut rows = connection
        .query(query, [identifier])
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let payload = row.get::<String>(8).map_err(storage)?;
    if payload.len() > MAX_RUN_RECORD_BYTES {
        return Err(AgentFailure::StorageUnavailable);
    }
    let record: RunRecord = serde_json::from_str(&payload).map_err(unavailable)?;
    record.validate(person_id)?;
    if row.get::<String>(0).map_err(storage)? != record.run_id.as_uuid().to_string()
        || row.get::<String>(1).map_err(storage)? != record.command_id.as_uuid().to_string()
        || row.get::<String>(2).map_err(storage)? != record.session_id.to_string()
        || row.get::<String>(3).map_err(storage)? != record.person_id.to_string()
        || row.get::<String>(4).map_err(storage)? != state_name(record.state)
        || row.get::<i64>(5).map_err(storage)? != integer(record.aggregate_revision)?
        || row.get::<i64>(6).map_err(storage)? != integer(record.journal_revision)?
        || row.get::<i64>(7).map_err(storage)? != integer(record.executor_generation)?
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(record))
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
            "SELECT generation FROM agent_conversation_executor WHERE id = 1",
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

async fn write_run(
    transaction: &Transaction<'_>,
    next: &RunRecord,
    expected_revision: u64,
    expected_generation: u64,
) -> Result<u64, AgentFailure> {
    transaction
        .execute(
            "UPDATE agent_conversation_runs SET state = ?, aggregate_revision = ?, executor_generation = ?, payload = ? WHERE run_id = ? AND aggregate_revision = ? AND executor_generation = ?",
            (
                state_name(next.state),
                integer(next.aggregate_revision)?,
                integer(next.executor_generation)?,
                encode_record(next)?,
                next.run_id.as_uuid().to_string(),
                integer(expected_revision)?,
                integer(expected_generation)?,
            ),
        )
        .await
        .map_err(storage)
}

fn parse_run_id(value: &str) -> Result<RunId, AgentFailure> {
    Uuid::parse_str(value)
        .map_err(unavailable)
        .and_then(|value| RunId::from_uuid(value).ok_or(AgentFailure::VaultUnavailable))
}

pub(super) fn encode_record(record: &RunRecord) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_RUN_RECORD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

pub(super) fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

pub(super) fn state_name(state: RunState) -> &'static str {
    match state {
        RunState::Working => "working",
        RunState::Blocked => "blocked",
        RunState::Completed => "completed",
        RunState::Failed => "failed",
        RunState::Cancelled => "cancelled",
        RunState::TimedOut => "timed_out",
        RunState::Interrupted => "interrupted",
    }
}

fn journal_kind(event: &JournalEvent) -> &'static str {
    match event {
        JournalEvent::ModelIntent { .. }
        | JournalEvent::ToolIntent { .. }
        | JournalEvent::DelegationIntent { .. } => "intent",
        JournalEvent::ModelResult { .. }
        | JournalEvent::ToolResult { .. }
        | JournalEvent::DelegationResult { .. } => "result",
        JournalEvent::Output { .. } => "output",
        JournalEvent::FinalizationStarted { .. }
        | JournalEvent::Checkpoint { .. }
        | JournalEvent::ValidatedBatch { .. }
        | JournalEvent::BatchProgress { .. } => "checkpoint",
    }
}
fn validate_journal(record: &RunRecord, entries: &[JournalEntry]) -> Result<(), AgentFailure> {
    let receipt = floe_conversation::project_run_receipt(record.clone())?;
    floe_conversation::validate_run_journal(&receipt, entries)
}
fn terminal_digest(
    run_id: RunId,
    expected_revision: u64,
    terminal: &RunTerminal,
) -> Result<String, AgentFailure> {
    let steps = terminal
        .steps
        .iter()
        .map(|step| match step {
            EngineStep::Tool(result) => serde_json::to_value(("tool", result)),
            EngineStep::Delegation(receipt) => serde_json::to_value(("delegation", receipt)),
            EngineStep::Answer { text, artifacts } => {
                serde_json::to_value(("answer", text, artifacts))
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let bytes = serde_json::to_vec(&(
        run_id,
        expected_revision,
        terminal.state,
        &terminal.output,
        steps,
        &terminal.coverage,
        terminal.issue,
        &terminal.blocked,
        &terminal.interactions,
    ))
    .map_err(|_| AgentFailure::StorageUnavailable)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
async fn terminal_receipt_on(
    connection: &turso::Connection,
    run_id: RunId,
) -> Result<Option<String>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT digest FROM agent_conversation_terminal_receipts WHERE run_id = ?",
            [run_id.as_uuid().to_string()],
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let digest = row.get::<String>(0).map_err(storage)?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(digest))
}
pub(super) fn same_publication(
    a: &floe_conversation::ConversationInteraction,
    b: &floe_conversation::ConversationInteraction,
) -> bool {
    a.id == b.id
        && a.person_id == b.person_id
        && a.session_id == b.session_id
        && a.origin_run_id == b.origin_run_id
        && a.origin_turn_id == b.origin_turn_id
        && a.origin == b.origin
        && a.audit == b.audit
        && a.kind == b.kind
        && a.requirement == b.requirement
        && a.requirement_digest == b.requirement_digest
        && a.target == b.target
        && a.target_digest == b.target_digest
        && a.created_at_unix_ms == b.created_at_unix_ms
        && a.expires_at_unix_ms == b.expires_at_unix_ms
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn enqueue_resume_on(
        &self,
        transaction: &Transaction<'_>,
        record: &RunRecord,
    ) -> Result<(), AgentFailure> {
        if !matches!(record.state, RunState::Completed | RunState::Blocked) {
            return Ok(());
        }
        let mut receipt = floe_conversation::project_run_receipt(record.clone())?;
        let entries = self.conversation_journal_on(transaction, record).await?;
        let accounting = floe_conversation::project_run_accounting(&receipt, &entries)?;
        receipt.attempt_refs = accounting.attempt_refs;
        receipt.task_refs = accounting.task_refs;
        receipt.unresolved_attempts = accounting.unresolved_attempts;
        let group = super::conversation_interactions::read_group_on(
            transaction,
            self.person_id,
            record.run_id,
        )
        .await?;
        let Some(pending) = floe_conversation::build_resume_required(&receipt, &group)? else {
            return Ok(());
        };
        if let Some((existing, _, _)) =
            resume_request_on(transaction, self.person_id, record.run_id).await?
        {
            if existing != pending {
                return Err(AgentFailure::Conflict);
            }
            return Ok(());
        }
        let session = self.session_on(transaction, record.session_id).await?;
        let state = if session.revision == pending.expected_session_revision
            && session.active_turn.is_none()
        {
            "pending"
        } else {
            "superseded"
        };
        let payload = serde_json::to_string(&pending).map_err(storage)?;
        transaction.execute("INSERT INTO agent_conversation_resume_requests (origin_run_id, person_id, session_id, state, child_run_id, payload) VALUES (?, ?, ?, ?, NULL, ?)",
            (record.run_id.as_uuid().to_string(), self.person_id.to_string(), record.session_id.to_string(), state, payload)).await.map_err(storage)?;
        Ok(())
    }
    pub async fn pending_conversation_resume_requests(
        &self,
        limit: usize,
    ) -> Result<Vec<ResumeRequired>, AgentFailure> {
        if limit == 0 || limit > 64 {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection.query("SELECT origin_run_id FROM agent_conversation_resume_requests WHERE person_id = ? AND state = 'pending' ORDER BY origin_run_id LIMIT ?", (self.person_id.to_string(), limit as i64 + 1)).await.map_err(storage)?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            ids.push(parse_run_id(&row.get::<String>(0).map_err(storage)?)?);
        }
        drop(rows);
        if ids.len() > limit {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut result = Vec::new();
        for id in ids {
            if let Some((request, state, _)) =
                resume_request_on(&connection, self.person_id, id).await?
            {
                if state == "pending" {
                    result.push(request);
                }
            }
        }
        self.check_access()?;
        Ok(result)
    }
    pub async fn claim_conversation_resume(
        &self,
        request: ResumeChildAdmission,
    ) -> Result<VaultConversationAdmission, AgentFailure> {
        request.validate()?;
        if request.request.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result: Result<Option<VaultConversationAdmission>, AgentFailure> = async {
            initialize(&transaction).await?;
            let (persisted, state, child_id) = resume_request_on(&transaction, self.person_id, request.request.origin_run_id).await?.ok_or(AgentFailure::Conflict)?;
            if persisted != request.request { return Err(AgentFailure::Conflict); }
            if state == "claimed" {
                let child = self.conversation_run_on(&transaction, child_id.ok_or(AgentFailure::StorageUnavailable)?).await?.ok_or(AgentFailure::StorageUnavailable)?;
                if child.resume_of != Some(persisted.origin_run_id) || child.session_id != persisted.session_id || child.person_id != persisted.person_id
                    || child.device_id != persisted.device_id || child.user_message_id != persisted.user_message_id || child.resume_lineage != persisted.lineage
                    || self.resume_slot_on(&transaction, persisted.origin_run_id).await? != Some(child.run_id) { return Err(AgentFailure::StorageUnavailable); }
                return Ok(Some(VaultConversationAdmission::Resumed(child)));
            }
            if state != "pending" { return Err(AgentFailure::Conflict); }
            let origin = self.conversation_run_on(&transaction, persisted.origin_run_id).await?.ok_or(AgentFailure::StorageUnavailable)?;
            let receipt = floe_conversation::project_run_receipt(origin)?;
            let group = super::conversation_interactions::read_group_on(&transaction, self.person_id, persisted.origin_run_id).await?;
            if floe_conversation::build_resume_required(&receipt, &group)?.as_ref() != Some(&persisted) { return Err(AgentFailure::Conflict); }
            let session = self.session_on(&transaction, persisted.session_id).await?;
            if session.revision != persisted.expected_session_revision || session.active_turn.is_some() {
                let changed = transaction.execute("UPDATE agent_conversation_resume_requests SET state = 'superseded' WHERE origin_run_id = ? AND state = 'pending'", [persisted.origin_run_id.as_uuid().to_string()]).await.map_err(storage)?;
                if changed != 1 { return Err(AgentFailure::Conflict); }
                self.check_access()?;
                return Ok(None);
            }
            let admission = self.admit_conversation_turn_on(&transaction, request.child).await?;
            let child = match &admission { VaultConversationAdmission::Created { record, .. } | VaultConversationAdmission::Existing(record) | VaultConversationAdmission::Resumed(record) => record };
            let changed = transaction.execute("UPDATE agent_conversation_resume_requests SET state = 'claimed', child_run_id = ? WHERE origin_run_id = ? AND state = 'pending'",
                (child.run_id.as_uuid().to_string(), persisted.origin_run_id.as_uuid().to_string())).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            self.check_access()?;
            Ok(Some(admission))
        }.await;
        self.finish_registry_transaction_checked(transaction, result)
            .await?
            .ok_or(AgentFailure::Conflict)
    }
}
async fn resume_request_on(
    connection: &turso::Connection,
    person: PersonId,
    origin: RunId,
) -> Result<Option<(ResumeRequired, String, Option<RunId>)>, AgentFailure> {
    let mut rows = connection.query("SELECT person_id, session_id, state, child_run_id, payload FROM agent_conversation_resume_requests WHERE origin_run_id = ?", [origin.as_uuid().to_string()]).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let payload = row.get::<String>(4).map_err(storage)?;
    if payload.len() > 8192 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let pending: ResumeRequired = serde_json::from_str(&payload).map_err(unavailable)?;
    pending.validate()?;
    let state = row.get::<String>(2).map_err(storage)?;
    let child = row
        .get::<Option<String>>(3)
        .map_err(storage)?
        .map(|id| parse_run_id(&id))
        .transpose()?;
    if pending.origin_run_id != origin
        || pending.person_id != person
        || row.get::<String>(0).map_err(storage)? != person.to_string()
        || row.get::<String>(1).map_err(storage)? != pending.session_id.to_string()
        || !matches!(state.as_str(), "pending" | "claimed" | "superseded")
        || (state == "claimed") != child.is_some()
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some((pending, state, child)))
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn start_conversation_session(
        &self,
        request: floe_conversation::StartSessionRequest,
    ) -> Result<floe_conversation::SessionReceipt, AgentFailure> {
        request.validate()?;
        if request.principal != self.person_id.to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            if let Some(receipt) = session_command_on(&transaction, self.person_id, request.command_id).await? {
                let session = self.session_on(&transaction, receipt.session_id).await?;
                if session.person_id != self.person_id || session.scope.is_some() || session.data_classes != [DataClass::Personal] { return Err(AgentFailure::StorageUnavailable); }
                return Ok(receipt);
            }
            if command_identity_used(&transaction, request.command_id.as_uuid()).await? { return Err(AgentFailure::Conflict); }
            if self.conversation_run_by_command_on(&transaction, request.command_id).await?.is_some() { return Err(AgentFailure::Conflict); }
            let mut rows = transaction.query("SELECT 1 FROM agent_conversation_commands WHERE command_id = ?", [request.command_id.as_uuid().to_string()]).await.map_err(storage)?;
            if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
            drop(rows);
            let mut rows = transaction.query("SELECT count(*) FROM agent_conversation_session_commands", ()).await.map_err(storage)?;
            let count = rows.next().await.map_err(storage)?.ok_or(AgentFailure::StorageUnavailable)?.get::<i64>(0).map_err(storage)?;
            if count < 0 || count >= MAX_COMMAND_ROWS { return Err(AgentFailure::BudgetExceeded); }
            drop(rows);
            let session = AgentSession::new(self.person_id);
            let receipt = floe_conversation::project_session_receipt(session.clone())?;
            transaction.execute("INSERT INTO agent_sessions (id, revision, payload) VALUES (?, 0, ?)", (session.id.to_string(), self.payload(&session)?)).await.map_err(storage)?;
            transaction.execute("INSERT INTO agent_conversation_session_commands (command_id, person_id, session_id, initial_revision) VALUES (?, ?, ?, 0)",
                (request.command_id.as_uuid().to_string(), self.person_id.to_string(), session.id.to_string())).await.map_err(storage)?;
            self.check_access()?;
            Ok(receipt)
        }.await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }
    pub async fn resume_conversation_session(&self) -> Result<AgentSession, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query("SELECT id FROM agent_sessions WHERE json_extract(payload, '$.scope') IS NULL AND json_extract(payload, '$.data_classes[0]') = 'personal' ORDER BY rowid DESC LIMIT 1", ()).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(unavailable)?;
        drop(rows);
        let session = self.session_on(&connection, id).await?;
        if session.person_id != self.person_id
            || session.scope.is_some()
            || session.data_classes != [DataClass::Personal]
        {
            return Err(AgentFailure::PolicyDenied);
        }
        self.check_access()?;
        Ok(session)
    }
}
async fn session_command_on(
    connection: &turso::Connection,
    person_id: PersonId,
    command_id: CommandId,
) -> Result<Option<floe_conversation::SessionReceipt>, AgentFailure> {
    let mut rows = connection.query("SELECT person_id, session_id, initial_revision FROM agent_conversation_session_commands WHERE command_id = ?", [command_id.as_uuid().to_string()]).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    if row.get::<String>(0).map_err(storage)? != person_id.to_string()
        || row.get::<i64>(2).map_err(storage)? != 0
    {
        return Err(AgentFailure::Conflict);
    }
    let receipt = floe_conversation::SessionReceipt {
        principal: person_id.to_string(),
        session_id: Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
            .map_err(unavailable)?,
        session_revision: 0,
    };
    receipt.validate()?;
    if rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(receipt))
}

pub(super) async fn command_identity_used(
    connection: &turso::Connection,
    command_id: Uuid,
) -> Result<bool, AgentFailure> {
    for table in [
        "agent_conversation_runs",
        "agent_conversation_commands",
        "agent_conversation_session_commands",
        "agent_conversation_recovery_commands",
        "agent_conversation_interaction_decisions",
        "agent_conversation_interaction_refreshes",
    ] {
        let mut exists = connection
            .query(
                "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?",
                [table],
            )
            .await
            .map_err(storage)?;
        if exists.next().await.map_err(storage)?.is_none() {
            continue;
        }
        drop(exists);
        let query = format!("SELECT 1 FROM {table} WHERE command_id = ?");
        let mut rows = connection
            .query(&query, [command_id.to_string()])
            .await
            .map_err(storage)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn journal_event_byte_limit(event: &JournalEvent) -> usize {
    match event {
        JournalEvent::DelegationResult { .. } => MAX_JOURNAL_ENTRY_BYTES,
        _ => 128 * 1024,
    }
}
