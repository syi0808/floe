//! Immutable physical Vault lifecycle outcomes. App owns their execution and
//! release; the adapter only archives exact receipts to bound the in-memory cache.
use crate::{StoreError, StoreErrorCode, TursoStore};
use floe_kernel::{AgentFailure, OwnerActor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredVaultLifecycleReceipt {
    pub operation_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub runtime_epoch: u64,
    pub intent: String,
    pub state: Option<String>,
    pub failure: Option<AgentFailure>,
}
impl StoredVaultLifecycleReceipt {
    fn validate(&self) -> Result<(), StoreError> {
        OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: self.runtime_epoch,
        }
        .validate()
        .map_err(|_| invalid())?;
        if self.operation_id.is_nil()
            || !matches!(self.intent.as_str(), "create" | "unlock" | "lock")
            || self.state.is_some() == self.failure.is_some()
            || self
                .state
                .as_deref()
                .is_some_and(|s| !matches!(s, "missing" | "locked" | "ready" | "unavailable"))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn invalid() -> StoreError {
    StoreError::new(StoreErrorCode::Storage, "invalid Vault lifecycle receipt")
}
fn unavailable(_: impl std::fmt::Debug) -> StoreError {
    StoreError::new(
        StoreErrorCode::Storage,
        "Vault lifecycle receipt storage is unavailable",
    )
}
fn decode(raw: String) -> Result<StoredVaultLifecycleReceipt, StoreError> {
    if raw.len() > 4096 {
        return Err(invalid());
    }
    let value: StoredVaultLifecycleReceipt = serde_json::from_str(&raw).map_err(unavailable)?;
    value.validate()?;
    Ok(value)
}
impl TursoStore {
    pub async fn load_vault_lifecycle_receipt(
        &self,
        id: Uuid,
    ) -> Result<Option<StoredVaultLifecycleReceipt>, StoreError> {
        if id.is_nil() {
            return Err(invalid());
        }
        let connection = self.connection().await?;
        let mut rows = connection
            .query(
                "SELECT payload FROM vault_lifecycle_receipts WHERE operation_id=?",
                (id.to_string(),),
            )
            .await
            .map_err(unavailable)?;
        let Some(row) = rows.next().await.map_err(unavailable)? else {
            return Ok(None);
        };
        let value = decode(row.get::<String>(0).map_err(unavailable)?)?;
        if value.operation_id != id {
            return Err(invalid());
        }
        Ok(Some(value))
    }
    pub async fn archive_vault_lifecycle_receipt(
        &self,
        value: StoredVaultLifecycleReceipt,
    ) -> Result<(), StoreError> {
        value.validate()?;
        let mut connection = self.connection().await?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(unavailable)?;
        let result = async {
            let mut rows = transaction
                .query(
                    "SELECT payload FROM vault_lifecycle_receipts WHERE operation_id=?",
                    (value.operation_id.to_string(),),
                )
                .await
                .map_err(unavailable)?;
            if let Some(row) = rows.next().await.map_err(unavailable)? {
                if decode(row.get::<String>(0).map_err(unavailable)?)? != value {
                    return Err(invalid());
                }
                return Ok(());
            }
            drop(rows);
            let payload = serde_json::to_string(&value).map_err(unavailable)?;
            if payload.len() > 4096 {
                return Err(invalid());
            }
            transaction
                .execute(
                    "INSERT INTO vault_lifecycle_receipts VALUES(?,?)",
                    (value.operation_id.to_string(), payload),
                )
                .await
                .map_err(unavailable)?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => transaction.commit().await.map_err(unavailable),
            Err(error) => {
                transaction.rollback().await.map_err(unavailable)?;
                Err(error)
            }
        }
    }
}
