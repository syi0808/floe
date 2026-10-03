use std::{collections::BTreeMap, sync::atomic::Ordering};

use floe_access::DependencyCoverage;
use floe_actions::{
    ActionAdmission, ActionDecision, ActionDigest, ActionOrigin, ActionPage, ActionRecord,
    ActionReconciliation, ActionState, ActionStoreError, ActionsAuthority, AuthorityChange,
    CollectionAck, CollectionTicket, DispatchAdmission, DispatchIntent, ExecutionIntent,
    ExecutionSettlement, PreDispatchStop, RecoveryPage, AdmittedAction,
    acknowledge_action_collection, action_digest, change_action_authority,
    decision_intent_digest, invalidate_action_dependency, invalidate_action_policy,
    prepare_action_dispatch, settle_action, stop_action, validate_action_admission,
};
use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

const ACTIONS_SCHEMA_VERSION: i64 = 1;
const ACTION_SELECT: &str = "SELECT person_id, action_id, revision, effect_digest, execution_id, state, collection_state, dispatch_revision, grant_key, length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE '' END, origin_kind FROM actions_records";

const ACTIONS_TABLES: &[(&str, &str)] = &[
    (
        "actions_schema",
        "CREATE TABLE actions_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
    ),
    (
        "actions_records",
        "CREATE TABLE actions_records (person_id TEXT NOT NULL, action_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK (revision > 0), effect_digest TEXT NOT NULL CHECK (length(effect_digest) = 64), execution_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending_review', 'approved', 'rejected', 'cancelled', 'expired', 'executing', 'blocked', 'failed', 'unknown', 'succeeded')), collection_state TEXT NOT NULL CHECK (collection_state IN ('none', 'pending', 'collected')), dispatch_revision INTEGER NOT NULL CHECK (dispatch_revision >= 0), grant_key TEXT NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) > 0 AND length(CAST(payload AS BLOB)) <= 65536), origin_kind TEXT NOT NULL CHECK (origin_kind IN ('direct', 'expert')), PRIMARY KEY (person_id, action_id), CHECK ((origin_kind = 'direct' AND grant_key = '') OR (origin_kind = 'expert' AND grant_key <> '')), CHECK ((state = 'succeeded' AND collection_state IN ('pending', 'collected')) OR (state <> 'succeeded' AND collection_state = 'none')), CHECK ((state IN ('executing', 'failed', 'unknown', 'succeeded') AND dispatch_revision > 0) OR (state NOT IN ('executing', 'failed', 'unknown', 'succeeded') AND dispatch_revision = 0)))",
    ),
    (
        "actions_authorities",
        "CREATE TABLE actions_authorities (person_id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK (revision > 0), mode TEXT NOT NULL CHECK (mode IN ('allow', 'ask', 'deny')), digest TEXT NOT NULL CHECK (length(digest) = 64), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) > 0 AND length(CAST(payload AS BLOB)) <= 4096))",
    ),
    (
        "actions_command_receipts",
        "CREATE TABLE actions_command_receipts (person_id TEXT NOT NULL, command_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('submit', 'decision', 'reconciliation', 'authority')), intent_digest TEXT NOT NULL CHECK (length(intent_digest) = 64), action_id TEXT NOT NULL, PRIMARY KEY (person_id, command_id), CHECK ((kind = 'authority' AND action_id = '') OR (kind <> 'authority' AND action_id <> ''))) ",
    ),
    (
        "actions_settlement_receipts",
        "CREATE TABLE actions_settlement_receipts (person_id TEXT NOT NULL, execution_id TEXT NOT NULL, expected_revision INTEGER NOT NULL CHECK (expected_revision > 0), action_id TEXT NOT NULL, effect_digest TEXT NOT NULL CHECK (length(effect_digest) = 64), outcome_digest TEXT NOT NULL CHECK (length(outcome_digest) = 64), PRIMARY KEY (person_id, execution_id, expected_revision))",
    ),
    (
        "actions_collection_receipts",
        "CREATE TABLE actions_collection_receipts (person_id TEXT NOT NULL, execution_id TEXT NOT NULL, receipt_digest TEXT NOT NULL CHECK (length(receipt_digest) = 64), ticket_revision INTEGER NOT NULL CHECK (ticket_revision > 0), action_id TEXT NOT NULL, intent_digest TEXT NOT NULL CHECK (length(intent_digest) = 64), PRIMARY KEY (person_id, execution_id, receipt_digest, ticket_revision))",
    ),
];

const ACTIONS_INDEXES: &[(&str, &str)] = &[
    (
        "actions_records_execution",
        "CREATE UNIQUE INDEX actions_records_execution ON actions_records (execution_id)",
    ),
    (
        "actions_records_page",
        "CREATE INDEX actions_records_page ON actions_records (person_id, action_id)",
    ),
    (
        "actions_records_recovery",
        "CREATE INDEX actions_records_recovery ON actions_records (person_id, state, collection_state, action_id)",
    ),
    (
        "actions_records_grant",
        "CREATE INDEX actions_records_grant ON actions_records (person_id, grant_key, state, action_id)",
    ),
];

const OLD_ACTION_TABLES: &[&str] = &[
    "agent_action_schema",
    "agent_action_envelopes",
    "agent_action_policy",
];

#[derive(Clone, Debug)]
struct StoredAction {
    record: ActionRecord,
    dispatch_revision: Option<u64>,
}

macro_rules! stored_action_from_row {
    ($row:expr) => {{
        let row = $row;
        decode_action_columns(
            row.get::<String>(0).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(1).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<i64>(2).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(3).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(4).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(5).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(6).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<i64>(7).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(8).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<i64>(9).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(10).map_err(|_| ActionStoreError::CorruptRecord)?,
            row.get::<String>(11).map_err(|_| ActionStoreError::CorruptRecord)?,
        )
    }};
}

fn decode_action_columns(
    person_id: String,
    action_id: String,
    revision: i64,
    effect_digest: String,
    execution_id: String,
    state: String,
    collection_state: String,
    dispatch_revision: i64,
    grant_key: String,
    payload_bytes: i64,
    payload: String,
    origin_kind: String,
) -> Result<StoredAction, ActionStoreError> {
    if payload_bytes <= 0 || payload_bytes > floe_actions::MAX_ACTION_BYTES as i64 {
        return Err(ActionStoreError::CorruptRecord);
    }
    if payload.len() != usize::try_from(payload_bytes).map_err(|_| ActionStoreError::CorruptRecord)? {
        return Err(ActionStoreError::CorruptRecord);
    }
    let record: ActionRecord =
        serde_json::from_str(&payload).map_err(|_| ActionStoreError::CorruptRecord)?;
    if record.validate().is_err()
        || serde_json::to_string(&record).map_err(|_|ActionStoreError::CorruptRecord)?!=payload
        || person_id != record.person_id.to_string()
        || action_id != record.id.to_string()
        || u64::try_from(revision).ok() != Some(record.revision)
        || effect_digest != digest_hex(&record.effect_digest)
        || execution_id != record.execution_id.to_string()
        || state != action_state_name(&record.state)
        || collection_state != action_collection_state_name(&record)
        || grant_key != action_grant_key(&record)
        || origin_kind != action_origin_name(&record.origin)
    {
        return Err(ActionStoreError::CorruptRecord);
    }
    let dispatch_revision = u64::try_from(dispatch_revision)
        .map_err(|_| ActionStoreError::CorruptRecord)?;
    let dispatch_revision = if dispatch_revision == 0 {
        None
    } else {
        Some(dispatch_revision)
    };
    match (&record.execution, dispatch_revision) {
        (None, None) => {}
        (Some(_), Some(revision)) if revision < record.revision => {}
        _ => return Err(ActionStoreError::CorruptRecord),
    }
    Ok(StoredAction {
        record,
        dispatch_revision,
    })
}

