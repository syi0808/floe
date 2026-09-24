//! Durable Access-owned recipient consent storage.
//!
//! Consents live in the same encrypted Vault as grants and interactions.
//! Identity is deterministic in the reviewed content (see
//! [`floe_access::recipient_consent_id`]), so the primary key on that id is
//! the uniqueness constraint: replays and re-grants settle the same row.
//! Every read revalidates the payload and cross-checks indexed columns, so a
//! tampered row fails closed instead of authorizing a dispatch.

use floe_access::RecipientConsent;
use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

use super::*;

const SCHEMA_VERSION: i64 = 1;
const MAX_CONSENT_PAYLOAD_BYTES: usize = 128 * 1024;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn grant_recipient_consent_record(
        &self,
        consent: RecipientConsent,
    ) -> Result<RecipientConsent, AgentFailure> {
        consent
            .validate()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if consent.person_id() != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            upsert_consent(&transaction, &consent).await?;
            self.check_access()?;
            Ok(consent)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn recipient_consent(
        &self,
        consent_id: Uuid,
    ) -> Result<Option<RecipientConsent>, AgentFailure> {
        if consent_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let record = read_consent(&self.connection()?, self.person_id, consent_id).await?;
        self.check_access()?;
        Ok(record)
    }

    pub async fn revoke_recipient_consent_record(
        &self,
        consent_id: Uuid,
    ) -> Result<(), AgentFailure> {
        if consent_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let Some(existing) = read_consent_on(&transaction, self.person_id, consent_id).await?
            else {
                return Err(AgentFailure::NotFound);
            };
            if existing.state() == floe_access::RecipientConsentState::Revoked {
                self.check_access()?;
                return Ok(());
            }
            let revoked = existing
                .revoked()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            upsert_consent(&transaction, &revoked).await?;
            self.check_access()?;
            Ok(())
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn prune_expired_recipient_consents(
        &self,
        now_unix_ms: i64,
    ) -> Result<u64, AgentFailure> {
        if now_unix_ms < 0 {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        if !table_exists(&connection, "recipient_consents").await? {
            self.check_access()?;
            return Ok(0);
        }
        let removed = connection
            .execute(
                "DELETE FROM recipient_consents WHERE expires_at <= ?",
                [now_unix_ms],
            )
            .await
            .map_err(storage)?;
        self.check_access()?;
        Ok(removed)
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
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('recipient_consent_schema', 'recipient_consents')",
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
                "CREATE TABLE recipient_consent_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE recipient_consents (consent_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, recipient TEXT NOT NULL, profile_id TEXT NOT NULL, session_id TEXT NOT NULL, origin_run_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('active', 'revoked')), revision INTEGER NOT NULL CHECK (revision > 0), created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX recipient_consents_expiry ON recipient_consents (expires_at)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO recipient_consent_schema (id, version) VALUES (1, 1)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found
        != [
            "recipient_consent_schema".to_owned(),
            "recipient_consents".to_owned(),
        ]
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut marker = transaction
        .query("SELECT id, version FROM recipient_consent_schema", ())
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
            "SELECT consent_id, person_id, recipient, profile_id, session_id, origin_run_id, state, revision, created_at, expires_at, payload FROM recipient_consents LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    let mut index = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'recipient_consents_expiry' AND tbl_name = 'recipient_consents'",
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

async fn read_consent(
    connection: &turso::Connection,
    person_id: PersonId,
    consent_id: Uuid,
) -> Result<Option<RecipientConsent>, AgentFailure> {
    if !table_exists(connection, "recipient_consents").await? {
        return Ok(None);
    }
    let mut rows = connection
        .query(
            "SELECT consent_id, person_id, recipient, profile_id, session_id, origin_run_id, state, revision, created_at, expires_at, payload FROM recipient_consents WHERE consent_id = ?",
            [consent_id.to_string()],
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

async fn read_consent_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    consent_id: Uuid,
) -> Result<Option<RecipientConsent>, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT consent_id, person_id, recipient, profile_id, session_id, origin_run_id, state, revision, created_at, expires_at, payload FROM recipient_consents WHERE consent_id = ?",
            [consent_id.to_string()],
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
) -> Result<RecipientConsent, AgentFailure> {
    let record: RecipientConsent =
        serde_json::from_str(&row.get::<String>(10).map_err(storage)?).map_err(unavailable)?;
    record
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let state_name = match record.state() {
        floe_access::RecipientConsentState::Active => "active",
        floe_access::RecipientConsentState::Revoked => "revoked",
    };
    if record.person_id() != person_id
        || row.get::<String>(0).map_err(storage)? != record.id().to_string()
        || row.get::<String>(1).map_err(storage)? != record.person_id().to_string()
        || row.get::<String>(2).map_err(storage)? != record.recipient()
        || row.get::<String>(3).map_err(storage)? != record.profile_id()
        || row.get::<String>(4).map_err(storage)? != record.lineage().session_id().to_string()
        || row.get::<String>(5).map_err(storage)? != record.lineage().origin_run_id().to_string()
        || row.get::<String>(6).map_err(storage)? != state_name
        || row.get::<i64>(7).map_err(storage)? != integer(record.revision())?
        || row.get::<i64>(8).map_err(storage)?
            != record.created_at().timestamp_millis()
        || row.get::<i64>(9).map_err(storage)? != record.expires_at().timestamp_millis()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(record)
}

async fn upsert_consent(
    transaction: &Transaction<'_>,
    consent: &RecipientConsent,
) -> Result<(), AgentFailure> {
    let payload = serde_json::to_vec(consent).map_err(|_| AgentFailure::VaultUnavailable)?;
    if payload.len() > MAX_CONSENT_PAYLOAD_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let payload = String::from_utf8(payload).map_err(|_| AgentFailure::VaultUnavailable)?;
    let state_name = match consent.state() {
        floe_access::RecipientConsentState::Active => "active",
        floe_access::RecipientConsentState::Revoked => "revoked",
    };
    let parameters: Vec<turso::Value> = vec![
        consent.id().to_string().into(),
        consent.person_id().to_string().into(),
        consent.recipient().to_owned().into(),
        consent.profile_id().to_owned().into(),
        consent.lineage().session_id().to_string().into(),
        consent.lineage().origin_run_id().to_string().into(),
        state_name.to_owned().into(),
        integer(consent.revision())?.into(),
        consent.created_at().timestamp_millis().into(),
        consent.expires_at().timestamp_millis().into(),
        payload.into(),
    ];
    transaction
        .execute(
            "INSERT OR REPLACE INTO recipient_consents (consent_id, person_id, recipient, profile_id, session_id, origin_run_id, state, revision, created_at, expires_at, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            parameters,
        )
        .await
        .map_err(storage)?;
    Ok(())
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::VaultUnavailable)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
    };

    use chrono::{DateTime, Duration, Utc};
    use floe_access::{RECIPIENT_CONSENT_TTL, RecipientConsentState};
    use floe_context_contract::{DataClass, RecipientLineage};

    use super::*;

    #[derive(Clone, Default)]
    struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for Keys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn consent(person_id: PersonId, created: DateTime<Utc>) -> RecipientConsent {
        RecipientConsent::try_new(
            person_id,
            "device",
            "client",
            "model.example",
            "server-model",
            "everyday_assistance",
            "conversation.root",
            vec![DataClass::Personal],
            vec![],
            RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap(),
            Uuid::new_v4(),
            1,
            created,
        )
        .unwrap()
    }

    async fn setup() -> (tempfile::TempDir, EncryptedAgentVault<Keys>, PersonId, Keys) {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = Keys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        (root, vault, person_id, keys)
    }

    #[tokio::test]
    async fn grant_find_revoke_and_prune_round_trip() {
        let (_root, vault, person_id, _keys) = setup().await;
        assert!(vault.recipient_consent(Uuid::new_v4()).await.unwrap().is_none());
        let granted = vault
            .grant_recipient_consent_record(consent(person_id, now()))
            .await
            .unwrap();
        let found = vault.recipient_consent(granted.id()).await.unwrap().unwrap();
        assert_eq!(found, granted);
        assert!(found.is_usable_at(now()));
        // Re-grant upserts the same content-bound row.
        let rereview = RecipientConsent::try_new(
            person_id,
            "device",
            "client",
            "model.example",
            "server-model",
            "everyday_assistance",
            "conversation.root",
            vec![DataClass::Personal],
            vec![],
            granted.lineage(),
            Uuid::new_v4(),
            2,
            now() + Duration::seconds(1),
        )
        .unwrap();
        assert_eq!(rereview.id(), granted.id());
        vault.grant_recipient_consent_record(rereview).await.unwrap();
        // Revoke ends it and is idempotent; unknown revocation is NotFound.
        vault.revoke_recipient_consent_record(granted.id()).await.unwrap();
        vault.revoke_recipient_consent_record(granted.id()).await.unwrap();
        let revoked = vault.recipient_consent(granted.id()).await.unwrap().unwrap();
        assert_eq!(revoked.state(), RecipientConsentState::Revoked);
        assert!(!revoked.is_usable_at(now() + Duration::seconds(2)));
        assert_eq!(
            vault.revoke_recipient_consent_record(Uuid::new_v4()).await,
            Err(AgentFailure::NotFound)
        );
        // Expired rows prune.
        let stale = consent(person_id, now() - RECIPIENT_CONSENT_TTL - Duration::seconds(1));
        vault.grant_recipient_consent_record(stale.clone()).await.unwrap();
        let removed = vault
            .prune_expired_recipient_consents(now().timestamp_millis())
            .await
            .unwrap();
        assert_eq!(removed, 1);
        assert!(vault.recipient_consent(stale.id()).await.unwrap().is_none());
        // The revoked-but-unexpired row survives pruning.
        assert!(vault.recipient_consent(granted.id()).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn consent_survives_vault_reopen() {
        let (root, vault, person_id, keys) = setup().await;
        let granted = vault
            .grant_recipient_consent_record(consent(person_id, now()))
            .await
            .unwrap();
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        let found = reopened.recipient_consent(granted.id()).await.unwrap().unwrap();
        assert_eq!(found, granted);
        assert!(found.is_usable_at(now()));
    }

    #[tokio::test]
    async fn foreign_person_consent_is_rejected() {
        let (_root, vault, person_id, _keys) = setup().await;
        let foreign = consent(PersonId::new(), now());
        assert_ne!(foreign.person_id(), person_id);
        assert_eq!(
            vault.grant_recipient_consent_record(foreign).await,
            Err(AgentFailure::CapabilityDenied)
        );
    }
}
