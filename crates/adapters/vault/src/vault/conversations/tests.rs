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
        continuation: None,
        retry_of: None,
        model_placement: ModelPlacement::DeviceLocal,
    }
}

#[tokio::test]
async fn cancel_command_receipt_replays_after_reopen_and_conflicts_on_changed_target() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    let start_command_id = CommandId::new();
    vault
        .admit_conversation_turn(request(person_id, session.id, run_id, start_command_id))
        .await
        .unwrap();
    let cancel = VaultConversationCancelRequest {
        command_id: CommandId::new(),
        run_id,
        person_id,
    };
    let receipt = VaultConversationCancelReceipt {
        command_id: cancel.command_id,
        run_id,
        person_id,
    };
    assert_eq!(
        vault
            .admit_conversation_cancel(cancel.clone())
            .await
            .unwrap(),
        VaultConversationCancelAdmission::Created(receipt.clone())
    );
    assert_eq!(
        vault
            .admit_conversation_cancel(VaultConversationCancelRequest {
                command_id: start_command_id,
                ..cancel.clone()
            })
            .await,
        Err(AgentFailure::Conflict)
    );
    drop(vault);

    let vault = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    assert_eq!(
        vault
            .admit_conversation_cancel(cancel.clone())
            .await
            .unwrap(),
        VaultConversationCancelAdmission::Existing(receipt)
    );
    assert_eq!(
        vault
            .admit_conversation_cancel(VaultConversationCancelRequest {
                run_id: RunId::new(),
                ..cancel
            })
            .await,
        Err(AgentFailure::Conflict)
    );
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
    assert_eq!(
        vault
            .activate_conversation_executor()
            .await
            .unwrap()
            .executor_generation,
        1
    );
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
    assert_eq!(
        vault.conversation_journal(run_id).await.unwrap(),
        vec![VaultConversationJournalEntry {
            revision: 1,
            kind: "model_intent".into(),
            payload: "{\"attempt\":1}".into(),
        }]
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
    let activation = vault.activate_conversation_executor().await.unwrap();
    assert_eq!(activation.executor_generation, 2);
    assert!(activation.interrupted.is_empty());
    assert_eq!(
        vault.conversation_run(run_id).await.unwrap(),
        Some(terminal)
    );
}

#[tokio::test]
async fn failed_execution_can_commit_a_distinct_final_reply_without_continuation() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person_id, Keys::default())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    vault
        .admit_conversation_turn(request(person_id, session.id, run_id, CommandId::new()))
        .await
        .unwrap();

    let failed = vault
        .finish_conversation_run(
            run_id,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::Failed,
                output: Some("The lookup succeeded, but the request did not complete.".into()),
                coverage: DependencyCoverage::Independent,
                issue: Some(AgentFailure::Stalled),
                appended_messages: vec![AgentMessage::Assistant {
                    turn_id: run_id.as_uuid(),
                    text: "The lookup succeeded, but the request did not complete.".into(),
                }],
            },
        )
        .await
        .unwrap();

    assert_eq!(failed.state, VaultConversationRunState::Failed);
    assert!(failed.output.is_some());
    let session = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(session.continuation, None);
    assert_eq!(
        session.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Stalled
        })
    );
    assert!(matches!(
        session.messages.as_slice(),
        [AgentMessage::User { .. }, AgentMessage::Assistant { text, .. }]
            if text == "The lookup succeeded, but the request did not complete."
    ));
}

#[tokio::test]
async fn continuation_admission_is_generation_bound_and_preserves_one_user_message() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person_id, Keys::default())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    let VaultConversationAdmission::Created { record, .. } = vault
        .admit_conversation_turn(request(person_id, session.id, run_id, CommandId::new()))
        .await
        .unwrap()
    else {
        panic!("expected created admission");
    };
    let timed_out = vault
        .finish_conversation_run(
            run_id,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                appended_messages: vec![],
            },
        )
        .await
        .unwrap();
    let stopped_session = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(
        stopped_session.continuation,
        Some(AgentContinuation {
            turn_id: run_id.as_uuid(),
            level: 0,
            usage: AgentUsage::default(),
            placement: ModelPlacement::DeviceLocal,
        })
    );

    let next_run_id = RunId::new();
    let next_command_id = CommandId::new();
    let mut continuation = request(person_id, session.id, next_run_id, next_command_id);
    continuation.expected_session_revision = timed_out.session_revision;
    continuation.continuation = Some(VaultConversationContinuationRef {
        run_id,
        executor_generation: record.executor_generation + 1,
        level: 1,
    });
    assert_eq!(
        vault.admit_conversation_turn(continuation.clone()).await,
        Err(AgentFailure::Conflict)
    );

    continuation
        .continuation
        .as_mut()
        .unwrap()
        .executor_generation = record.executor_generation;
    let VaultConversationAdmission::Created {
        record: continued,
        session: active_session,
    } = vault.admit_conversation_turn(continuation).await.unwrap()
    else {
        panic!("expected continuation admission");
    };
    assert_eq!(continued.continuation_of, Some(run_id));
    assert_eq!(
        continued.continuation_executor_generation,
        Some(record.executor_generation)
    );
    assert_eq!(continued.continuation_level, 1);
    assert_eq!(active_session.messages.len(), 1);
    assert!(matches!(
        active_session.messages.as_slice(),
        [AgentMessage::User { turn_id, .. }] if *turn_id == run_id.as_uuid()
    ));
}

