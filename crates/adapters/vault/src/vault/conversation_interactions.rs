//! Durable Conversation interaction storage beside Conversation state.
//!
//! Interactions, decisions and publication linkage live in the same encrypted
//! Vault as Sessions and Runs, but no interaction row grants authority: it
//! records the admitted origin, the owner-produced requirement, the immutable
//! reviewed target and the decision intent that a later owner operation acts
//! on. Publication identity is deterministic (origin plus canonical digests),
//! so crash replay settles the same row instead of duplicating it. The
//! primary key on that deterministic id is the origin+digest uniqueness
//! constraint: every write revalidates that the id commits to the stored
//! origin and digests, so two distinct origins (for example two identical
//! tool calls) publish two rows while the same publication replays one.
//! Decision command identity is a second primary key with the same
//! replay-or-conflict contract.

use floe_agent_contract::AgentFailure;
use floe_conversation::{
    ConversationInteraction, DecisionAdmission, ExpireInteraction, ExpireOutcome,
    InteractionDecision, InteractionResolution, InteractionState, MAX_ACTIVE_INTERACTIONS_PER_RUN,
    MAX_STORED_INTERACTIONS_PER_RUN, PublishAdmission, SupersedeInteraction,
    next_state_after_decision, state_after_resolution,
};
use floe_kernel::{PersonId, RunId};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

use super::*;

