use floe_agent::{
    AgentContinuation, AgentMessage, AgentOutcome, AgentUsage, DataClass, ModelPlacement,
};
use floe_agent_contract::{CommandId, DependencyCoverage, RunId};
use serde::{Deserialize, Serialize};
use turso::transaction::{Transaction, TransactionBehavior};

use super::*;

const SCHEMA_VERSION: i64 = 7;
const MAX_RUN_RECORD_BYTES: usize = 128 * 1024;
const MAX_JOURNAL_ENTRY_BYTES: usize = 128 * 1024;
const MAX_RUN_ROWS: i64 = 4_096;
const MAX_COMMAND_ROWS: i64 = 4_096;
const MAX_JOURNAL_ENTRIES: u64 = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultConversationRunState {
    Working,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

impl VaultConversationRunState {
    fn terminal(self) -> bool {
        self != Self::Working
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VaultConversationRunRecord {
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub person_id: PersonId,
    pub initial_session_revision: u64,
    pub request_digest: [u8; 32],
    pub state: VaultConversationRunState,
    pub output: Option<String>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub session_revision: u64,
    pub aggregate_revision: u64,
    pub journal_revision: u64,
    pub executor_generation: u64,
    pub continuation_of: Option<RunId>,
    pub continuation_executor_generation: Option<u64>,
    pub continuation_level: u8,
    pub retry_of: Option<RunId>,
    pub model_placement: ModelPlacement,
}

impl VaultConversationRunRecord {
    fn validate(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        let admitted_revision = self
            .initial_session_revision
            .checked_add(1)
            .ok_or(AgentFailure::VaultUnavailable)?;
        let terminal_revision = self
            .initial_session_revision
            .checked_add(2)
            .ok_or(AgentFailure::VaultUnavailable)?;
        if self.person_id != person_id
            || !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.request_digest == [0; 32]
            || self.session_revision <= self.initial_session_revision
            || self.aggregate_revision == 0
            || self.executor_generation == 0
            || self.continuation_level > 3
            || self.retry_of == Some(self.run_id)
            || self.journal_revision > MAX_JOURNAL_ENTRIES
            || self.coverage.validate().is_err()
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        if self.continuation_of.is_some() != (self.continuation_level > 0)
            || self.continuation_executor_generation.is_some() != (self.continuation_level > 0)
            || self.continuation_executor_generation == Some(0)
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let valid = match self.state {
            VaultConversationRunState::Working => {
                self.session_revision == admitted_revision
                    && self.aggregate_revision == 1
                    && self.output.is_none()
                    && self.coverage == DependencyCoverage::Unknown
                    && self.issue.is_none()
            }
            VaultConversationRunState::Completed => {
                self.session_revision == terminal_revision
                    && self.aggregate_revision == 2
                    && self
                        .output
                        .as_deref()
                        .is_some_and(|output| !output.trim().is_empty())
                    && self.issue.is_none()
            }
            VaultConversationRunState::Failed => match self.output.as_deref() {
                Some(output) => {
                    self.session_revision == terminal_revision
                        && self.aggregate_revision == 2
                        && !output.trim().is_empty()
                        && self.coverage != DependencyCoverage::Unknown
                        && matches!(
                            self.issue,
                            Some(AgentFailure::BudgetExceeded | AgentFailure::Stalled)
                        )
                }
                None => {
                    self.session_revision == terminal_revision
                        && self.aggregate_revision == 2
                        && self.coverage == DependencyCoverage::Unknown
                        && self.issue.is_some()
                }
            },
            VaultConversationRunState::Cancelled
            | VaultConversationRunState::TimedOut
            | VaultConversationRunState::Interrupted => {
                self.session_revision == terminal_revision
                    && self.aggregate_revision == 2
                    && self.output.is_none()
                    && self.coverage == DependencyCoverage::Unknown
                    && self.issue.is_some()
            }
        };
        valid.then_some(()).ok_or(AgentFailure::VaultUnavailable)
    }

    fn exact_admission(&self, request: &VaultConversationAdmissionRequest) -> bool {
        self.command_id == request.command_id
            && self.session_id == request.session_id
            && self.person_id == request.person_id
            && self.initial_session_revision == request.expected_session_revision
            && self.request_digest == request.request_digest
            && self.continuation_of == request.continuation.as_ref().map(|value| value.run_id)
            && self.continuation_executor_generation
                == request
                    .continuation
                    .as_ref()
                    .map(|value| value.executor_generation)
            && self.continuation_level
                == request.continuation.as_ref().map_or(0, |value| value.level)
            && self.retry_of == request.retry_of
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationContinuationRef {
    pub run_id: RunId,
    pub executor_generation: u64,
    pub level: u8,
}

#[derive(Clone, Debug)]
pub struct VaultConversationAdmissionRequest {
    pub run_id: RunId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub person_id: PersonId,
    pub expected_session_revision: u64,
    pub request_digest: [u8; 32],
    pub text: String,
    pub continuation: Option<VaultConversationContinuationRef>,
    pub retry_of: Option<RunId>,
    pub model_placement: ModelPlacement,
}

impl VaultConversationAdmissionRequest {
    fn validate(&self) -> Result<(), AgentFailure> {
        if !self.run_id.is_valid()
            || !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.person_id.0.is_nil()
            || self.request_digest == [0; 32]
            || self.text.trim().is_empty()
            || self.text.len() > floe_agent_contract::MAX_OUTPUT_BYTES
            || self.continuation.as_ref().is_some_and(|reference| {
                !reference.run_id.is_valid()
                    || reference.executor_generation == 0
                    || reference.level == 0
                    || reference.level > 3
            })
            || self.retry_of.is_some_and(|run_id| !run_id.is_valid())
            || self.retry_of.is_some() && self.continuation.is_some()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VaultConversationAdmission {
    Created {
        record: VaultConversationRunRecord,
        session: AgentSession,
    },
    Existing(VaultConversationRunRecord),
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
    pub interrupted: Vec<VaultConversationRunRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConversationJournalEntry {
    pub revision: u64,
    pub kind: String,
    pub payload: String,
}

#[derive(Clone, Debug)]
pub struct VaultConversationTerminal {
    pub state: VaultConversationRunState,
    pub output: Option<String>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
    pub appended_messages: Vec<AgentMessage>,
}

impl VaultConversationTerminal {
    fn validate(&self, run_id: RunId) -> Result<(), AgentFailure> {
        if !self.state.terminal()
            || self.coverage.validate().is_err()
            || self.appended_messages.len() > floe_agent_contract::MAX_AGENT_MESSAGES
            || self
                .appended_messages
                .iter()
                .any(|message| message.turn_id() != run_id.as_uuid())
            || self.appended_messages.iter().any(|message| {
                matches!(
                    message,
                    AgentMessage::User { .. } | AgentMessage::Compaction { .. }
                )
            })
            || self
                .output
                .as_ref()
                .is_some_and(|output| output.len() > floe_agent_contract::MAX_OUTPUT_BYTES)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let valid = match self.state {
            VaultConversationRunState::Completed => {
                self.issue.is_none()
                    && self
                        .output
                        .as_deref()
                        .is_some_and(|output| !output.trim().is_empty())
                    && self.appended_messages.last().is_some_and(|message| {
                        matches!(message, AgentMessage::Assistant { text, .. } if Some(text) == self.output.as_ref())
                    })
            }
            VaultConversationRunState::Failed if self.output.is_some() => {
                self.issue.is_some()
                    && self.coverage != DependencyCoverage::Unknown
                    && matches!(
                        self.issue,
                        Some(AgentFailure::BudgetExceeded | AgentFailure::Stalled)
                    )
                    && self.appended_messages.last().is_some_and(|message| {
                        matches!(message, AgentMessage::Assistant { text, .. } if Some(text) == self.output.as_ref())
                    })
            }
            VaultConversationRunState::Failed
            | VaultConversationRunState::Cancelled
            | VaultConversationRunState::TimedOut
            | VaultConversationRunState::Interrupted => {
                self.output.is_none()
                    && self.coverage == DependencyCoverage::Unknown
                    && self.issue.is_some()
                    && self.appended_messages.is_empty()
            }
            VaultConversationRunState::Working => false,
        };
        valid.then_some(()).ok_or(AgentFailure::InvalidInput)
    }
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
                if current.state != VaultConversationRunState::Working
                    || current.executor_generation >= next_generation
                {
                    return Err(AgentFailure::Conflict);
                }
                let mut session = self.session_on(&transaction, current.session_id).await?;
                if session.person_id != self.person_id
                    || session.revision != current.session_revision
                    || session.active_turn != Some(run_id.as_uuid())
                {
                    return Err(AgentFailure::VaultUnavailable);
                }
                let previous_session_revision = session.revision;
                session.revision = session
                    .revision
                    .checked_add(1)
                    .ok_or(AgentFailure::Conflict)?;
                session.active_turn = None;
                session.continuation = None;
                session.last_outcome = Some(AgentOutcome::Halted {
                    reason: AgentFailure::Interrupted,
                });
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
                let next = VaultConversationRunRecord {
                    state: VaultConversationRunState::Interrupted,
                    issue: Some(AgentFailure::Interrupted),
                    session_revision: session.revision,
                    aggregate_revision: current
                        .aggregate_revision
                        .checked_add(1)
                        .ok_or(AgentFailure::Conflict)?,
                    executor_generation: next_generation,
                    ..current.clone()
                };
                next.validate(self.person_id)?;
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
        request: VaultConversationAdmissionRequest,
    ) -> Result<VaultConversationAdmission, AgentFailure> {
        request.validate()?;
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
            if let Some(existing) = self
                .conversation_run_by_command_on(&transaction, request.command_id)
                .await?
            {
                return if existing.exact_admission(&request) {
                    Ok(VaultConversationAdmission::Existing(existing))
                } else {
                    Err(AgentFailure::Conflict)
                };
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
                .active_conversation_executor_generation(&transaction)
                .await?;
            let mut session = self.session_on(&transaction, request.session_id).await?;
            if session.person_id != request.person_id
                || session.scope.is_some()
                || session.data_classes != [DataClass::Personal]
                || session.revision != request.expected_session_revision
                || session.active_turn.is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            if let Some(reference) = &request.continuation {
                let source = self
                    .conversation_run_on(&transaction, reference.run_id)
                    .await?
                    .ok_or(AgentFailure::Conflict)?;
                let resumable = matches!(
                    (source.state, source.issue),
                    (
                        VaultConversationRunState::TimedOut,
                        Some(AgentFailure::DeadlineExceeded)
                    ) | (
                        VaultConversationRunState::Failed,
                        Some(AgentFailure::BudgetExceeded)
                    )
                );
                if !resumable
                    || source.session_id != request.session_id
                    || source.person_id != request.person_id
                    || source.session_revision != request.expected_session_revision
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
                    .conversation_run_on(&transaction, retry_of)
                    .await?
                    .ok_or(AgentFailure::Conflict)?;
                if source.session_id != request.session_id
                    || source.person_id != request.person_id
                    || !source.state.terminal()
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
            let previous_revision = session.revision;
            session.revision = session
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            if request.continuation.is_none() {
                session.messages.push(AgentMessage::User {
                    turn_id: request.run_id.as_uuid(),
                    text: request.text,
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
                &transaction,
                self.person_id,
                session.id,
                request.run_id.as_uuid(),
                DependencyCoverage::Independent,
            )
            .await?;
            let record = VaultConversationRunRecord {
                run_id: request.run_id,
                command_id: request.command_id,
                session_id: request.session_id,
                person_id: request.person_id,
                initial_session_revision: request.expected_session_revision,
                request_digest: request.request_digest,
                state: VaultConversationRunState::Working,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: None,
                session_revision: session.revision,
                aggregate_revision: 1,
                journal_revision: 0,
                executor_generation,
                continuation_of: request.continuation.as_ref().map(|value| value.run_id),
                continuation_executor_generation: request
                    .continuation
                    .as_ref()
                    .map(|value| value.executor_generation),
                continuation_level: request.continuation.as_ref().map_or(0, |value| value.level),
                retry_of: request.retry_of,
                model_placement: request.model_placement,
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
            self.check_access()?;
            Ok(VaultConversationAdmission::Created { record, session })
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
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
                return if receipt.person_id == request.person_id
                    && receipt.run_id == request.run_id
                {
                    Ok(VaultConversationCancelAdmission::Existing(receipt))
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
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
            if run.person_id != request.person_id {
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
                        request.person_id.to_string(),
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
                    person_id: request.person_id,
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
            if record.state != VaultConversationRunState::Working
                || record.journal_revision >= MAX_JOURNAL_ENTRIES
            {
                return Err(AgentFailure::Conflict);
            }
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
        terminal: VaultConversationTerminal,
    ) -> Result<VaultConversationRunRecord, AgentFailure> {
        terminal.validate(run_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let current = self
                .conversation_run_on(&transaction, run_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.executor_generation
                != self.active_conversation_executor_generation(&transaction).await?
            {
                return Err(AgentFailure::Conflict);
            }
            if current.state != VaultConversationRunState::Working
                || current.aggregate_revision != expected_aggregate_revision
            {
                return Err(AgentFailure::Conflict);
            }
            let mut session = self.session_on(&transaction, current.session_id).await?;
            if session.person_id != self.person_id
                || session.revision != current.session_revision
                || session.active_turn != Some(run_id.as_uuid())
            {
                return Err(AgentFailure::Conflict);
            }
            session.messages.extend(terminal.appended_messages);
            session.active_turn = None;
            session.continuation = (terminal.output.is_none()
                && matches!(
                    (terminal.state, terminal.issue),
                    (
                        VaultConversationRunState::TimedOut,
                        Some(AgentFailure::DeadlineExceeded)
                    ) | (
                        VaultConversationRunState::Failed,
                        Some(AgentFailure::BudgetExceeded)
                    )
                ))
            .then(|| current.continuation_level.checked_add(1))
            .flatten()
            .filter(|level| *level <= 3)
            .map(|_| AgentContinuation {
                turn_id: run_id.as_uuid(),
                level: current.continuation_level,
                usage: AgentUsage::default(),
                placement: current.model_placement,
            });
            session.last_outcome = Some(match terminal.state {
                VaultConversationRunState::Completed => AgentOutcome::Completed,
                _ => AgentOutcome::Halted {
                    reason: terminal.issue.ok_or(AgentFailure::InvalidInput)?,
                },
            });
            let previous_session_revision = session.revision;
            session.revision = session
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            let payload = self.payload(&session)?;
            let changed = transaction
                .execute(
                    "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                    (
                        integer(session.revision)?,
                        payload,
                        session.id.to_string(),
                        integer(previous_session_revision)?,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            context_dependencies::merge_context_dependency_coverage(
                &transaction,
                self.person_id,
                session.id,
                run_id.as_uuid(),
                terminal.coverage.clone(),
            )
            .await?;
            let next = VaultConversationRunRecord {
                state: terminal.state,
                output: terminal.output,
                coverage: terminal.coverage,
                issue: terminal.issue,
                session_revision: session.revision,
                aggregate_revision: current
                    .aggregate_revision
                    .checked_add(1)
                    .ok_or(AgentFailure::Conflict)?,
                ..current.clone()
            };
            next.validate(self.person_id)?;
            let changed = transaction
                .execute(
                    "UPDATE agent_conversation_runs SET state = ?, aggregate_revision = ?, payload = ? WHERE run_id = ? AND state = 'working' AND aggregate_revision = ? AND executor_generation = ?",
                    (
                        state_name(next.state),
                        integer(next.aggregate_revision)?,
                        encode_record(&next)?,
                        run_id.as_uuid().to_string(),
                        integer(expected_aggregate_revision)?,
                        integer(current.executor_generation)?,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(next)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn conversation_run(
        &self,
        run_id: RunId,
    ) -> Result<Option<VaultConversationRunRecord>, AgentFailure> {
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
    ) -> Result<Option<VaultConversationRunRecord>, AgentFailure> {
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
        self.check_access()?;
        Ok(entries)
    }

    pub async fn recover_conversation_session(
        &self,
        session_id: Uuid,
        person_id: PersonId,
        expected_session_revision: u64,
    ) -> Result<u64, AgentFailure> {
        if session_id.is_nil() {
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
                if run.state == VaultConversationRunState::Working
                    && run.executor_generation
                        == self
                            .active_conversation_executor_generation(&transaction)
                            .await?
                {
                    return Err(AgentFailure::Conflict);
                }
                return Err(AgentFailure::VaultUnavailable);
            }
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
    ) -> Result<Option<VaultConversationRunRecord>, AgentFailure> {
        read_record(
            self.person_id,
            connection,
            "SELECT run_id, command_id, session_id, person_id, state, aggregate_revision, journal_revision, executor_generation, payload FROM agent_conversation_runs WHERE command_id = ?",
            command_id.as_uuid().to_string(),
        )
        .await
    }

    async fn conversation_run_on(
        &self,
        connection: &turso::Connection,
        run_id: RunId,
    ) -> Result<Option<VaultConversationRunRecord>, AgentFailure> {
        read_record(
            self.person_id,
            connection,
            "SELECT run_id, command_id, session_id, person_id, state, aggregate_revision, journal_revision, executor_generation, payload FROM agent_conversation_runs WHERE run_id = ?",
            run_id.as_uuid().to_string(),
        )
        .await
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
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_conversation_schema', 'agent_conversation_executor', 'agent_conversation_runs', 'agent_conversation_journal', 'agent_conversation_commands')",
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
                "CREATE TABLE agent_conversation_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 7))",
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
                "CREATE TABLE agent_conversation_runs (run_id TEXT PRIMARY KEY, command_id TEXT NOT NULL UNIQUE, session_id TEXT NOT NULL, person_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('working', 'completed', 'failed', 'cancelled', 'timed_out', 'interrupted')), aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0), journal_revision INTEGER NOT NULL CHECK (journal_revision >= 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_journal (run_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK (revision > 0), kind TEXT NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072), PRIMARY KEY (run_id, revision), FOREIGN KEY (run_id) REFERENCES agent_conversation_runs(run_id))",
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
                "CREATE INDEX agent_conversation_active_session ON agent_conversation_runs (session_id, state, run_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_conversation_schema (id, version) VALUES (1, 7)",
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
            "agent_conversation_runs".to_owned(),
            "agent_conversation_schema".to_owned(),
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
    executor_generation(transaction).await?;
    Ok(())
}

async fn read_record(
    person_id: PersonId,
    connection: &turso::Connection,
    query: &str,
    identifier: String,
) -> Result<Option<VaultConversationRunRecord>, AgentFailure> {
    let mut rows = connection
        .query(query, [identifier])
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let record: VaultConversationRunRecord =
        serde_json::from_str(&row.get::<String>(8).map_err(storage)?).map_err(unavailable)?;
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
    next: &VaultConversationRunRecord,
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

fn encode_record(record: &VaultConversationRunRecord) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_RUN_RECORD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

fn state_name(state: VaultConversationRunState) -> &'static str {
    match state {
        VaultConversationRunState::Working => "working",
        VaultConversationRunState::Completed => "completed",
        VaultConversationRunState::Failed => "failed",
        VaultConversationRunState::Cancelled => "cancelled",
        VaultConversationRunState::TimedOut => "timed_out",
        VaultConversationRunState::Interrupted => "interrupted",
    }
}

#[cfg(test)]
mod tests;
