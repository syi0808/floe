//! Real encrypted submission journal. No native provider or external effect is invoked.
use super::*;
use crate::{EncryptedAgentVault, RootKey, VaultKeyProvider};
use chrono::{Duration, Utc};
use floe_access::OperationApprovalRef;
use floe_calendar_operations::*;
use floe_context_contract::{CalendarProvider, ConnectionId, SourceAuthority};
use floe_kernel::CommandFailure;
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
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
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("floe-calendar-admission-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn admission(person_id: PersonId) -> OperationAdmission {
    let device_id = "device-a".to_owned();
    let command_id = Uuid::new_v4();
    let connection_id = ConnectionId::new();
    let source = ActionSourceFence {
        connection_id: connection_id.clone(),
        revision: 1,
        authority: SourceAuthority::new(),
        execution_owner: format!("apple:{device_id}"),
        resources: vec!["calendar-work".to_owned()],
        native_subject_fingerprint: "native-subject".to_owned(),
    };
    let now = Utc::now();
    let expires_at = now + Duration::minutes(15);
    let effect = CalendarEffect::Create {
        destination: CalendarDestination {
            provider: CalendarProvider::EventKit,
            connection_id,
            connection_revision: 1,
            calendar_id: "calendar-work".to_owned(),
            calendar_name: "Work".to_owned(),
        },
        title: "Manual focus block".to_owned(),
        schedule: floe_day::TimedSchedule::new(
            now + Duration::hours(1),
            now + Duration::hours(2),
            "UTC",
        )
        .expect("valid schedule"),
    };
    let effect_digest = effect.digest().expect("valid effect digest");
    let id = action_uuid(b"floe.actions.action.v1\0", person_id, command_id);
    let review = OperationApprovalRef {
        id: action_uuid(b"floe.actions.review.v1\0", person_id, command_id),
        operation_id: id,
        effect_digest,
        source_digest: source.digest().expect("valid source digest"),
        person_id,
        device_id: device_id.clone(),
        policy_revision: None,
        created_at: now,
        expires_at,
    };
    let authorization = floe_access::direct_operation_receipt(command_id, &review.subject())
        .expect("valid direct operation receipt");
    let record = ActionRecord {
        id,
        person_id,
        device_id: device_id.clone(),
        revision: 1,
        origin: ActionOrigin::Direct {
            command_id,
            actor_device_id: device_id.clone(),
        },
        effect,
        effect_digest,
        execution_id: action_uuid(b"floe.actions.execution.v1\0", person_id, command_id),
        source: source.clone(),
        dependency: None,
        review,
        authorization: Some(authorization),
        created_at: now,
        expires_at,
        state: ActionState::Approved,
        execution: None,
        collection: None,
    };
    let admission = OperationAdmission {
        command_id,
        request_digest: [7; 32],
        publication: None,
        record,
    };
    admission
}

async fn create(root: &Root, person: PersonId, keys: &Keys) -> EncryptedAgentVault<Keys> {
    EncryptedAgentVault::create(&root.0, person, keys.clone())
        .await
        .unwrap()
}

#[tokio::test]
async fn negative_receipt_survives_reopen_and_excludes_prepared_admission() {
    let root = Root::new();
    let keys = Keys::default();
    let person = PersonId::new();
    let prepared = admission(person);
    let vault = create(&root, person, &keys).await;
    assert!(matches!(
        vault
            .actions_reject_admission(
                person,
                prepared.command_id,
                prepared.request_digest,
                AgentFailure::PolicyDenied
            )
            .await
            .unwrap(),
        OperationAdmissionResolution::Rejected(AgentFailure::PolicyDenied)
    ));
    drop(vault);
    let vault = EncryptedAgentVault::open(&root.0, person, keys)
        .await
        .unwrap();
    assert_eq!(
        vault
            .actions_find_admission(person, prepared.command_id, prepared.request_digest)
            .await
            .unwrap_err(),
        CalendarOperationStoreError::Rejected(AgentFailure::PolicyDenied)
    );
    assert!(matches!(
        vault.actions_admit(prepared.clone()).await,
        Err(CommandFailure::NotApplied(
            CalendarOperationStoreError::Rejected(AgentFailure::PolicyDenied)
        ))
    ));
    assert!(
        vault
            .actions_get(person, prepared.record.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        vault
            .actions_reject_admission(
                person,
                prepared.command_id,
                prepared.request_digest,
                AgentFailure::StaleContext
            )
            .await
            .unwrap(),
        OperationAdmissionResolution::Rejected(AgentFailure::PolicyDenied)
    ));
    let mut changed = prepared.clone();
    changed.request_digest = [9; 32];
    assert!(matches!(
        vault.actions_admit(changed).await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::Conflict
        ))
    ));
    assert!(matches!(
        vault
            .actions_reject_admission(
                person,
                prepared.command_id,
                [9; 32],
                AgentFailure::PolicyDenied
            )
            .await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::Conflict
        ))
    ));
}