fn action_state_name(state: &ActionState) -> &'static str {
    match state {
        ActionState::PendingReview => "pending_review",
        ActionState::Approved => "approved",
        ActionState::Rejected => "rejected",
        ActionState::Cancelled => "cancelled",
        ActionState::Expired => "expired",
        ActionState::Executing { .. } => "executing",
        ActionState::Blocked { .. } => "blocked",
        ActionState::Failed { .. } => "failed",
        ActionState::Unknown { .. } => "unknown",
        ActionState::Succeeded { .. } => "succeeded",
    }
}

fn action_origin_name(origin: &ActionOrigin) -> &'static str {
    match origin {
        ActionOrigin::Direct { .. } => "direct",
        ActionOrigin::Expert { .. } => "expert",
    }
}

fn action_collection_state_name(record: &ActionRecord) -> &'static str {
    match record.collection.as_ref().map(|ticket| &ticket.state) {
        None => "none",
        Some(floe_actions::ActionCollectionState::Pending { .. }) => "pending",
        Some(floe_actions::ActionCollectionState::Collected { .. }) => "collected",
    }
}

fn action_grant_key(record: &ActionRecord) -> String {
    record
        .dependency
        .as_ref()
        .map(|dependency| {
            format!(
                "{}:{}:{}",
                dependency.grant_id().as_uuid(),
                dependency.grant_authority().incarnation(),
                dependency.grant_authority().access_epoch().get()
            )
        })
        .unwrap_or_default()
}

fn authority_mode_name(mode: floe_actions::ActionAuthorityMode) -> &'static str {
    match mode {
        floe_actions::ActionAuthorityMode::Allow => "allow",
        floe_actions::ActionAuthorityMode::Ask => "ask",
        floe_actions::ActionAuthorityMode::Deny => "deny",
    }
}

fn digest_hex(value: &ActionDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}

fn digest_text_is_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid_record(error: AgentFailure) -> ActionStoreError {
    match error {
        AgentFailure::Conflict => ActionStoreError::Conflict,
        AgentFailure::NotFound => ActionStoreError::NotFound,
        AgentFailure::StorageUnavailable => ActionStoreError::Unavailable,
        AgentFailure::VaultLocked => ActionStoreError::VaultLocked,
        AgentFailure::VaultUnavailable => ActionStoreError::Unavailable,
        AgentFailure::UnsupportedVersion => {
            ActionStoreError::CorruptRecord
        }
        AgentFailure::BudgetExceeded => ActionStoreError::BudgetExceeded,
        AgentFailure::InvalidInput | AgentFailure::PolicyDenied => {
            ActionStoreError::InvalidRecord
        }
        _ => ActionStoreError::Unavailable,
    }
}

fn historical_action_error(error:AgentFailure)->ActionStoreError{
    match error {
        AgentFailure::VaultLocked=>ActionStoreError::VaultLocked,
        AgentFailure::StorageUnavailable|AgentFailure::VaultUnavailable=>ActionStoreError::Unavailable,
        _=>ActionStoreError::CorruptRecord,
    }
}

fn access_error(error: AgentFailure) -> ActionStoreError {
    match error {
        AgentFailure::VaultLocked => ActionStoreError::VaultLocked,
        AgentFailure::VaultUnavailable => ActionStoreError::Unavailable,
        other => invalid_record(other),
    }
}