const SCHEMA_VERSION: i64 = 1;
const MAX_INTERACTION_PAYLOAD_BYTES: usize = 32 * 1024;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn publish_conversation_interaction(
        &self,
        record: ConversationInteraction,
    ) -> Result<PublishAdmission, AgentFailure> {
        record
            .validate()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if record.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            if let Some(existing) =
                read_interaction(&transaction, self.person_id, record.id).await?
            {
                if existing.requirement_digest != record.requirement_digest
                    || existing.target_digest != record.target_digest
                    || existing.origin_run_id != record.origin_run_id
                    || existing.origin != record.origin
                {
                    return Err(AgentFailure::VaultUnavailable);
                }
                return Ok(PublishAdmission::Existing(existing));
            }
            self.check_interaction_origin(&transaction, &record).await?;
            let stored = count_interactions(&transaction, record.origin_run_id, false).await?;
            if stored >= u64::try_from(MAX_STORED_INTERACTIONS_PER_RUN).unwrap_or(u64::MAX) {
                return Err(AgentFailure::BudgetExceeded);
            }
            let active = count_interactions(&transaction, record.origin_run_id, true).await?;
            if active >= u64::try_from(MAX_ACTIVE_INTERACTIONS_PER_RUN).unwrap_or(u64::MAX) {
                return Err(AgentFailure::BudgetExceeded);
            }
            insert_interaction(&transaction, &record).await?;
            self.check_access()?;
            Ok(PublishAdmission::Created(record))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn conversation_interaction(
        &self,
        interaction_id: Uuid,
    ) -> Result<Option<ConversationInteraction>, AgentFailure> {
        if interaction_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let record = read_interaction(&self.connection()?, self.person_id, interaction_id).await?;
        self.check_access()?;
        Ok(record)
    }

    pub async fn run_conversation_interactions(
        &self,
        origin_run_id: RunId,
    ) -> Result<Vec<ConversationInteraction>, AgentFailure> {
        if !origin_run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        if !table_exists(&connection, "agent_conversation_interactions").await? {
            self.check_access()?;
            return Ok(vec![]);
        }
        let mut rows = connection
            .query(
                "SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions WHERE origin_run_id = ? ORDER BY created_at ASC, interaction_id ASC",
                [origin_run_id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            records.push(parse_row(self.person_id, &row).await?);
        }
        if records.len() > MAX_STORED_INTERACTIONS_PER_RUN {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.check_access()?;
        Ok(records)
    }

    pub async fn record_conversation_interaction_decision(
        &self,
        decision: InteractionDecision,
    ) -> Result<DecisionAdmission, AgentFailure> {
        decision
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if decision.principal != self.person_id.to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            if let Some(recorded) = read_decision(&transaction, decision.command_id).await? {
                if !decision.matches_recorded(&recorded) {
                    return Err(AgentFailure::Conflict);
                }
                let current =
                    read_interaction(&transaction, self.person_id, decision.interaction_id)
                        .await?
                        .ok_or(AgentFailure::VaultUnavailable)?;
                return Ok(DecisionAdmission::Rejoined(current));
            }
            let current = read_interaction(&transaction, self.person_id, decision.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.revision != decision.interaction_revision
                || current.target_digest != decision.target_digest
                || decision.decided_at_unix_ms < current.created_at_unix_ms
                || decision.decided_at_unix_ms >= current.expires_at_unix_ms
            {
                return Err(AgentFailure::Conflict);
            }
            let next = next_state_after_decision(&current.state, &decision)?;
            let mut updated = current;
            updated.state = next;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            insert_decision(&transaction, &decision).await?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            self.check_access()?;
            Ok(DecisionAdmission::Applied(updated))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn resolve_conversation_interaction(
        &self,
        resolution: InteractionResolution,
    ) -> Result<ConversationInteraction, AgentFailure> {
        resolution
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if resolution.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let recorded = read_decision(&transaction, resolution.decision_id)
                .await?
                .ok_or(AgentFailure::Conflict)?;
            if recorded.interaction_id != resolution.interaction_id
                || resolution.resolved_at_unix_ms < recorded.decided_at_unix_ms
            {
                return Err(AgentFailure::Conflict);
            }
            let current = read_interaction(&transaction, self.person_id, resolution.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.revision != resolution.expected_revision {
                return Err(AgentFailure::Conflict);
            }
            let next = state_after_resolution(
                &current.state,
                resolution.decision_id,
                resolution.owner_operation_id,
                resolution.resolved_at_unix_ms,
            )?;
            let mut updated = current;
            updated.state = next;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            self.check_access()?;
            Ok(updated)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn supersede_conversation_interaction(
        &self,
        supersede: SupersedeInteraction,
    ) -> Result<ConversationInteraction, AgentFailure> {
        supersede
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if supersede.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let current = read_interaction(&transaction, self.person_id, supersede.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.revision != supersede.expected_revision || current.state.is_terminal() {
                return Err(AgentFailure::Conflict);
            }
            let mut updated = current;
            updated.state = InteractionState::Superseded {
                superseded_by: supersede.superseded_by,
            };
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            self.check_access()?;
            Ok(updated)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn expire_conversation_interaction(
        &self,
        expire: ExpireInteraction,
    ) -> Result<ExpireOutcome, AgentFailure> {
        expire.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if expire.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let current = read_interaction(&transaction, self.person_id, expire.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.state.is_terminal() {
                return Ok(ExpireOutcome::AlreadyTerminal(current));
            }
            if expire.now_unix_ms < current.expires_at_unix_ms {
                return Ok(ExpireOutcome::NotExpired(current));
            }
            let mut updated = current;
            updated.state = InteractionState::Expired;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            self.check_access()?;
            Ok(ExpireOutcome::Expired(updated))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    /// Crash case 5: a Run cancelled before publication leaves no actionable
    /// orphan card. Unknown runs fail closed; terminal-but-not-cancelled runs
    /// still publish, since replay may arrive after the origin Run completed.
    async fn check_interaction_origin(
        &self,
        transaction: &Transaction<'_>,
        record: &ConversationInteraction,
    ) -> Result<(), AgentFailure> {
        if !table_exists(transaction, "agent_conversation_runs").await? {
            return Err(AgentFailure::NotFound);
        }
        let run = self
            .conversation_run_on(transaction, record.origin_run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if run.person_id != self.person_id
            || run.session_id != record.session_id
            || run.command_id.as_uuid().is_nil()
        {
            return Err(AgentFailure::Conflict);
        }
        if run.state == crate::VaultConversationRunState::Cancelled {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
}

async fn table_exists(connection: &turso::Connection, table: &str) -> Result<bool, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?",
            [table],
        )
        .await
        .map_err(storage)?;
    Ok(rows.next().await.map_err(storage)?.is_some())
}

async fn initialize(transaction: &Transaction<'_>) -> Result<(), AgentFailure> {
    let mut tables = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_conversation_interaction_schema', 'agent_conversation_interactions', 'agent_conversation_interaction_decisions')",
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
                "CREATE TABLE agent_conversation_interaction_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_interactions (interaction_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, person_id TEXT NOT NULL, origin_run_id TEXT NOT NULL, requirement_digest TEXT NOT NULL, target_digest TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'denied', 'cancelled', 'superseded', 'expired')), revision INTEGER NOT NULL CHECK (revision > 0), created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 32768))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_interaction_decisions (command_id TEXT PRIMARY KEY, interaction_id TEXT NOT NULL REFERENCES agent_conversation_interactions(interaction_id), kind TEXT NOT NULL CHECK (kind IN ('approve', 'deny', 'dismiss')), target_digest TEXT NOT NULL, principal TEXT NOT NULL, interaction_revision INTEGER NOT NULL CHECK (interaction_revision > 0), decided_at INTEGER NOT NULL)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_conversation_interactions_run ON agent_conversation_interactions (origin_run_id, state, interaction_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_conversation_interaction_schema (id, version) VALUES (1, 1)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found
        != [
            "agent_conversation_interaction_decisions".to_owned(),
            "agent_conversation_interaction_schema".to_owned(),
            "agent_conversation_interactions".to_owned(),
        ]
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut marker = transaction
        .query(
            "SELECT id, version FROM agent_conversation_interaction_schema",
            (),
        )
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
            "SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction
        .query(
            "SELECT command_id, interaction_id, kind, target_digest, principal, interaction_revision, decided_at FROM agent_conversation_interaction_decisions LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    let mut index = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'agent_conversation_interactions_run' AND tbl_name = 'agent_conversation_interactions'",
            (),
        )
        .await
        .map_err(storage)?;
    if index.next().await.map_err(storage)?.is_none()
        || index.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

async fn read_interaction(
    connection: &turso::Connection,
    person_id: PersonId,
    interaction_id: Uuid,
) -> Result<Option<ConversationInteraction>, AgentFailure> {
    if !table_exists(connection, "agent_conversation_interactions").await? {
        return Ok(None);
    }
    let mut rows = connection
        .query(
            "SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions WHERE interaction_id = ?",
            [interaction_id.to_string()],
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let record = parse_row(person_id, &row).await?;
    if rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(record))
}

async fn parse_row(
    person_id: PersonId,
    row: &turso::Row,
) -> Result<ConversationInteraction, AgentFailure> {
    let record: ConversationInteraction =
        serde_json::from_str(&row.get::<String>(10).map_err(storage)?).map_err(unavailable)?;
    record
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if record.person_id != person_id
        || row.get::<String>(0).map_err(storage)? != record.id.to_string()
        || row.get::<String>(1).map_err(storage)? != record.session_id.to_string()
        || row.get::<String>(2).map_err(storage)? != record.person_id.to_string()
        || row.get::<String>(3).map_err(storage)? != record.origin_run_id.as_uuid().to_string()
        || row.get::<String>(4).map_err(storage)? != hex_digest(&record.requirement_digest)
        || row.get::<String>(5).map_err(storage)? != hex_digest(&record.target_digest)
        || row.get::<String>(6).map_err(storage)? != state_name(&record.state)
        || row.get::<i64>(7).map_err(storage)? != integer(record.revision)?
        || row.get::<i64>(8).map_err(storage)? != record.created_at_unix_ms
        || row.get::<i64>(9).map_err(storage)? != record.expires_at_unix_ms
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(record)
}

async fn count_interactions(
    transaction: &Transaction<'_>,
    origin_run_id: RunId,
    active_only: bool,
) -> Result<u64, AgentFailure> {
    let sql = if active_only {
        "SELECT count(*) FROM agent_conversation_interactions WHERE origin_run_id = ? AND state IN ('pending', 'resolving')"
    } else {
        "SELECT count(*) FROM agent_conversation_interactions WHERE origin_run_id = ?"
    };
    let mut rows = transaction
        .query(sql, [origin_run_id.as_uuid().to_string()])
        .await
        .map_err(storage)?;
    let count = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    u64::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)
}

async fn insert_interaction(
    transaction: &Transaction<'_>,
    record: &ConversationInteraction,
) -> Result<(), AgentFailure> {
    let payload = encode_record(record)?;
    let parameters: Vec<turso::Value> = vec![
        record.id.to_string().into(),
        record.session_id.to_string().into(),
        record.person_id.to_string().into(),
        record.origin_run_id.as_uuid().to_string().into(),
        hex_digest(&record.requirement_digest).into(),
        hex_digest(&record.target_digest).into(),
        state_name(&record.state).to_owned().into(),
        integer(record.revision)?.into(),
        record.created_at_unix_ms.into(),
        record.expires_at_unix_ms.into(),
        payload.into(),
    ];
    transaction
        .execute(
            "INSERT INTO agent_conversation_interactions (interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            parameters,
        )
        .await
        .map_err(storage)?;
    Ok(())
}

async fn update_interaction(
    transaction: &Transaction<'_>,
    record: &ConversationInteraction,
    expected_revision: u64,
) -> Result<(), AgentFailure> {
    let changed = transaction
        .execute(
            "UPDATE agent_conversation_interactions SET state = ?, revision = ?, payload = ? WHERE interaction_id = ? AND revision = ?",
            (
                state_name(&record.state).to_owned(),
                integer(record.revision)?,
                encode_record(record)?,
                record.id.to_string(),
                integer(expected_revision)?,
            ),
        )
        .await
        .map_err(storage)?;
    if changed != 1 {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

async fn read_decision(
    connection: &turso::Connection,
    command_id: Uuid,
) -> Result<Option<InteractionDecision>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT command_id, interaction_id, kind, target_digest, principal, interaction_revision, decided_at FROM agent_conversation_interaction_decisions WHERE command_id = ?",
            [command_id.to_string()],
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let decision = InteractionDecision {
        command_id: Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        interaction_id: Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        kind: match row.get::<String>(2).map_err(storage)?.as_str() {
            "approve" => floe_conversation::InteractionDecisionKind::Approve,
            "deny" => floe_conversation::InteractionDecisionKind::Deny,
            "dismiss" => floe_conversation::InteractionDecisionKind::Dismiss,
            _ => return Err(AgentFailure::VaultUnavailable),
        },
        target_digest: parse_digest(&row.get::<String>(3).map_err(storage)?)?,
        principal: row.get::<String>(4).map_err(storage)?,
        interaction_revision: u64::try_from(row.get::<i64>(5).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        decided_at_unix_ms: row.get::<i64>(6).map_err(storage)?,
    };
    decision
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if decision.command_id != command_id || rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(decision))
}

async fn insert_decision(
    transaction: &Transaction<'_>,
    decision: &InteractionDecision,
) -> Result<(), AgentFailure> {
    transaction
        .execute(
            "INSERT INTO agent_conversation_interaction_decisions (command_id, interaction_id, kind, target_digest, principal, interaction_revision, decided_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                decision.command_id.to_string(),
                decision.interaction_id.to_string(),
                decision_kind_name(decision.kind).to_owned(),
                hex_digest(&decision.target_digest),
                decision.principal.clone(),
                integer(decision.interaction_revision)?,
                decision.decided_at_unix_ms,
            ),
        )
        .await
        .map_err(storage)?;
    Ok(())
}

fn encode_record(record: &ConversationInteraction) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_INTERACTION_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

fn state_name(state: &InteractionState) -> &'static str {
    match state {
        InteractionState::Pending => "pending",
        InteractionState::Resolving { .. } => "resolving",
        InteractionState::Resolved { .. } => "resolved",
        InteractionState::Denied { .. } => "denied",
        InteractionState::Cancelled { .. } => "cancelled",
        InteractionState::Superseded { .. } => "superseded",
        InteractionState::Expired => "expired",
    }
}

fn decision_kind_name(kind: floe_conversation::InteractionDecisionKind) -> &'static str {
    match kind {
        floe_conversation::InteractionDecisionKind::Approve => "approve",
        floe_conversation::InteractionDecisionKind::Deny => "deny",
        floe_conversation::InteractionDecisionKind::Dismiss => "dismiss",
    }
}

fn hex_digest(digest: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn parse_digest(value: &str) -> Result<[u8; 32], AgentFailure> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut digest = [0; 32];
    for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| AgentFailure::VaultUnavailable)?;
        digest[index] = u8::from_str_radix(text, 16).map_err(|_| AgentFailure::VaultUnavailable)?;
    }
    Ok(digest)
}

#[cfg(test)]
mod tests;
