use floe_access::{ConsumerPolicyAuthority, GrantId};
use floe_agent_contract::AgentFailure;
use serde::{Deserialize, Serialize};
use turso::transaction::TransactionBehavior;

use super::{EncryptedAgentVault, VaultKeyProvider, storage};

const CALENDAR_GRANT_POLICY_SCHEMA_VERSION: i64 = 1;
const MAX_CALENDAR_GRANT_POLICIES: usize = 128;
const MAX_POLICY_PAYLOAD_BYTES: usize = 4 * 1024;

/// What Access remembers about one reviewed Calendar grant.
///
/// The policy names the grant and the consumer-policy authority the review
/// established. Native sources also record the subject fingerprint the review
/// approved. It never names a Registry setup, assignment, view or container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarGrantPolicy {
    pub grant_id: GrantId,
    pub person_id: floe_kernel::PersonId,
    pub consumer_policy: ConsumerPolicyAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_native_subject_fingerprint: Option<String>,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_calendar_grant_policy_store(
        &self,
        fresh: bool,
    ) -> Result<(), AgentFailure> {
        // Old profiles that still carry either mapping table cannot be read:
        // there is no migration from setup-bound rows to source-bound policy.
        let probe = self.connection()?;
        let mut legacy = probe
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('calendar_grant_mappings', 'remote_calendar_grant_mappings', 'calendar_grant_schema', 'remote_calendar_grant_schema')",
                (),
            )
            .await
            .map_err(storage)?;
        if legacy.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::UnsupportedVersion);
        }
        drop(probe);

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result: Result<(), AgentFailure> = async {
            if fresh {
                transaction
                    .execute(
                        "CREATE TABLE calendar_grant_policy_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "INSERT INTO calendar_grant_policy_schema (id, version) VALUES (1, ?)",
                        [CALENDAR_GRANT_POLICY_SCHEMA_VERSION],
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE calendar_grant_policies (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, policy_incarnation TEXT NOT NULL, policy_epoch INTEGER NOT NULL, reviewed_native_subject_fingerprint TEXT, payload TEXT NOT NULL)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE INDEX calendar_grant_policies_person ON calendar_grant_policies (person_id, grant_id)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
            } else {
                let mut marker = transaction
                    .query(
                        "SELECT version FROM calendar_grant_policy_schema WHERE id = 1",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                let version = marker
                    .next()
                    .await
                    .map_err(storage)?
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .get::<i64>(0)
                    .map_err(storage)?;
                if version != CALENDAR_GRANT_POLICY_SCHEMA_VERSION {
                    return Err(AgentFailure::UnsupportedVersion);
                }
                transaction
                    .query("SELECT * FROM calendar_grant_policies LIMIT 0", ())
                    .await
                    .map_err(storage)?;
            }
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await?;
        self.validate_calendar_grant_policies().await
    }

    async fn validate_calendar_grant_policies(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id, person_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_policies",
                (),
            )
            .await
            .map_err(storage)?;
        let mut count = 0usize;
        while let Some(row) = rows.next().await.map_err(storage)? {
            count = count.saturating_add(1);
            if count > MAX_CALENDAR_GRANT_POLICIES {
                return Err(AgentFailure::BudgetExceeded);
            }
            let policy = decode_policy_row(
                &row.get::<String>(0).map_err(storage)?,
                &row.get::<String>(1).map_err(storage)?,
                &row.get::<String>(2).map_err(storage)?,
                row.get::<i64>(3).map_err(storage)?,
                row.get::<Option<String>>(4).map_err(storage)?.as_deref(),
                &row.get::<String>(5).map_err(storage)?,
            )?;
            if policy.person_id != self.person_id {
                return Err(AgentFailure::VaultUnavailable);
            }
            // The policy must name a grant this vault owns.
            let grant = self
                .get_data_access_grant(policy.grant_id)
                .await
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            if grant.authority_owner() != self.vault_id {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
        Ok(())
    }

    pub(super) async fn calendar_grant_policy(
        &self,
        grant_id: GrantId,
    ) -> Result<CalendarGrantPolicy, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id, person_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload FROM calendar_grant_policies WHERE grant_id = ? AND person_id = ?",
                (
                    grant_id.as_uuid().to_string(),
                    self.person_id.to_string(),
                ),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        decode_policy_row(
            &row.get::<String>(0).map_err(storage)?,
            &row.get::<String>(1).map_err(storage)?,
            &row.get::<String>(2).map_err(storage)?,
            row.get::<i64>(3).map_err(storage)?,
            row.get::<Option<String>>(4).map_err(storage)?.as_deref(),
            &row.get::<String>(5).map_err(storage)?,
        )
    }

    pub(super) async fn upsert_calendar_grant_policy_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        policy: &CalendarGrantPolicy,
    ) -> Result<(), AgentFailure> {
        if policy.person_id != self.person_id || !policy.consumer_policy.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(fingerprint) = policy.reviewed_native_subject_fingerprint.as_deref() {
            super::calendar_grants::validate_native_subject_fingerprint(fingerprint)?;
        }
        let payload =
            serde_json::to_string(policy).map_err(|_| AgentFailure::StorageUnavailable)?;
        if payload.len() > MAX_POLICY_PAYLOAD_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let changed = transaction
            .execute(
                "UPDATE calendar_grant_policies SET person_id = ?, policy_incarnation = ?, policy_epoch = ?, reviewed_native_subject_fingerprint = ?, payload = ? WHERE grant_id = ?",
                (
                    self.person_id.to_string(),
                    policy.consumer_policy.incarnation().to_string(),
                    policy.consumer_policy.epoch().get() as i64,
                    policy.reviewed_native_subject_fingerprint.clone(),
                    payload.clone(),
                    policy.grant_id.as_uuid().to_string(),
                ),
            )
            .await
            .map_err(storage)?;
        if changed == 0 {
            let count = transaction
                .query("SELECT COUNT(*) FROM calendar_grant_policies", ())
                .await
                .map_err(storage)?
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            if usize::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)?
                >= MAX_CALENDAR_GRANT_POLICIES
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction
                .execute(
                    "INSERT INTO calendar_grant_policies (grant_id, person_id, policy_incarnation, policy_epoch, reviewed_native_subject_fingerprint, payload) VALUES (?, ?, ?, ?, ?, ?)",
                    (
                        policy.grant_id.as_uuid().to_string(),
                        self.person_id.to_string(),
                        policy.consumer_policy.incarnation().to_string(),
                        policy.consumer_policy.epoch().get() as i64,
                        policy.reviewed_native_subject_fingerprint.clone(),
                        payload,
                    ),
                )
                .await
                .map_err(storage)?;
        }
        Ok(())
    }

    pub(super) async fn ensure_calendar_grant_policy_schema(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        transaction
            .query(
                "SELECT version FROM calendar_grant_policy_schema WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        Ok(())
    }
}

