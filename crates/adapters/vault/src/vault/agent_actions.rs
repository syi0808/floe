use chrono::{DateTime, Utc};
use floe_access::{ContextDependency, DependencyCoverage};
use floe_actions::{
    AgentActionAdmission, AgentActionEnvelope, CalendarAction, CalendarActionState,
    action_policy_mode_name as policy_mode_name, action_state_name as state_name,
    valid_action_digest as valid_digest,
};
use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

const AGENT_ACTION_SCHEMA_VERSION: i64 = 1;
const MAX_AGENT_ACTION_BYTES: usize = 65_536;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn invalidate_agent_actions_for_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        grant_id: floe_access::GrantId,
        authority: floe_access::GrantAuthority,
    ) -> Result<(), AgentFailure> {
        self.ensure_agent_action_schema(transaction).await?;
        let mut rows = transaction
            .query(
                "SELECT execution_id, state, digest, payload FROM agent_action_envelopes WHERE person_id = ? AND grant_id = ? AND grant_incarnation = ? AND grant_epoch = ? AND state IN ('pending', 'approved')",
                (
                    self.person_id.to_string(),
                    grant_id.as_uuid().to_string(),
                    authority.incarnation().to_string(),
                    i64::try_from(authority.access_epoch().get())
                        .map_err(|_| AgentFailure::Conflict)?,
                ),
            )
            .await
            .map_err(storage)?;
        let mut updates = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let execution_id = row.get::<String>(0).map_err(storage)?;
            let state = row.get::<String>(1).map_err(storage)?;
            let stored_digest = row.get::<String>(2).map_err(storage)?;
            let payload = row.get::<String>(3).map_err(storage)?;
            let mut envelope: AgentActionEnvelope =
                serde_json::from_str(&payload).map_err(|_| AgentFailure::VaultUnavailable)?;
            envelope
                .validate(self.person_id)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            if state_name(&envelope.action.state) != state || envelope.digest()? != stored_digest {
                return Err(AgentFailure::VaultUnavailable);
            }
            envelope.write_approval = false;
            envelope.action.state = CalendarActionState::Blocked {
                reason: floe_actions::ActionBlockReason::PolicyDenied,
            };
            let payload =
                serde_json::to_string(&envelope).map_err(|_| AgentFailure::InvalidInput)?;
            let updated_digest = envelope.digest()?;
            updates.push((execution_id, stored_digest, updated_digest, payload));
        }
        drop(rows);
        for (execution_id, expected_digest, updated_digest, payload) in updates {
            let changed = transaction
                .execute(
                    "UPDATE agent_action_envelopes SET state = 'blocked', digest = ?, payload = ? WHERE execution_id = ? AND person_id = ? AND state IN ('pending', 'approved') AND digest = ?",
                    (
                        updated_digest,
                        payload,
                        execution_id,
                        self.person_id.to_string(),
                        expected_digest,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(())
    }

    async fn invalidate_agent_actions_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        self.ensure_agent_action_schema(transaction).await?;
        let mut rows = transaction
            .query(
                "SELECT execution_id, grant_id, grant_incarnation, grant_epoch FROM agent_action_envelopes WHERE person_id = ? AND state IN ('pending', 'approved')",
                [self.person_id.to_string()],
            )
            .await
            .map_err(storage)?;
        let mut authorities = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let grant_id = floe_access::GrantId::from_uuid(
                Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?,
            )
            .ok_or(AgentFailure::VaultUnavailable)?;
            let authority = floe_access::GrantAuthority::from_parts(
                Uuid::parse_str(&row.get::<String>(2).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?,
                std::num::NonZeroU64::new(
                    u64::try_from(row.get::<i64>(3).map_err(storage)?)
                        .map_err(|_| AgentFailure::VaultUnavailable)?,
                )
                .ok_or(AgentFailure::VaultUnavailable)?,
            )
            .ok_or(AgentFailure::VaultUnavailable)?;
            authorities.push((grant_id, authority));
        }
        drop(rows);
        for (grant_id, authority) in authorities {
            self.invalidate_agent_actions_for_grant_in_transaction(
                transaction,
                grant_id,
                authority,
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn initialize_agent_action_store(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('agent_action_schema', 'agent_action_envelopes', 'agent_action_policy')",
                (),
            )
            .await
            .map_err(storage)?;
        let mut names = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            names.push(row.get::<String>(0).map_err(storage)?);
        }
        names.sort();
        if names.is_empty() {
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let result = async {
                transaction
                    .execute(
                        "CREATE TABLE agent_action_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE agent_action_envelopes (execution_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, grant_id TEXT NOT NULL, grant_incarnation TEXT NOT NULL, grant_epoch INTEGER NOT NULL, state TEXT NOT NULL, digest TEXT NOT NULL, payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 65536))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE agent_action_policy (person_id TEXT PRIMARY KEY, mode TEXT NOT NULL CHECK(mode IN ('allow', 'ask', 'deny')))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE INDEX agent_action_envelopes_person ON agent_action_envelopes(person_id)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE INDEX agent_action_envelopes_grant ON agent_action_envelopes(person_id, grant_id, grant_incarnation, grant_epoch)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "INSERT INTO agent_action_schema (id, version) VALUES (1, 1)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                Ok(())
            }
            .await;
            return self
                .finish_access_grant_transaction(transaction, result)
                .await;
        }
        if names
            != [
                "agent_action_envelopes".to_owned(),
                "agent_action_policy".to_owned(),
                "agent_action_schema".to_owned(),
            ]
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut marker = connection
            .query("SELECT id, version FROM agent_action_schema", ())
            .await
            .map_err(storage)?;
        let Some(row) = marker.next().await.map_err(storage)? else {
            return Err(AgentFailure::VaultUnavailable);
        };
        if row.get::<i64>(0).map_err(storage)? != 1
            || row.get::<i64>(1).map_err(storage)? != AGENT_ACTION_SCHEMA_VERSION
            || marker.next().await.map_err(storage)?.is_some()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut indexes = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'agent_action_envelopes_person'",
                (),
            )
            .await
            .map_err(storage)?;
        if indexes.next().await.map_err(storage)?.is_none() {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut grant_indexes = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'agent_action_envelopes_grant'",
                (),
            )
            .await
            .map_err(storage)?;
        if grant_indexes.next().await.map_err(storage)?.is_none() {
            return Err(AgentFailure::VaultUnavailable);
        }
        connection
            .query(
                "SELECT execution_id, person_id, grant_id, grant_incarnation, grant_epoch, state, digest, payload FROM agent_action_envelopes LIMIT 0",
                (),
            )
            .await
            .map_err(storage)?;
        self.check_access()
    }

    pub async fn agent_action_policy(
        &self,
    ) -> Result<floe_actions::ActionAuthorityMode, AgentFailure> {
        self.initialize_agent_action_store().await?;
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT mode FROM agent_action_policy WHERE person_id = ?",
                [self.person_id.to_string()],
            )
            .await
            .map_err(storage)?;
        let mode = rows
            .next()
            .await
            .map_err(storage)?
            .map(|row| parse_policy_mode(row.get::<String>(0).map_err(storage)?))
            .transpose()?
            .unwrap_or(floe_actions::ActionAuthorityMode::Ask);
        self.check_access()?;
        Ok(mode)
    }

    pub async fn set_agent_action_policy(
        &self,
        mode: floe_actions::ActionAuthorityMode,
    ) -> Result<floe_actions::ActionAuthorityMode, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let mode_name = policy_mode_name(mode);
            let mut rows = transaction
                .query(
                    "SELECT mode FROM agent_action_policy WHERE person_id = ?",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let previous = rows
                .next()
                .await
                .map_err(storage)?
                .map(|row| parse_policy_mode(row.get::<String>(0).map_err(storage)?))
                .transpose()?
                .unwrap_or(floe_actions::ActionAuthorityMode::Ask);
            if previous != mode {
                transaction
                    .execute(
                        "INSERT INTO agent_action_policy (person_id, mode) VALUES (?, ?) ON CONFLICT(person_id) DO UPDATE SET mode = excluded.mode",
                        (self.person_id.to_string(), mode_name),
                    )
                    .await
                    .map_err(storage)?;
                self.invalidate_agent_actions_in_transaction(&transaction)
                    .await?;
            }
            self.check_access()?;
            Ok(mode)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(crate) async fn store_agent_action_envelope(
        &self,
        envelope: AgentActionEnvelope,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        envelope.validate(self.person_id)?;
        let digest = envelope.digest()?;
        let payload = serde_json::to_string(&envelope).map_err(|_| AgentFailure::InvalidInput)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let changed = transaction
                .execute(
                    "INSERT INTO agent_action_envelopes (execution_id, person_id, grant_id, grant_incarnation, grant_epoch, state, digest, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        envelope.action.execution_id.to_string(),
                        self.person_id.to_string(),
                        envelope.dependency.grant_id().as_uuid().to_string(),
                        envelope.dependency.grant_authority().incarnation().to_string(),
                        i64::try_from(envelope.dependency.grant_authority().access_epoch().get()).map_err(|_| AgentFailure::Conflict)?,
                        state_name(&envelope.action.state),
                        digest.clone(),
                        payload,
                    ),
                )
                .await
                .map_err(|error| match error {
                    turso::Error::Constraint(_) => AgentFailure::Conflict,
                    other => storage(other),
                })?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            Ok(AgentActionAdmission {
                envelope,
                digest,
            })
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn admit_agent_action_dispatch(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        self.admit_agent_action_dispatch_with_cancellation(
            execution_id,
            expected_digest,
            now,
            Cancellation::default(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn admit_agent_action_dispatch_with_cancellation(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        now: DateTime<Utc>,
        cancellation: Cancellation,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        self.admit_agent_action_dispatch_with_cancellation_and_fence(
            execution_id,
            expected_digest,
            now,
            cancellation,
            || Ok(()),
        )
        .await
    }

    pub(crate) async fn admit_agent_action_dispatch_with_cancellation_and_fence(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        now: DateTime<Utc>,
        cancellation: Cancellation,
        fence: impl Fn() -> Result<(), AgentFailure> + Send + Sync,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        if execution_id.is_nil() || !valid_digest(expected_digest) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let (mut envelope, stored_digest, state) = self
                .read_agent_action_transaction(&transaction, execution_id)
                .await?;
            if stored_digest != expected_digest
                || state != "approved"
                || envelope.action.state != CalendarActionState::Approved
                || !envelope.write_approval
                || envelope
                    .action
                    .approved_at
                    .is_none_or(|approved_at| approved_at > now)
                || envelope.action.expires_at <= now
            {
                return Err(AgentFailure::Conflict);
            }
            if envelope
                .action
                .agent_origin
                .as_ref()
                .is_some_and(|origin| origin.automatic)
                && self.agent_action_policy_in_transaction(&transaction).await?
                    != floe_actions::ActionAuthorityMode::Allow
            {
                return Err(AgentFailure::PolicyDenied);
            }
            self.validate_dependency_transaction(&transaction, &envelope.dependency, now)
                .await?;
            if cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            fence()?;
            envelope.action.state = CalendarActionState::Executing;
            let payload = serde_json::to_string(&envelope).map_err(|_| AgentFailure::InvalidInput)?;
            let changed = transaction
                .execute(
                    "UPDATE agent_action_envelopes SET state = 'executing', payload = ? WHERE execution_id = ? AND person_id = ? AND state = 'approved' AND digest = ?",
                    (
                        payload,
                        execution_id.to_string(),
                        self.person_id.to_string(),
                        expected_digest,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            Ok(AgentActionAdmission {
                envelope,
                digest: stored_digest,
            })
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn agent_action_admission(
        &self,
        execution_id: Uuid,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let (envelope, digest, _) = self
                .read_agent_action_transaction(&transaction, execution_id)
                .await?;
            Ok(AgentActionAdmission { envelope, digest })
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn agent_calendar_action(
        &self,
        execution_id: Uuid,
    ) -> Result<CalendarAction, AgentFailure> {
        Ok(self
            .agent_action_admission(execution_id)
            .await?
            .envelope
            .action)
    }

    pub async fn agent_calendar_actions(&self) -> Result<Vec<CalendarAction>, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let mut rows = transaction
                .query(
                    "SELECT execution_id FROM agent_action_envelopes WHERE person_id = ? ORDER BY execution_id",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let mut actions = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                let execution_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
                actions.push(
                    self.read_agent_action_transaction(&transaction, execution_id)
                        .await?
                        .0
                        .action,
                );
            }
            Ok(actions)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(crate) async fn decide_agent_action(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        if execution_id.is_nil() || !valid_digest(expected_digest) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let (mut envelope, stored_digest, state) = self
                .read_agent_action_transaction(&transaction, execution_id)
                .await?;
            if stored_digest != expected_digest || state != "pending" {
                return Err(AgentFailure::Conflict);
            }
            let policy = self.agent_action_policy_in_transaction(&transaction).await?;
            envelope.write_approval = approve
                && policy != floe_actions::ActionAuthorityMode::Deny
                && envelope.action.expires_at > now;
            envelope.action.state = if envelope.write_approval {
                envelope.action.approved_at = Some(now);
                CalendarActionState::Approved
            } else if approve && policy != floe_actions::ActionAuthorityMode::Deny {
                CalendarActionState::Blocked {
                    reason: if policy == floe_actions::ActionAuthorityMode::Deny {
                        floe_actions::ActionBlockReason::PolicyDenied
                    } else {
                        floe_actions::ActionBlockReason::Expired
                    },
                }
            } else {
                CalendarActionState::Rejected
            };
            let payload = serde_json::to_string(&envelope).map_err(|_| AgentFailure::InvalidInput)?;
            let updated_digest = envelope.digest()?;
            let changed = transaction
                .execute(
                    "UPDATE agent_action_envelopes SET state = ?, digest = ?, payload = ? WHERE execution_id = ? AND person_id = ? AND state = 'pending' AND digest = ?",
                    (
                        state_name(&envelope.action.state),
                        updated_digest.clone(),
                        payload,
                        execution_id.to_string(),
                        self.person_id.to_string(),
                        expected_digest,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            Ok(AgentActionAdmission {
                envelope,
                digest: updated_digest,
            })
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(crate) async fn cancel_agent_action(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        if execution_id.is_nil() || !valid_digest(expected_digest) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let (mut envelope, stored_digest, state) = self
                .read_agent_action_transaction(&transaction, execution_id)
                .await?;
            if stored_digest != expected_digest || !matches!(state.as_str(), "pending" | "approved") {
                return Err(AgentFailure::Conflict);
            }
            envelope.write_approval = false;
            envelope.action.state = CalendarActionState::Blocked {
                reason: floe_actions::ActionBlockReason::PolicyDenied,
            };
            let payload = serde_json::to_string(&envelope).map_err(|_| AgentFailure::InvalidInput)?;
            let updated_digest = envelope.digest()?;
            let changed = transaction
                .execute(
                    "UPDATE agent_action_envelopes SET state = 'blocked', digest = ?, payload = ? WHERE execution_id = ? AND person_id = ? AND state IN ('pending', 'approved') AND digest = ?",
                    (
                        updated_digest.clone(),
                        payload,
                        execution_id.to_string(),
                        self.person_id.to_string(),
                        expected_digest,
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            Ok(AgentActionAdmission {
                envelope,
                digest: updated_digest,
            })
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(crate) async fn settle_agent_action(
        &self,
        admission: &AgentActionAdmission,
        state: CalendarActionState,
    ) -> Result<CalendarAction, AgentFailure> {
        if admission.envelope.action.execution_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if !matches!(
            state,
            CalendarActionState::Succeeded { .. }
                | CalendarActionState::Unknown { .. }
                | CalendarActionState::Blocked { .. }
        ) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = async {
            self.ensure_agent_action_schema(&transaction).await?;
            let (mut envelope, digest, current_state) = self
                .read_agent_action_transaction(&transaction, admission.envelope.action.execution_id)
                .await?;
            if digest != admission.digest {
                return Err(AgentFailure::Conflict);
            }
            if current_state != "executing" {
                if envelope.action.state == state {
                    return Ok(envelope.action);
                }
                if current_state != "unknown"
                    || !matches!(
                        state,
                        CalendarActionState::Succeeded { .. }
                            | CalendarActionState::Unknown { .. }
                    )
                {
                    return Err(AgentFailure::Conflict);
                }
            }
            envelope.action.state = state;
            let payload = serde_json::to_string(&envelope).map_err(|_| AgentFailure::InvalidInput)?;
            let state_name = state_name(&envelope.action.state);
            let changed = transaction
                .execute(
                    "UPDATE agent_action_envelopes SET state = ?, payload = ? WHERE execution_id = ? AND person_id = ? AND state IN ('executing', 'unknown') AND digest = ?",
                    (
                        state_name,
                        payload,
                        envelope.action.execution_id.to_string(),
                        self.person_id.to_string(),
                        admission.digest.clone(),
                    ),
                )
                .await
                .map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            Ok(envelope.action)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn ensure_agent_action_schema(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query("SELECT version FROM agent_action_schema WHERE id = 1", ())
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Err(AgentFailure::VaultUnavailable);
        };
        if row.get::<i64>(0).map_err(storage)? != AGENT_ACTION_SCHEMA_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        Ok(())
    }

    async fn agent_action_policy_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<floe_actions::ActionAuthorityMode, AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT mode FROM agent_action_policy WHERE person_id = ?",
                [self.person_id.to_string()],
            )
            .await
            .map_err(storage)?;
        rows.next()
            .await
            .map_err(storage)?
            .map(|row| parse_policy_mode(row.get::<String>(0).map_err(storage)?))
            .transpose()
            .map(|mode| mode.unwrap_or(floe_actions::ActionAuthorityMode::Ask))
    }

    async fn read_agent_action_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        execution_id: Uuid,
    ) -> Result<(AgentActionEnvelope, String, String), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT state, digest, payload FROM agent_action_envelopes WHERE execution_id = ? AND person_id = ?",
                (execution_id.to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Err(AgentFailure::NotFound);
        };
        let state = row.get::<String>(0).map_err(storage)?;
        let stored_digest = row.get::<String>(1).map_err(storage)?;
        if !valid_digest(&stored_digest) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let payload = row.get::<String>(2).map_err(storage)?;
        if payload.is_empty() || payload.len() > MAX_AGENT_ACTION_BYTES {
            return Err(AgentFailure::VaultUnavailable);
        }
        let envelope: AgentActionEnvelope =
            serde_json::from_str(&payload).map_err(|_| AgentFailure::VaultUnavailable)?;
        envelope
            .validate(self.person_id)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if state_name(&envelope.action.state) != state {
            return Err(AgentFailure::VaultUnavailable);
        }
        if envelope.digest()? != stored_digest {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok((envelope, stored_digest, state))
    }

    async fn validate_dependency_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &ContextDependency,
        now: DateTime<Utc>,
    ) -> Result<(), AgentFailure> {
        if dependency.expires_at() <= now {
            return Err(AgentFailure::CapabilityDenied);
        }
        let coverage = DependencyCoverage::dependent(dependency.clone())
            .map_err(|_| AgentFailure::PolicyDenied)?;
        self.validate_context_dependency_coverage_in_transaction(transaction, &coverage)
            .await
    }
}

fn parse_policy_mode(value: String) -> Result<floe_actions::ActionAuthorityMode, AgentFailure> {
    match value.as_str() {
        "allow" => Ok(floe_actions::ActionAuthorityMode::Allow),
        "ask" => Ok(floe_actions::ActionAuthorityMode::Ask),
        "deny" => Ok(floe_actions::ActionAuthorityMode::Deny),
        _ => Err(AgentFailure::VaultUnavailable),
    }
}

fn storage(_: turso::Error) -> AgentFailure {
    AgentFailure::StorageUnavailable
}
