use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

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

fn request(
    person_id: PersonId,
    session_id: Uuid,
    run_id: RunId,
    command_id: CommandId,
) -> VaultConversationAdmissionRequest {
    VaultConversationAdmissionRequest {
        run_id,
        command_id,
        session_id,
        person_id,
        expected_session_revision: 0,
        request_digest: [7; 32],
        text: "hello".into(),
    }
}

#[tokio::test]
async fn admission_journal_and_terminal_claim_commit_survive_reopen() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let mut vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    let command_id = CommandId::new();
    let admission = request(person_id, session.id, run_id, command_id);

    let VaultConversationAdmission::Created {
        record,
        session: admitted_session,
    } = vault
        .admit_conversation_turn(admission.clone())
        .await
        .unwrap()
    else {
        panic!("expected created admission");
    };
    assert_eq!(record.state, VaultConversationRunState::Working);
    assert_eq!(record.session_revision, 1);
    assert_eq!(admitted_session.active_turn, Some(run_id.as_uuid()));
    assert!(matches!(
        admitted_session.messages.as_slice(),
        [AgentMessage::User { turn_id, text }] if *turn_id == run_id.as_uuid() && text == "hello"
    ));
    assert_eq!(
        vault
            .admit_conversation_turn(admission.clone())
            .await
            .unwrap(),
        VaultConversationAdmission::Existing(record.clone())
    );
    let mut changed = admission;
    changed.request_digest = [8; 32];
    assert_eq!(
        vault.admit_conversation_turn(changed).await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        vault
            .append_conversation_journal(run_id, "model_intent", "{\"attempt\":1}")
            .await
            .unwrap(),
        1
    );

    let terminal = vault
        .finish_conversation_run(
            run_id,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::Completed,
                output: Some("hello back".into()),
                coverage: DependencyCoverage::Independent,
                issue: None,
                appended_messages: vec![AgentMessage::Assistant {
                    turn_id: run_id.as_uuid(),
                    text: "hello back".into(),
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!(terminal.state, VaultConversationRunState::Completed);
    assert_eq!(terminal.aggregate_revision, 2);
    assert_eq!(terminal.journal_revision, 1);
    assert_eq!(terminal.session_revision, 2);
    assert_eq!(
        vault
            .append_conversation_journal(run_id, "checkpoint", "{}")
            .await,
        Err(AgentFailure::Conflict)
    );
    let completed_session = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(completed_session.revision, 2);
    assert_eq!(completed_session.active_turn, None);
    assert_eq!(
        completed_session.last_outcome,
        Some(AgentOutcome::Completed)
    );

    drop(vault);
    vault = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    assert_eq!(
        vault.conversation_run(run_id).await.unwrap(),
        Some(terminal)
    );
}

#[tokio::test]
async fn active_claim_blocks_another_command_and_invalid_terminal_cannot_release_it() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person_id, Keys::default())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    vault
        .admit_conversation_turn(request(person_id, session.id, run_id, CommandId::new()))
        .await
        .unwrap();
    let mut second = request(person_id, session.id, RunId::new(), CommandId::new());
    second.expected_session_revision = 1;
    assert_eq!(
        vault.admit_conversation_turn(second).await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        vault
            .finish_conversation_run(
                run_id,
                1,
                VaultConversationTerminal {
                    state: VaultConversationRunState::Completed,
                    output: Some("expected".into()),
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                    appended_messages: vec![AgentMessage::Assistant {
                        turn_id: run_id.as_uuid(),
                        text: "forged".into(),
                    }],
                },
            )
            .await,
        Err(AgentFailure::InvalidInput)
    );
    assert_eq!(
        vault.load(person_id, session.id).await.unwrap().active_turn,
        Some(run_id.as_uuid())
    );
    let cancelled = vault
        .finish_conversation_run(
            run_id,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::Cancelled,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::Cancelled),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(cancelled.state, VaultConversationRunState::Cancelled);
    assert_eq!(
        vault.load(person_id, session.id).await.unwrap().active_turn,
        None
    );
}