fn sql_error(error: turso::Error) -> ActionStoreError {
    match error {
        turso::Error::Busy(_) | turso::Error::BusySnapshot(_) | turso::Error::Constraint(_) => {
            ActionStoreError::Conflict
        }
        _ => ActionStoreError::Unavailable,
    }
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn record_payload(record: &ActionRecord) -> Result<String, ActionStoreError> {
    record
        .validate()
        .map_err(|_| ActionStoreError::InvalidRecord)?;
    let payload = serde_json::to_string(record).map_err(|_| ActionStoreError::InvalidRecord)?;
    if payload.is_empty() || payload.len() > floe_actions::MAX_ACTION_BYTES {
        return Err(ActionStoreError::InvalidRecord);
    }
    Ok(payload)
}

fn authority_digest(authority: &ActionsAuthority) -> Result<ActionDigest, ActionStoreError> {
    action_digest(b"floe.actions.authority.v1\0", authority)
        .map_err(|_| ActionStoreError::InvalidRecord)
}

fn command_digest<T: serde::Serialize>(
    domain: &[u8],
    value: &T,
) -> Result<ActionDigest, ActionStoreError> {
    action_digest(domain, value).map_err(|_| ActionStoreError::InvalidRecord)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Existing-Vault open is read-only with respect to the Actions schema.
    /// Missing tables may represent lost uncertain effects, never a clean slate.
    pub(super) async fn validate_actions_store(&self)->Result<(),AgentFailure>{
        let mut connection=self.connection()?;
        let transaction=connection.transaction_with_behavior(TransactionBehavior::Deferred).await.map_err(|_|AgentFailure::StorageUnavailable)?;
        let result=async {
            self.validate_actions_schema(&transaction).await?;
            Ok(())
        }.await;
        self.finish_actions_transaction(transaction,result).await.map_err(AgentFailure::from)
    }

    pub(super) async fn initialize_actions_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| match error {
                turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
                _ => AgentFailure::StorageUnavailable,
            })?;
        let result = async {
            let mut objects = transaction
                .query(
                    "SELECT name FROM sqlite_master WHERE name GLOB 'actions_*' AND type IN ('table', 'index') ORDER BY name",
                    (),
                )
                .await
                .map_err(sql_error)?;
            let mut names = Vec::new();
            while let Some(row) = objects.next().await.map_err(sql_error)? {
                names.push(
                    row.get::<String>(0)
                        .map_err(|_| ActionStoreError::CorruptRecord)?,
                );
            }
            drop(objects);
            if names.is_empty() {
                let mut legacy = transaction
                    .query(
                        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('agent_action_schema', 'agent_action_envelopes', 'agent_action_policy')",
                        (),
                    )
                    .await
                    .map_err(sql_error)?;
                if legacy.next().await.map_err(sql_error)?.is_some() {
                    return Err(ActionStoreError::CorruptRecord);
                }
                drop(legacy);
                for (_, statement) in ACTIONS_TABLES {
                    transaction
                        .execute(statement, ())
                        .await
                        .map_err(sql_error)?;
                }
                for (_, statement) in ACTIONS_INDEXES {
                    transaction
                        .execute(statement, ())
                        .await
                        .map_err(sql_error)?;
                }
                transaction
                    .execute(
                        "INSERT INTO actions_schema (id, version) VALUES (1, ?)",
                        [ACTIONS_SCHEMA_VERSION],
                    )
                    .await
                    .map_err(sql_error)?;
                self.write_authority(&transaction,&ActionsAuthority::default_for(self.person_id),None).await?;
            }
            self.validate_actions_schema(&transaction).await
        }
        .await;
        self.finish_actions_transaction(transaction, result)
            .await
            .map_err(AgentFailure::from)
    }

    async fn validate_actions_schema(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), ActionStoreError> {
        let mut unexpected=transaction.query("SELECT name FROM sqlite_master WHERE type IN ('trigger','view') AND (name GLOB 'actions_*' OR tbl_name GLOB 'actions_*')",()).await.map_err(sql_error)?;
        if unexpected.next().await.map_err(sql_error)?.is_some(){return Err(ActionStoreError::CorruptRecord);}
        drop(unexpected);
        for name in OLD_ACTION_TABLES {
            let mut legacy = transaction
                .query(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
                    [name.to_string()],
                )
                .await
                .map_err(sql_error)?;
            if legacy.next().await.map_err(sql_error)?.is_some() {
                return Err(ActionStoreError::CorruptRecord);
            }
        }
        let mut expected = BTreeMap::new();
        for (name, statement) in ACTIONS_TABLES {
            expected.insert((*name).to_owned(), normalize_schema_sql(statement));
        }
        for (name, statement) in ACTIONS_INDEXES {
            expected.insert((*name).to_owned(), normalize_schema_sql(statement));
        }
        let mut rows = transaction
            .query(
                "SELECT name, sql FROM sqlite_master WHERE name GLOB 'actions_*' AND type IN ('table', 'index') ORDER BY name",
                (),
            )
            .await
            .map_err(sql_error)?;
        let mut actual = BTreeMap::new();
        while let Some(row) = rows.next().await.map_err(sql_error)? {
            let name = row
                .get::<String>(0)
                .map_err(|_| ActionStoreError::CorruptRecord)?;
            let sql = row
                .get::<String>(1)
                .map_err(|_| ActionStoreError::CorruptRecord)?;
            actual.insert(name, normalize_schema_sql(&sql));
        }
        drop(rows);
        if actual != expected {
            return Err(ActionStoreError::CorruptRecord);
        }
        let mut marker = transaction
            .query("SELECT id, version FROM actions_schema", ())
            .await
            .map_err(sql_error)?;
        let row = marker
            .next()
            .await
            .map_err(sql_error)?
            .ok_or(ActionStoreError::CorruptRecord)?;
        if row.get::<i64>(0).map_err(|_| ActionStoreError::CorruptRecord)? != 1
            || row.get::<i64>(1).map_err(|_| ActionStoreError::CorruptRecord)?
                != ACTIONS_SCHEMA_VERSION
            || marker.next().await.map_err(sql_error)?.is_some()
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        drop(marker);
        self.authority_in_transaction(transaction,self.person_id).await?;
        Ok(())
    }

    async fn finish_actions_transaction<T>(
        &self,
        transaction: Transaction<'_>,
        result: Result<T, ActionStoreError>,
    ) -> Result<T, ActionStoreError> {
        match result {
            Ok(value) => {
                if let Err(error) = self.check_access() {
                    if transaction.rollback().await.is_err() {
                        self.unavailable.store(true, Ordering::Release);
                        return Err(ActionStoreError::Unavailable);
                    }
                    return Err(access_error(error));
                }
                if transaction.commit().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(ActionStoreError::Unavailable);
                }
                self.check_access().map_err(access_error)?;
                Ok(value)
            }
            Err(error) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(ActionStoreError::Unavailable);
                }
                Err(error)
            }
        }
    }

    async fn ensure_actions_schema(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), ActionStoreError> {
        self.validate_actions_schema(transaction).await
    }

    async fn action_by_id(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        action_id: Uuid,
    ) -> Result<Option<StoredAction>, ActionStoreError> {
        let query = format!("{ACTION_SELECT} WHERE person_id = ? AND action_id = ?");
        let mut rows = transaction
            .query(&query, (person_id.to_string(), action_id.to_string()))
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            return Ok(None);
        };
        let action = stored_action_from_row!(row)?;
        if action.record.person_id != person_id || action.record.id != action_id {
            return Err(ActionStoreError::CorruptRecord);
        }
        if rows.next().await.map_err(sql_error)?.is_some() {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(Some(action))
    }

    async fn action_by_execution(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> Result<Option<StoredAction>, ActionStoreError> {
        let query = format!("{ACTION_SELECT} WHERE person_id = ? AND execution_id = ?");
        let mut rows = transaction
            .query(&query, (person_id.to_string(), execution_id.to_string()))
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            return Ok(None);
        };
        let action = stored_action_from_row!(row)?;
        if action.record.person_id != person_id || action.record.execution_id != execution_id {
            return Err(ActionStoreError::CorruptRecord);
        }
        if rows.next().await.map_err(sql_error)?.is_some() {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(Some(action))
    }

    async fn pending_expert_count(&self,transaction:&Transaction<'_>)->Result<usize,ActionStoreError>{
        let mut rows=transaction.query("SELECT count(*) FROM actions_records WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved')",(self.person_id.to_string(),)).await.map_err(sql_error)?;
        let row=rows.next().await.map_err(sql_error)?.ok_or(ActionStoreError::CorruptRecord)?;
        let count=usize::try_from(row.get::<i64>(0).map_err(|_|ActionStoreError::CorruptRecord)?).map_err(|_|ActionStoreError::CorruptRecord)?;
        if count>floe_actions::MAX_PENDING_EXPERT_ACTIONS{return Err(ActionStoreError::BudgetExceeded);}
        Ok(count)
    }

    async fn insert_action(
        &self,
        transaction: &Transaction<'_>,
        record: &ActionRecord,
        dispatch_revision: Option<u64>,
    ) -> Result<(), ActionStoreError> {
        if record.person_id != self.person_id || dispatch_revision.is_some() {
            return Err(ActionStoreError::InvalidRecord);
        }
        if matches!(record.origin,ActionOrigin::Expert{..}) && matches!(record.state,ActionState::PendingReview|ActionState::Approved)
            && self.pending_expert_count(transaction).await? >= floe_actions::MAX_PENDING_EXPERT_ACTIONS {
            return Err(ActionStoreError::BudgetExceeded);
        }
        let payload = record_payload(record)?;
        let changed = transaction
            .execute(
                "INSERT INTO actions_records (person_id, action_id, revision, effect_digest, execution_id, state, collection_state, dispatch_revision, grant_key, payload, origin_kind) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)",
                (
                    record.person_id.to_string(),
                    record.id.to_string(),
                    to_i64(record.revision)?,
                    digest_hex(&record.effect_digest),
                    record.execution_id.to_string(),
                    action_state_name(&record.state),
                    action_collection_state_name(record),
                    action_grant_key(record),
                    payload,
                    action_origin_name(&record.origin),
                ),
            )
            .await
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(ActionStoreError::Conflict);
        }
        Ok(())
    }

    async fn update_action(
        &self,
        transaction: &Transaction<'_>,
        current: &StoredAction,
        next: &ActionRecord,
        dispatch_revision: Option<u64>,
    ) -> Result<(), ActionStoreError> {
        if current.record.person_id != self.person_id
            || next.person_id != current.record.person_id
            || next.id != current.record.id
            || next.execution_id != current.record.execution_id
            || current.record.revision.checked_add(1)!=Some(next.revision)
            || next.device_id!=current.record.device_id || next.origin!=current.record.origin
            || next.effect!=current.record.effect || next.effect_digest!=current.record.effect_digest
            || next.source!=current.record.source || next.dependency!=current.record.dependency
            || next.review!=current.record.review || next.created_at!=current.record.created_at || next.expires_at!=current.record.expires_at
            || current.record.execution.as_ref().is_some_and(|intent|next.execution.as_ref()!=Some(intent))
        {
            return Err(ActionStoreError::InvalidRecord);
        }
        if next.authorization!=current.record.authorization && !(current.record.authorization.is_none()
            && current.record.state==ActionState::PendingReview && next.state==ActionState::Approved
            && matches!(next.authorization,Some(floe_actions::ActionAuthorization::ReviewedDecision{..}))) {
            return Err(ActionStoreError::InvalidRecord);
        }
        match (current.dispatch_revision,dispatch_revision) {
            (Some(before),Some(after)) if before==after=>{},
            (None,None) if next.execution.is_none()=>{},
            (None,Some(revision)) if revision==current.record.revision && current.record.state==ActionState::Approved
                && matches!(next.state,ActionState::Executing{..}) && next.execution.is_some()=>{},
            _=>return Err(ActionStoreError::InvalidRecord),
        }
        let payload = record_payload(next)?;
        let changed = transaction
            .execute(
                "UPDATE actions_records SET revision = ?, effect_digest = ?, state = ?, collection_state = ?, dispatch_revision = ?, grant_key = ?, payload = ?, origin_kind = ? WHERE person_id = ? AND action_id = ? AND revision = ? AND effect_digest = ? AND execution_id = ? AND dispatch_revision = ?",
                (
                    to_i64(next.revision)?,
                    digest_hex(&next.effect_digest),
                    action_state_name(&next.state),
                    action_collection_state_name(next),
                    to_i64(dispatch_revision.unwrap_or(0))?,
                    action_grant_key(next),
                    payload,
                    action_origin_name(&next.origin),
                    current.record.person_id.to_string(),
                    current.record.id.to_string(),
                    to_i64(current.record.revision)?,
                    digest_hex(&current.record.effect_digest),
                    current.record.execution_id.to_string(),
                    to_i64(current.dispatch_revision.unwrap_or(0))?,
                ),
            )
            .await
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(ActionStoreError::Conflict);
        }
        Ok(())
    }

    async fn command_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        command_id: Uuid,
    ) -> Result<Option<CommandReceipt>, ActionStoreError> {
        let mut rows = transaction
            .query(
                "SELECT kind, intent_digest, action_id FROM actions_command_receipts WHERE person_id = ? AND command_id = ?",
                (person_id.to_string(), command_id.to_string()),
            )
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            return Ok(None);
        };
        let receipt = CommandReceipt {
            kind: row
                .get::<String>(0)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
            intent_digest: row
                .get::<String>(1)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
            action_id: row
                .get::<String>(2)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
        };
        let is_authority = receipt.kind == "authority";
        if !matches!(
            receipt.kind.as_str(),
            "submit" | "decision" | "reconciliation" | "authority"
        ) || !digest_text_is_valid(&receipt.intent_digest)
            || (is_authority && !receipt.action_id.is_empty())
            || (!is_authority && !canonical_uuid(&receipt.action_id))
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(Some(receipt))
    }

    async fn insert_command_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        command_id: Uuid,
        kind: &'static str,
        digest: &ActionDigest,
        action_id: Option<Uuid>,
    ) -> Result<(), ActionStoreError> {
        if command_id.is_nil() || person_id != self.person_id {
            return Err(ActionStoreError::InvalidRecord);
        }
        let action_id = action_id.map(|id| id.to_string()).unwrap_or_default();
        let changed = transaction
            .execute(
                "INSERT INTO actions_command_receipts (person_id, command_id, kind, intent_digest, action_id) VALUES (?, ?, ?, ?, ?)",
                (
                    person_id.to_string(),
                    command_id.to_string(),
                    kind,
                    digest_hex(digest),
                    action_id,
                ),
            )
            .await
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(ActionStoreError::Conflict);
        }
        Ok(())
    }

    async fn record_for_command_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        receipt: &CommandReceipt,
    ) -> Result<ActionRecord, ActionStoreError> {
        let action_id = parse_canonical_uuid(&receipt.action_id)?;
        self.action_by_id(transaction, person_id, action_id)
            .await?
            .map(|stored| stored.record)
            .ok_or(ActionStoreError::CorruptRecord)
    }

    async fn authority_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
    ) -> Result<ActionsAuthority, ActionStoreError> {
        let mut rows = transaction
            .query(
                "SELECT person_id, revision, mode, digest, length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 4096 THEN payload ELSE '' END FROM actions_authorities WHERE person_id = ?",
                [person_id.to_string()],
            )
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            // The explicit creator commits the initial Ask policy with schema.
            // An absent policy in a published Vault is lost authority, not Ask.
            return Err(ActionStoreError::CorruptRecord);
        };
        let row_person = row
            .get::<String>(0)
            .map_err(|_| ActionStoreError::CorruptRecord)?;
        let revision = row
            .get::<i64>(1)
            .map_err(|_| ActionStoreError::CorruptRecord)?;
        let mode = row
            .get::<String>(2)
            .map_err(|_| ActionStoreError::CorruptRecord)?;
        let digest = row
            .get::<String>(3)
            .map_err(|_| ActionStoreError::CorruptRecord)?;
        let payload_bytes = row
            .get::<i64>(4)
            .map_err(|_| ActionStoreError::CorruptRecord)?;
        let payload = row
            .get::<String>(5)
            .map_err(|_| ActionStoreError::CorruptRecord)?;
        if row_person != person_id.to_string()
            || payload_bytes <= 0
            || payload_bytes > 4096
            || usize::try_from(payload_bytes).ok() != Some(payload.len())
            || !digest_text_is_valid(&digest)
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        let authority: ActionsAuthority =
            serde_json::from_str(&payload).map_err(|_| ActionStoreError::CorruptRecord)?;
        if authority.person_id != person_id
            || u64::try_from(revision).ok() != Some(authority.revision)
            || authority.revision == 0
            || mode != authority_mode_name(authority.calendar_create)
            || digest != digest_hex(&authority_digest(&authority)?)
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(authority)
    }

    async fn authority_row_exists(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
    ) -> Result<bool, ActionStoreError> {
        let mut rows = transaction
            .query(
                "SELECT person_id FROM actions_authorities WHERE person_id = ?",
                [person_id.to_string()],
            )
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            return Ok(false);
        };
        if row
            .get::<String>(0)
            .map_err(|_| ActionStoreError::CorruptRecord)?
            != person_id.to_string()
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(true)
    }

    async fn write_authority(
        &self,
        transaction: &Transaction<'_>,
        authority: &ActionsAuthority,
        expected_revision: Option<u64>,
    ) -> Result<(), ActionStoreError> {
        if authority.person_id != self.person_id || authority.revision == 0 {
            return Err(ActionStoreError::InvalidRecord);
        }
        let payload = serde_json::to_string(authority).map_err(|_| ActionStoreError::InvalidRecord)?;
        if payload.is_empty() || payload.len() > 4096 {
            return Err(ActionStoreError::InvalidRecord);
        }
        let digest = digest_hex(&authority_digest(authority)?);
        let changed = if let Some(expected_revision) = expected_revision {
            transaction
                .execute(
                    "UPDATE actions_authorities SET revision = ?, mode = ?, digest = ?, payload = ? WHERE person_id = ? AND revision = ?",
                    (
                        to_i64(authority.revision)?,
                        authority_mode_name(authority.calendar_create),
                        digest,
                        payload,
                        authority.person_id.to_string(),
                        to_i64(expected_revision)?,
                    ),
                )
                .await
                .map_err(sql_error)?
        } else {
            transaction
                .execute(
                    "INSERT INTO actions_authorities (person_id, revision, mode, digest, payload) VALUES (?, ?, ?, ?, ?)",
                    (
                        authority.person_id.to_string(),
                        to_i64(authority.revision)?,
                        authority_mode_name(authority.calendar_create),
                        digest,
                        payload,
                    ),
                )
                .await
                .map_err(sql_error)?
        };
        if changed != 1 {
            return Err(ActionStoreError::Conflict);
        }
        Ok(())
    }

    async fn validate_current_action_coverage(
        &self,
        transaction: &Transaction<'_>,
        coverage: &DependencyCoverage,
    ) -> Result<(), ActionStoreError> {
        coverage.validate().map_err(|_|ActionStoreError::InvalidRecord)?;
        if !matches!(coverage,DependencyCoverage::Dependent{..}){return Err(ActionStoreError::InvalidRecord);}
        self.validate_context_dependency_coverage_in_transaction(transaction, coverage)
            .await
            .map_err(|error| match error {
                AgentFailure::Conflict => ActionStoreError::Conflict,
                AgentFailure::StorageUnavailable => ActionStoreError::Unavailable,
                AgentFailure::VaultLocked => ActionStoreError::VaultLocked,
                AgentFailure::VaultUnavailable => ActionStoreError::Unavailable,
                AgentFailure::UnsupportedVersion => {
                    ActionStoreError::CorruptRecord
                }
                AgentFailure::PolicyDenied | AgentFailure::NotFound => ActionStoreError::Conflict,
                _ => ActionStoreError::InvalidRecord,
            })
    }

    async fn invalidate_pending_actions_for_policy(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), ActionStoreError> {
        let mut cursor: Option<String> = None;
        loop {
            let query = if cursor.is_some() {
                format!("{ACTION_SELECT} WHERE person_id = ? AND state IN ('pending_review', 'approved') AND action_id > ? ORDER BY action_id COLLATE BINARY LIMIT 100")
            } else {
                format!("{ACTION_SELECT} WHERE person_id = ? AND state IN ('pending_review', 'approved') ORDER BY action_id COLLATE BINARY LIMIT 100")
            };
            let mut rows = if let Some(cursor) = cursor.as_ref() {
                transaction
                    .query(&query, (self.person_id.to_string(), cursor.clone()))
                    .await
                    .map_err(sql_error)?
            } else {
                transaction
                    .query(&query, [self.person_id.to_string()])
                    .await
                    .map_err(sql_error)?
            };
            let mut page = Vec::with_capacity(100);
            while let Some(row) = rows.next().await.map_err(sql_error)? {
                let stored = stored_action_from_row!(row)?;
                if stored.record.person_id != self.person_id
                    || !matches!(stored.record.state, ActionState::PendingReview | ActionState::Approved)
                    || stored.dispatch_revision.is_some()
                {
                    return Err(ActionStoreError::CorruptRecord);
                }
                page.push(stored);
            }
            drop(rows);
            let page_len = page.len();
            let Some(last) = page.last() else {
                return Ok(());
            };
            cursor = Some(last.record.id.to_string());
            for current in page {
                let Some(next) = invalidate_action_policy(&current.record).map_err(invalid_record)?
                else {
                    return Err(ActionStoreError::CorruptRecord);
                };
                self.update_action(transaction, &current, &next, current.dispatch_revision)
                    .await?;
            }
            if page_len < 100 {
                return Ok(());
            }
        }
    }

    pub(super) async fn invalidate_agent_actions_for_grant_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        grant_id: floe_context_contract::GrantId,
        authority: floe_context_contract::GrantAuthority,
    ) -> Result<(), AgentFailure> {
        self.ensure_actions_schema(transaction)
            .await
            .map_err(AgentFailure::from)?;
        self.pending_expert_count(transaction).await.map_err(AgentFailure::from)?;
        let mut scanned=0usize;
        let mut cursor: Option<String> = None;
        loop {
            // The Calendar index alone cannot cover inherited Task context.
            // Authenticate each pending Expert's immutable receipt instead.
            let query = if cursor.is_some() {
                format!("{ACTION_SELECT} WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved') AND action_id > ? ORDER BY action_id COLLATE BINARY LIMIT 100")
            } else {
                format!("{ACTION_SELECT} WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved') ORDER BY action_id COLLATE BINARY LIMIT 100")
            };
            let mut rows = if let Some(cursor) = cursor.as_ref() {
                transaction.query(&query,(self.person_id.to_string(),cursor.clone())).await
            } else {
                transaction.query(&query,(self.person_id.to_string(),)).await
            }.map_err(sql_error).map_err(AgentFailure::from)?;
            let mut page = Vec::with_capacity(100);
            while let Some(row) = rows.next().await.map_err(sql_error).map_err(AgentFailure::from)? {
                let stored = stored_action_from_row!(row).map_err(AgentFailure::from)?;
                if stored.record.person_id != self.person_id
                    || !matches!(stored.record.origin,ActionOrigin::Expert{..})
                    || !matches!(stored.record.state, ActionState::PendingReview | ActionState::Approved)
                    || stored.dispatch_revision.is_some()
                { return Err(AgentFailure::from(ActionStoreError::CorruptRecord)); }
                page.push(stored);
            }
            drop(rows);
            let page_len = page.len();
            scanned=scanned.checked_add(page_len).ok_or(AgentFailure::BudgetExceeded)?;
            if scanned>floe_actions::MAX_PENDING_EXPERT_ACTIONS{return Err(AgentFailure::BudgetExceeded);}
            let Some(last) = page.last() else { return Ok(()); };
            cursor = Some(last.record.id.to_string());
            for current in page {
                let evidence=self.expert_action_evidence_in_transaction(transaction,&current.record).await
                    .map_err(AgentFailure::from)?.ok_or(AgentFailure::StorageUnavailable)?;
                let Some(next)=invalidate_action_dependency(&current.record,&evidence,grant_id,authority)
                    .map_err(invalid_record).map_err(AgentFailure::from)? else {continue;};
                self.update_action(transaction,&current,&next,current.dispatch_revision).await.map_err(AgentFailure::from)?;
            }
            if page_len < 100 { return Ok(()); }
        }
    }

    async fn validate_expert_task_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        record: &ActionRecord,
    ) -> Result<(), ActionStoreError> {
        if let Some(evidence)=self.expert_action_evidence_in_transaction(transaction,record).await? {
            self.validate_current_action_coverage(transaction,&evidence.coverage).await?;
        }
        Ok(())
    }

    /// Historical receipt authentication is also usable after a grant is
    /// revoked; live grant checks belong only to new admission/dispatch.
    async fn expert_action_evidence_in_transaction(
        &self,
        transaction:&Transaction<'_>,
        record:&ActionRecord,
    )->Result<Option<floe_actions::ExpertProposalEvidence>,ActionStoreError>{
        let ActionOrigin::Expert { task_id,evidence_ref,artifact_id,.. } = &record.origin else {
            return Ok(None);
        };
        let task_id = floe_agent_contract::TaskId::from_uuid(*task_id)
            .ok_or(ActionStoreError::InvalidRecord)?;
        let task = self
            .task_on(transaction, task_id)
            .await
            .map_err(historical_action_error)?
            .ok_or(ActionStoreError::CorruptRecord)?;
        let receipt=self.read_execution_receipt_on(transaction,evidence_ref).await.map_err(historical_action_error)?;
        if task.snapshot.task_id != task_id
            || task.snapshot.principal != self.person_id.to_string()
            || task.snapshot.state != floe_agent_contract::TaskState::Completed
            || task.receipt.as_ref()!=Some(&receipt)
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        let evidence=super::expert_actions::decode_task_proposal(&task,evidence_ref,*artifact_id,record.person_id,&record.device_id).map_err(historical_action_error)?;
        floe_actions::validate_expert_action_evidence(record,&evidence).map_err(historical_action_error)?;
        Ok(Some(evidence))
    }

    async fn settlement_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        execution_id: Uuid,
        expected_revision: u64,
    ) -> Result<Option<SettlementReceipt>, ActionStoreError> {
        let mut rows = transaction
            .query(
                "SELECT action_id, effect_digest, outcome_digest FROM actions_settlement_receipts WHERE person_id = ? AND execution_id = ? AND expected_revision = ?",
                (
                    person_id.to_string(),
                    execution_id.to_string(),
                    to_i64(expected_revision)?,
                ),
            )
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            return Ok(None);
        };
        let receipt = SettlementReceipt {
            action_id: row
                .get::<String>(0)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
            effect_digest: row
                .get::<String>(1)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
            outcome_digest: row
                .get::<String>(2)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
        };
        if !canonical_uuid(&receipt.action_id)
            || !digest_text_is_valid(&receipt.effect_digest)
            || !digest_text_is_valid(&receipt.outcome_digest)
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(Some(receipt))
    }

    async fn insert_settlement_receipt(
        &self,
        transaction: &Transaction<'_>,
        settlement: &ExecutionSettlement,
        action_id: Uuid,
        outcome_digest: &ActionDigest,
    ) -> Result<(), ActionStoreError> {
        let changed = transaction
            .execute(
                "INSERT INTO actions_settlement_receipts (person_id, execution_id, expected_revision, action_id, effect_digest, outcome_digest) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    settlement.person_id.to_string(),
                    settlement.execution_id.to_string(),
                    to_i64(settlement.expected_revision)?,
                    action_id.to_string(),
                    digest_hex(&settlement.effect_digest),
                    digest_hex(outcome_digest),
                ),
            )
            .await
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(ActionStoreError::Conflict);
        }
        Ok(())
    }

    async fn collection_receipt(
        &self,
        transaction: &Transaction<'_>,
        ack: &CollectionAck,
    ) -> Result<Option<CollectionReceipt>, ActionStoreError> {
        let mut rows = transaction
            .query(
                "SELECT action_id, intent_digest FROM actions_collection_receipts WHERE person_id = ? AND execution_id = ? AND receipt_digest = ? AND ticket_revision = ?",
                (
                    ack.person_id.to_string(),
                    ack.execution_id.to_string(),
                    digest_hex(&ack.receipt_digest),
                    to_i64(ack.expected_ticket_revision)?,
                ),
            )
            .await
            .map_err(sql_error)?;
        let Some(row) = rows.next().await.map_err(sql_error)? else {
            return Ok(None);
        };
        let receipt = CollectionReceipt {
            action_id: row
                .get::<String>(0)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
            intent_digest: row
                .get::<String>(1)
                .map_err(|_| ActionStoreError::CorruptRecord)?,
        };
        if !canonical_uuid(&receipt.action_id)
            || !digest_text_is_valid(&receipt.intent_digest)
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(ActionStoreError::CorruptRecord);
        }
        Ok(Some(receipt))
    }

    async fn insert_collection_receipt(
        &self,
        transaction: &Transaction<'_>,
        ack: &CollectionAck,
        action_id: Uuid,
        intent_digest: &ActionDigest,
    ) -> Result<(), ActionStoreError> {
        let changed = transaction
            .execute(
                "INSERT INTO actions_collection_receipts (person_id, execution_id, receipt_digest, ticket_revision, action_id, intent_digest) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    ack.person_id.to_string(),
                    ack.execution_id.to_string(),
                    digest_hex(&ack.receipt_digest),
                    to_i64(ack.expected_ticket_revision)?,
                    action_id.to_string(),
                    digest_hex(intent_digest),
                ),
            )
            .await
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(ActionStoreError::Conflict);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct CommandReceipt {
    kind: String,
    intent_digest: String,
    action_id: String,
}

