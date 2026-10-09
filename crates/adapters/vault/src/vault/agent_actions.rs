use std::sync::atomic::Ordering;

use floe_access::{
    DependencyCoverage, OperationAuthorizationPolicy, OperationPolicyChange, OperationPolicyMode,
    change_operation_policy,
};
use floe_agent_contract::AgentFailure;
use floe_calendar_operations::{
    ActionDecision, ActionDigest, ActionOrigin, ActionPage, ActionReconciliation, ActionRecord,
    ActionState, AdmittedOperation, CalendarOperationStoreError, CollectionAck, CollectionTicket,
    DispatchAdmission, DispatchIntent, ExecutionSettlement, OperationAdmission, PreDispatchStop,
    RecoveryPage, acknowledge_action_collection, action_digest, decision_intent_digest,
    invalidate_action_dependency, invalidate_action_policy, prepare_action_dispatch, settle_action,
    stop_action, validate_action_admission,
};
use floe_kernel::{CommandFailure, PersonId};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

const ACTION_SELECT: &str = "SELECT person_id, action_id, revision, effect_digest, execution_id, state, collection_state, dispatch_revision, grant_key, length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE '' END, origin_kind FROM actions_records";

#[derive(Clone, Debug)]
struct StoredAction {
    record: ActionRecord,
    dispatch_revision: Option<u64>,
}

