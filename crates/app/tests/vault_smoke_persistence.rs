#![cfg(all(unix, feature = "development-storage"))]

#[path = "../examples/vault_keyring_smoke/persistence.rs"]
mod persistence;

use floe_kernel::PersonId;
use floe_vault::{DevelopmentVaultKeys, EncryptedAgentVault};

#[tokio::test]
async fn keyring_diagnostic_storage_steps_reopen_with_current_session_format() {
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let person = PersonId::new();
    let keys = DevelopmentVaultKeys::new(root.path()).unwrap();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let expected = persistence::persist_session(&vault, person)
        .await
        .expect("diagnostic storage must work before any OS Keychain checks");
    assert!(expected.session.messages.is_empty());
    assert_eq!(expected.session.revision, 1);
    drop(vault);
    let reopened = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    persistence::verify_session(&reopened, &expected)
        .await
        .unwrap();
    let mut mismatched = persistence::PersistenceProbe {
        session: expected.session.clone(),
        run: expected.run.clone(),
    };
    mismatched.session.revision += 1;
    assert_eq!(
        persistence::verify_session(&reopened, &mismatched).await,
        Err(floe_agent_contract::AgentFailure::StorageUnavailable)
    );
    use floe_conversation::{AgentMessage, SessionStore};
    mismatched.session.messages.push(AgentMessage::User {
        message_id: uuid::Uuid::new_v4(),
        turn_id: uuid::Uuid::new_v4(),
        text: "Obsolete inline diagnostic payload".into(),
    });
    assert_eq!(
        reopened
            .compare_and_swap(&mismatched.session, expected.session.revision)
            .await,
        Err(floe_agent_contract::AgentFailure::UnsupportedVersion)
    );
    persistence::verify_session(&reopened, &expected)
        .await
        .unwrap();
}
