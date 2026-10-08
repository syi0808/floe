use super::database_failure;
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

const MAX_RUN_RECORD_BYTES: usize = 128 * 1024;
const MAX_JOURNAL_ENTRY_BYTES: usize = floe_agent_contract::MAX_TASK_RECEIPT_BYTES + 4096;
pub(super) const MAX_RUN_ROWS: i64 = 4_096;
const MAX_COMMAND_ROWS: i64 = 4_096;
pub(super) const MAX_JOURNAL_ENTRIES: u64 = 512;

#[cfg(test)]
static MODEL_SELECTION_ACK_LOSS_VAULTS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>,
> = std::sync::OnceLock::new();

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

/// Result of the one transaction-scoped linked-Resume owner transition.
/// `Superseded` must be committed by the transaction owner and reported as a
/// conflict after commit; rolling it back would resurrect a request already
/// made stale by a real New admission.
pub(super) enum ResumeClaimOutcome {
    Admitted(VaultConversationAdmission),
    Superseded,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationJournalEntry {
    pub revision: u64,
    pub kind: String,
    pub payload: String,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn activate_conversation_executor(
        &self,
    ) -> Result<VaultConversationActivation, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            validate_schema(&transaction).await?;
            let current_generation = executor_generation(&transaction).await?;
            let next_generation = current_generation
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            // Advancing the fence blocks every abandoned driver immediately.
            // Actor-scoped owner recovery pages settle those Runs after Ready;
            // activation never drains an arbitrarily accumulated backlog.
            let changed = transaction
                .execute(
                    "UPDATE agent_conversation_executor SET generation = ? WHERE id = 1 AND generation = ?",
                    (integer(next_generation)?, integer(current_generation)?),
                )
                .await
                .map_err(database_failure)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(VaultConversationActivation {
                executor_generation: next_generation,
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
    ) -> Result<VaultConversationAdmission, floe_kernel::CommandFailure<AgentFailure>> {
        if let Some(failure) = direct_resume_admission_failure(&request) {
            return Err(failure);
        }
        if !request.command_id.is_valid() {
            return Err(floe_kernel::CommandFailure::NotApplied(
                AgentFailure::InvalidInput,
            ));
        }
        let mut connection = self
            .connection()
            .map_err(floe_kernel::CommandFailure::Indeterminate)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| floe_kernel::CommandFailure::Indeterminate(database_failure(error)))?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = self
            .admit_conversation_turn_on(
                &transaction,
                request,
                &mut replay_checked,
                &mut prior_command,
            )
            .await
            .map_err(|failure| {
                if prior_command {
                    floe_kernel::CommandFailure::Indeterminate(failure)
                } else if replay_checked {
                    floe_kernel::CommandFailure::NotApplied(failure)
                } else {
                    floe_kernel::CommandFailure::Indeterminate(failure)
                }
            });
        self.finish_registry_command_transaction(transaction, result)
            .await
    }

    pub(super) async fn admit_conversation_turn_on(
        &self,
        transaction: &Transaction<'_>,
        request: TurnAdmissionRequest,
        replay_checked: &mut bool,
        prior_command: &mut bool,
    ) -> Result<VaultConversationAdmission, AgentFailure> {
        if !request.command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        validate_schema(transaction).await?;
        if session_command_on(transaction, self.person_id, request.command_id)
            .await?
            .is_some()
        {
            *prior_command = true;
            return Err(AgentFailure::Conflict);
        }
        if let Some(existing) = self
            .conversation_run_by_command_on(transaction, request.command_id)
            .await?
        {
            *prior_command = true;
            return if existing.matches_admission(&request) {
                Ok(VaultConversationAdmission::Existing(existing))
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        if command_occupant(transaction, request.command_id.as_uuid())
            .await?
            .is_some()
        {
            *prior_command = true;
            return Err(AgentFailure::Conflict);
        }
        let mut conflicting_command = transaction
            .query(
                "SELECT 1 FROM agent_conversation_commands WHERE command_id = ?",
                [request.command_id.as_uuid().to_string()],
            )
            .await
            .map_err(database_failure)?;
        if conflicting_command
            .next()
            .await
            .map_err(database_failure)?
            .is_some()
        {
            *prior_command = true;
            return Err(AgentFailure::Conflict);
        }
        *replay_checked = true;

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
            .map_err(database_failure)?;
        let rows = count
            .next()
            .await
            .map_err(database_failure)?
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
                (session.id.to_string(), self.person_id.to_string())).await.map_err(database_failure)?;
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
            .map_err(database_failure)?;
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
            pending_terminal: None,
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
            .map_err(database_failure)?;
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
                .map_err(database_failure)?;
        }
        self.check_access()?;
        Ok(VaultConversationAdmission::Created { record, session })
    }

    pub async fn admit_conversation_cancel(
        &self,
        request: VaultConversationCancelRequest,
    ) -> Result<VaultConversationCancelAdmission, floe_kernel::CommandFailure<AgentFailure>> {
        if !request.command_id.is_valid() {
            return Err(floe_kernel::CommandFailure::NotApplied(
                AgentFailure::InvalidInput,
            ));
        }
        if request.person_id != self.person_id {
            return Err(floe_kernel::CommandFailure::NotAdmitted(
                AgentFailure::CapabilityDenied,
            ));
        }
        let mut connection = self
            .connection()
            .map_err(floe_kernel::CommandFailure::Indeterminate)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| {
                floe_kernel::CommandFailure::Indeterminate(
                    self.registry_transaction_start_error(error),
                )
            })?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = async {
            validate_schema(&transaction).await?;
            if session_command_on(&transaction, self.person_id, request.command_id).await?.is_some() { prior_command = true; return Err(AgentFailure::Conflict); }
            let mut existing = transaction
                .query(
                    "SELECT person_id, target_id, kind FROM agent_conversation_commands WHERE command_id = ?",
                    [request.command_id.as_uuid().to_string()],
                )
                .await
                .map_err(database_failure)?;
            if let Some(row) = existing.next().await.map_err(database_failure)? {
                prior_command = true;
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
            if command_occupant(&transaction, request.command_id.as_uuid()).await?.is_some() { prior_command = true; return Err(AgentFailure::Conflict); }
            if self
                .conversation_run_by_command_on(&transaction, request.command_id)
                .await?
                .is_some()
            {
                prior_command = true;
                return Err(AgentFailure::Conflict);
            }
            replay_checked = true;
            if !request.run_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
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
                .map_err(database_failure)?;
            let rows = count
                .next()
                .await
                .map_err(database_failure)?
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
                .map_err(database_failure)?;
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
        let result = result.map_err(|failure| {
            if prior_command {
                floe_kernel::CommandFailure::Indeterminate(failure)
            } else if replay_checked {
                floe_kernel::CommandFailure::NotApplied(failure)
            } else {
                floe_kernel::CommandFailure::Indeterminate(failure)
            }
        });
        self.finish_registry_command_transaction(transaction, result)
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
            if record.state != RunState::Working || record.pending_terminal.is_some() { return Err(AgentFailure::Conflict); }
            if record.journal_revision >= MAX_JOURNAL_ENTRIES { return Err(AgentFailure::BudgetExceeded); }
            let event: JournalEvent = serde_json::from_str(payload).map_err(|_| AgentFailure::InvalidInput)?;
            if kind != journal_kind(&event) || payload.len() > journal_event_byte_limit(&event) { return Err(AgentFailure::InvalidInput); }
            let mut entries = self.conversation_journal_on(&transaction, &record).await?;
            if let JournalEvent::ModelIntent { plan, .. } = &event {
                let prior = self
                    .model_selection_for_append_on(&transaction, &record, &entries)
                    .await?;
                floe_agent_contract::validate_model_intent_selection(&prior, plan)?;
            }
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
                .map_err(database_failure)?;
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
                .map_err(database_failure)?;
            self.check_access()?;
            Ok(record.journal_revision)
        }
        .await;
        let revision = self
            .finish_registry_transaction_checked(transaction, result)
            .await?;
        #[cfg(test)]
        if MODEL_SELECTION_ACK_LOSS_VAULTS
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .remove(&self.vault_id)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(revision)
    }

    async fn model_selection_for_append_on(
        &self,
        transaction: &Transaction<'_>,
        record: &RunRecord,
        entries: &[JournalEntry],
    ) -> Result<floe_agent_contract::ModelSelectionState, AgentFailure> {
        let mut chain = vec![(record.clone(), entries.to_vec())];
        let mut seen = std::collections::HashSet::from([record.run_id]);
        let mut child = record.clone();
        let mut total_entries = entries.len();
        let mut continuation_depth = 0_usize;

        while let Some(parent_id) = child.continuation_of {
            if continuation_depth >= usize::from(MAX_RESUME_LINEAGE) || !seen.insert(parent_id) {
                return Err(AgentFailure::StorageUnavailable);
            }
            let parent = self
                .conversation_run_on(transaction, parent_id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let child_receipt = floe_conversation::project_run_receipt(child.clone())?;
            let parent_receipt = floe_conversation::project_run_receipt(parent.clone())?;
            let reference = parent_receipt
                .continuation()
                .ok_or(AgentFailure::StorageUnavailable)?;
            if child.person_id != parent.person_id
                || child_receipt.principal != parent_receipt.principal
                || child.session_id != parent.session_id
                || child.device_id != parent.device_id
                || child.continuation_of != Some(parent_id)
                || reference.run_id != parent_id
                || child.continuation_executor_generation != Some(parent.executor_generation)
                || child.continuation_level != reference.level
            {
                return Err(AgentFailure::StorageUnavailable);
            }

            let parent_entries = self.conversation_journal_on(transaction, &parent).await?;
            total_entries = total_entries
                .checked_add(parent_entries.len())
                .ok_or(AgentFailure::StorageUnavailable)?;
            if total_entries > floe_conversation::MAX_CONTINUATION_JOURNAL_ENTRIES {
                return Err(AgentFailure::BudgetExceeded);
            }
            chain.push((parent.clone(), parent_entries));
            child = parent;
            continuation_depth += 1;
        }

        chain.reverse();
        let mut fold = floe_conversation::ResumeLineageFold::default();
        let chain_len = chain.len();
        for (index, (ancestor, ancestor_entries)) in chain.into_iter().enumerate() {
            let receipt = floe_conversation::project_run_receipt(ancestor)?;
            let projection =
                floe_conversation::project_model_selection_journal(&receipt, &ancestor_entries)?;
            if index + 1 == chain_len
                && matches!(
                    &projection.lineage,
                    floe_conversation::JournalLineage::ResumeBatchOnly { .. }
                )
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let live = projection
                .pending_batch
                .clone()
                .zip(projection.cursor.clone());
            fold = floe_conversation::fold_resume_lineage(
                &fold,
                &projection.lineage,
                live,
                &projection.selection,
            )?;
        }
        if fold.pending.is_some() {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(fold.execution_selection)
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
            .map_err(database_failure)?;
        let result = self
            .finish_conversation_run_on(&transaction, run_id, expected_aggregate_revision, terminal)
            .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub(super) async fn finish_conversation_run_on(
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
                    >= expected_aggregate_revision
                        .checked_add(1)
                        .ok_or(AgentFailure::Conflict)?
            {
                Ok(current)
            } else {
                Err(AgentFailure::Conflict)
            };
        }
        if let Some(pending) = current.pending_terminal {
            return if pending.failure == terminal.issue.ok_or(AgentFailure::Conflict)?
                && pending.requested_from_revision == expected_aggregate_revision
                && terminal.state == RunTerminal::from_failure(pending.failure).state
                && terminal.output.is_none()
                && terminal.steps.is_empty()
                && terminal.interactions.is_empty()
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
        let journal = self.conversation_journal_on(transaction, &current).await?;
        if !floe_conversation::unresolved_run_delegations(
            &floe_conversation::project_run_receipt(current.clone())?,
            &journal,
        )?
        .is_empty()
        {
            let failure = terminal.issue.ok_or(AgentFailure::Conflict)?;
            if terminal.output.is_some()
                || !terminal.steps.is_empty()
                || !terminal.interactions.is_empty()
                || terminal.state != RunTerminal::from_failure(failure).state
            {
                return Err(AgentFailure::Conflict);
            }
            let next = floe_conversation::defer_run_terminal(&current, failure)?;
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
            self.check_access()?;
            return Ok(next);
        }
        self.apply_conversation_terminal_on(transaction, &current, terminal, digest)
            .await
    }

    pub(super) async fn apply_conversation_terminal_on(
        &self,
        transaction: &Transaction<'_>,
        current: &RunRecord,
        terminal: RunTerminal,
        digest: String,
    ) -> Result<RunRecord, AgentFailure> {
        let run_id = current.run_id;
        let session = self.session_on(transaction, current.session_id).await?;
        let journal = self.conversation_journal_on(transaction, current).await?;
        floe_conversation::validate_terminal_steps(&terminal, &journal)?;
        let (next, mut next_session) =
            floe_conversation::apply_terminal(current, &session, &terminal, &journal)?;
        let accounting = self
            .conversation_lineage_accounting_on(transaction, current, &journal)
            .await?;
        if (!accounting.unresolved_attempts.is_empty()
            || !accounting.unresolved_delegations.is_empty())
            && matches!(terminal.state, RunState::Completed | RunState::Blocked)
        {
            return Err(AgentFailure::Conflict);
        }
        next_session.usage = accounting.usage;
        if !accounting.unresolved_attempts.is_empty()
            || !accounting.unresolved_delegations.is_empty()
        {
            next_session.continuation = None;
        }
        if let Some(continuation) = &mut next_session.continuation {
            continuation.usage = accounting.usage;
        }
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
            .map_err(database_failure)?;
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
            .map_err(database_failure)?;
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
            validate_schema(&transaction).await?;
            super::conversation_interactions::validate_schema(&transaction).await?;
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
            self.validate_blocked_task_audits_on(&transaction, &commit)
                .await?;
            for publication in &commit.publications {
                if publication.record.device_id != current.device_id {
                    return Err(AgentFailure::Conflict);
                }
                self.store_review_audit_on(&transaction, &current, &publication.record, replay)
                    .await?;
                for interaction in &publication.interactions {
                    let link = block
                        .interactions
                        .iter()
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
        let mut rows = connection.query("SELECT revision, kind, payload FROM agent_conversation_journal WHERE run_id = ? ORDER BY revision LIMIT 513", [record.run_id.as_uuid().to_string()]).await.map_err(database_failure)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
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

    pub async fn conversation_command_occupant(
        &self,
        command_id: CommandId,
    ) -> Result<Option<floe_conversation::ConversationCommandKind>, AgentFailure> {
        if !command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let occupant = command_occupant(&connection, command_id.as_uuid()).await?;
        self.check_access()?;
        Ok(occupant)
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
            .map_err(database_failure)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
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
            .map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            return Ok(None);
        };
        let child = RunId::from_uuid(
            Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        )
        .ok_or(AgentFailure::VaultUnavailable)?;
        if rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(Some(child))
    }

    /// The origin group admits a child only once every card is terminal
    /// and at least one resolved. Missing rows conflict: nothing was reviewed.
    async fn check_resume_group(
        &self,
        transaction: &Transaction<'_>,
        origin_run_id: RunId,
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT state FROM agent_conversation_interactions WHERE origin_run_id = ?",
                [origin_run_id.as_uuid().to_string()],
            )
            .await
            .map_err(database_failure)?;
        let mut count = 0u64;
        let mut resolved = false;
        while let Some(row) = rows.next().await.map_err(database_failure)? {
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

    pub(super) async fn active_conversation_executor_generation(
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

/// Linked resume admissions must go through the atomic resume-slot claim. This
/// lower-level entry point cannot prove a valid command ID is unused before it
/// rejects the mode, so preserve that ID as uncertain instead of freeing it.
fn direct_resume_admission_failure(
    request: &TurnAdmissionRequest,
) -> Option<floe_kernel::CommandFailure<AgentFailure>> {
    matches!(&request.mode, TurnMode::Resume(_)).then(|| {
        if request.command_id.is_valid() {
            floe_kernel::CommandFailure::Indeterminate(AgentFailure::InvalidInput)
        } else {
            floe_kernel::CommandFailure::NotApplied(AgentFailure::InvalidInput)
        }
    })
}

#[cfg(test)]
mod command_admission_tests {
    use super::*;

    fn resume_request(
        command_id: CommandId,
        revision: u64,
        digest: [u8; 32],
    ) -> TurnAdmissionRequest {
        TurnAdmissionRequest {
            expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
            run_id: RunId::new(),
            command_id,
            session_id: Uuid::new_v4(),
            expected_session_revision: revision,
            principal: "person".to_owned(),
            device_id: "device".to_owned(),
            request_digest: digest,
            mode: TurnMode::Resume(floe_conversation::InteractionResumeRef {
                origin_run_id: RunId::new(),
                lineage: 1,
            }),
            retry_of: None,
            input: TurnInput::ExistingMessage {
                message_id: Uuid::new_v4(),
            },
        }
    }

    #[test]
    fn rejected_direct_resume_keeps_a_reused_valid_command_id_uncertain() {
        let command_id = CommandId::new();
        let original = resume_request(command_id, 4, [2; 32]);
        let changed_body = resume_request(command_id, 9, [3; 32]);

        assert_eq!(
            direct_resume_admission_failure(&original),
            Some(floe_kernel::CommandFailure::Indeterminate(
                AgentFailure::InvalidInput
            ))
        );
        assert_eq!(
            direct_resume_admission_failure(&changed_body),
            Some(floe_kernel::CommandFailure::Indeterminate(
                AgentFailure::InvalidInput
            ))
        );
        assert_eq!(
            direct_resume_admission_failure(&resume_request(CommandId(Uuid::nil()), 4, [2; 32])),
            Some(floe_kernel::CommandFailure::NotApplied(
                AgentFailure::InvalidInput
            ))
        );
    }
}

pub(super) async fn validate_schema(transaction: &turso::Connection) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(transaction, crate::schema::Family::Conversation)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)?;
    executor_generation(transaction).await?;
    Ok(())
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
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
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
        || rows.next().await.map_err(database_failure)?.is_some()
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
        .map_err(database_failure)?;
    let value = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    if value < 0 || rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(value as u64)
}

pub(super) async fn write_run(
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
        .map_err(database_failure)
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

pub(super) fn journal_kind(event: &JournalEvent) -> &'static str {
    match event {
        JournalEvent::ModelIntent { .. }
        | JournalEvent::ToolIntent { .. }
        | JournalEvent::DelegationIntent { .. } => "intent",
        JournalEvent::ModelResult { .. }
        | JournalEvent::ToolResult { .. }
        | JournalEvent::ToolReviewRequired { .. }
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
pub(super) fn terminal_digest(
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
pub(super) async fn terminal_receipt_on(
    connection: &turso::Connection,
    run_id: RunId,
) -> Result<Option<String>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT digest FROM agent_conversation_terminal_receipts WHERE run_id = ?",
            [run_id.as_uuid().to_string()],
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let digest = row.get::<String>(0).map_err(storage)?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || rows.next().await.map_err(database_failure)?.is_some()
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
            (record.run_id.as_uuid().to_string(), self.person_id.to_string(), record.session_id.to_string(), state, payload)).await.map_err(database_failure)?;
        Ok(())
    }
    pub async fn pending_conversation_resume_requests(
        &self,
        actor: &floe_kernel::OwnerActor,
        after: Option<RunId>,
        limit: usize,
    ) -> Result<floe_conversation::RecoveryPage<ResumeRequired, RunId>, AgentFailure> {
        actor.validate()?;
        if actor.person_id != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        if limit == 0 || limit > 64 || after.is_some_and(|id| !id.is_valid()) {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection.query("SELECT origin_run_id FROM agent_conversation_resume_requests WHERE person_id = ? AND json_extract(payload, '$.device_id') = ? AND state = 'pending' AND origin_run_id > ? ORDER BY origin_run_id LIMIT ?", (self.person_id.to_string(), actor.device_id.clone(), after.map_or_else(String::new, |id| id.as_uuid().to_string()), limit as i64 + 1)).await.map_err(database_failure)?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            ids.push(parse_run_id(&row.get::<String>(0).map_err(storage)?)?);
        }
        drop(rows);
        let more = ids.len() > limit;
        ids.truncate(limit);
        let next_cursor = if more { ids.last().copied() } else { None };
        let mut result = Vec::new();
        for id in ids {
            if let Some((request, state, _)) =
                resume_request_on(&connection, self.person_id, id).await?
            {
                if request.device_id != actor.device_id {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if state == "pending" {
                    result.push(request);
                }
            }
        }
        self.check_access()?;
        Ok(floe_conversation::RecoveryPage {
            items: result,
            next_cursor,
        })
    }
    pub async fn pending_conversation_resume_request(
        &self,
        actor: &floe_kernel::OwnerActor,
        origin: RunId,
    ) -> Result<Option<ResumeRequired>, AgentFailure> {
        actor.validate()?;
        if actor.person_id != self.person_id || !origin.is_valid() {
            return Err(AgentFailure::PolicyDenied);
        }
        let connection = self.connection()?;
        let result = match resume_request_on(&connection, self.person_id, origin).await? {
            Some((request, state, _))
                if request.device_id == actor.device_id && state == "pending" =>
            {
                Some(request)
            }
            Some((request, _, _)) if request.device_id != actor.device_id => {
                return Err(AgentFailure::PolicyDenied);
            }
            _ => None,
        };
        self.check_access()?;
        Ok(result)
    }
    pub async fn reconcile_conversation_resume_request(
        &self,
        actor: &floe_kernel::OwnerActor,
        origin: RunId,
    ) -> Result<Option<ResumeRequired>, AgentFailure> {
        actor.validate()?;
        if actor.person_id != self.person_id || !origin.is_valid() {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            self.active_conversation_executor_generation(&transaction).await?;
            let Some((request, state, _)) = resume_request_on(&transaction, self.person_id, origin).await? else { return Ok(None); };
            if request.device_id != actor.device_id { return Err(AgentFailure::PolicyDenied); }
            if state != "pending" { return Ok(None); }
            let session = self.session_on(&transaction, request.session_id).await?;
            if session.revision != request.expected_session_revision || session.active_turn.is_some() {
                if transaction.execute("UPDATE agent_conversation_resume_requests SET state = 'superseded' WHERE origin_run_id = ? AND state = 'pending'",
                    [origin.as_uuid().to_string()]).await.map_err(database_failure)? != 1 { return Err(AgentFailure::Conflict); }
                return Ok(None);
            }
            self.check_access()?;
            Ok(Some(request))
        }.await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
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
        let result = self
            .claim_conversation_resume_on(&transaction, request)
            .await
            .map(|outcome| match outcome {
                ResumeClaimOutcome::Admitted(admission) => Some(admission),
                ResumeClaimOutcome::Superseded => None,
            });
        self.finish_registry_transaction_checked(transaction, result)
            .await?
            .ok_or(AgentFailure::Conflict)
    }

    /// The single linked-Resume owner implementation, shared by the public
    /// owner-only wrapper and the internal owner/Core composer. The caller
    /// owns the Immediate transaction and must commit `Superseded` before
    /// returning its public conflict.
    pub(super) async fn claim_conversation_resume_on(
        &self,
        transaction: &Transaction<'_>,
        request: ResumeChildAdmission,
    ) -> Result<ResumeClaimOutcome, AgentFailure> {
        request.validate()?;
        if request.request.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        validate_schema(transaction).await?;
        let (persisted, state, child_id) =
            resume_request_on(transaction, self.person_id, request.request.origin_run_id)
                .await?
                .ok_or(AgentFailure::Conflict)?;
        if persisted != request.request {
            return Err(AgentFailure::Conflict);
        }
        if state == "claimed" {
            let child = self
                .conversation_run_on(
                    transaction,
                    child_id.ok_or(AgentFailure::StorageUnavailable)?,
                )
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if child.resume_of != Some(persisted.origin_run_id)
                || child.session_id != persisted.session_id
                || child.person_id != persisted.person_id
                || child.device_id != persisted.device_id
                || child.user_message_id != persisted.user_message_id
                || child.resume_lineage != persisted.lineage
                || self
                    .resume_slot_on(transaction, persisted.origin_run_id)
                    .await?
                    != Some(child.run_id)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            return Ok(ResumeClaimOutcome::Admitted(
                VaultConversationAdmission::Resumed(child),
            ));
        }
        if state != "pending" {
            return Err(AgentFailure::Conflict);
        }
        let origin = self
            .conversation_run_on(transaction, persisted.origin_run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let receipt = floe_conversation::project_run_receipt(origin)?;
        let group = super::conversation_interactions::read_group_on(
            transaction,
            self.person_id,
            persisted.origin_run_id,
        )
        .await?;
        if floe_conversation::build_resume_required(&receipt, &group)?.as_ref() != Some(&persisted)
        {
            return Err(AgentFailure::Conflict);
        }
        let session = self.session_on(transaction, persisted.session_id).await?;
        if session.revision != persisted.expected_session_revision || session.active_turn.is_some()
        {
            let changed = transaction
                .execute(
                    "UPDATE agent_conversation_resume_requests SET state = 'superseded' WHERE origin_run_id = ? AND state = 'pending'",
                    [persisted.origin_run_id.as_uuid().to_string()],
                )
                .await
                .map_err(database_failure)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            return Ok(ResumeClaimOutcome::Superseded);
        }
        let mut replay_checked = false;
        let mut prior_command = false;
        let admission = self
            .admit_conversation_turn_on(
                transaction,
                request.child,
                &mut replay_checked,
                &mut prior_command,
            )
            .await?;
        let child = match &admission {
            VaultConversationAdmission::Created { record, .. }
            | VaultConversationAdmission::Existing(record)
            | VaultConversationAdmission::Resumed(record) => record,
        };
        let changed = transaction
            .execute(
                "UPDATE agent_conversation_resume_requests SET state = 'claimed', child_run_id = ? WHERE origin_run_id = ? AND state = 'pending'",
                (
                    child.run_id.as_uuid().to_string(),
                    persisted.origin_run_id.as_uuid().to_string(),
                ),
            )
            .await
            .map_err(database_failure)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        self.check_access()?;
        Ok(ResumeClaimOutcome::Admitted(admission))
    }
}
async fn resume_request_on(
    connection: &turso::Connection,
    person: PersonId,
    origin: RunId,
) -> Result<Option<(ResumeRequired, String, Option<RunId>)>, AgentFailure> {
    let mut rows = connection.query("SELECT person_id, session_id, state, child_run_id, payload FROM agent_conversation_resume_requests WHERE origin_run_id = ?", [origin.as_uuid().to_string()]).await.map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
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
        || rows.next().await.map_err(database_failure)?.is_some()
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some((pending, state, child)))
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn start_conversation_session(
        &self,
        request: floe_conversation::StartSessionRequest,
    ) -> Result<floe_conversation::SessionStartAdmission, floe_conversation::SessionStartFailure>
    {
        use floe_conversation::{
            ConversationCommandKind as K, SessionStartAdmission as A, SessionStartFailure as F,
            SessionStartRefusal as R,
        };
        request.validate().map_err(F::NotAdmitted)?;
        if request.principal != self.person_id.to_string() {
            return Err(F::NotAdmitted(AgentFailure::CapabilityDenied));
        }
        let mut connection = self.connection().map_err(F::NotAdmitted)?;
        let (mut writer, transaction) =
            self.journal_transaction(&mut connection)
                .await
                .map_err(|failure| {
                    if failure == AgentFailure::StorageBusy {
                        F::NotAdmitted(failure)
                    } else {
                        F::Indeterminate(failure)
                    }
                })?;
        let result: Result<A, AgentFailure> = async {
            validate_schema(&transaction).await?;
            let existing = session_command_on(&transaction, self.person_id, request.command_id).await?;
            // The fixed query validates both families and rejects double occupancy.
            let occupant = command_occupant(&transaction, request.command_id.as_uuid()).await?;
            if let Some(receipt) = existing {
                if occupant != Some(K::SessionStart) {
                    return Err(AgentFailure::VaultUnavailable);
                }
                let session = self.session_on(&transaction, receipt.session_id).await
                    .map_err(|failure| if failure == AgentFailure::NotFound {
                        AgentFailure::VaultUnavailable
                    } else { failure })?;
                if session.person_id != self.person_id || session.scope.is_some()
                    || session.data_classes != [DataClass::Personal] {
                    return Err(AgentFailure::StorageUnavailable);
                }
                return Ok(A::Replayed(receipt));
            }
            if let Some(kind) = occupant {
                if kind == K::SessionStart { return Err(AgentFailure::VaultUnavailable); }
                return Ok(A::NotApplied(R::ForeignCommand(kind)));
            }
            let mut rows = transaction.query("SELECT count(*) FROM agent_conversation_session_commands", ()).await.map_err(database_failure)?;
            let count = rows.next().await.map_err(database_failure)?.ok_or(AgentFailure::StorageUnavailable)?.get::<i64>(0).map_err(storage)?;
            if count < 0 { return Err(AgentFailure::VaultUnavailable); }
            if count >= MAX_COMMAND_ROWS { return Ok(A::NotApplied(R::Capacity)); }
            drop(rows);
            let session = AgentSession::new(self.person_id);
            let receipt = floe_conversation::project_session_receipt(session.clone())?;
            transaction.execute("INSERT INTO agent_sessions (id, revision, payload) VALUES (?, 0, ?)", (session.id.to_string(), self.payload(&session)?)).await.map_err(database_failure)?;
            transaction.execute("INSERT INTO agent_conversation_session_commands (command_id, person_id, session_id, initial_revision) VALUES (?, ?, ?, 0)",
                (request.command_id.as_uuid().to_string(), self.person_id.to_string(), session.id.to_string())).await.map_err(database_failure)?;
            self.check_access()?;
            Ok(A::Started(receipt))
        }.await;
        // Dropping this future during BEGIN/body/commit/rollback leaves the guard
        // armed. The shared store is retired before another Start can claim absence.
        let finished = self
            .finish_registry_transaction_checked(transaction, result)
            .await;
        writer.settled();
        finished.map_err(F::Indeterminate)
    }
    pub async fn resume_conversation_session(&self) -> Result<Option<AgentSession>, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query("SELECT id FROM agent_sessions WHERE json_extract(payload, '$.scope') IS NULL AND json_extract(payload, '$.data_classes[0]') = 'personal' ORDER BY rowid DESC LIMIT 1", ()).await.map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            self.check_access()?;
            return Ok(None);
        };
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
        Ok(Some(session))
    }
}
async fn session_command_on(
    connection: &turso::Connection,
    person_id: PersonId,
    command_id: CommandId,
) -> Result<Option<floe_conversation::SessionReceipt>, AgentFailure> {
    let mut rows = connection.query("SELECT person_id, session_id, initial_revision FROM agent_conversation_session_commands WHERE command_id = ?", [command_id.as_uuid().to_string()]).await.map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    if row.get::<String>(0).map_err(storage)? != person_id.to_string()
        || row.get::<i64>(2).map_err(storage)? != 0
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let receipt = floe_conversation::SessionReceipt {
        principal: person_id.to_string(),
        session_id: Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
            .map_err(unavailable)?,
        session_revision: 0,
    };
    receipt.validate()?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(receipt))
}

/// One fixed occupancy read across the current Conversation identity owners.
/// No row in these tables is evicted or changes owner kind. Start's structural
/// NotApplied proof depends on those invariants, not merely on a rollback.
pub(super) async fn command_occupant(
    connection: &turso::Connection,
    command_id: Uuid,
) -> Result<Option<floe_conversation::ConversationCommandKind>, AgentFailure> {
    use floe_conversation::ConversationCommandKind as K;
    for family in [
        crate::schema::Family::Conversation,
        crate::schema::Family::Interactions,
    ] {
        crate::schema::inspect_family(connection, family)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?;
    }
    let mut rows = connection.query(
        "SELECT 'run' FROM agent_conversation_runs WHERE command_id = ?1
         UNION ALL SELECT 'cancel' FROM agent_conversation_commands WHERE command_id = ?1
         UNION ALL SELECT 'start' FROM agent_conversation_session_commands WHERE command_id = ?1
         UNION ALL SELECT 'decision' FROM agent_conversation_interaction_decisions WHERE command_id = ?1
         UNION ALL SELECT 'refresh' FROM agent_conversation_interaction_refreshes WHERE command_id = ?1",
        [command_id.to_string()],
    ).await.map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let kind = match row.get::<String>(0).map_err(storage)?.as_str() {
        "run" => K::Run,
        "cancel" => K::Cancel,
        "start" => K::SessionStart,
        "decision" => K::InteractionDecision,
        "refresh" => K::InteractionRefresh,
        _ => return Err(AgentFailure::VaultUnavailable),
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(kind))
}

fn journal_event_byte_limit(event: &JournalEvent) -> usize {
    match event {
        JournalEvent::DelegationResult { .. } => MAX_JOURNAL_ENTRY_BYTES,
        _ => 128 * 1024,
    }
}

#[cfg(test)]
mod model_selection_owner_tests {
    use std::{
        collections::HashMap,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::{RootKey, VaultKeyProvider};
    use floe_agent_contract::{
        BatchCursor, InvocationKey, ModelBudgetProfile, ModelCapabilities,
        ModelSelectionCommitment, ModelSelectionState, ModelStep, PreparedModelPlan,
        ProcessingBoundary, ProjectionRef, ValidatedModelBatch,
    };
    use floe_conversation::{CanonicalTurnIntent, StartSessionRequest};
    use floe_kernel::PersonId;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<RootKey, AgentFailure> {
            self.0
                .lock()
                .map_err(|_| AgentFailure::VaultUnavailable)?
                .get(&(person_id, vault_id))
                .copied()
                .map(RootKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &RootKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .map_err(|_| AgentFailure::VaultUnavailable)?
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("floe-model-selection-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create isolated Vault test root");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict isolated Vault test root");
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct StartedRun {
        root: TestRoot,
        keys: TestKeys,
        person_id: PersonId,
        vault: Arc<EncryptedAgentVault<TestKeys>>,
        record: RunRecord,
    }

    async fn start_run() -> StartedRun {
        let root = TestRoot::new();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = Arc::new(
            EncryptedAgentVault::create(&root.0, person_id, keys.clone())
                .await
                .expect("create isolated encrypted Vault"),
        );
        let session = match vault
            .start_conversation_session(StartSessionRequest {
                principal: person_id.to_string(),
                command_id: CommandId::new(),
            })
            .await
            .expect("start persisted Conversation session")
        {
            floe_conversation::SessionStartAdmission::Started(receipt) => receipt,
            other => panic!("unexpected initial session admission: {other:?}"),
        };
        vault
            .activate_conversation_executor()
            .await
            .expect("activate fenced Conversation executor");

        let run_id = RunId::new();
        let command_id = CommandId::new();
        let principal = person_id.to_string();
        let text = "Pin this model execution.".to_owned();
        let intent = CanonicalTurnIntent {
            session_id: session.session_id,
            expected_revision: session.session_revision,
            text: text.clone(),
            mode: TurnMode::New,
            retry_of: None,
        };
        let request = TurnAdmissionRequest {
            expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [1; 32],
            },
            run_id,
            command_id,
            session_id: session.session_id,
            expected_session_revision: session.session_revision,
            principal: principal.clone(),
            device_id: "device-model-selection".into(),
            request_digest: intent.digest(&principal).expect("canonical turn digest"),
            mode: TurnMode::New,
            retry_of: None,
            input: TurnInput::NewMessage(floe_agent_contract::AgentMessage {
                message_id: command_id.as_uuid(),
                role: floe_agent_contract::MessageRole::User,
                text,
                call_id: None,
                coverage: DependencyCoverage::Independent,
            }),
        };
        let admission = vault
            .admit_conversation_turn(request)
            .await
            .expect("admit owner Run");
        let VaultConversationAdmission::Created { record, .. } = admission else {
            panic!("initial Run was not newly admitted: {admission:?}");
        };
        StartedRun {
            root,
            keys,
            person_id,
            vault,
            record,
        }
    }

    fn plan(record: &RunRecord, commitment: u8) -> PreparedModelPlan {
        PreparedModelPlan {
            operation_id: Uuid::new_v4(),
            principal: record.person_id.to_string(),
            device_id: record.device_id.clone(),
            purpose: "everyday_assistance".into(),
            consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
            capabilities: ModelCapabilities::chat(),
            boundary: ProcessingBoundary::Device,
            binding_digest: floe_agent_contract::ModelBindingDigest([2; 32]),
            selection_commitment: Some(ModelSelectionCommitment([commitment; 32])),
            budget_profile: Some(ModelBudgetProfile::unknown()),
        }
    }

    fn model_intent(
        attempt_id: Uuid,
        projection_ref: ProjectionRef,
        plan: PreparedModelPlan,
    ) -> JournalEvent {
        JournalEvent::ModelIntent {
            attempt_id,
            parent_task_id: None,
            reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                tokens: 10,
                cost_micros: 10,
            },
            projection_ref,
            plan,
        }
    }

    async fn append_event(
        vault: &EncryptedAgentVault<TestKeys>,
        run_id: RunId,
        event: JournalEvent,
    ) -> Result<u64, AgentFailure> {
        let kind = journal_kind(&event);
        let payload = serde_json::to_string(&event).expect("serialize journal event");
        vault
            .append_conversation_journal(run_id, kind, &payload)
            .await
    }

    fn resolved_binding_interaction(
        record: &RunRecord,
    ) -> floe_conversation::ConversationInteraction {
        use floe_conversation::{
            BlockedReviewEvidence, ConversationInteraction, InteractionOrigin,
            InteractionRequirement, InteractionRequirementKind, InteractionResolutionCause,
            InteractionResolutionReceipt, InteractionState, OwnerResolutionReceipt,
            ReviewAuditRecord, ReviewedTarget, canonical_requirement_digest,
            canonical_target_digest, interaction_publication_id,
        };

        let review_ref = floe_experts::BindingReviewRef {
            id: Uuid::new_v4(),
            digest: [3; 32],
        };
        let execution = floe_agent_contract::TaskExecutionReceiptRef {
            execution: floe_agent_contract::TaskExecutionKey {
                task_id: floe_agent_contract::TaskId::new(),
                execution_id: Uuid::new_v4(),
                executor_generation: 1,
            },
            task_revision: 2,
            journal_revision: 0,
            digest: [4; 32],
        };
        let origin = InteractionOrigin::Task {
            execution: execution.clone(),
            capability_call_id: None,
        };
        let target = ReviewedTarget::ExpertBinding(review_ref.clone());
        let requirement = InteractionRequirement {
            kind: InteractionRequirementKind::ConfigureExpertBinding,
            source_id: "floe.expert.binding".into(),
            connection_id: None,
            consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
            purpose: "configuration".into(),
            inline: false,
        };
        let owner_receipt = OwnerResolutionReceipt::ExpertBinding {
            receipt: floe_experts::BindingMutationReceipt {
                command_id: CommandId::new(),
                review_ref: review_ref.clone(),
                assignment_ref: Uuid::new_v4(),
                binding_revision: 1,
                registry_revision: 1,
                committed_at_unix_ms: 1,
            },
        };
        let receipt = InteractionResolutionReceipt {
            cause: InteractionResolutionCause::Refresh {
                command_id: Uuid::new_v4(),
            },
            owner_command_id: owner_receipt.command_id(),
            owner_operation_id: owner_receipt.operation_id(),
            owner_receipt,
            resolved_at_unix_ms: 1,
        };
        let audit = ReviewAuditRecord {
            person_id: record.person_id,
            device_id: record.device_id.clone(),
            session_id: record.session_id,
            run_id: record.run_id,
            executor_generation: record.executor_generation,
            operation_id: Uuid::new_v4(),
            evidence: BlockedReviewEvidence::ExpertBinding {
                execution,
                requirement_key: "calendar".into(),
                review: review_ref,
            },
        };
        let requirement_digest =
            canonical_requirement_digest(&requirement).expect("valid binding requirement digest");
        let target_digest = canonical_target_digest(&target).expect("valid binding target digest");
        let id =
            interaction_publication_id(record.run_id, &origin, &requirement_digest, &target_digest)
                .expect("valid publication identity");
        let interaction = ConversationInteraction {
            id,
            person_id: record.person_id,
            session_id: record.session_id,
            origin_run_id: record.run_id,
            origin_turn_id: record.run_id.as_uuid(),
            origin,
            audit,
            kind: floe_agent_contract::UserInteractionKind::ExpertBinding,
            requirement,
            requirement_digest,
            target,
            target_digest,
            state: InteractionState::Resolved { receipt },
            revision: 2,
            created_at_unix_ms: 1,
            expires_at_unix_ms: 1 + floe_conversation::INTERACTION_PENDING_LIFETIME_MS,
        };
        interaction
            .validate()
            .expect("valid resolved interaction fixture");
        interaction
    }

    async fn persist_resolved_interaction_fixture(
        vault: &EncryptedAgentVault<TestKeys>,
        interaction: &floe_conversation::ConversationInteraction,
    ) {
        let mut connection = vault.connection().expect("connect to fixture Vault");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .expect("begin interaction fixture transaction");
        super::conversation_interactions::insert_interaction(&transaction, interaction)
            .await
            .expect("persist valid resolved interaction fixture");
        transaction
            .commit()
            .await
            .expect("commit interaction fixture");
    }

    async fn replace_session_fixture(
        vault: &EncryptedAgentVault<TestKeys>,
        session: &AgentSession,
    ) {
        let mut connection = vault
            .connection()
            .expect("connect to Session fixture Vault");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .expect("begin Session fixture transaction");
        let changed = transaction
            .execute(
                "UPDATE agent_sessions SET payload = ? WHERE id = ? AND revision = ?",
                (
                    serde_json::to_string(session).expect("serialize Session fixture"),
                    session.id.to_string(),
                    i64::try_from(session.revision).expect("Session revision fits SQLite"),
                ),
            )
            .await
            .expect("replace Session fixture payload");
        assert_eq!(changed, 1);
        transaction
            .commit()
            .await
            .expect("commit Session fixture transaction");
    }

    async fn replace_interaction_origin_fixture(
        vault: &EncryptedAgentVault<TestKeys>,
        interaction_id: Uuid,
        origin_run_id: RunId,
    ) {
        let mut connection = vault
            .connection()
            .expect("connect to interaction fixture Vault");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .expect("begin interaction-link fixture transaction");
        let changed = transaction
            .execute(
                "UPDATE agent_conversation_interactions SET origin_run_id = ? WHERE interaction_id = ?",
                (
                    origin_run_id.as_uuid().to_string(),
                    interaction_id.to_string(),
                ),
            )
            .await
            .expect("replace interaction origin link fixture");
        assert_eq!(changed, 1);
        transaction
            .commit()
            .await
            .expect("commit interaction-link fixture transaction");
    }

    async fn replace_interaction_session_fixture(
        vault: &EncryptedAgentVault<TestKeys>,
        interaction: &floe_conversation::ConversationInteraction,
    ) {
        interaction
            .validate()
            .expect("cross-session interaction fixture remains self-consistent");
        let mut connection = vault
            .connection()
            .expect("connect to interaction-session fixture Vault");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .expect("begin interaction-session fixture transaction");
        let changed = transaction
            .execute(
                "UPDATE agent_conversation_interactions SET session_id = ?, payload = ? WHERE interaction_id = ?",
                (
                    interaction.session_id.to_string(),
                    serde_json::to_string(interaction)
                        .expect("serialize self-consistent cross-session interaction"),
                    interaction.id.to_string(),
                ),
            )
            .await
            .expect("replace interaction session linkage fixture");
        assert_eq!(changed, 1);
        transaction
            .commit()
            .await
            .expect("commit interaction-session fixture transaction");
    }

    async fn finish_completed_run(
        vault: &EncryptedAgentVault<TestKeys>,
        record: &RunRecord,
    ) -> RunRecord {
        use floe_agent_contract::{BatchCursor, EngineStep, JournalEvent, ModelStep};

        let text = "A completed owner run.".to_owned();
        let artifacts = Vec::new();
        let attempt_id = Uuid::new_v4();
        let projection_ref = ProjectionRef::new();
        append_event(
            vault,
            record.run_id,
            model_intent(attempt_id, projection_ref, plan(record, 8)),
        )
        .await
        .expect("append a pinned model intent");
        append_event(
            vault,
            record.run_id,
            JournalEvent::ModelResult {
                attempt_id,
                usage: floe_agent_contract::ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await
        .expect("settle the model attempt");
        let batch = ValidatedModelBatch {
            execution_id: record.run_id.as_uuid(),
            attempt_id,
            projection_ref,
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Answer {
                text: text.clone(),
                artifacts: artifacts.clone(),
            }],
            catalog_revision: record.expert_environment.revision,
            tool_revisions: vec![],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
            delegation_context: None,
        };
        append_event(
            vault,
            record.run_id,
            JournalEvent::ValidatedBatch {
                batch: batch.clone(),
            },
        )
        .await
        .expect("persist the answer batch");
        append_event(
            vault,
            record.run_id,
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: batch.batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await
        .expect("persist the answer cursor");
        append_event(
            vault,
            record.run_id,
            JournalEvent::Output {
                text: text.clone(),
                artifacts: artifacts.clone(),
            },
        )
        .await
        .expect("persist the final answer");
        vault
            .finish_conversation_run(
                record.run_id,
                record.aggregate_revision,
                RunTerminal {
                    state: RunState::Completed,
                    output: Some(text.clone()),
                    steps: vec![EngineStep::Answer { text, artifacts }],
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                    blocked: None,
                    interactions: vec![],
                },
            )
            .await
            .expect("finish owner Run and enqueue any eligible resume")
    }

    async fn finish_failed_run(
        vault: &EncryptedAgentVault<TestKeys>,
        record: &RunRecord,
        failure: AgentFailure,
    ) -> RunRecord {
        vault
            .finish_conversation_run(
                record.run_id,
                record.aggregate_revision,
                RunTerminal::from_failure(failure),
            )
            .await
            .expect("finish owner Run with the requested failure")
    }

    struct OpenInteractionScenario {
        started: StartedRun,
        origin: RunRecord,
        session: AgentSession,
        interaction: floe_conversation::ConversationInteraction,
        future_resume: floe_conversation::ResumeRequired,
    }

    async fn start_open_interaction_scenario(resolving: bool) -> OpenInteractionScenario {
        let started = start_run().await;
        let resolved_sibling = resolved_binding_interaction(&started.record);
        let future_resolved = resolved_binding_interaction(&started.record);
        let mut open = future_resolved.clone();
        open.state = floe_conversation::InteractionState::Pending;
        open.revision = 1;
        persist_resolved_interaction_fixture(&started.vault, &resolved_sibling).await;
        persist_resolved_interaction_fixture(&started.vault, &open).await;

        if resolving {
            let decision = floe_conversation::InteractionDecision {
                command_id: Uuid::new_v4(),
                interaction_id: open.id,
                interaction_revision: open.revision,
                kind: floe_conversation::InteractionDecisionKind::Approve,
                target_digest: open.target_digest,
                principal: started.person_id.to_string(),
                decided_at_unix_ms: 2,
            };
            open = match started
                .vault
                .record_conversation_interaction_decision(decision)
                .await
                .expect("admit owner decision for interaction")
            {
                floe_conversation::DecisionAdmission::Applied(updated) => updated,
                other => panic!("unexpected interaction decision result: {other:?}"),
            };
            assert!(matches!(
                open.state,
                floe_conversation::InteractionState::Resolving { .. }
            ));
        }

        let origin = finish_completed_run(&started.vault, &started.record).await;
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            origin.session_id,
        )
        .await
        .expect("load Session for resumable open interaction");
        let actor = floe_kernel::OwnerActor {
            person_id: started.person_id,
            device_id: origin.device_id.clone(),
            runtime_epoch: 1,
        };
        let group = started
            .vault
            .run_conversation_interactions(origin.run_id)
            .await
            .expect("read interaction group through owner API");
        assert_eq!(group.len(), 2);
        let persisted_open = group
            .iter()
            .find(|interaction| interaction.id == open.id)
            .expect("open interaction is retained");
        assert_eq!(persisted_open, &open);
        assert_eq!(
            started
                .vault
                .pending_conversation_resume_request(&actor, origin.run_id)
                .await
                .expect("read absent resume while group is open"),
            None
        );

        let mut resolved_after_settlement = open.clone();
        resolved_after_settlement.state = future_resolved.state;
        resolved_after_settlement.revision = open.revision + 1;
        if let floe_conversation::InteractionState::Resolved { receipt } =
            &mut resolved_after_settlement.state
        {
            receipt.resolved_at_unix_ms = 3;
        }
        resolved_after_settlement
            .validate()
            .expect("interaction remains structurally valid after resolution");
        let future_group = group
            .iter()
            .map(|interaction| {
                if interaction.id == open.id {
                    resolved_after_settlement.clone()
                } else {
                    interaction.clone()
                }
            })
            .collect::<Vec<_>>();
        let future_resume = floe_conversation::build_resume_required(
            &floe_conversation::project_run_receipt(origin.clone())
                .expect("project completed resume origin"),
            &future_group,
        )
        .expect("validate settled interaction group")
        .expect("a resolved terminal group can enqueue a linked child");
        assert_eq!(future_resume.expected_session_revision, session.revision);

        OpenInteractionScenario {
            started,
            origin,
            session,
            interaction: open,
            future_resume,
        }
    }

    async fn read_continuation_snapshot(
        vault: &Arc<EncryptedAgentVault<TestKeys>>,
        person_id: PersonId,
        device_id: &str,
        session_id: Uuid,
    ) -> Result<floe_conversation::SessionSnapshot, AgentFailure> {
        use std::time::Duration;

        let repository = crate::VaultConversationRepository::new(vault.clone());
        let scope = floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(30),
            floe_execution::budget::BudgetLedger::new(
                floe_execution::budget::BudgetConfig::new(1_000, 1_000),
                floe_execution::budget::ModelUsage::default(),
            )
            .work_lease(),
            floe_kernel::TraceContext::new(Uuid::new_v4()),
        );
        let actor = floe_kernel::OwnerActor {
            person_id,
            device_id: device_id.to_owned(),
            runtime_epoch: 1,
        };
        floe_conversation::read_session_snapshot(
            &repository,
            vault.as_ref(),
            &actor,
            session_id,
            None,
            &scope,
        )
        .await
    }

    #[tokio::test]
    async fn compaction_refuses_a_live_issued_continuation() {
        let started = start_run().await;
        let failed = finish_failed_run(
            &started.vault,
            &started.record,
            AgentFailure::DeadlineExceeded,
        )
        .await;
        assert_eq!(failed.state, RunState::TimedOut);
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            failed.session_id,
        )
        .await
        .expect("load Session carrying the live continuation marker");
        assert_eq!(
            session.continuation.as_ref().map(|value| value.turn_id),
            Some(failed.run_id.as_uuid())
        );
        let before = read_continuation_snapshot(
            &started.vault,
            started.person_id,
            &failed.device_id,
            session.id,
        )
        .await
        .expect("read the owner-issued continuation before compaction");
        assert!(before.continuation_ref.is_some());

        let compacted = started
            .vault
            .compact_session(
                session.id,
                session.revision,
                failed.run_id.as_uuid(),
                "Timed out turn summary".into(),
            )
            .await;
        match compacted {
            Err(AgentFailure::Conflict) => {
                assert_eq!(
                    floe_conversation::SessionStore::load(
                        started.vault.as_ref(),
                        started.person_id,
                        session.id,
                    )
                    .await
                    .expect("reload Session after refused compaction"),
                    session
                );
                assert_eq!(
                    read_continuation_snapshot(
                        &started.vault,
                        started.person_id,
                        &failed.device_id,
                        session.id,
                    )
                    .await
                    .expect("continuation remains issuable after refused compaction"),
                    before
                );
            }
            other => {
                let after = read_continuation_snapshot(
                    &started.vault,
                    started.person_id,
                    &failed.device_id,
                    session.id,
                )
                .await;
                panic!(
                    "compaction must protect the live continuation; got {other:?}, then owner snapshot {after:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn new_input_clears_continue_capability_before_compaction() {
        let started = start_run().await;
        let failed = finish_failed_run(
            &started.vault,
            &started.record,
            AgentFailure::DeadlineExceeded,
        )
        .await;
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            failed.session_id,
        )
        .await
        .expect("load Session with issued continuation");
        let before = read_continuation_snapshot(
            &started.vault,
            started.person_id,
            &failed.device_id,
            session.id,
        )
        .await
        .expect("read current Continue capability");
        assert!(before.continuation_ref.is_some());

        let new_run = admit_new_run(&started, session.revision, "A fresh explicit turn").await;
        let admitted_session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            session.id,
        )
        .await
        .expect("load Session after New admission");
        assert!(admitted_session.continuation.is_none());
        let new_run = finish_completed_run(&started.vault, &new_run).await;
        let current_session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            session.id,
        )
        .await
        .expect("load Session after New turn completes");
        assert!(current_session.continuation.is_none());
        assert!(
            read_continuation_snapshot(
                &started.vault,
                started.person_id,
                &failed.device_id,
                current_session.id,
            )
            .await
            .expect("read Session after New clears Continue")
            .continuation_ref
            .is_none()
        );

        let compaction = started
            .vault
            .compact_session(
                current_session.id,
                current_session.revision,
                failed.run_id.as_uuid(),
                "Superseded failure turn summary".into(),
            )
            .await
            .expect("compact after New cleared the stale Continue capability");
        assert_eq!(compaction.session.revision, current_session.revision + 1);
        assert!(
            compaction
                .session
                .messages
                .iter()
                .any(|message| message.turn_id() == new_run.run_id.as_uuid()),
            "the newly admitted turn remains in the retained suffix"
        );
    }

    #[tokio::test]
    async fn compaction_refuses_a_current_pending_interaction_with_resume_eligibility() {
        assert_current_open_interaction_is_protected(false).await;
    }

    #[tokio::test]
    async fn compaction_refuses_a_current_resolving_interaction_with_resume_eligibility() {
        assert_current_open_interaction_is_protected(true).await;
    }

    async fn assert_current_open_interaction_is_protected(resolving: bool) {
        let scenario = start_open_interaction_scenario(resolving).await;
        assert_eq!(
            matches!(
                scenario.interaction.state,
                floe_conversation::InteractionState::Resolving { .. }
            ),
            resolving
        );
        let compacted = scenario
            .started
            .vault
            .compact_session(
                scenario.session.id,
                scenario.session.revision,
                scenario.origin.run_id.as_uuid(),
                "Completed turn summary".into(),
            )
            .await;
        match compacted {
            Err(AgentFailure::Conflict) => {
                assert_eq!(
                    floe_conversation::SessionStore::load(
                        scenario.started.vault.as_ref(),
                        scenario.started.person_id,
                        scenario.session.id,
                    )
                    .await
                    .expect("reload Session after refused compaction"),
                    scenario.session
                );
                assert_eq!(
                    scenario
                        .started
                        .vault
                        .conversation_interaction(scenario.interaction.id)
                        .await
                        .expect("read unchanged open interaction")
                        .expect("interaction remains stored"),
                    scenario.interaction
                );
                assert_eq!(
                    count_session_archives(&scenario.started.vault, scenario.session.id).await,
                    0
                );
            }
            other => panic!(
                "compaction must preserve this current resume opportunity (expected revision {}, got {other:?})",
                scenario.future_resume.expected_session_revision
            ),
        }
    }

    #[tokio::test]
    async fn compaction_after_new_input_allows_stale_open_interaction_history() {
        let scenario = start_open_interaction_scenario(false).await;
        let new_run = admit_new_run(
            &scenario.started,
            scenario.session.revision,
            "Supersede old interaction opportunity",
        )
        .await;
        let new_run = finish_completed_run(&scenario.started.vault, &new_run).await;
        let current_session = floe_conversation::SessionStore::load(
            scenario.started.vault.as_ref(),
            scenario.started.person_id,
            scenario.session.id,
        )
        .await
        .expect("load Session after New supersedes the old opportunity");
        assert!(current_session.revision > scenario.session.revision);
        assert_eq!(
            scenario
                .started
                .vault
                .conversation_interaction(scenario.interaction.id)
                .await
                .expect("read historical interaction after New")
                .expect("New preserves interaction history"),
            scenario.interaction,
            "New does not rewrite the old Pending interaction"
        );

        let compaction = scenario
            .started
            .vault
            .compact_session(
                current_session.id,
                current_session.revision,
                scenario.origin.run_id.as_uuid(),
                "Earlier completed turn summary".into(),
            )
            .await
            .expect("compact when only a stale interaction opportunity remains");
        assert_eq!(compaction.session.revision, current_session.revision + 1);
        assert!(
            compaction
                .session
                .messages
                .iter()
                .any(|message| message.turn_id() == new_run.run_id.as_uuid()),
            "the new turn remains in the retained Session suffix"
        );
        assert_eq!(
            count_session_archives(&scenario.started.vault, current_session.id).await,
            1
        );
        assert_eq!(
            scenario
                .started
                .vault
                .conversation_interaction(scenario.interaction.id)
                .await
                .expect("read retained historical interaction")
                .expect("historical interaction remains stored"),
            scenario.interaction
        );
    }

    #[tokio::test]
    async fn compaction_fails_closed_for_self_consistent_wrong_session_group_member() {
        let scenario = start_open_interaction_scenario(false).await;
        let group = scenario
            .started
            .vault
            .run_conversation_interactions(scenario.origin.run_id)
            .await
            .expect("read interaction group before cross-record corruption");
        let resolved_sibling = group
            .into_iter()
            .find(|interaction| interaction.id != scenario.interaction.id)
            .expect("group has a resolved sibling for the current resume opportunity");

        let other_session = match scenario
            .started
            .vault
            .start_conversation_session(StartSessionRequest {
                principal: scenario.started.person_id.to_string(),
                command_id: CommandId::new(),
            })
            .await
            .expect("start a distinct owner Session")
        {
            floe_conversation::SessionStartAdmission::Started(receipt) => receipt.session_id,
            other => panic!("unexpected second Session admission: {other:?}"),
        };
        let new_run = admit_new_run(
            &scenario.started,
            scenario.session.revision,
            "Make the original interaction group stale",
        )
        .await;
        let _new_run = finish_completed_run(&scenario.started.vault, &new_run).await;
        let current_session = floe_conversation::SessionStore::load(
            scenario.started.vault.as_ref(),
            scenario.started.person_id,
            scenario.session.id,
        )
        .await
        .expect("load current Session after New");

        let mut wrong_session_member = resolved_sibling;
        wrong_session_member.session_id = other_session;
        wrong_session_member.audit.session_id = other_session;
        replace_interaction_session_fixture(&scenario.started.vault, &wrong_session_member).await;
        assert_eq!(
            scenario
                .started
                .vault
                .conversation_interaction(wrong_session_member.id)
                .await
                .expect("read self-consistent interaction fixture through owner API"),
            Some(wrong_session_member),
            "the row and its payload agree, but the loaded origin Run belongs to another Session"
        );

        assert_eq!(
            scenario
                .started
                .vault
                .compact_session(
                    current_session.id,
                    current_session.revision,
                    scenario.origin.run_id.as_uuid(),
                    "Must not compact a stale group with a wrong-Session member".into(),
                )
                .await,
            Err(AgentFailure::VaultUnavailable),
            "every group member must link to the requested Session and loaded origin Run before stale history is skipped"
        );
        assert_eq!(
            floe_conversation::SessionStore::load(
                scenario.started.vault.as_ref(),
                scenario.started.person_id,
                current_session.id,
            )
            .await
            .expect("reload Session after cross-record refusal"),
            current_session
        );
        assert_eq!(
            count_session_archives(&scenario.started.vault, current_session.id).await,
            0
        );
    }

    #[tokio::test]
    async fn compaction_after_exhausted_continue_chain_succeeds() {
        let started = start_run().await;
        let mut parent = started.record.clone();
        let mut expected_level = 0;
        let exhausted = loop {
            let failed =
                finish_failed_run(&started.vault, &parent, AgentFailure::BudgetExceeded).await;
            assert_eq!(failed.continuation_level, expected_level);
            let receipt = floe_conversation::project_run_receipt(failed.clone())
                .expect("project failed lineage receipt");
            if expected_level == 3 {
                assert!(receipt.continuation().is_none());
                break failed;
            }
            assert!(receipt.continuation().is_some());
            parent = admit_continue_child(
                &started.vault,
                &failed,
                "Continue the bounded failure lineage",
            )
            .await;
            expected_level += 1;
        };

        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            exhausted.session_id,
        )
        .await
        .expect("load Session after continuation exhaustion");
        assert!(session.continuation.is_none());
        let compaction = started
            .vault
            .compact_session(
                session.id,
                session.revision,
                started.record.run_id.as_uuid(),
                "Exhausted failure lineage summary".into(),
            )
            .await
            .expect("compact after Continue lineage is exhausted");
        assert_eq!(compaction.session.revision, session.revision + 1);
        assert_eq!(count_session_archives(&started.vault, session.id).await, 1);
    }

    #[tokio::test]
    async fn compaction_after_explicit_expiry_preserves_terminal_interaction_history() {
        let started = start_run().await;
        let mut interaction = resolved_binding_interaction(&started.record);
        interaction.state = floe_conversation::InteractionState::Pending;
        interaction.revision = 1;
        interaction
            .validate()
            .expect("valid pending interaction fixture");
        persist_resolved_interaction_fixture(&started.vault, &interaction).await;

        let expired = started
            .vault
            .expire_conversation_interaction(floe_conversation::ExpireInteraction {
                interaction_id: interaction.id,
                person_id: started.person_id,
                now_unix_ms: interaction.expires_at_unix_ms,
            })
            .await
            .expect("expire interaction through owner API at its explicit deadline");
        let floe_conversation::ExpireOutcome::Expired(expired) = expired else {
            panic!("owner expiry did not transition the due interaction: {expired:?}");
        };
        assert_eq!(expired.state, floe_conversation::InteractionState::Expired);

        let origin = finish_completed_run(&started.vault, &started.record).await;
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            origin.session_id,
        )
        .await
        .expect("load Session with terminal interaction group");
        let actor = floe_kernel::OwnerActor {
            person_id: started.person_id,
            device_id: origin.device_id.clone(),
            runtime_epoch: 1,
        };
        assert_eq!(
            started
                .vault
                .pending_conversation_resume_request(&actor, origin.run_id)
                .await
                .expect("read absent resume for group with no resolved member"),
            None
        );
        let compaction = started
            .vault
            .compact_session(
                session.id,
                session.revision,
                origin.run_id.as_uuid(),
                "Expired interaction history summary".into(),
            )
            .await
            .expect("compact after explicit expiry closed the group");
        assert_eq!(compaction.session.revision, session.revision + 1);
        assert_eq!(
            started
                .vault
                .conversation_interaction(interaction.id)
                .await
                .expect("read expired historical interaction")
                .expect("expired interaction remains stored"),
            expired
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 1);
    }

    #[tokio::test]
    async fn compaction_fails_closed_when_continuation_run_is_missing() {
        let started = start_run().await;
        let failed = finish_failed_run(
            &started.vault,
            &started.record,
            AgentFailure::DeadlineExceeded,
        )
        .await;
        let mut session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            failed.session_id,
        )
        .await
        .expect("load Session with current continuation marker");
        let missing_run_id = RunId::new();
        session
            .continuation
            .as_mut()
            .expect("failed Run has a continuation marker")
            .turn_id = missing_run_id.as_uuid();
        replace_session_fixture(&started.vault, &session).await;

        assert_eq!(
            started
                .vault
                .compact_session(
                    session.id,
                    session.revision,
                    failed.run_id.as_uuid(),
                    "Must not launder a missing continuation origin".into(),
                )
                .await,
            Err(AgentFailure::StorageUnavailable),
            "missing referenced Run evidence follows the owner snapshot failure"
        );
        assert_eq!(
            floe_conversation::SessionStore::load(
                started.vault.as_ref(),
                started.person_id,
                session.id,
            )
            .await
            .expect("reload corrupt but structurally valid Session"),
            session
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 0);
    }

    #[tokio::test]
    async fn compaction_fails_closed_for_ineligible_continuation_link() {
        let started = start_run().await;
        let completed = finish_completed_run(&started.vault, &started.record).await;
        let mut session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            completed.session_id,
        )
        .await
        .expect("load completed Session without continuation");
        assert!(session.continuation.is_none());
        session.continuation = Some(floe_conversation::AgentContinuation {
            turn_id: completed.run_id.as_uuid(),
            level: completed.continuation_level,
            usage: session.usage,
        });
        replace_session_fixture(&started.vault, &session).await;

        assert_eq!(
            started
                .vault
                .compact_session(
                    session.id,
                    session.revision,
                    completed.run_id.as_uuid(),
                    "Must not launder an ineligible continuation link".into(),
                )
                .await,
            Err(AgentFailure::StorageUnavailable),
            "a marker linked to a non-continuable Run is corrupt owner evidence"
        );
        assert_eq!(
            floe_conversation::SessionStore::load(
                started.vault.as_ref(),
                started.person_id,
                session.id,
            )
            .await
            .expect("reload structurally valid Session after refusal"),
            session
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 0);
    }

    #[tokio::test]
    async fn compaction_fails_closed_when_interaction_origin_run_is_missing() {
        let scenario = start_open_interaction_scenario(false).await;
        let missing_run_id = RunId::new();
        replace_interaction_origin_fixture(
            &scenario.started.vault,
            scenario.interaction.id,
            missing_run_id,
        )
        .await;

        assert_eq!(
            scenario
                .started
                .vault
                .compact_session(
                    scenario.session.id,
                    scenario.session.revision,
                    scenario.origin.run_id.as_uuid(),
                    "Must not compact past a missing interaction origin".into(),
                )
                .await,
            Err(AgentFailure::VaultUnavailable),
            "missing interaction-origin evidence fails closed"
        );
        assert_eq!(
            floe_conversation::SessionStore::load(
                scenario.started.vault.as_ref(),
                scenario.started.person_id,
                scenario.session.id,
            )
            .await
            .expect("reload Session after missing-link refusal"),
            scenario.session
        );
        assert_eq!(
            count_session_archives(&scenario.started.vault, scenario.session.id).await,
            0
        );
    }

    #[tokio::test]
    async fn compaction_fails_closed_when_interaction_origin_link_disagrees_with_payload() {
        let scenario = start_open_interaction_scenario(false).await;
        let new_run = admit_new_run(
            &scenario.started,
            scenario.session.revision,
            "Create a distinct owner run for link validation",
        )
        .await;
        let new_run = finish_completed_run(&scenario.started.vault, &new_run).await;
        let current_session = floe_conversation::SessionStore::load(
            scenario.started.vault.as_ref(),
            scenario.started.person_id,
            scenario.session.id,
        )
        .await
        .expect("load current Session before corrupt-link refusal");
        replace_interaction_origin_fixture(
            &scenario.started.vault,
            scenario.interaction.id,
            new_run.run_id,
        )
        .await;

        assert_eq!(
            scenario
                .started
                .vault
                .compact_session(
                    current_session.id,
                    current_session.revision,
                    scenario.origin.run_id.as_uuid(),
                    "Must not compact with conflicting interaction linkage".into(),
                )
                .await,
            Err(AgentFailure::VaultUnavailable),
            "the indexed run link must match the validated interaction payload"
        );
        assert_eq!(
            floe_conversation::SessionStore::load(
                scenario.started.vault.as_ref(),
                scenario.started.person_id,
                current_session.id,
            )
            .await
            .expect("reload Session after conflicting-link refusal"),
            current_session
        );
        assert_eq!(
            count_session_archives(&scenario.started.vault, current_session.id).await,
            0
        );
    }

    async fn admit_new_run(started: &StartedRun, expected_revision: u64, text: &str) -> RunRecord {
        let command_id = CommandId::new();
        let run_id = RunId::new();
        let principal = started.person_id.to_string();
        let intent = CanonicalTurnIntent {
            session_id: started.record.session_id,
            expected_revision,
            text: text.to_owned(),
            mode: TurnMode::New,
            retry_of: None,
        };
        let admission = started
            .vault
            .admit_conversation_turn(TurnAdmissionRequest {
                expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [1; 32],
                },
                run_id,
                command_id,
                session_id: started.record.session_id,
                expected_session_revision: expected_revision,
                principal: principal.clone(),
                device_id: started.record.device_id.clone(),
                request_digest: intent.digest(&principal).expect("canonical turn digest"),
                mode: TurnMode::New,
                retry_of: None,
                input: TurnInput::NewMessage(floe_agent_contract::AgentMessage {
                    message_id: command_id.as_uuid(),
                    role: floe_agent_contract::MessageRole::User,
                    text: text.to_owned(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                }),
            })
            .await
            .expect("admit new owner Run");
        let VaultConversationAdmission::Created { record, .. } = admission else {
            panic!("new input did not create a Run: {admission:?}");
        };
        record
    }

    async fn count_session_archives(
        vault: &EncryptedAgentVault<TestKeys>,
        session_id: Uuid,
    ) -> u64 {
        let connection = vault.connection().expect("connect to test Vault");
        let mut rows = connection
            .query(
                "SELECT count(*) FROM agent_session_archives WHERE session_id = ?",
                [session_id.to_string()],
            )
            .await
            .expect("count session archives");
        let row = rows
            .next()
            .await
            .expect("read archive count")
            .expect("count row");
        u64::try_from(row.get::<i64>(0).expect("archive count value"))
            .expect("nonnegative archive count")
    }

    #[tokio::test]
    async fn pending_resume_refuses_compaction_without_changing_any_state() {
        let mut started = start_run().await;
        let first_run = finish_completed_run(&started.vault, &started.record).await;
        started.record = first_run.clone();
        let first_session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            first_run.session_id,
        )
        .await
        .expect("load first completed Session");
        let origin = admit_new_run(&started, first_session.revision, "Resume this turn").await;
        let interaction = resolved_binding_interaction(&origin);
        persist_resolved_interaction_fixture(&started.vault, &interaction).await;
        let origin = finish_completed_run(&started.vault, &origin).await;

        let actor = floe_kernel::OwnerActor {
            person_id: started.person_id,
            device_id: origin.device_id.clone(),
            runtime_epoch: 1,
        };
        let pending = started
            .vault
            .pending_conversation_resume_request(&actor, origin.run_id)
            .await
            .expect("read durable resume request")
            .expect("completed Run with resolved interaction queues a resume");
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            started.record.session_id,
        )
        .await
        .expect("load completed Session");
        assert_eq!(pending.expected_session_revision, session.revision);
        assert_ne!(first_run.run_id.as_uuid(), origin.run_id.as_uuid());
        let prefix_end = session
            .messages
            .iter()
            .rposition(|message| message.turn_id() == first_run.run_id.as_uuid())
            .expect("first completed turn defines the archive prefix");
        let origin_position = session
            .messages
            .iter()
            .position(|message| message.turn_id() == origin.run_id.as_uuid())
            .expect("resume origin remains in the Session");
        assert!(
            origin_position > prefix_end,
            "resume origin is outside the selected archive prefix"
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 0);