macro_rules! stored_action_from_row {
    ($row:expr) => {{
        let row = $row;
        decode_action_columns(
            row.get::<String>(0)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(1)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<i64>(2)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(3)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(4)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(5)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(6)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<i64>(7)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(8)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<i64>(9)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(10)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            row.get::<String>(11)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
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
) -> Result<StoredAction, CalendarOperationStoreError> {
    if payload_bytes <= 0 || payload_bytes > floe_calendar_operations::MAX_ACTION_BYTES as i64 {
        return Err(CalendarOperationStoreError::CorruptRecord);
    }
    if payload.len()
        != usize::try_from(payload_bytes).map_err(|_| CalendarOperationStoreError::CorruptRecord)?
    {
        return Err(CalendarOperationStoreError::CorruptRecord);
    }
    let record: ActionRecord =
        serde_json::from_str(&payload).map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
    if record.validate().is_err()
        || serde_json::to_string(&record).map_err(|_| CalendarOperationStoreError::CorruptRecord)?
            != payload
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
        return Err(CalendarOperationStoreError::CorruptRecord);
    }
    let dispatch_revision =
        u64::try_from(dispatch_revision).map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
    let dispatch_revision = if dispatch_revision == 0 {
        None
    } else {
        Some(dispatch_revision)
    };
    match (&record.execution, dispatch_revision) {
        (None, None) => {}
        (Some(_), Some(revision)) if revision < record.revision => {}
        _ => return Err(CalendarOperationStoreError::CorruptRecord),
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
        Some(floe_calendar_operations::ActionCollectionState::Pending { .. }) => "pending",
        Some(floe_calendar_operations::ActionCollectionState::Collected { .. }) => "collected",
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

fn authority_mode_name(mode: OperationPolicyMode) -> &'static str {
    match mode {
        OperationPolicyMode::Allow => "allow",
        OperationPolicyMode::Ask => "ask",
        OperationPolicyMode::Deny => "deny",
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

fn invalid_record(error: AgentFailure) -> CalendarOperationStoreError {
    match error {
        AgentFailure::Conflict => CalendarOperationStoreError::Conflict,
        AgentFailure::NotFound => CalendarOperationStoreError::NotFound,
        AgentFailure::StorageUnavailable => CalendarOperationStoreError::Unavailable,
        AgentFailure::StorageBusy => CalendarOperationStoreError::StorageBusy,
        AgentFailure::VaultLocked => CalendarOperationStoreError::VaultLocked,
        AgentFailure::VaultUnavailable => CalendarOperationStoreError::Unavailable,
        AgentFailure::UnsupportedVersion => CalendarOperationStoreError::CorruptRecord,
        AgentFailure::BudgetExceeded => CalendarOperationStoreError::BudgetExceeded,
        AgentFailure::InvalidInput | AgentFailure::PolicyDenied => {
            CalendarOperationStoreError::InvalidRecord
        }
        _ => CalendarOperationStoreError::Unavailable,
    }
}

fn historical_action_error(error: AgentFailure) -> CalendarOperationStoreError {
    match error {
        AgentFailure::VaultLocked => CalendarOperationStoreError::VaultLocked,
        AgentFailure::StorageBusy => CalendarOperationStoreError::StorageBusy,
        AgentFailure::StorageUnavailable | AgentFailure::VaultUnavailable => {
            CalendarOperationStoreError::Unavailable
        }
        _ => CalendarOperationStoreError::CorruptRecord,
    }
}

fn access_error(error: AgentFailure) -> CalendarOperationStoreError {
    match error {
        AgentFailure::VaultLocked => CalendarOperationStoreError::VaultLocked,
        AgentFailure::VaultUnavailable => CalendarOperationStoreError::Unavailable,
        other => invalid_record(other),
    }
}

fn sql_error(error: turso::Error) -> CalendarOperationStoreError {
    match error {
        turso::Error::Constraint(_) => CalendarOperationStoreError::Conflict,
        other => match super::database_failure(other) {
            AgentFailure::StorageBusy => CalendarOperationStoreError::StorageBusy,
            _ => CalendarOperationStoreError::Unavailable,
        },
    }
}

fn record_payload(record: &ActionRecord) -> Result<String, CalendarOperationStoreError> {
    record
        .validate()
        .map_err(|_| CalendarOperationStoreError::InvalidRecord)?;
    let payload =
        serde_json::to_string(record).map_err(|_| CalendarOperationStoreError::InvalidRecord)?;
    if payload.is_empty() || payload.len() > floe_calendar_operations::MAX_ACTION_BYTES {
        return Err(CalendarOperationStoreError::InvalidRecord);
    }
    Ok(payload)
}

fn authority_digest(
    authority: &OperationAuthorizationPolicy,
) -> Result<ActionDigest, CalendarOperationStoreError> {
    action_digest(b"floe.actions.authority.v1\0", authority)
        .map_err(|_| CalendarOperationStoreError::InvalidRecord)
}

fn command_digest<T: serde::Serialize>(
    domain: &[u8],
    value: &T,
) -> Result<ActionDigest, CalendarOperationStoreError> {
    action_digest(domain, value).map_err(|_| CalendarOperationStoreError::InvalidRecord)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Existing-Vault open is read-only with respect to the Actions schema.
    /// Missing tables may represent lost uncertain effects, never a clean slate.
    pub(super) async fn validate_actions_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(super::database_failure)?;
        let result = async {
            self.validate_actions_schema(&transaction).await?;
            Ok(())
        }
        .await;
        self.finish_actions_transaction(transaction, result)
            .await
            .map_err(AgentFailure::from)
    }

    pub(super) async fn seed_actions_authority(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        self.write_authority(
            transaction,
            &OperationAuthorizationPolicy::default_for(self.person_id),
            None,
        )
        .await
        .map_err(AgentFailure::from)
    }

    async fn validate_actions_schema(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), CalendarOperationStoreError> {
        crate::schema::inspect_family(transaction, crate::schema::Family::Actions)
            .await
            .map_err(|failure| match failure {
                crate::schema::SchemaFailure::Unsupported { .. }
                | crate::schema::SchemaFailure::StoredCorrupt => {
                    CalendarOperationStoreError::CorruptRecord
                }
                crate::schema::SchemaFailure::Busy => CalendarOperationStoreError::StorageBusy,
                crate::schema::SchemaFailure::Unavailable
                | crate::schema::SchemaFailure::InvalidDefinition => {
                    CalendarOperationStoreError::Unavailable
                }
            })?;
        self.authority_in_transaction(transaction, self.person_id)
            .await?;
        Ok(())
    }

    async fn finish_actions_transaction<T>(
        &self,
        transaction: Transaction<'_>,
        result: Result<T, CalendarOperationStoreError>,
    ) -> Result<T, CalendarOperationStoreError> {
        match result {
            Ok(value) => {
                if let Err(error) = self.check_access() {
                    if transaction.rollback().await.is_err() {
                        self.unavailable.store(true, Ordering::Release);
                        return Err(CalendarOperationStoreError::Unavailable);
                    }
                    return Err(access_error(error));
                }
                if transaction.commit().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(CalendarOperationStoreError::Unavailable);
                }
                self.check_access().map_err(access_error)?;
                Ok(value)
            }
            Err(error) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(CalendarOperationStoreError::Unavailable);
                }
                Err(error)
            }
        }
    }

    pub(super) async fn finish_actions_command_transaction<T>(
        &self,
        transaction: Transaction<'_>,
        result: Result<T, CalendarOperationStoreError>,
        replay_checked: bool,
        prior_command: bool,
    ) -> Result<T, CommandFailure<CalendarOperationStoreError>> {
        match result {
            Ok(value) => {
                if let Err(error) = self.check_access() {
                    if transaction.rollback().await.is_err() {
                        self.unavailable.store(true, Ordering::Release);
                        return Err(actions_command_rollback_unknown());
                    }
                    return Err(if prior_command {
                        CommandFailure::Admitted(access_error(error))
                    } else {
                        CommandFailure::NotApplied(access_error(error))
                    });
                }
                if transaction.commit().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(actions_command_commit_unknown(prior_command));
                }
                self.check_access()
                    .map_err(|error| CommandFailure::Admitted(access_error(error)))?;
                Ok(value)
            }
            Err(error) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(actions_command_rollback_unknown());
                }
                Err(actions_command_rolled_back_failure(
                    error,
                    replay_checked,
                    prior_command,
                ))
            }
        }
    }

    async fn ensure_actions_schema(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), CalendarOperationStoreError> {
        self.validate_actions_schema(transaction).await
    }

    async fn action_by_id(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        action_id: Uuid,
    ) -> Result<Option<StoredAction>, CalendarOperationStoreError> {
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
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        if rows.next().await.map_err(sql_error)?.is_some() {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        Ok(Some(action))
    }

    pub(super) async fn action_record_on_transaction(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        action_id: Uuid,
    ) -> Result<Option<ActionRecord>, AgentFailure> {
        self.ensure_actions_schema(transaction)
            .await
            .map_err(AgentFailure::from)?;
        self.action_by_id(transaction, person_id, action_id)
            .await
            .map(|stored| stored.map(|value| value.record))
            .map_err(AgentFailure::from)
    }

    async fn action_by_execution(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> Result<Option<StoredAction>, CalendarOperationStoreError> {
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
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        if rows.next().await.map_err(sql_error)?.is_some() {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        Ok(Some(action))
    }

    async fn pending_expert_count(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<usize, CalendarOperationStoreError> {
        let mut rows=transaction.query("SELECT count(*) FROM actions_records WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved')",(self.person_id.to_string(),)).await.map_err(sql_error)?;
        let row = rows
            .next()
            .await
            .map_err(sql_error)?
            .ok_or(CalendarOperationStoreError::CorruptRecord)?;
        let count = usize::try_from(
            row.get::<i64>(0)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
        )
        .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        if count > floe_calendar_operations::MAX_PENDING_EXPERT_ACTIONS {
            return Err(CalendarOperationStoreError::BudgetExceeded);
        }
        Ok(count)
    }

    async fn insert_action(
        &self,
        transaction: &Transaction<'_>,
        record: &ActionRecord,
        dispatch_revision: Option<u64>,
    ) -> Result<(), CalendarOperationStoreError> {
        if record.person_id != self.person_id || dispatch_revision.is_some() {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        if matches!(record.origin, ActionOrigin::Expert { .. })
            && matches!(
                record.state,
                ActionState::PendingReview | ActionState::Approved
            )
            && self.pending_expert_count(transaction).await?
                >= floe_calendar_operations::MAX_PENDING_EXPERT_ACTIONS
        {
            return Err(CalendarOperationStoreError::BudgetExceeded);
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
            return Err(CalendarOperationStoreError::Conflict);
        }
        Ok(())
    }

    async fn update_action(
        &self,
        transaction: &Transaction<'_>,
        current: &StoredAction,
        next: &ActionRecord,
        dispatch_revision: Option<u64>,
    ) -> Result<(), CalendarOperationStoreError> {
        if current.record.person_id != self.person_id
            || next.person_id != current.record.person_id
            || next.id != current.record.id
            || next.execution_id != current.record.execution_id
            || current.record.revision.checked_add(1) != Some(next.revision)
            || next.device_id != current.record.device_id
            || next.origin != current.record.origin
            || next.effect != current.record.effect
            || next.effect_digest != current.record.effect_digest
            || next.source != current.record.source
            || next.dependency != current.record.dependency
            || next.review != current.record.review
            || next.created_at != current.record.created_at
            || next.expires_at != current.record.expires_at
            || current
                .record
                .execution
                .as_ref()
                .is_some_and(|intent| next.execution.as_ref() != Some(intent))
        {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        if next.authorization != current.record.authorization
            && !(current.record.authorization.is_none()
                && current.record.state == ActionState::PendingReview
                && next.state == ActionState::Approved
                && matches!(
                    next.authorization,
                    Some(floe_access::OperationDecisionReceipt::ReviewedDecision { .. })
                ))
        {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        match (current.dispatch_revision, dispatch_revision) {
            (Some(before), Some(after)) if before == after => {}
            (None, None) if next.execution.is_none() => {}
            (None, Some(revision))
                if revision == current.record.revision
                    && current.record.state == ActionState::Approved
                    && matches!(next.state, ActionState::Executing { .. })
                    && next.execution.is_some() => {}
            _ => return Err(CalendarOperationStoreError::InvalidRecord),
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
            return Err(CalendarOperationStoreError::Conflict);
        }
        Ok(())
    }

    async fn command_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        command_id: Uuid,
    ) -> Result<Option<CommandReceipt>, CalendarOperationStoreError> {
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
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            intent_digest: row
                .get::<String>(1)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            action_id: row
                .get::<String>(2)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
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
            return Err(CalendarOperationStoreError::CorruptRecord);
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
    ) -> Result<(), CalendarOperationStoreError> {
        if command_id.is_nil() || person_id != self.person_id {
            return Err(CalendarOperationStoreError::InvalidRecord);
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
            return Err(CalendarOperationStoreError::Conflict);
        }
        Ok(())
    }

    async fn record_for_command_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        receipt: &CommandReceipt,
    ) -> Result<ActionRecord, CalendarOperationStoreError> {
        let action_id = parse_canonical_uuid(&receipt.action_id)?;
        self.action_by_id(transaction, person_id, action_id)
            .await?
            .map(|stored| stored.record)
            .ok_or(CalendarOperationStoreError::CorruptRecord)
    }

    async fn authority_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
    ) -> Result<OperationAuthorizationPolicy, CalendarOperationStoreError> {
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
            return Err(CalendarOperationStoreError::CorruptRecord);
        };
        let row_person = row
            .get::<String>(0)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        let revision = row
            .get::<i64>(1)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        let mode = row
            .get::<String>(2)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        let digest = row
            .get::<String>(3)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        let payload_bytes = row
            .get::<i64>(4)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        let payload = row
            .get::<String>(5)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        if row_person != person_id.to_string()
            || payload_bytes <= 0
            || payload_bytes > 4096
            || usize::try_from(payload_bytes).ok() != Some(payload.len())
            || !digest_text_is_valid(&digest)
        {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        let authority: OperationAuthorizationPolicy = serde_json::from_str(&payload)
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
        if authority.person_id != person_id
            || u64::try_from(revision).ok() != Some(authority.revision)
            || authority.revision == 0
            || mode != authority_mode_name(authority.calendar_create)
            || digest != digest_hex(&authority_digest(&authority)?)
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        Ok(authority)
    }

    async fn authority_row_exists(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
    ) -> Result<bool, CalendarOperationStoreError> {
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
            .map_err(|_| CalendarOperationStoreError::CorruptRecord)?
            != person_id.to_string()
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        Ok(true)
    }

    async fn write_authority(
        &self,
        transaction: &Transaction<'_>,
        authority: &OperationAuthorizationPolicy,
        expected_revision: Option<u64>,
    ) -> Result<(), CalendarOperationStoreError> {
        if authority.person_id != self.person_id || authority.revision == 0 {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        let payload = serde_json::to_string(authority)
            .map_err(|_| CalendarOperationStoreError::InvalidRecord)?;
        if payload.is_empty() || payload.len() > 4096 {
            return Err(CalendarOperationStoreError::InvalidRecord);
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
            return Err(CalendarOperationStoreError::Conflict);
        }
        Ok(())
    }

    async fn validate_current_action_coverage(
        &self,
        transaction: &Transaction<'_>,
        coverage: &DependencyCoverage,
    ) -> Result<(), CalendarOperationStoreError> {
        coverage
            .validate()
            .map_err(|_| CalendarOperationStoreError::InvalidRecord)?;
        if !matches!(coverage, DependencyCoverage::Dependent { .. }) {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        self.validate_context_dependency_coverage_in_transaction(transaction, coverage)
            .await
            .map_err(|error| match error {
                AgentFailure::Conflict => CalendarOperationStoreError::Conflict,
                AgentFailure::StorageUnavailable => CalendarOperationStoreError::Unavailable,
                AgentFailure::StorageBusy => CalendarOperationStoreError::StorageBusy,
                AgentFailure::VaultLocked => CalendarOperationStoreError::VaultLocked,
                AgentFailure::VaultUnavailable => CalendarOperationStoreError::Unavailable,
                AgentFailure::UnsupportedVersion => CalendarOperationStoreError::CorruptRecord,
                AgentFailure::PolicyDenied | AgentFailure::NotFound => {
                    CalendarOperationStoreError::Conflict
                }
                _ => CalendarOperationStoreError::InvalidRecord,
            })
    }

    async fn invalidate_pending_actions_for_policy(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), CalendarOperationStoreError> {
        let mut cursor: Option<String> = None;
        loop {
            let query = if cursor.is_some() {
                format!(
                    "{ACTION_SELECT} WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved') AND action_id > ? ORDER BY action_id COLLATE BINARY LIMIT 100"
                )
            } else {
                format!(
                    "{ACTION_SELECT} WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved') ORDER BY action_id COLLATE BINARY LIMIT 100"
                )
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
                    || !matches!(stored.record.origin, ActionOrigin::Expert { .. })
                    || !matches!(
                        stored.record.state,
                        ActionState::PendingReview | ActionState::Approved
                    )
                    || stored.dispatch_revision.is_some()
                {
                    return Err(CalendarOperationStoreError::CorruptRecord);
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
                let Some(next) =
                    invalidate_action_policy(&current.record).map_err(invalid_record)?
                else {
                    return Err(CalendarOperationStoreError::CorruptRecord);
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
        self.pending_expert_count(transaction)
            .await
            .map_err(AgentFailure::from)?;
        let mut scanned = 0usize;
        let mut cursor: Option<String> = None;
        loop {
            // The Calendar index alone cannot cover inherited Task context.
            // Authenticate each pending Expert's immutable receipt instead.
            let query = if cursor.is_some() {
                format!(
                    "{ACTION_SELECT} WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved') AND action_id > ? ORDER BY action_id COLLATE BINARY LIMIT 100"
                )
            } else {
                format!(
                    "{ACTION_SELECT} WHERE person_id = ? AND origin_kind = 'expert' AND state IN ('pending_review', 'approved') ORDER BY action_id COLLATE BINARY LIMIT 100"
                )
            };
            let mut rows = if let Some(cursor) = cursor.as_ref() {
                transaction
                    .query(&query, (self.person_id.to_string(), cursor.clone()))
                    .await
            } else {
                transaction
                    .query(&query, (self.person_id.to_string(),))
                    .await
            }
            .map_err(sql_error)
            .map_err(AgentFailure::from)?;
            let mut page = Vec::with_capacity(100);
            while let Some(row) = rows
                .next()
                .await
                .map_err(sql_error)
                .map_err(AgentFailure::from)?
            {
                let stored = stored_action_from_row!(row).map_err(AgentFailure::from)?;
                if stored.record.person_id != self.person_id
                    || !matches!(stored.record.origin, ActionOrigin::Expert { .. })
                    || !matches!(
                        stored.record.state,
                        ActionState::PendingReview | ActionState::Approved
                    )
                    || stored.dispatch_revision.is_some()
                {
                    return Err(AgentFailure::from(
                        CalendarOperationStoreError::CorruptRecord,
                    ));
                }
                page.push(stored);
            }
            drop(rows);
            let page_len = page.len();
            scanned = scanned
                .checked_add(page_len)
                .ok_or(AgentFailure::BudgetExceeded)?;
            if scanned > floe_calendar_operations::MAX_PENDING_EXPERT_ACTIONS {
                return Err(AgentFailure::BudgetExceeded);
            }
            let Some(last) = page.last() else {
                return Ok(());
            };
            cursor = Some(last.record.id.to_string());
            for current in page {
                let evidence = self
                    .expert_action_evidence_in_transaction(transaction, &current.record)
                    .await
                    .map_err(AgentFailure::from)?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let Some(next) =
                    invalidate_action_dependency(&current.record, &evidence, grant_id, authority)
                        .map_err(invalid_record)
                        .map_err(AgentFailure::from)?
                else {
                    continue;
                };
                self.update_action(transaction, &current, &next, current.dispatch_revision)
                    .await
                    .map_err(AgentFailure::from)?;
            }
            if page_len < 100 {
                return Ok(());
            }
        }
    }

    async fn validate_expert_task_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        record: &ActionRecord,
    ) -> Result<(), CalendarOperationStoreError> {
        if let Some(evidence) = self
            .expert_action_evidence_in_transaction(transaction, record)
            .await?
        {
            self.validate_current_action_coverage(transaction, &evidence.coverage)
                .await?;
        }
        Ok(())
    }

    /// Historical receipt authentication is also usable after a grant is
    /// revoked; live grant checks belong only to new admission/dispatch.
    async fn expert_action_evidence_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        record: &ActionRecord,
    ) -> Result<Option<floe_calendar_operations::ExpertProposalEvidence>, CalendarOperationStoreError>
    {
        let ActionOrigin::Expert {
            task_id,
            evidence_ref,
            artifact_id,
            ..
        } = &record.origin
        else {
            return Ok(None);
        };
        let task_id = floe_agent_contract::TaskId::from_uuid(*task_id)
            .ok_or(CalendarOperationStoreError::InvalidRecord)?;
        let task = self
            .task_on(transaction, task_id)
            .await
            .map_err(historical_action_error)?
            .ok_or(CalendarOperationStoreError::CorruptRecord)?;
        let receipt = self
            .read_execution_receipt_on(transaction, evidence_ref)
            .await
            .map_err(historical_action_error)?;
        if task.snapshot.task_id != task_id
            || task.snapshot.principal != self.person_id.to_string()
            || task.snapshot.state != floe_agent_contract::TaskState::Completed
            || task.receipt.as_ref() != Some(&receipt)
        {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        let evidence = super::expert_actions::decode_task_proposal(
            &task,
            evidence_ref,
            *artifact_id,
            record.person_id,
            &record.device_id,
        )
        .map_err(historical_action_error)?;
        floe_calendar_operations::validate_expert_action_evidence(record, &evidence)
            .map_err(historical_action_error)?;
        Ok(Some(evidence))
    }

    async fn settlement_receipt(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        execution_id: Uuid,
        expected_revision: u64,
    ) -> Result<Option<SettlementReceipt>, CalendarOperationStoreError> {
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
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            effect_digest: row
                .get::<String>(1)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            outcome_digest: row
                .get::<String>(2)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
        };
        if !canonical_uuid(&receipt.action_id)
            || !digest_text_is_valid(&receipt.effect_digest)
            || !digest_text_is_valid(&receipt.outcome_digest)
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        Ok(Some(receipt))
    }

    async fn insert_settlement_receipt(
        &self,
        transaction: &Transaction<'_>,
        settlement: &ExecutionSettlement,
        action_id: Uuid,
        outcome_digest: &ActionDigest,
    ) -> Result<(), CalendarOperationStoreError> {
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
            return Err(CalendarOperationStoreError::Conflict);
        }
        Ok(())
    }

    async fn collection_receipt(
        &self,
        transaction: &Transaction<'_>,
        ack: &CollectionAck,
    ) -> Result<Option<CollectionReceipt>, CalendarOperationStoreError> {
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
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
            intent_digest: row
                .get::<String>(1)
                .map_err(|_| CalendarOperationStoreError::CorruptRecord)?,
        };
        if !canonical_uuid(&receipt.action_id)
            || !digest_text_is_valid(&receipt.intent_digest)
            || rows.next().await.map_err(sql_error)?.is_some()
        {
            return Err(CalendarOperationStoreError::CorruptRecord);
        }
        Ok(Some(receipt))
    }

    async fn insert_collection_receipt(
        &self,
        transaction: &Transaction<'_>,
        ack: &CollectionAck,
        action_id: Uuid,
        intent_digest: &ActionDigest,
    ) -> Result<(), CalendarOperationStoreError> {
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
            return Err(CalendarOperationStoreError::Conflict);
        }
        Ok(())
    }
}

fn actions_command_commit_unknown(
    prior_command: bool,
) -> CommandFailure<CalendarOperationStoreError> {
    if prior_command {
        CommandFailure::Admitted(CalendarOperationStoreError::Unavailable)
    } else {
        CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
    }
}

fn actions_command_rollback_unknown() -> CommandFailure<CalendarOperationStoreError> {
    CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
}

fn actions_command_rolled_back_failure(
    error: CalendarOperationStoreError,
    replay_checked: bool,
    prior_command: bool,
) -> CommandFailure<CalendarOperationStoreError> {
    if prior_command || !replay_checked {
        CommandFailure::Indeterminate(error)
    } else {
        CommandFailure::NotApplied(error)
    }
}

#[cfg(test)]
mod command_finish_tests {
    use super::{
        CalendarOperationStoreError, actions_command_commit_unknown,
        actions_command_rollback_unknown, actions_command_rolled_back_failure,
    };
    use floe_kernel::CommandFailure;

    #[test]
    fn commit_failure_keeps_fresh_or_previously_admitted_command_identity() {
        assert_eq!(
            actions_command_commit_unknown(false),
            CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
        );
        assert_eq!(
            actions_command_commit_unknown(true),
            CommandFailure::Admitted(CalendarOperationStoreError::Unavailable)
        );
    }

    #[test]
    fn rollback_failure_is_indeterminate_and_only_verified_rollback_releases_fresh_id() {
        assert_eq!(
            actions_command_rollback_unknown(),
            CommandFailure::Indeterminate(CalendarOperationStoreError::Unavailable)
        );
        assert_eq!(
            actions_command_rolled_back_failure(CalendarOperationStoreError::Conflict, true, false),
            CommandFailure::NotApplied(CalendarOperationStoreError::Conflict)
        );
        assert_eq!(
            actions_command_rolled_back_failure(CalendarOperationStoreError::Conflict, true, true),
            CommandFailure::Indeterminate(CalendarOperationStoreError::Conflict)
        );
        assert_eq!(
            actions_command_rolled_back_failure(
                CalendarOperationStoreError::Conflict,
                false,
                false
            ),
            CommandFailure::Indeterminate(CalendarOperationStoreError::Conflict)
        );
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

fn to_i64(value: u64) -> Result<i64, CalendarOperationStoreError> {
    i64::try_from(value).map_err(|_| CalendarOperationStoreError::InvalidRecord)
}

fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value)
}

fn parse_canonical_uuid(value: &str) -> Result<Uuid, CalendarOperationStoreError> {
    let uuid = Uuid::parse_str(value).map_err(|_| CalendarOperationStoreError::CorruptRecord)?;
    if uuid.to_string() != value || uuid.is_nil() {
        return Err(CalendarOperationStoreError::CorruptRecord);
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
        replay_checked: &mut bool,
        prior_command: &mut bool,
    ) -> Result<Option<ActionRecord>, CalendarOperationStoreError> {
        let receipt = self
            .command_receipt(transaction, person_id, command_id)
            .await?;
        *replay_checked = true;
        let Some(receipt) = receipt else {
            return Ok(None);
        };
        *prior_command = true;
        if receipt.kind != kind || receipt.intent_digest != digest_hex(digest) {
            return Err(CalendarOperationStoreError::Conflict);
        }
        self.record_for_command_receipt(transaction, person_id, &receipt)
            .await
            .map(Some)
    }

    pub(crate) async fn actions_get(
        &self,
        person_id: PersonId,
        action_id: Uuid,
    ) -> Result<Option<ActionRecord>, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            // Inspection is read-only; do not reserve the single writer slot
            // while a dispatch intent or receipt settlement is committing.
            .transaction_with_behavior(TransactionBehavior::Deferred)
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
    ) -> Result<ActionPage, CalendarOperationStoreError> {
        self.actions_list_origin(person_id, cursor, limit, false)
            .await
    }

    pub(crate) async fn actions_list_direct(
        &self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
    ) -> Result<ActionPage, CalendarOperationStoreError> {
        self.actions_list_origin(person_id, cursor, limit, true)
            .await
    }

    async fn actions_list_origin(
        &self,
        person_id: PersonId,
        cursor: Option<Uuid>,
        limit: u16,
        direct_only: bool,
    ) -> Result<ActionPage, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if !(1..=100).contains(&limit) {
                return Err(CalendarOperationStoreError::InvalidRecord);
            }
            if person_id != self.person_id {
                return Ok(ActionPage {
                    records: Vec::new(),
                    next_cursor: None,
                });
            }
            let fetch_limit = i64::from(limit) + 1;
            let origin_clause = if direct_only {
                " AND origin_kind = 'direct'"
            } else {
                ""
            };
            let cursor_clause = if cursor.is_some() {
                " AND action_id > ?"
            } else {
                ""
            };
            let query = format!(
                "{ACTION_SELECT} WHERE person_id = ?{origin_clause}{cursor_clause} ORDER BY action_id COLLATE BINARY LIMIT ?"
            );
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
                    return Err(CalendarOperationStoreError::CorruptRecord);
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
    ) -> Result<Option<ActionRecord>, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if person_id != self.person_id {
                return Ok(None);
            }
            if command_id.is_nil() {
                return Err(CalendarOperationStoreError::InvalidRecord);
            }
            let mut replay_checked = false;
            let mut prior_command = false;
            if super::conversations::command_occupant(&transaction, command_id)
                .await
                .map_err(invalid_record)?
                .is_some()
            {
                return Err(CalendarOperationStoreError::Conflict);
            }
            self.replay_command_action(
                &transaction,
                person_id,
                command_id,
                "submit",
                &request_digest,
                &mut replay_checked,
                &mut prior_command,
            )
            .await
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_admit(
        &self,
        admission: OperationAdmission,
    ) -> Result<AdmittedOperation, CommandFailure<CalendarOperationStoreError>> {
        let mut connection = self
            .connection()
            .map_err(|error| CommandFailure::Indeterminate(access_error(error)))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| CommandFailure::Indeterminate(sql_error(error)))?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = self
            .actions_admit_on(
                &transaction,
                admission,
                &mut replay_checked,
                &mut prior_command,
            )
            .await;
        self.finish_actions_command_transaction(transaction, result, replay_checked, prior_command)
            .await
    }

    pub(super) async fn actions_admit_on(
        &self,
        transaction: &Transaction<'_>,
        admission: OperationAdmission,
        replay_checked: &mut bool,
        prior_command: &mut bool,
    ) -> Result<AdmittedOperation, CalendarOperationStoreError> {
        self.ensure_actions_schema(transaction).await?;
        if admission.command_id.is_nil() || admission.record.person_id != self.person_id {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        if let Some(record) = self
            .replay_command_action(
                transaction,
                self.person_id,
                admission.command_id,
                "submit",
                &admission.request_digest,
                replay_checked,
                prior_command,
            )
            .await?
        {
            return Ok(AdmittedOperation {
                record,
                replayed: true,
            });
        }
        if super::conversations::command_occupant(transaction, admission.command_id)
            .await
            .map_err(invalid_record)?
            .is_some()
        {
            *prior_command = true;
            return Err(CalendarOperationStoreError::Conflict);
        }
        admission
            .record
            .validate()
            .map_err(|_| CalendarOperationStoreError::InvalidRecord)?;
        // The original submit command is the sole replay authority. A new
        // command for an already retained proposal remains a conflict,
        // including when pending admission capacity is exhausted.
        if self
            .action_by_id(transaction, self.person_id, admission.record.id)
            .await?
            .is_some()
        {
            return Err(CalendarOperationStoreError::Conflict);
        }
        let authority = if matches!(admission.record.origin, ActionOrigin::Expert { .. }) {
            Some(
                self.authority_in_transaction(transaction, self.person_id)
                    .await?,
            )
        } else {
            None
        };
        validate_action_admission(&admission, authority.as_ref()).map_err(invalid_record)?;
        self.validate_expert_task_in_transaction(transaction, &admission.record)
            .await?;
        self.insert_action(transaction, &admission.record, None)
            .await?;
        self.insert_command_receipt(
            transaction,
            self.person_id,
            admission.command_id,
            "submit",
            &admission.request_digest,
            Some(admission.record.id),
        )
        .await?;
        Ok(AdmittedOperation {
            record: admission.record,
            replayed: false,
        })
    }

    pub(crate) async fn actions_record_decision(
        &self,
        decision: ActionDecision,
    ) -> Result<ActionRecord, CommandFailure<CalendarOperationStoreError>> {
        let mut connection = self
            .connection()
            .map_err(|error| CommandFailure::Indeterminate(access_error(error)))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| CommandFailure::Indeterminate(sql_error(error)))?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            let digest = decision_intent_digest(&decision).map_err(invalid_record)?;
            if decision.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            if let Some(record) = self
                .replay_command_action(
                    &transaction,
                    self.person_id,
                    decision.command_id,
                    "decision",
                    &digest,
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await?
            {
                if record.id != decision.action_id {
                    return Err(CalendarOperationStoreError::CorruptRecord);
                }
                return Ok(record);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, decision.action_id)
                .await?
                .ok_or(CalendarOperationStoreError::NotFound)?;
            let authority = if matches!(current.record.origin, ActionOrigin::Expert { .. }) {
                Some(
                    self.authority_in_transaction(&transaction, self.person_id)
                        .await?,
                )
            } else {
                None
            };
            let next = floe_calendar_operations::decide_action(
                &current.record,
                &decision,
                authority.as_ref(),
            )
            .map_err(invalid_record)?;
            self.update_action(&transaction, &current, &next, current.dispatch_revision)
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
        self.finish_actions_command_transaction(transaction, result, replay_checked, prior_command)
            .await
    }

    pub(crate) async fn actions_admit_reconciliation(
        &self,
        command: ActionReconciliation,
    ) -> Result<ActionRecord, CommandFailure<CalendarOperationStoreError>> {
        let mut connection = self
            .connection()
            .map_err(|error| CommandFailure::Indeterminate(access_error(error)))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| CommandFailure::Indeterminate(sql_error(error)))?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            let digest = command_digest(b"floe.actions.reconciliation.v1\0", &command)?;
            if command.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            if let Some(record) = self
                .replay_command_action(
                    &transaction,
                    self.person_id,
                    command.command_id,
                    "reconciliation",
                    &digest,
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await?
            {
                if record.id != command.action_id {
                    return Err(CalendarOperationStoreError::CorruptRecord);
                }
                return Ok(record);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, command.action_id)
                .await?
                .ok_or(CalendarOperationStoreError::NotFound)?;
            if command
                .expected_origin
                .is_some_and(|expected| match expected {
                    floe_calendar_operations::ActionOriginKind::Direct => !matches!(
                        &current.record.origin,
                        floe_calendar_operations::ActionOrigin::Direct { .. }
                    ),
                    floe_calendar_operations::ActionOriginKind::Expert => !matches!(
                        &current.record.origin,
                        floe_calendar_operations::ActionOrigin::Expert { .. }
                    ),
                })
            {
                return Err(CalendarOperationStoreError::Conflict);
            }
            let collection_pending = matches!(
                &current.record.state,
                ActionState::Succeeded {
                    collection: floe_calendar_operations::ActionCollectionState::Pending { .. },
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
                return Err(CalendarOperationStoreError::Conflict);
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
        self.finish_actions_command_transaction(transaction, result, replay_checked, prior_command)
            .await
    }

    pub(crate) async fn actions_stop_before_dispatch(
        &self,
        stop: PreDispatchStop,
    ) -> Result<ActionRecord, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if stop.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, stop.action_id)
                .await?
                .ok_or(CalendarOperationStoreError::NotFound)?;
            let next = stop_action(&current.record, &stop).map_err(invalid_record)?;
            self.update_action(&transaction, &current, &next, current.dispatch_revision)
                .await?;
            Ok(next)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_prepare_dispatch(
        &self,
        request: DispatchIntent,
    ) -> Result<DispatchAdmission, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if request.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            let current = self
                .action_by_id(&transaction, self.person_id, request.action_id)
                .await?
                .ok_or(CalendarOperationStoreError::NotFound)?;
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
                    return Err(CalendarOperationStoreError::Conflict);
                }
                return Ok(DispatchAdmission {
                    intent: existing.clone(),
                    record_revision: current.record.revision,
                    dispatch_required: false,
                });
            }
            if current.dispatch_revision.is_some() {
                return Err(CalendarOperationStoreError::CorruptRecord);
            }
            self.validate_expert_task_in_transaction(&transaction, &current.record)
                .await?;
            let authority = if matches!(current.record.origin, ActionOrigin::Expert { .. }) {
                Some(
                    self.authority_in_transaction(&transaction, self.person_id)
                        .await?,
                )
            } else {
                None
            };
            let (next, execution) =
                prepare_action_dispatch(&current.record, &request, authority.as_ref())
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
    ) -> Result<Option<DispatchAdmission>, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
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
            let Some(intent) = stored.record.execution.clone() else {
                if stored.dispatch_revision.is_some() {
                    return Err(CalendarOperationStoreError::CorruptRecord);
                }
                return Ok(None);
            };
            if stored.dispatch_revision.is_none()
                || intent.execution_id != execution_id
                || intent.person_id != person_id
            {
                return Err(CalendarOperationStoreError::CorruptRecord);
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
    ) -> Result<ActionRecord, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if settlement.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            let outcome_digest = command_digest(b"floe.actions.settlement.v1\0", &settlement)?;
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
                    return Err(CalendarOperationStoreError::Conflict);
                }
                let action_id = parse_canonical_uuid(&receipt.action_id)?;
                let current = self
                    .action_by_id(&transaction, self.person_id, action_id)
                    .await?
                    .ok_or(CalendarOperationStoreError::CorruptRecord)?;
                if current.record.execution_id != settlement.execution_id
                    || current.record.effect_digest != settlement.effect_digest
                {
                    return Err(CalendarOperationStoreError::CorruptRecord);
                }
                return Ok(current.record);
            }
            let current = self
                .action_by_execution(&transaction, self.person_id, settlement.execution_id)
                .await?
                .ok_or(CalendarOperationStoreError::NotFound)?;
            if current.record.effect_digest != settlement.effect_digest {
                return Err(CalendarOperationStoreError::Conflict);
            }
            let next = settle_action(&current.record, &settlement).map_err(invalid_record)?;
            self.update_action(&transaction, &current, &next, current.dispatch_revision)
                .await?;
            self.insert_settlement_receipt(&transaction, &settlement, next.id, &outcome_digest)
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
    ) -> Result<RecoveryPage, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if !(1..=100).contains(&limit) {
                return Err(CalendarOperationStoreError::InvalidRecord);
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
                    return Err(CalendarOperationStoreError::CorruptRecord);
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
    ) -> Result<CollectionTicket, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if ack.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            let intent_digest = command_digest(b"floe.actions.collection-ack.v1\0", &ack)?;
            if let Some(receipt) = self.collection_receipt(&transaction, &ack).await? {
                if receipt.intent_digest != digest_hex(&intent_digest) {
                    return Err(CalendarOperationStoreError::Conflict);
                }
                let action_id = parse_canonical_uuid(&receipt.action_id)?;
                let current = self
                    .action_by_execution(&transaction, self.person_id, ack.execution_id)
                    .await?
                    .ok_or(CalendarOperationStoreError::CorruptRecord)?;
                let ticket = current
                    .record
                    .collection
                    .as_ref()
                    .ok_or(CalendarOperationStoreError::CorruptRecord)?;
                let expected_next_revision = ack
                    .expected_ticket_revision
                    .checked_add(1)
                    .ok_or(CalendarOperationStoreError::CorruptRecord)?;
                if current.record.id != action_id
                    || ticket.receipt_digest != ack.receipt_digest
                    || ticket.revision != expected_next_revision
                    || !matches!(
                        &ticket.state,
                        floe_calendar_operations::ActionCollectionState::Collected { day_projection_ref }
                            if day_projection_ref == &ack.day_projection_ref
                    )
                {
                    return Err(CalendarOperationStoreError::CorruptRecord);
                }
                return Ok(ticket.clone());
            }
            let current = self
                .action_by_execution(&transaction, self.person_id, ack.execution_id)
                .await?
                .ok_or(CalendarOperationStoreError::NotFound)?;
            let next =
                acknowledge_action_collection(&current.record, &ack).map_err(invalid_record)?;
            self.update_action(&transaction, &current, &next, current.dispatch_revision)
                .await?;
            self.insert_collection_receipt(&transaction, &ack, next.id, &intent_digest)
                .await?;
            next.collection.ok_or(CalendarOperationStoreError::CorruptRecord)
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_validate_proposal_coverage(
        &self,
        person_id: PersonId,
        coverage: &DependencyCoverage,
    ) -> Result<(), CalendarOperationStoreError> {
        if person_id != self.person_id {
            return Err(CalendarOperationStoreError::NotFound);
        }
        let DependencyCoverage::Dependent { dependencies } = coverage else {
            return Err(CalendarOperationStoreError::InvalidRecord);
        };
        if dependencies
            .iter()
            .any(|entry| entry.person_id() != person_id)
        {
            return Err(CalendarOperationStoreError::InvalidRecord);
        }
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(sql_error)?;
        let result = self
            .validate_current_action_coverage(&transaction, coverage)
            .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_read_authority(
        &self,
        person_id: PersonId,
    ) -> Result<OperationAuthorizationPolicy, CalendarOperationStoreError> {
        let mut connection = self.connection().map_err(access_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(sql_error)?;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            self.authority_in_transaction(&transaction, person_id).await
        }
        .await;
        self.finish_actions_transaction(transaction, result).await
    }

    pub(crate) async fn actions_compare_and_set_authority(
        &self,
        change: OperationPolicyChange,
    ) -> Result<OperationAuthorizationPolicy, CommandFailure<CalendarOperationStoreError>> {
        let mut connection = self
            .connection()
            .map_err(|error| CommandFailure::Indeterminate(access_error(error)))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| CommandFailure::Indeterminate(sql_error(error)))?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = async {
            self.ensure_actions_schema(&transaction).await?;
            if change.person_id != self.person_id {
                return Err(CalendarOperationStoreError::NotFound);
            }
            if change.command_id.is_nil() {
                return Err(CalendarOperationStoreError::InvalidRecord);
            }
            let intent_digest = command_digest(b"floe.actions.authority-change.v1\0", &change)?;
            let receipt = self
                .command_receipt(&transaction, self.person_id, change.command_id)
                .await?;
            replay_checked = true;
            if let Some(receipt) = receipt {
                prior_command = true;
                if receipt.kind != "authority"
                    || receipt.intent_digest != digest_hex(&intent_digest)
                {
                    return Err(CalendarOperationStoreError::Conflict);
                }
                if !self
                    .authority_row_exists(&transaction, self.person_id)
                    .await?
                {
                    return Err(CalendarOperationStoreError::CorruptRecord);
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
            let next = change_operation_policy(&current, &change).map_err(invalid_record)?;
            self.write_authority(&transaction, &next, stored.then_some(current.revision))
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
            if next.revision != current.revision {
                self.invalidate_pending_actions_for_policy(&transaction)
                    .await?;
            }
            Ok(next)
        }
        .await;
        self.finish_actions_command_transaction(transaction, result, replay_checked, prior_command)
            .await
    }
}