#[tokio::test]
async fn retry_admission_persists_terminal_source_and_replay_lineage() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person_id, Keys::default())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let source_run_id = RunId::new();
    vault
        .admit_conversation_turn(request(
            person_id,
            session.id,
            source_run_id,
            CommandId::new(),
        ))
        .await
        .unwrap();
    let source = vault
        .finish_conversation_run(
            source_run_id,
            1,
            VaultConversationTerminal {
                state: VaultConversationRunState::Completed,
                output: Some("done".into()),
                coverage: DependencyCoverage::Independent,
                issue: None,
                appended_messages: vec![AgentMessage::Assistant {
                    turn_id: source_run_id.as_uuid(),
                    text: "done".into(),
                }],
            },
        )
        .await
        .unwrap();

    let retry_run_id = RunId::new();
    let retry_command_id = CommandId::new();
    let mut retry = request(person_id, session.id, retry_run_id, retry_command_id);
    retry.expected_session_revision = source.session_revision;
    retry.retry_of = Some(source_run_id);
    let VaultConversationAdmission::Created { record, .. } =
        vault.admit_conversation_turn(retry.clone()).await.unwrap()
    else {
        panic!("expected retry admission");
    };
    assert_eq!(record.retry_of, Some(source_run_id));
    assert_eq!(
        vault
            .conversation_run(retry_run_id)
            .await
            .unwrap()
            .unwrap()
            .retry_of,
        Some(source_run_id)
    );
    assert_eq!(
        vault.admit_conversation_turn(retry.clone()).await.unwrap(),
        VaultConversationAdmission::Existing(record)
    );
    retry.retry_of = None;
    assert_eq!(
        vault.admit_conversation_turn(retry).await,
        Err(AgentFailure::Conflict)
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
    vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    vault
        .admit_conversation_turn(request(person_id, session.id, run_id, CommandId::new()))
        .await
        .unwrap();
    assert_eq!(
        vault
            .recover_conversation_session(session.id, person_id, 1)
            .await,
        Err(AgentFailure::Conflict)
    );
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
    assert_eq!(
        vault
            .recover_conversation_session(session.id, person_id, cancelled.session_revision)
            .await
            .unwrap(),
        cancelled.session_revision
    );
}

#[tokio::test]
async fn activation_interrupts_orphan_and_releases_claim_without_replaying_work() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
        .await
        .unwrap();
    let first_activation = vault.activate_conversation_executor().await.unwrap();
    let session = vault.create_session().await.unwrap();
    let run_id = RunId::new();
    let command_id = CommandId::new();
    let admission = request(person_id, session.id, run_id, command_id);
    let VaultConversationAdmission::Created { record, .. } = vault
        .admit_conversation_turn(admission.clone())
        .await
        .unwrap()
    else {
        panic!("expected created admission");
    };
    assert_eq!(
        record.executor_generation,
        first_activation.executor_generation
    );

    drop(vault);
    let vault = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    let recovery = vault.activate_conversation_executor().await.unwrap();
    assert_eq!(recovery.executor_generation, 2);
    assert_eq!(recovery.interrupted.len(), 1);
    let interrupted = &recovery.interrupted[0];
    assert_eq!(interrupted.run_id, run_id);
    assert_eq!(interrupted.state, VaultConversationRunState::Interrupted);
    assert_eq!(interrupted.issue, Some(AgentFailure::Interrupted));
    assert_eq!(interrupted.executor_generation, 2);
    assert_eq!(interrupted.aggregate_revision, 2);
    assert_eq!(interrupted.session_revision, 2);
    let recovered_session = vault.load(person_id, session.id).await.unwrap();
    assert_eq!(recovered_session.active_turn, None);
    assert_eq!(
        recovered_session.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Interrupted
        })
    );
    assert_eq!(
        vault
            .recover_conversation_session(session.id, person_id, interrupted.session_revision,)
            .await
            .unwrap(),
        interrupted.session_revision
    );
    assert_eq!(
        vault.admit_conversation_turn(admission).await.unwrap(),
        VaultConversationAdmission::Existing(interrupted.clone())
    );
    assert_eq!(
        vault
            .finish_conversation_run(
                run_id,
                1,
                VaultConversationTerminal {
                    state: VaultConversationRunState::Completed,
                    output: Some("late".into()),
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                    appended_messages: vec![AgentMessage::Assistant {
                        turn_id: run_id.as_uuid(),
                        text: "late".into(),
                    }],
                },
            )
            .await,
        Err(AgentFailure::Conflict)
    );

    let mut next = request(person_id, session.id, RunId::new(), CommandId::new());
    next.expected_session_revision = interrupted.session_revision;
    assert!(matches!(
        vault.admit_conversation_turn(next).await.unwrap(),
        VaultConversationAdmission::Created { .. }
    ));
}