        assert_eq!(
            started
                .vault
                .compact_session(
                    session.id,
                    session.revision,
                    first_run.run_id.as_uuid(),
                    "Completed turn summary".into(),
                )
                .await,
            Err(AgentFailure::Conflict),
            "the origin turn being retained does not make a revision bump safe"
        );
        assert_eq!(
            floe_conversation::SessionStore::load(
                started.vault.as_ref(),
                started.person_id,
                session.id,
            )
            .await
            .expect("reload Session after refusal"),
            session,
            "refused compaction preserves the complete Session"
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 0);
        assert_eq!(
            started
                .vault
                .pending_conversation_resume_request(&actor, origin.run_id)
                .await
                .expect("read preserved resume request"),
            Some(pending.clone())
        );
        assert_eq!(
            started
                .vault
                .reconcile_conversation_resume_request(&actor, origin.run_id)
                .await
                .expect("reconcile stored resume"),
            Some(pending.clone()),
            "the exact expected revision remains reconcilable"
        );
        assert_eq!(
            started
                .vault
                .pending_conversation_resume_request(&actor, origin.run_id)
                .await
                .expect("read pending resume after reconciliation"),
            Some(pending)
        );
    }

    #[tokio::test]
    async fn compaction_without_a_pending_resume_preserves_coverage_and_archive() {
        let started = start_run().await;
        let completed = finish_completed_run(&started.vault, &started.record).await;
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            completed.session_id,
        )
        .await
        .expect("load completed Session");
        let actor = floe_kernel::OwnerActor {
            person_id: started.person_id,
            device_id: completed.device_id.clone(),
            runtime_epoch: 1,
        };
        assert_eq!(
            started
                .vault
                .pending_conversation_resume_request(&actor, completed.run_id)
                .await
                .expect("read absent resume request"),
            None
        );

        let compaction = started
            .vault
            .compact_session(
                session.id,
                session.revision,
                completed.run_id.as_uuid(),
                "Completed turn summary".into(),
            )
            .await
            .expect("compact quiescent Session");
        assert_eq!(compaction.session.revision, session.revision + 1);
        assert_eq!(
            compaction.summary_coverage,
            DependencyCoverage::Independent,
            "the archived turn's coverage is merged into its summary"
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 1);
        let recovered = started
            .vault
            .recover_session_with_coverage(&compaction.recovery)
            .await
            .expect("recover archived source snapshot");
        assert_eq!(recovered.session, session);
        assert_eq!(
            recovered.coverage_by_turn.get(&completed.run_id.as_uuid()),
            Some(&DependencyCoverage::Independent)
        );
    }

    #[tokio::test]
    async fn new_input_supersedes_pending_resume_before_compaction() {
        let started = start_run().await;
        let interaction = resolved_binding_interaction(&started.record);
        persist_resolved_interaction_fixture(&started.vault, &interaction).await;
        let origin = finish_completed_run(&started.vault, &started.record).await;
        let actor = floe_kernel::OwnerActor {
            person_id: started.person_id,
            device_id: origin.device_id.clone(),
            runtime_epoch: 1,
        };
        let pending = started
            .vault
            .pending_conversation_resume_request(&actor, origin.run_id)
            .await
            .expect("read pending resume")
            .expect("resolved interaction queues resume");
        let next = admit_new_run(
            &started,
            pending.expected_session_revision,
            "A new explicit turn",
        )
        .await;
        assert_eq!(
            started
                .vault
                .pending_conversation_resume_request(&actor, origin.run_id)
                .await
                .expect("read request after New input"),
            None,
            "New input legitimately supersedes the old resume slot"
        );

        let next = finish_completed_run(&started.vault, &next).await;
        let session = floe_conversation::SessionStore::load(
            started.vault.as_ref(),
            started.person_id,
            next.session_id,
        )
        .await
        .expect("load Session after new turn");
        let compaction = started
            .vault
            .compact_session(
                session.id,
                session.revision,
                origin.run_id.as_uuid(),
                "Earlier turn summary".into(),
            )
            .await
            .expect("compact after the request was superseded");
        assert_eq!(compaction.session.revision, session.revision + 1);
        assert!(
            compaction
                .session
                .messages
                .iter()
                .any(|message| message.turn_id() == next.run_id.as_uuid()),
            "newer turn remains in the retained suffix"
        );
        assert_eq!(count_session_archives(&started.vault, session.id).await, 1);
    }

    fn batch(
        record: &RunRecord,
        attempt_id: Uuid,
        projection_ref: ProjectionRef,
    ) -> ValidatedModelBatch {
        ValidatedModelBatch {
            execution_id: record.run_id.as_uuid(),
            attempt_id,
            projection_ref,
            batch_id: Uuid::new_v4(),
            steps: vec![
                ModelStep::CallTool {
                    tool_id: "floe.source.calendar".into(),
                    definition_revision: 1,
                    input: "{}".into(),
                },
                ModelStep::Answer {
                    text: "A persisted answer.".into(),
                    artifacts: vec![],
                },
            ],
            catalog_revision: record.expert_environment.revision,
            tool_revisions: vec![floe_agent_contract::PinnedToolRevision {
                tool_id: "floe.source.calendar".into(),
                definition_revision: 1,
            }],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
            delegation_context: None,
        }
    }

    fn stable_tool_call(
        batch: &ValidatedModelBatch,
        ordinal: u32,
    ) -> floe_agent_contract::ToolCall {
        let call_id = Uuid::new_v5(
            &batch.execution_id,
            format!("{}:{}:{ordinal}:call", batch.execution_id, batch.batch_id).as_bytes(),
        );
        let invocation_key = Uuid::new_v5(
            &batch.execution_id,
            format!("{}:{}:{ordinal}:tool", batch.execution_id, batch.batch_id).as_bytes(),
        );
        floe_agent_contract::ToolCall {
            call_id,
            invocation_key: InvocationKey::from_uuid(invocation_key)
                .expect("stable invocation key is non-nil"),
            tool_id: "floe.source.calendar".into(),
            definition_revision: 1,
            input: "{}".into(),
        }
    }

    async fn start_parent_with_pending_batch()
    -> (StartedRun, ValidatedModelBatch, PreparedModelPlan) {
        let mut started = start_run().await;
        let parent = started.record.clone();
        let attempt_id = Uuid::new_v4();
        let projection_ref = ProjectionRef::new();
        let parent_plan = plan(&parent, 7);
        append_event(
            &started.vault,
            parent.run_id,
            model_intent(attempt_id, projection_ref, parent_plan.clone()),
        )
        .await
        .expect("append the parent's pinned model intent");
        append_event(
            &started.vault,
            parent.run_id,
            JournalEvent::ModelResult {
                attempt_id,
                usage: floe_agent_contract::ModelUsage::default(),
                accounting: floe_execution::budget::ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        )
        .await
        .expect("settle the parent's model attempt");

        let pending_batch = batch(&parent, attempt_id, projection_ref);
        append_event(
            &started.vault,
            parent.run_id,
            JournalEvent::ValidatedBatch {
                batch: pending_batch.clone(),
            },
        )
        .await
        .expect("persist the parent batch");
        append_event(
            &started.vault,
            parent.run_id,
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: pending_batch.batch_id,
                    next_step_index: 0,
                },
            },
        )
        .await
        .expect("persist the parent's pending cursor");
        let settled_call = stable_tool_call(&pending_batch, 0);
        append_event(
            &started.vault,
            parent.run_id,
            JournalEvent::ToolIntent {
                call: settled_call.clone(),
            },
        )
        .await
        .expect("persist the parent tool intent");
        append_event(
            &started.vault,
            parent.run_id,
            JournalEvent::ToolResult {
                result: floe_agent_contract::ToolResult {
                    call_id: settled_call.call_id,
                    text: "Persisted calendar observation.".into(),
                    artifacts: vec![],
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                },
            },
        )
        .await
        .expect("settle the parent tool result");
        let parent_cursor = BatchCursor {
            batch_id: pending_batch.batch_id,
            next_step_index: 1,
        };
        append_event(
            &started.vault,
            parent.run_id,
            JournalEvent::BatchProgress {
                cursor: parent_cursor,
            },
        )
        .await
        .expect("advance beyond the settled tool result");
        started.record = started
            .vault
            .finish_conversation_run(
                parent.run_id,
                parent.aggregate_revision,
                RunTerminal::from_failure(AgentFailure::BudgetExceeded),
            )
            .await
            .expect("fail parent with its pending batch recoverable");
        (started, pending_batch, parent_plan)
    }

    async fn admit_continue_child(
        vault: &EncryptedAgentVault<TestKeys>,
        parent: &RunRecord,
        text: &str,
    ) -> RunRecord {
        let continuation = floe_conversation::project_run_receipt(parent.clone())
            .expect("project parent receipt")
            .continuation()
            .expect("failed parent yields a continuation");
        let mode = TurnMode::Continue(continuation);
        let principal = parent.person_id.to_string();
        let intent = CanonicalTurnIntent {
            session_id: parent.session_id,
            expected_revision: parent.session_revision,
            text: text.to_owned(),
            mode: mode.clone(),
            retry_of: None,
        };
        let admission = vault
            .admit_conversation_turn(TurnAdmissionRequest {
                expert_environment: parent.expert_environment,
                run_id: RunId::new(),
                command_id: CommandId::new(),
                session_id: parent.session_id,
                expected_session_revision: parent.session_revision,
                principal: principal.clone(),
                device_id: parent.device_id.clone(),
                request_digest: intent.digest(&principal).expect("continuation digest"),
                mode,
                retry_of: None,
                input: TurnInput::ExistingMessage {
                    message_id: parent.user_message_id,
                },
            })
            .await
            .expect("admit exact Continue child");
        match admission {
            VaultConversationAdmission::Created { record, .. } => record,
            other => panic!("Continue child was not newly admitted: {other:?}"),
        }
    }

    async fn replay_pending_batch(
        vault: &EncryptedAgentVault<TestKeys>,
        child: &RunRecord,
        pending_batch: &ValidatedModelBatch,
        parent_cursor: &BatchCursor,
    ) {
        append_event(
            vault,
            child.run_id,
            JournalEvent::ValidatedBatch {
                batch: pending_batch.clone(),
            },
        )
        .await
        .expect("re-record the inherited batch");
        for next_step_index in parent_cursor.next_step_index..=pending_batch.steps.len() as u32 {
            append_event(
                vault,
                child.run_id,
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: pending_batch.batch_id,
                        next_step_index,
                    },
                },
            )
            .await
            .expect("replay the stored batch through its acknowledged cursor");
        }
        append_event(
            vault,
            child.run_id,
            JournalEvent::Checkpoint { iteration: 1 },
        )
        .await
        .expect("settle replayed batch iteration");
    }

    #[tokio::test]
    async fn run_owner_requires_a_complete_pin_and_recovers_a_committed_lost_ack() {
        let StartedRun {
            root,
            keys,
            person_id,
            vault,
            record,
        } = start_run().await;
        let attempt_id = Uuid::new_v4();
        let projection_ref = ProjectionRef::new();
        let mut incomplete = plan(&record, 1);
        incomplete.selection_commitment = None;
        incomplete.budget_profile = None;
        assert_eq!(
            append_event(
                &vault,
                record.run_id,
                model_intent(attempt_id, projection_ref, incomplete),
            )
            .await,
            Err(AgentFailure::PolicyDenied),
            "a newly appended plan must include complete model and budget evidence"
        );

        let accepted = plan(&record, 1);
        MODEL_SELECTION_ACK_LOSS_VAULTS
            .get_or_init(Default::default)
            .lock()
            .expect("fault-injection mutex")
            .insert(vault.vault_id);
        assert_eq!(
            append_event(
                &vault,
                record.run_id,
                model_intent(attempt_id, projection_ref, accepted.clone()),
            )
            .await,
            Err(AgentFailure::StorageUnavailable),
            "the injected failure models a lost acknowledgement after commit"
        );
        assert_eq!(
            append_event(
                &vault,
                record.run_id,
                model_intent(Uuid::new_v4(), ProjectionRef::new(), plan(&record, 2)),
            )
            .await,
            Err(AgentFailure::PolicyDenied),
            "the still-open owner retains the committed pin after its ACK was lost"
        );

        drop(vault);
        let reopened = EncryptedAgentVault::open(&root.0, person_id, keys)
            .await
            .expect("reopen after uncertain append");
        let persisted = reopened
            .conversation_journal(record.run_id)
            .await
            .expect("read back committed journal");
        assert_eq!(persisted.len(), 1);
        let recovered: JournalEvent =
            serde_json::from_str(&persisted[0].payload).expect("decode acknowledged intent");
        assert!(matches!(
            recovered,
            JournalEvent::ModelIntent { ref plan, .. } if plan == &accepted
        ));
        let recovered_record = reopened
            .conversation_run(record.run_id)
            .await
            .expect("read run after reopen")
            .expect("run receipt persisted");
        assert_eq!(recovered_record.journal_revision, 1);
    }

    #[tokio::test]
    async fn run_owner_restores_a_parent_pin_through_empty_and_batch_only_runs() {
        for middle_has_batch_only_record in [false, true] {
            let (started, pending_batch, parent_plan) = start_parent_with_pending_batch().await;
            let vault = started.vault.as_ref();
            let parent = &started.record;
            let parent_receipt = floe_conversation::project_run_receipt(parent.clone())
                .expect("project parent receipt");
            let parent_connection = vault.connection().expect("read parent journal");
            let parent_entries = vault
                .conversation_journal_on(&parent_connection, parent)
                .await
                .expect("projectable parent journal");
            let parent_session =
                floe_conversation::SessionStore::load(vault, started.person_id, parent.session_id)
                    .await
                    .expect("load the admitted turn transcript");
            let continuation_snapshot = floe_conversation::project_continuation(
                &floe_conversation::AdmittedTurn {
                    receipt: parent_receipt.clone(),
                    transcript: floe_conversation::project_transcript(&parent_session.messages)
                        .expect("project the admitted transcript"),
                },
                &parent_entries,
            )
            .expect("project Continue with its stored tool result");
            assert_eq!(
                continuation_snapshot.model_selection,
                ModelSelectionState::Pinned(
                    parent_plan
                        .model_execution_selection()
                        .expect("valid parent plan")
                        .expect("complete parent selection")
                )
            );
            assert_eq!(
                continuation_snapshot
                    .batch_cursor
                    .as_ref()
                    .map(|cursor| cursor.next_step_index),
                Some(1)
            );
            assert_eq!(continuation_snapshot.replay.len(), 1);
            assert_eq!(
                continuation_snapshot.replay[0].result,
                "Persisted calendar observation."
            );

            let middle = admit_continue_child(
                vault,
                parent,
                if middle_has_batch_only_record {
                    "Continue through a batch-only run."
                } else {
                    "Continue through an empty run."
                },
            )
            .await;
            if middle_has_batch_only_record {
                append_event(
                    vault,
                    middle.run_id,
                    JournalEvent::ValidatedBatch {
                        batch: pending_batch.clone(),
                    },
                )
                .await
                .expect("middle run re-records the inherited batch without claiming it");
            }
            let middle = vault
                .finish_conversation_run(
                    middle.run_id,
                    middle.aggregate_revision,
                    RunTerminal::from_failure(AgentFailure::BudgetExceeded),
                )
                .await
                .expect("middle continuation remains eligible");
            let child = admit_continue_child(vault, &middle, "Claim the inherited batch.").await;
            let parent_cursor = BatchCursor {
                batch_id: pending_batch.batch_id,
                next_step_index: 1,
            };
            replay_pending_batch(vault, &child, &pending_batch, &parent_cursor).await;

            let before = vault
                .conversation_run(child.run_id)
                .await
                .expect("read child before model intent")
                .expect("child receipt persists");
            append_event(
                vault,
                child.run_id,
                model_intent(Uuid::new_v4(), ProjectionRef::new(), plan(&child, 7)),
            )
            .await
            .expect("unchanged selection inherits the pin through the intermediate run");
            let after_accepted = vault
                .conversation_run(child.run_id)
                .await
                .expect("read child after accepted model intent")
                .expect("child receipt persists");
            let before_rejected = vault
                .conversation_journal(child.run_id)
                .await
                .expect("read child journal before changed selection");
            assert_eq!(after_accepted.journal_revision, before.journal_revision + 1);
            assert_eq!(
                append_event(
                    vault,
                    child.run_id,
                    model_intent(Uuid::new_v4(), ProjectionRef::new(), plan(&child, 8),),
                )
                .await,
                Err(AgentFailure::PolicyDenied),
                "a changed target is denied before journal commit"
            );
            let after_rejected = vault
                .conversation_run(child.run_id)
                .await
                .expect("read child after rejected model intent")
                .expect("child receipt persists");
            let after_rejected_journal = vault
                .conversation_journal(child.run_id)
                .await
                .expect("read child journal after rejected selection");
            assert_eq!(
                after_rejected.journal_revision,
                after_accepted.journal_revision
            );
            assert_eq!(after_rejected_journal.len(), before_rejected.len());
        }
    }

    #[tokio::test]
    async fn run_owner_validates_unpinned_intermediate_ancestry_before_model_intent() {
        let (started, pending_batch, _) = start_parent_with_pending_batch().await;
        let StartedRun {
            root,
            keys,
            person_id,
            vault,
            record: parent,
        } = started;
        let middle =
            admit_continue_child(&vault, &parent, "Create an empty intermediate run.").await;
        let middle = vault
            .finish_conversation_run(
                middle.run_id,
                middle.aggregate_revision,
                RunTerminal::from_failure(AgentFailure::BudgetExceeded),
            )
            .await
            .expect("middle continuation remains eligible");
        let child = admit_continue_child(&vault, &middle, "Claim the inherited batch.").await;
        let parent_cursor = BatchCursor {
            batch_id: pending_batch.batch_id,
            next_step_index: 1,
        };
        replay_pending_batch(&vault, &child, &pending_batch, &parent_cursor).await;

        let mut corrupt_middle = vault
            .conversation_run(middle.run_id)
            .await
            .expect("read intermediate receipt")
            .expect("intermediate receipt persists");
        corrupt_middle.continuation_executor_generation = Some(
            corrupt_middle
                .executor_generation
                .checked_add(1)
                .expect("test generation does not overflow"),
        );
        let connection = vault.connection().expect("connect test Vault");
        connection
            .execute(
                "UPDATE agent_conversation_runs SET payload = ? WHERE run_id = ?",
                (
                    encode_record(&corrupt_middle).expect("encode changed intermediate lineage"),
                    middle.run_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("inject an invalid unpinned intermediate link");
        drop(connection);
        let before = vault
            .conversation_run(child.run_id)
            .await
            .expect("read child before rejected intent")
            .expect("child receipt persists");
        let before_journal_len = vault
            .conversation_journal(child.run_id)
            .await
            .expect("read child journal before rejected intent")
            .len();
        assert_eq!(
            append_event(
                &vault,
                child.run_id,
                model_intent(Uuid::new_v4(), ProjectionRef::new(), plan(&child, 7),),
            )
            .await,
            Err(AgentFailure::StorageUnavailable),
            "the complete chain is validated even though the middle run has no pin"
        );
        drop(vault);
        let reopened = EncryptedAgentVault::open(&root.0, person_id, keys)
            .await
            .expect("reopen the isolated test Vault after corruption is fenced");
        let after = reopened
            .conversation_run(child.run_id)
            .await
            .expect("read child after rejected intent")
            .expect("child receipt persists");
        let after_journal_len = reopened
            .conversation_journal(child.run_id)
            .await
            .expect("read child journal after rejected intent")
            .len();
        assert_eq!(after.journal_revision, before.journal_revision);
        assert_eq!(after_journal_len, before_journal_len);
    }

    #[tokio::test]
    async fn run_owner_rejects_new_dispatch_after_historical_unproven_intent() {
        let StartedRun {
            vault, mut record, ..
        } = start_run().await;
        let mut legacy = plan(&record, 1);
        legacy.selection_commitment = None;
        legacy.budget_profile = None;
        let event = model_intent(Uuid::new_v4(), ProjectionRef::new(), legacy);
        let payload = serde_json::to_string(&event).expect("serialize historical intent");
        let connection = vault.connection().expect("connect test Vault");
        connection
            .execute(
                "INSERT INTO agent_conversation_journal (run_id, revision, kind, payload) VALUES (?, 1, 'intent', ?)",
                (record.run_id.as_uuid().to_string(), payload),
            )
            .await
            .expect("seed a readable historical prefix");
        record.journal_revision = 1;
        connection
            .execute(
                "UPDATE agent_conversation_runs SET journal_revision = 1, payload = ? WHERE run_id = ?",
                (encode_record(&record).expect("encode historical Run head"), record.run_id.as_uuid().to_string()),
            )
            .await
            .expect("advance historical Run head");
        assert_eq!(
            append_event(
                &vault,
                record.run_id,
                model_intent(Uuid::new_v4(), ProjectionRef::new(), plan(&record, 2)),
            )
            .await,
            Err(AgentFailure::PolicyDenied),
            "old evidence remains projectable but cannot authorize a newly appended dispatch"
        );
    }
}