#[derive(Clone, Debug)]
struct SettlementReceipt {
    action_id: String,
    effect_digest: String,
    outcome_digest: String,
}

#[derive(Clone, Debug)]
struct CollectionReceipt {
    action_id: String,
    intent_digest: String,
}

fn to_i64(value: u64) -> Result<i64, ActionStoreError> {
    i64::try_from(value).map_err(|_| ActionStoreError::InvalidRecord)
}

fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value)
}

fn parse_canonical_uuid(value: &str) -> Result<Uuid, ActionStoreError> {
    let uuid = Uuid::parse_str(value).map_err(|_| ActionStoreError::CorruptRecord)?;
    if uuid.to_string() != value || uuid.is_nil() {
        return Err(ActionStoreError::CorruptRecord);
    }
    Ok(uuid)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    async fn replay_command_action(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        command_id: Uuid,
        kind: &'static str,
        digest: &ActionDigest,
    ) -> Result<Option<ActionRecord>, ActionStoreError> {
        let Some(receipt) = self
            .command_receipt(transaction, person_id, command_id)
            .await?
        else {
            return Ok(None);
        };
        if receipt.kind != kind || receipt.intent_digest != digest_hex(digest) {
            return Err(ActionStoreError::Conflict);
        }
        self.record_for_command_receipt(transaction, person_id, &receipt)
            .await
            .map(Some)
    }

    pub(crate) async fn actions_get(
        &self,
        person_id: PersonId,
        action_id: Uuid,
    ) -> Result<Option<ActionRecord>, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if person_id != self.person_id || action_id.is_nil() {
                return Ok(None);
            }
            Ok(self
                .action_by_id(&transaction, person_id, action_id)
                .await?
                .map(|stored| stored.record))
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_list(
        &self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<ActionPage, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if !(1..=100).contains(&limit) {
                return Err(ActionStoreError::InvalidRecord);
            }
            if person_id != self.person_id {
                return Ok(ActionPage {
                    records: Vec::new(),
                    next_cursor: None,
                });
            }
            let fetch_limit = i64::from(limit) + 1;
            let query = if cursor.is_some() {
                format!("{ACTION_SELECT} WHERE person_id = ? AND action_id > ? ORDER BY action_id COLLATE BINARY LIMIT ?")
            } else {
                format!("{ACTION_SELECT} WHERE person_id = ? ORDER BY action_id COLLATE BINARY LIMIT ?")
            };
            let mut rows = if let Some(cursor) = cursor {
                transaction
                    .query(
                        &query,
                        (
                            person_id.to_string(),
                            cursor.to_string(),
                            fetch_limit,
                        ),
                    )
                    .await
                    .map_err(sql_error)?
            } else {
                transaction
                    .query(&query, (person_id.to_string(), fetch_limit))
                    .await
                    .map_err(sql_error)?
            };
            let mut records = Vec::with_capacity(usize::from(limit) + 1);
            while let Some(row) = rows.next().await.map_err(sql_error)? {
                let stored = stored_action_from_row!(row)?;
                if stored.record.person_id != person_id
                    || cursor.is_some_and(|value| stored.record.id <= value)
                {
                    return Err(ActionStoreError::CorruptRecord);
                }
                records.push(stored.record);
            }
            let has_more = records.len() > usize::from(limit);
            if has_more {
                records.truncate(usize::from(limit));
            }
            let next_cursor = if has_more {
                records.last().map(|record| record.id)
            } else {
                None
            };
            Ok(ActionPage {
                records,
                next_cursor,
            })
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_find_admission(
        &self,
        person_id: PersonId,
        command_id: Uuid,
        request_digest: ActionDigest,
    ) -> Result<Option<ActionRecord>, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if person_id != self.person_id {
                return Ok(None);
            }
            if command_id.is_nil() {
                return Err(ActionStoreError::InvalidRecord);
            }
            self.replay_command_action(
                &transaction,
                person_id,
                command_id,
                "submit",
                &request_digest,
            )
            .await
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_admit(
        &self,
        admission: ActionAdmission,
    ) -> Result<AdmittedAction, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if admission.command_id.is_nil() || admission.record.person_id != self.person_id {
                return Err(ActionStoreError::InvalidRecord);
            }
            if let Some(record) = self
                .replay_command_action(
                    &transaction,
                    self.person_id,
                    admission.command_id,
                    "submit",
                    &admission.request_digest,
                )
                .await?
            {
                return Ok(AdmittedAction {
                    record,
                    replayed: true,
                });
            }
            admission
                .record
                .validate()
                .map_err(|_| ActionStoreError::InvalidRecord)?;
            // The original submit command is the sole replay authority. A new
            // command for an already retained proposal remains a conflict,
            // including when pending admission capacity is exhausted.
            if self.action_by_id(&transaction,self.person_id,admission.record.id).await?.is_some(){
                return Err(ActionStoreError::Conflict);
            }
            let authority = self
                .authority_in_transaction(&transaction, self.person_id)
                .await?;
            validate_action_admission(&admission, &authority)
                .map_err(invalid_record)?;
            self.validate_expert_task_in_transaction(&transaction, &admission.record)
                .await?;
            self.insert_action(&transaction, &admission.record, None)
                .await?;
            self.insert_command_receipt(
                &transaction,
                self.person_id,
                admission.command_id,
                "submit",
                &admission.request_digest,
                Some(admission.record.id),
            )
            .await?;
            Ok(AdmittedAction {
                record: admission.record,
                replayed: false,
            })
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_record_decision(
        &self,
        decision: ActionDecision,
    ) -> Result<ActionRecord, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            let digest = decision_intent_digest(&decision).map_err(invalid_record)?;
            if decision.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            if let Some(record) = self
                .replay_command_action(
                    &transaction,
                    self.person_id,
                    decision.command_id,
                    "decision",
                    &digest,
                )
                .await?
            {
                if record.id != decision.action_id {
                    return Err(ActionStoreError::CorruptRecord);
                }
                return Ok(record);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, decision.action_id)
                .await?
                .ok_or(ActionStoreError::NotFound)?;
            let authority = self
                .authority_in_transaction(&transaction, self.person_id)
                .await?;
            let next = floe_actions::decide_action(&current.record, &decision, &authority)
                .map_err(invalid_record)?;
            self.update_action(
                &transaction,
                &current,
                &next,
                current.dispatch_revision,
            )
            .await?;
            self.insert_command_receipt(
                &transaction,
                self.person_id,
                decision.command_id,
                "decision",
                &digest,
                Some(next.id),
            )
            .await?;
            Ok(next)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_admit_reconciliation(
        &self,
        command: ActionReconciliation,
    ) -> Result<ActionRecord, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            let digest = command_digest(b"floe.actions.reconciliation.v1\0", &command)?;
            if command.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            if let Some(record) = self
                .replay_command_action(
                    &transaction,
                    self.person_id,
                    command.command_id,
                    "reconciliation",
                    &digest,
                )
                .await?
            {
                if record.id != command.action_id {
                    return Err(ActionStoreError::CorruptRecord);
                }
                return Ok(record);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, command.action_id)
                .await?
                .ok_or(ActionStoreError::NotFound)?;
            let collection_pending = matches!(
                &current.record.state,
                ActionState::Succeeded {
                    collection: floe_actions::ActionCollectionState::Pending { .. },
                    ..
                }
            );
            if current.record.person_id != command.person_id
                || current.record.device_id != command.device_id
                || current.record.revision != command.expected_revision
                || !matches!(
                    current.record.state,
                    ActionState::Executing { .. } | ActionState::Unknown { .. }
                ) && !collection_pending
            {
                return Err(ActionStoreError::Conflict);
            }
            self.insert_command_receipt(
                &transaction,
                self.person_id,
                command.command_id,
                "reconciliation",
                &digest,
                Some(current.record.id),
            )
            .await?;
            Ok(current.record)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_stop_before_dispatch(
        &self,
        stop: PreDispatchStop,
    ) -> Result<ActionRecord, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if stop.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, stop.action_id)
                .await?
                .ok_or(ActionStoreError::NotFound)?;
            let next = stop_action(&current.record, &stop).map_err(invalid_record)?;
            self.update_action(
                &transaction,
                &current,
                &next,
                current.dispatch_revision,
            )
            .await?;
            Ok(next)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_prepare_dispatch(
        &self,
        request: DispatchIntent,
    ) -> Result<DispatchAdmission, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if request.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, request.action_id)
                .await?
                .ok_or(ActionStoreError::NotFound)?;
            if let Some(existing) = current.record.execution.as_ref() {
                if current.dispatch_revision != Some(request.expected_revision)
                    || existing.action_id != request.action_id
                    || existing.person_id != request.person_id
                    || existing.device_id != request.device_id
                    || existing.execution_id != request.execution_id
                    || existing.effect_digest != request.effect_digest
                    || existing.authorization != request.authorization
                    || existing.source != request.current_source_fence
                    || existing.executor_generation != request.executor_generation
                {
                    return Err(ActionStoreError::Conflict);
                }
                return Ok(DispatchAdmission {
                    intent: existing.clone(),
                    record_revision: current.record.revision,
                    dispatch_required: false,
                });
            }
            if current.dispatch_revision.is_some() {
                return Err(ActionStoreError::CorruptRecord);
            }
            self.validate_expert_task_in_transaction(&transaction, &current.record)
                .await?;
            let authority = self
                .authority_in_transaction(&transaction, self.person_id)
                .await?;
            let (next, execution) =
                prepare_action_dispatch(&current.record, &request, &authority)
                    .map_err(invalid_record)?;
            self.update_action(
                &transaction,
                &current,
                &next,
                Some(request.expected_revision),
            )
            .await?;
            Ok(DispatchAdmission {
                intent: execution,
                record_revision: next.revision,
                dispatch_required: true,
            })
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_load_execution(
        &self,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> Result<Option<DispatchAdmission>, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if person_id != self.person_id {
                return Ok(None);
            }
            let Some(stored) = self
                .action_by_execution(&transaction, person_id, execution_id)
                .await?
            else {
                return Ok(None);
            };
            let Some(intent)=stored.record.execution.clone() else {
                if stored.dispatch_revision.is_some(){return Err(ActionStoreError::CorruptRecord);}
                return Ok(None);
            };
            if stored.dispatch_revision.is_none()
                || intent.execution_id != execution_id
                || intent.person_id != person_id
            {
                return Err(ActionStoreError::CorruptRecord);
            }
            Ok(Some(DispatchAdmission {
                intent,
                record_revision: stored.record.revision,
                dispatch_required: false,
            }))
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_settle_execution(
        &self,
        settlement: ExecutionSettlement,
    ) -> Result<ActionRecord, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if settlement.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            let outcome_digest =
                command_digest(b"floe.actions.settlement.v1\0", &settlement)?;
            if let Some(receipt) = self
                .settlement_receipt(
                    &transaction,
                    self.person_id,
                    settlement.execution_id,
                    settlement.expected_revision,
                )
                .await?
            {
                if receipt.effect_digest != digest_hex(&settlement.effect_digest)
                    || receipt.outcome_digest != digest_hex(&outcome_digest)
                {
                    return Err(ActionStoreError::Conflict);
                }
                let action_id = parse_canonical_uuid(&receipt.action_id)?;
                let current = self
                    .action_by_id(&transaction, self.person_id, action_id)
                    .await?
                    .ok_or(ActionStoreError::CorruptRecord)?;
                if current.record.execution_id != settlement.execution_id
                    || current.record.effect_digest != settlement.effect_digest
                {
                    return Err(ActionStoreError::CorruptRecord);
                }
                return Ok(current.record);
            }
            let current = self
                .action_by_execution(
                    &transaction,
                    self.person_id,
                    settlement.execution_id,
                )
                .await?
                .ok_or(ActionStoreError::NotFound)?;
            if current.record.effect_digest != settlement.effect_digest {
                return Err(ActionStoreError::Conflict);
            }
            let next = settle_action(&current.record, &settlement).map_err(invalid_record)?;
            self.update_action(
                &transaction,
                &current,
                &next,
                current.dispatch_revision,
            )
            .await?;
            self.insert_settlement_receipt(
                &transaction,
                &settlement,
                next.id,
                &outcome_digest,
            )
            .await?;
            Ok(next)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_pending_recovery(
        &self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<RecoveryPage, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if !(1..=100).contains(&limit) {
                return Err(ActionStoreError::InvalidRecord);
            }
            if person_id != self.person_id {
                return Ok(RecoveryPage {
                    records: Vec::new(),
                    next_cursor: None,
                });
            }
            let fetch_limit = i64::from(limit) + 1;
            let base = format!(
                "{ACTION_SELECT} WHERE person_id = ? AND (state IN ('executing', 'unknown') OR collection_state = 'pending')"
            );
            let query = if cursor.is_some() {
                format!("{base} AND action_id > ? ORDER BY action_id COLLATE BINARY LIMIT ?")
            } else {
                format!("{base} ORDER BY action_id COLLATE BINARY LIMIT ?")
            };
            let mut rows = if let Some(cursor) = cursor {
                transaction
                    .query(
                        &query,
                        (
                            person_id.to_string(),
                            cursor.to_string(),
                            fetch_limit,
                        ),
                    )
                    .await
                    .map_err(sql_error)?
            } else {
                transaction
                    .query(&query, (person_id.to_string(), fetch_limit))
                    .await
                    .map_err(sql_error)?
            };
            let mut records = Vec::with_capacity(usize::from(limit) + 1);
            while let Some(row) = rows.next().await.map_err(sql_error)? {
                let stored = stored_action_from_row!(row)?;
                if stored.record.person_id != person_id
                    || !stored.record.pending_recovery()
                    || cursor.is_some_and(|value| stored.record.id <= value)
                {
                    return Err(ActionStoreError::CorruptRecord);
                }
                records.push(stored.record);
            }
            let has_more = records.len() > usize::from(limit);
            if has_more {
                records.truncate(usize::from(limit));
            }
            let next_cursor = if has_more {
                records.last().map(|record| record.id)
            } else {
                None
            };
            Ok(RecoveryPage {
                records,
                next_cursor,
            })
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_ack_collection(
        &self,
        ack: CollectionAck,
    ) -> Result<CollectionTicket, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if ack.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            let intent_digest = command_digest(b"floe.actions.collection-ack.v1\0", &ack)?;
            if let Some(receipt) = self.collection_receipt(&transaction, &ack).await? {
                if receipt.intent_digest != digest_hex(&intent_digest) {
                    return Err(ActionStoreError::Conflict);
                }
                let action_id = parse_canonical_uuid(&receipt.action_id)?;
                let current = self
                    .action_by_execution(&transaction, self.person_id, ack.execution_id)
                    .await?
                    .ok_or(ActionStoreError::CorruptRecord)?;
                let ticket = current
                    .record
                    .collection
                    .as_ref()
                    .ok_or(ActionStoreError::CorruptRecord)?;
                let expected_next_revision = ack
                    .expected_ticket_revision
                    .checked_add(1)
                    .ok_or(ActionStoreError::CorruptRecord)?;
                if current.record.id != action_id
                    || ticket.receipt_digest != ack.receipt_digest
                    || ticket.revision != expected_next_revision
                    || !matches!(
                        &ticket.state,
                        floe_actions::ActionCollectionState::Collected { day_projection_ref }
                            if day_projection_ref == &ack.day_projection_ref
                    )
                {
                    return Err(ActionStoreError::CorruptRecord);
                }
                return Ok(ticket.clone());
            }
            let current = self
                .action_by_execution(&transaction, self.person_id, ack.execution_id)
                .await?
                .ok_or(ActionStoreError::NotFound)?;
            let next = acknowledge_action_collection(&current.record, &ack)
                .map_err(invalid_record)?;
            self.update_action(
                &transaction,
                &current,
                &next,
                current.dispatch_revision,
            )
            .await?;
            self.insert_collection_receipt(
                &transaction,
                &ack,
                next.id,
                &intent_digest,
            )
            .await?;
            next.collection
                .ok_or(ActionStoreError::CorruptRecord)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_read_authority(
        &self,
        person_id: PersonId,
    ) -> Result<ActionsAuthority, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            self.authority_in_transaction(&transaction, person_id).await
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_compare_and_set_authority(
        &self,
        change: AuthorityChange,
    ) -> Result<ActionsAuthority, ActionStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if change.person_id != self.person_id {
                return Err(ActionStoreError::NotFound);
            }
            if change.command_id.is_nil() {
                return Err(ActionStoreError::InvalidRecord);
            }
            let intent_digest = command_digest(b"floe.actions.authority-change.v1\0", &change)?;
            if let Some(receipt) = self
                .command_receipt(&transaction, self.person_id, change.command_id)
                .await?
            {
                if receipt.kind != "authority"
                    || receipt.intent_digest != digest_hex(&intent_digest)
                {
                    return Err(ActionStoreError::Conflict);
                }
                if !self
                    .authority_row_exists(&transaction, self.person_id)
                    .await?
                {
                    return Err(ActionStoreError::CorruptRecord);
                }
                return self
                    .authority_in_transaction(&transaction, self.person_id)
                    .await;
            }
            let current = self
                .authority_in_transaction(&transaction, self.person_id)
                .await?;
            let stored = self
                .authority_row_exists(&transaction, self.person_id)
                .await?;
            let next = change_action_authority(&current, &change).map_err(invalid_record)?;
            self.write_authority(
                &transaction,
                &next,
                stored.then_some(current.revision),
            )
            .await?;
            self.insert_command_receipt(
                &transaction,
                self.person_id,
                change.command_id,
                "authority",
                &intent_digest,
                None,
            )
            .await?;
            if next.revision!=current.revision {
                self.invalidate_pending_actions_for_policy(&transaction).await?;
            }
            Ok(next)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }
}