fn decode_policy_row(
    grant_id: &str,
    person_id: &str,
    policy_incarnation: &str,
    policy_epoch: i64,
    fingerprint: Option<&str>,
    payload: &str,
) -> Result<CalendarGrantPolicy, AgentFailure> {
    if payload.len() > MAX_POLICY_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    let policy: CalendarGrantPolicy =
        serde_json::from_str(payload).map_err(|_| AgentFailure::VaultUnavailable)?;
    let grant_id = GrantId::from_uuid(
        uuid::Uuid::parse_str(grant_id).map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let person_id = floe_kernel::PersonId(
        uuid::Uuid::parse_str(person_id).map_err(|_| AgentFailure::VaultUnavailable)?,
    );
    let incarnation =
        uuid::Uuid::parse_str(policy_incarnation).map_err(|_| AgentFailure::VaultUnavailable)?;
    let epoch = std::num::NonZeroU64::new(
        u64::try_from(policy_epoch).map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let indexed_policy = ConsumerPolicyAuthority::from_parts(incarnation, epoch)
        .ok_or(AgentFailure::VaultUnavailable)?;
    if policy.grant_id != grant_id
        || policy.person_id != person_id
        || policy.consumer_policy != indexed_policy
        || policy.reviewed_native_subject_fingerprint.as_deref() != fingerprint
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    if let Some(fingerprint) = policy.reviewed_native_subject_fingerprint.as_deref() {
        super::calendar_grants::validate_native_subject_fingerprint(fingerprint)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
    }
    Ok(policy)
}
