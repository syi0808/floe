use super::*;
use crate::RootKey;
use floe_kernel::PersonId;
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Default)]
struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);
impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<RootKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person, vault))
            .copied()
            .map(RootKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }
    fn insert(&self, person: PersonId, vault: Uuid, key: &RootKey) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person, vault), *key.as_bytes());
        Ok(())
    }
}
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn fixture() -> (Root, Keys, EncryptedAgentVault<Keys>, ConnectionsRecord) {
    let root =
        Root(std::env::temp_dir().join(format!("floe-connection-publication-{}", Uuid::new_v4())));
    std::fs::create_dir(&root.0).unwrap();
    std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(&root.0, person, keys.clone())
        .await
        .unwrap();
    let record = ConnectionsRecord {
        record_ref: Uuid::new_v4(),
        person_id: person,
        device_id: "synthetic-device".into(),
        command_id: Uuid::new_v4(),
        intent_digest: [23; 32],
        revision: 1,
        payload: ConnectionsPayload::ObserveReview {
            reference: floe_access::ReviewRef {
                id: Uuid::new_v4(),
                revision: 1,
                digest: [24; 32],
            },
            source_ref: Uuid::new_v4(),
            source_revision: 1,
            choice: ProcessingChoice::DeviceOnly,
        },
    };
    record.validate().unwrap();
    (root, keys, vault, record)
}
#[tokio::test]
async fn admitted_review_publication_survives_short_writer_contention_and_replays_read_only() {
    let (root, keys, vault, record) = fixture().await;
    let mut connection = vault.connection().unwrap();
    let writer = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    let (result, ()) = tokio::join!(
        ConnectionsProductRepository::insert(&vault, record.clone()),
        async {
            tokio::time::sleep(Duration::from_millis(60)).await;
            writer.rollback().await.unwrap();
        }
    );
    assert_eq!(
        result.expect("the exact local publication survives a short unrelated writer"),
        record
    );
    let writer = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        ConnectionsProductRepository::insert(&vault, record.clone())
            .await
            .unwrap(),
        record,
        "exact existing publication is a read, even while the writer remains held"
    );
    let mut different = record.clone();
    different.intent_digest[0] ^= 1;
    assert_eq!(
        ConnectionsProductRepository::insert(&vault, different).await,
        Err(AgentFailure::Conflict)
    );
    writer.rollback().await.unwrap();
    connection
        .execute(
            "UPDATE connections_product_records SET command_id = ? WHERE record_ref = ?",
            (Uuid::new_v4().to_string(), record.record_ref.to_string()),
        )
        .await
        .unwrap();
    assert_eq!(
        ConnectionsProductRepository::insert(&vault, record.clone()).await,
        Err(AgentFailure::StorageUnavailable)
    );
    connection
        .execute(
            "UPDATE connections_product_records SET command_id = ? WHERE record_ref = ?",
            (record.command_id.to_string(), record.record_ref.to_string()),
        )
        .await
        .unwrap();
    let rejection = ConnectionsCommandRejection {
        identity: ConnectionsCommandIdentity {
            journal: ConnectionsCommandJournal::Product,
            record_ref: record.record_ref,
            person_id: record.person_id,
            device_id: record.device_id.clone(),
            command_id: record.command_id,
            intent_digest: record.intent_digest,
        },
        reason: AgentFailure::Conflict,
    };
    connection
        .execute(
            "INSERT INTO connections_command_rejections VALUES (?,?,?)",
            (
                record.person_id.to_string(),
                record.command_id.to_string(),
                bounded_encode(&rejection).unwrap(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        ConnectionsProductRepository::insert(&vault, record.clone()).await,
        Err(AgentFailure::Conflict),
        "a fast replay cannot ignore conflicting negative custody"
    );
    connection
        .execute(
            "DELETE FROM connections_command_rejections WHERE person_id = ? AND command_id = ?",
            (record.person_id.to_string(), record.command_id.to_string()),
        )
        .await
        .unwrap();
    drop(connection);
    drop(vault);
    let reopened = EncryptedAgentVault::open(&root.0, record.person_id, keys)
        .await
        .unwrap();
    assert_eq!(
        ConnectionsProductRepository::insert(&reopened, record.clone())
            .await
            .unwrap(),
        record
    );
}
#[tokio::test]
async fn sustained_publication_contention_remains_uncertain_and_does_not_retire_store() {
    let (_root, _keys, vault, record) = fixture().await;
    let mut connection = vault.connection().unwrap();
    let writer = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        ConnectionsProductRepository::insert(&vault, record.clone()).await,
        Err(AgentFailure::StorageBusy)
    );
    assert!(
        ConnectionsProductRepository::load(&vault, record.person_id, record.record_ref)
            .await
            .unwrap()
            .is_none()
    );
    vault.check_access().unwrap();
    writer.rollback().await.unwrap();
    assert_eq!(
        ConnectionsProductRepository::insert(&vault, record.clone())
            .await
            .unwrap(),
        record
    );
}