#[tokio::test]
async fn positive_receipt_wins_over_late_rejection_and_reopens_exactly() {
    let root = Root::new();
    let keys = Keys::default();
    let person = PersonId::new();
    let prepared = admission(person);
    let vault = create(&root, person, &keys).await;
    assert!(
        !vault
            .actions_admit(prepared.clone())
            .await
            .unwrap()
            .replayed
    );
    let OperationAdmissionResolution::Admitted(record) = vault
        .actions_reject_admission(
            person,
            prepared.command_id,
            prepared.request_digest,
            AgentFailure::PolicyDenied,
        )
        .await
        .unwrap()
    else {
        panic!("positive receipt was replaced");
    };
    assert_eq!(record, prepared.record);
    drop(vault);
    let vault = EncryptedAgentVault::open(&root.0, person, keys)
        .await
        .unwrap();
    let replay = vault.actions_admit(prepared.clone()).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.record, prepared.record);
}

#[tokio::test]
async fn invalid_prepared_submission_is_durably_rejected_before_not_applied() {
    let root = Root::new();
    let keys = Keys::default();
    let person = PersonId::new();
    let prepared = admission(person);
    let vault = create(&root, person, &keys).await;
    let mut invalid = prepared.clone();
    invalid.record.revision = 2;
    assert!(matches!(
        vault.actions_admit(invalid).await,
        Err(CommandFailure::NotApplied(
            CalendarOperationStoreError::Rejected(_)
        ))
    ));
    assert!(matches!(
        vault.actions_admit(prepared.clone()).await,
        Err(CommandFailure::NotApplied(
            CalendarOperationStoreError::Rejected(_)
        ))
    ));
    assert!(
        vault
            .actions_get(person, prepared.record.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn composed_publication_rollback_fences_the_prepared_submission() {
    let root = Root::new();
    let keys = Keys::default();
    let person = PersonId::new();
    let prepared = admission(person);
    let vault = create(&root, person, &keys).await;
    let publication = floe_conversation::OperationApprovalPublication {
        session_id: Uuid::nil(),
        run_id: floe_agent_contract::RunId::new(),
        artifact_id: Uuid::new_v4(),
        execution: floe_agent_contract::TaskExecutionReceiptRef {
            execution: floe_agent_contract::TaskExecutionKey {
                task_id: floe_agent_contract::TaskId::new(),
                execution_id: Uuid::new_v4(),
                executor_generation: 1,
            },
            task_revision: 2,
            journal_revision: 0,
            digest: [3; 32],
        },
    };
    assert!(matches!(
        vault
            .admit_calendar_operation_and_conversation_interaction(prepared.clone(), publication)
            .await,
        Err(CommandFailure::NotApplied(AgentFailure::InvalidInput))
    ));
    assert!(
        vault
            .actions_get(person, prepared.record.id)
            .await
            .unwrap()
            .is_none()
    );
    drop(vault);
    let vault = EncryptedAgentVault::open(&root.0, person, keys)
        .await
        .unwrap();
    assert!(matches!(
        vault.actions_admit(prepared).await,
        Err(CommandFailure::NotApplied(
            CalendarOperationStoreError::Rejected(AgentFailure::InvalidInput)
        ))
    ));
}

#[tokio::test]
async fn foreign_person_and_occupied_command_cannot_create_negative_proof() {
    let root = Root::new();
    let keys = Keys::default();
    let person = PersonId::new();
    let prepared = admission(person);
    let vault = create(&root, person, &keys).await;
    assert!(matches!(
        vault
            .actions_reject_admission(
                PersonId::new(),
                prepared.command_id,
                prepared.request_digest,
                AgentFailure::PolicyDenied
            )
            .await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::InvalidRecord
        ))
    ));
    let mut connection = vault.connection().unwrap();
    let tx = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .unwrap();
    vault
        .insert_command_receipt(
            &tx,
            person,
            prepared.command_id,
            "authority",
            &[8; 32],
            None,
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        vault
            .actions_reject_admission(
                person,
                prepared.command_id,
                prepared.request_digest,
                AgentFailure::PolicyDenied
            )
            .await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::Conflict
        ))
    ));
    assert!(matches!(
        vault.actions_admit(prepared).await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::Conflict
        ))
    ));
}

#[tokio::test]
async fn unavailable_or_corrupt_rejection_never_proves_not_applied() {
    let root = Root::new();
    let keys = Keys::default();
    let person = PersonId::new();
    let prepared = admission(person);
    let vault = create(&root, person, &keys).await;
    vault
        .actions_reject_admission(
            person,
            prepared.command_id,
            prepared.request_digest,
            AgentFailure::PolicyDenied,
        )
        .await
        .unwrap();
    let connection = vault.connection().unwrap();
    connection
        .execute(
            "UPDATE actions_command_receipts SET rejection = ? WHERE command_id = ?",
            ("\"unknown_failure\"", prepared.command_id.to_string()),
        )
        .await
        .unwrap();
    assert!(matches!(
        vault.actions_admit(prepared.clone()).await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::CorruptRecord
        ))
    ));
    assert!(matches!(
        vault
            .actions_reject_admission(
                person,
                prepared.command_id,
                prepared.request_digest,
                AgentFailure::PolicyDenied
            )
            .await,
        Err(CommandFailure::Indeterminate(
            CalendarOperationStoreError::CorruptRecord
        ))
    ));
    vault.unavailable.store(true, Ordering::Release);
    assert!(matches!(
        vault
            .actions_reject_admission(
                person,
                Uuid::new_v4(),
                prepared.request_digest,
                AgentFailure::PolicyDenied
            )
            .await,
        Err(CommandFailure::Indeterminate(_))
    ));
}
