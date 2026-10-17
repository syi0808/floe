use super::*;
use floe_conversation::{SessionStartAdmission, StartSessionRequest};
use floe_conversation_contract::MessageId;
use floe_kernel::CommandId;
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
struct Fixture {
    root: PathBuf,
    keys: Keys,
    actor: OwnerActor,
    session: Uuid,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
async fn fixture() -> (Fixture, EncryptedAgentVault<Keys>) {
    let root = std::env::temp_dir().join(format!("floe-surface-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "device-a".into(),
        runtime_epoch: 1,
    };
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(&root, actor.person_id, keys.clone())
        .await
        .unwrap();
    let SessionStartAdmission::Started(receipt) = vault
        .start_conversation_session(StartSessionRequest {
            principal: actor.person_id.to_string(),
            command_id: CommandId::new(),
        })
        .await
        .unwrap()
    else {
        panic!("fresh session")
    };
    (
        Fixture {
            root,
            keys,
            actor,
            session: receipt.session_id,
        },
        vault,
    )
}
async fn request(f: &Fixture, vault: &EncryptedAgentVault<Keys>, text: &str) -> AppendSurfaceInput {
    AppendSurfaceInput {
        command_id: CommandId::new(),
        expected: vault.surface_head(&f.actor, f.session).await.unwrap(),
        input: floe_conversation::SurfaceInput {
            message_id: MessageId::new(),
            text: text.into(),
            origin: SurfaceInputOrigin::Text,
        },
    }
}
fn work(input: &SurfaceInputReceipt) -> SubmitSurfaceWork {
    SubmitSurfaceWork {
        command_id: CommandId::new(),
        anchor: input.through,
        input_id: input.input.message_id,
        origin: WorkOrigin::Text,
    }
}
fn execution(
    f: &Fixture,
    receipt: &SurfaceWorkReceipt,
    text: &str,
    revision: u64,
) -> floe_conversation::TurnAdmissionRequest {
    use floe_conversation::{CanonicalTurnIntent, TurnAdmissionRequest, TurnInput, TurnMode};
    let intent = CanonicalTurnIntent {
        session_id: f.session,
        expected_revision: revision,
        text: text.into(),
        mode: TurnMode::New,
        retry_of: receipt.origin.retry_of(),
    };
    TurnAdmissionRequest {
        expert_environment: floe_experts::RunExpertEnvironmentIdentity {
            revision: 1,
            digest: [41; 32],
        },
        run_id: floe_kernel::RunId::new(),
        command_id: receipt.execution_command_id(),
        session_id: f.session,
        expected_session_revision: revision,
        principal: f.actor.person_id.to_string(),
        device_id: f.actor.device_id.clone(),
        request_digest: intent.digest(&f.actor.person_id.to_string()).unwrap(),
        mode: TurnMode::New,
        retry_of: receipt.origin.retry_of(),
        input: TurnInput::NewMessage(floe_agent_contract::AgentMessage {
            message_id: receipt.execution_command_id().as_uuid(),
            role: floe_agent_contract::MessageRole::User,
            text: text.into(),
            call_id: None,
            coverage: floe_agent_contract::DependencyCoverage::Independent,
        }),
    }
}
async fn completed_terminal(
    vault: &EncryptedAgentVault<Keys>,
    run: &floe_conversation::RunRecord,
) -> floe_conversation::RunTerminal {
    use floe_agent_contract::*;
    let text = "Synthetic surface result".to_owned();
    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch_id = Uuid::new_v4();
    let events = vec![
        JournalEvent::ModelIntent {
            attempt_id,
            parent_task_id: None,
            reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                tokens: 128,
                cost_micros: 128,
            },
            projection_ref,
            plan: PreparedModelPlan {
                operation_id: Uuid::new_v4(),
                principal: run.person_id.to_string(),
                device_id: run.device_id.clone(),
                purpose: "everyday_assistance".into(),
                consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                capabilities: ModelCapabilities::chat(),
                boundary: ProcessingBoundary::Device,
                binding_digest: ModelBindingDigest([71; 32]),
                selection_commitment: ModelSelectionCommitment([72; 32]),
                budget_profile: ModelBudgetProfile::unknown(),
            },
        },
        JournalEvent::ModelResult {
            attempt_id,
            usage: ModelUsage::default(),
            accounting: floe_execution::budget::ModelAccounting {
                observed_tokens: None,
                observed_cost_micros: None,
                unknown_tokens: true,
                unknown_cost: true,
            },
        },
        JournalEvent::ValidatedBatch {
            batch: ValidatedModelBatch {
                execution_id: run.run_id.as_uuid(),
                attempt_id,
                projection_ref,
                batch_id,
                steps: vec![ModelStep::Answer {
                    text: text.clone(),
                    artifacts: vec![],
                }],
                catalog_revision: run.expert_environment.revision,
                tool_revisions: vec![],
                agent_revisions: vec![],
                projection_coverage: DependencyCoverage::Independent,
                delegation_context: None,
            },
        },
        JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id,
                next_step_index: 0,
            },
        },
        JournalEvent::Output {
            text: text.clone(),
            artifacts: vec![],
        },
    ];
    for event in events {
        vault
            .record_manager_conversation_journal(
                run.run_id,
                super::super::conversations::journal_kind(&event),
                event,
            )
            .await
            .unwrap();
    }
    floe_conversation::RunTerminal {
        state: floe_conversation::RunState::Completed,
        output: Some(text.clone()),
        steps: vec![EngineStep::Answer {
            text,
            artifacts: vec![],
        }],
        coverage: DependencyCoverage::Independent,
        issue: None,
        blocked: None,
        interactions: vec![],
    }
}

#[tokio::test]
async fn publication_rolls_back_with_core_and_cannot_be_delegated_as_user_input() {
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let identity = floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
    let input = vault
        .append_surface_input(&f.actor, request(&f, &vault, "one").await)
        .await
        .unwrap();
    let work = vault
        .submit_surface_work(&f.actor, work(&input))
        .await
        .unwrap();
    let execution = execution(&f, &work, "one", 0);
    vault
        .claim_manager_surface_work(work.clone(), execution.clone(), identity)
        .await
        .unwrap();
    let run = vault
        .conversation_run(execution.run_id)
        .await
        .unwrap()
        .unwrap();
    let terminal = completed_terminal(&vault, &run).await;
    vault
        .conversation_core_fault_after_owner_settlement
        .store(true, std::sync::atomic::Ordering::Release);
    assert!(
        vault
            .settle_manager_conversation_run(run.run_id, 1, terminal.clone())
            .await
            .is_err()
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        input.through
    );
    assert_eq!(
        vault
            .conversation_run(run.run_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        floe_conversation::RunState::Working
    );
    let completed = vault
        .settle_manager_conversation_run(run.run_id, 1, terminal.clone())
        .await
        .unwrap();
    assert_eq!(completed.state, floe_conversation::RunState::Completed);
    let head = vault.surface_head(&f.actor, f.session).await.unwrap();
    assert_eq!(head.sequence, 2);
    let entries = vault.surface_entries(&f.actor, head, 0, 128).await.unwrap();
    let SurfaceEntryReceipt::PublishedResult(result) = &entries[1] else {
        panic!("actual terminal commit must publish one result");
    };
    assert_eq!(
        result.result,
        SurfaceResultReference::for_run(&completed).unwrap()
    );
    assert_eq!(
        vault
            .settle_manager_conversation_run(run.run_id, 1, terminal)
            .await
            .unwrap(),
        completed
    );
    assert_eq!(vault.surface_head(&f.actor, f.session).await.unwrap(), head);
    let forged = SubmitSurfaceWork {
        command_id: CommandId::new(),
        anchor: head,
        input_id: result.result.message_id(),
        origin: WorkOrigin::Text,
    };
    assert_eq!(
        vault.submit_surface_work(&f.actor, forged).await,
        Err(CommandFailure::NotApplied(AgentFailure::NotFound))
    );
    assert!(
        vault
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .is_empty()
    );
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        vault.surface_entries(&f.actor, head, 0, 128).await.unwrap(),
        entries
    );
    // Even a self-consistent rewritten prefix cannot substitute a body
    // reference that disagrees with the canonical Run owner.
    let mut forged_reference = result.result.clone();
    forged_reference.output_digest[0] ^= 1;
    let forged = SurfaceResultReceipt::admit(&f.actor, result.before, forged_reference).unwrap();
    let connection = vault.connection().unwrap();
    connection
        .execute(
            "UPDATE agent_surface_entries SET payload = ? WHERE command_id = ?",
            (
                encode(&SurfaceEntryReceipt::PublishedResult(forged.clone())).unwrap(),
                forged.result.command_id().as_uuid().to_string(),
            ),
        )
        .await
        .unwrap();
    connection
        .execute(
            "UPDATE agent_surface_heads SET digest = ? WHERE session_id = ?",
            (forged.through.digest.to_vec(), f.session.to_string()),
        )
        .await
        .unwrap();
    assert!(
        vault
            .surface_entries(&f.actor, forged.through, 0, 128)
            .await
            .is_err()
    );
    drop(connection);
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn manager_busy_accepts_second_work_and_claims_once_after_real_core_settlement() {
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let identity = floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
    let first_request = request(&f, &vault, "first").await;
    let first_input = vault
        .append_surface_input(&f.actor, first_request)
        .await
        .unwrap();
    let first = vault
        .submit_surface_work(&f.actor, work(&first_input))
        .await
        .unwrap();
    let first_execution = execution(&f, &first, "first", 0);
    let first_run = first_execution.run_id;
    assert!(matches!(
        vault
            .claim_manager_surface_work(first.clone(), first_execution.clone(), identity.clone())
            .await
            .unwrap(),
        super::super::VaultManagerConversationAdmission::Created(_)
    ));
    // Actual active Manager/Core recorder, not a manually fabricated flag.
    assert_eq!(
        vault
            .load(f.actor.person_id, f.session)
            .await
            .unwrap()
            .active_turn,
        Some(first_run.as_uuid())
    );
    let second_request = request(&f, &vault, "second").await;
    let second_input = vault
        .append_surface_input(&f.actor, second_request)
        .await
        .unwrap();
    let second = vault
        .submit_surface_work(&f.actor, work(&second_input))
        .await
        .unwrap();
    assert_eq!(
        vault
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap(),
        vec![second.clone()]
    );
    assert_eq!(
        vault
            .read_surface_work(&f.actor, first.command_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        floe_conversation::SurfaceWorkState::Linked(first_run)
    );
    let blocked = execution(&f, &second, "second", 1);
    assert!(
        vault
            .claim_manager_surface_work(second.clone(), blocked.clone(), identity.clone())
            .await
            .is_err()
    );
    assert!(
        vault
            .conversation_run(blocked.run_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        vault
            .surface_entries(&f.actor, first.anchor, 0, 128)
            .await
            .unwrap(),
        vec![SurfaceEntryReceipt::Input(first_input)]
    );
    vault
        .settle_manager_conversation_run(
            first_run,
            1,
            floe_conversation::RunTerminal::from_failure(AgentFailure::Cancelled),
        )
        .await
        .unwrap();
    let revision = vault
        .load(f.actor.person_id, f.session)
        .await
        .unwrap()
        .revision;
    let next = execution(&f, &second, "second", revision);
    let next_run = next.run_id;
    assert!(matches!(
        vault
            .claim_manager_surface_work(second.clone(), next.clone(), identity.clone())
            .await
            .unwrap(),
        super::super::VaultManagerConversationAdmission::Created(_)
    ));
    assert!(matches!(
        vault
            .claim_manager_surface_work(second.clone(), next.clone(), identity.clone())
            .await
            .unwrap(),
        super::super::VaultManagerConversationAdmission::Existing(_)
    ));
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert!(matches!(
        vault
            .claim_manager_surface_work(second.clone(), next.clone(), identity.clone())
            .await
            .unwrap(),
        super::super::VaultManagerConversationAdmission::Existing(_)
    ));
    let mut changed = next;
    changed.run_id = floe_kernel::RunId::new();
    assert!(
        vault
            .claim_manager_surface_work(second, changed.clone(), identity)
            .await
            .is_err()
    );
    assert!(
        vault
            .conversation_run(changed.run_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        vault
            .load(f.actor.person_id, f.session)
            .await
            .unwrap()
            .active_turn,
        Some(next_run.as_uuid())
    );
}
#[tokio::test]
async fn work_claim_fifo_and_payload_are_checked_before_creating_any_run() {
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let identity = floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
    let first = request(&f, &vault, "one").await;
    let first = vault.append_surface_input(&f.actor, first).await.unwrap();
    let w1 = vault
        .submit_surface_work(&f.actor, work(&first))
        .await
        .unwrap();
    let second = request(&f, &vault, "two").await;
    let second = vault.append_surface_input(&f.actor, second).await.unwrap();
    let w2 = vault
        .submit_surface_work(&f.actor, work(&second))
        .await
        .unwrap();
    let too_early = execution(&f, &w2, "two", 0);
    assert!(
        vault
            .claim_manager_surface_work(w2, too_early.clone(), identity.clone())
            .await
            .is_err()
    );
    let wrong_text = execution(&f, &w1, "forged", 0);
    assert!(
        vault
            .claim_manager_surface_work(w1.clone(), wrong_text.clone(), identity.clone())
            .await
            .is_err()
    );
    assert!(
        vault
            .conversation_run(too_early.run_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        vault
            .conversation_run(wrong_text.run_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        vault
            .claim_manager_surface_work(w1.clone(), execution(&f, &w1, "one", 0), identity)
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn exact_surface_and_work_replay_survive_advance_and_encrypted_reopen() {
    let (f, vault) = fixture().await;
    let first = request(&f, &vault, "오늘 일정").await;
    let input = vault
        .append_surface_input(&f.actor, first.clone())
        .await
        .unwrap();
    let pending = work(&input);
    let receipt = vault
        .submit_surface_work(&f.actor, pending.clone())
        .await
        .unwrap();
    let second = request(&f, &vault, "내일 날씨").await;
    let latest = vault
        .append_surface_input(&f.actor, second.clone())
        .await
        .unwrap();
    assert_eq!(
        vault
            .surface_entries(&f.actor, input.through, 0, 128)
            .await
            .unwrap(),
        vec![SurfaceEntryReceipt::Input(input.clone())]
    );
    assert_eq!(
        vault
            .load(f.actor.person_id, f.session)
            .await
            .unwrap()
            .revision,
        0
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        latest.through
    );
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        vault
            .append_surface_input(&f.actor, first.clone())
            .await
            .unwrap(),
        input
    );
    assert_eq!(
        vault
            .submit_surface_work(&f.actor, pending.clone())
            .await
            .unwrap(),
        receipt
    );
    let mut changed = first;
    changed.input.text = "different".into();
    assert!(matches!(
        vault.append_surface_input(&f.actor, changed).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    let mut changed = pending;
    changed.anchor = latest.through;
    assert!(matches!(
        vault.submit_surface_work(&f.actor, changed).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    let mut foreign = f.actor.clone();
    foreign.person_id = PersonId::new();
    assert!(vault.surface_head(&foreign, f.session).await.is_err());
    let mut foreign = f.actor.clone();
    foreign.device_id = "device-b".into();
    assert!(
        vault
            .submit_surface_work(&foreign, work(&input))
            .await
            .is_err()
    );
    vault.seal();
    assert!(vault.surface_head(&f.actor, f.session).await.is_err());
}
#[tokio::test]
async fn exact_replay_does_not_reserve_unrelated_writer() {
    let (f, vault) = fixture().await;
    let request = request(&f, &vault, "one").await;
    let input = vault
        .append_surface_input(&f.actor, request.clone())
        .await
        .unwrap();
    let work = work(&input);
    let receipt = vault
        .submit_surface_work(&f.actor, work.clone())
        .await
        .unwrap();
    let mut connection = vault.connection().unwrap();
    let writer = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault.append_surface_input(&f.actor, request).await.unwrap(),
        input
    );
    assert_eq!(
        vault.submit_surface_work(&f.actor, work).await.unwrap(),
        receipt
    );
    writer.rollback().await.unwrap();
}
#[tokio::test]
async fn stale_anchor_and_cross_command_family_cannot_duplicate_custody() {
    let (f, vault) = fixture().await;
    let first = request(&f, &vault, "one").await;
    let mut stale = first.clone();
    stale.command_id = CommandId::new();
    stale.input.message_id = MessageId::new();
    let input = vault
        .append_surface_input(&f.actor, first.clone())
        .await
        .unwrap();
    assert!(matches!(
        vault.append_surface_input(&f.actor, stale).await,
        Err(CommandFailure::NotApplied(AgentFailure::Conflict))
    ));
    let mut collision = work(&input);
    collision.command_id = first.command_id;
    assert!(
        vault
            .submit_surface_work(&f.actor, collision)
            .await
            .is_err()
    );
    assert!(matches!(
        vault
            .start_conversation_session(StartSessionRequest {
                principal: f.actor.person_id.to_string(),
                command_id: first.command_id
            })
            .await
            .unwrap(),
        SessionStartAdmission::NotApplied(_)
    ));
    let second = request(&f, &vault, "two").await;
    let second = vault.append_surface_input(&f.actor, second).await.unwrap();
    let mut future = work(&second);
    future.anchor = input.through;
    assert!(matches!(
        vault.submit_surface_work(&f.actor, future).await,
        Err(CommandFailure::NotApplied(AgentFailure::NotFound))
    ));
    let mut forged = input.through;
    forged.digest[0] ^= 1;
    assert!(vault.surface_entries(&f.actor, forged, 0, 1).await.is_err());
}
#[tokio::test]
async fn voice_metadata_cannot_delegate_another_session_utterance() {
    let (f, vault) = fixture().await;
    let voice = Uuid::new_v4();
    let mut incoming = request(&f, &vault, "read schedule").await;
    incoming.input.origin = SurfaceInputOrigin::Voice {
        voice_session_id: voice,
        utterance_id: "u1".into(),
    };
    let input = vault
        .append_surface_input(&f.actor, incoming)
        .await
        .unwrap();
    let mut pending = work(&input);
    pending.origin = WorkOrigin::Voice {
        voice_session_id: Uuid::new_v4(),
        delegation_id: "d1".into(),
    };
    assert!(matches!(
        vault.submit_surface_work(&f.actor, pending.clone()).await,
        Err(CommandFailure::NotApplied(AgentFailure::PolicyDenied))
    ));
    pending.origin = WorkOrigin::Voice {
        voice_session_id: voice,
        delegation_id: "d1".into(),
    };
    let receipt = vault
        .submit_surface_work(&f.actor, pending.clone())
        .await
        .unwrap();
    pending.command_id = CommandId::new();
    assert!(vault.submit_surface_work(&f.actor, pending).await.is_err());
    receipt.validate().unwrap();
    // A duplicate semantic identity is a normal refusal, not corruption
    // and must not seal the Vault or poison subsequent admissions.
    let next = request(&f, &vault, "still usable").await;
    vault.append_surface_input(&f.actor, next).await.unwrap();
}
#[tokio::test]
async fn pending_budget_rejects_only_work_not_new_input_or_exact_retry() {
    let (f, vault) = fixture().await;
    let mut first = None;
    for n in 0..=MAX_PENDING_SURFACE_WORK {
        let request = request(&f, &vault, &format!("work {n}")).await;
        let input = vault.append_surface_input(&f.actor, request).await.unwrap();
        let pending = work(&input);
        let result = vault.submit_surface_work(&f.actor, pending.clone()).await;
        if n == MAX_PENDING_SURFACE_WORK {
            assert_eq!(
                result,
                Err(CommandFailure::NotApplied(AgentFailure::BudgetExceeded))
            );
        } else {
            let receipt = result.unwrap();
            if n == 0 {
                first = Some((pending, receipt));
            }
        }
    }
    let (request, receipt) = first.unwrap();
    assert_eq!(
        vault.submit_surface_work(&f.actor, request).await.unwrap(),
        receipt
    );
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        65
    );
}
#[tokio::test]
async fn text_input_and_work_are_atomic_and_exact_on_lost_ack_retry() {
    let (f, vault) = fixture().await;
    let mut first = None;
    for n in 0..=MAX_PENDING_SURFACE_WORK {
        let expected = vault.surface_head(&f.actor, f.session).await.unwrap();
        let request = floe_conversation::SubmitTextWork {
            retry_of: None,
            command_id: CommandId::new(),
            expected,
            text: format!("queued {n}"),
        };
        let result = vault.submit_text_work(&f.actor, request.clone()).await;
        if n == MAX_PENDING_SURFACE_WORK {
            assert_eq!(
                result,
                Err(CommandFailure::NotApplied(AgentFailure::BudgetExceeded))
            );
            assert_eq!(
                vault.surface_head(&f.actor, f.session).await.unwrap(),
                expected
            );
            assert!(
                vault
                    .read_surface_work(&f.actor, request.command_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            let (input, _) = request.prepare(&f.actor).unwrap();
            assert!(
                vault
                    .surface_input_command_on(&vault.connection().unwrap(), input.command_id)
                    .await
                    .unwrap()
                    .is_none()
            );
        } else {
            let receipt = result.unwrap();
            assert_eq!(receipt.anchor.sequence, n as u64 + 1);
            if n == 0 {
                first = Some((request, receipt));
            }
        }
    }
    let (request, receipt) = first.unwrap();
    assert_eq!(
        vault
            .submit_text_work(&f.actor, request.clone())
            .await
            .unwrap(),
        receipt
    );
    let mut invalid = request.clone();
    invalid.text.clear();
    assert_eq!(
        vault.submit_text_work(&f.actor, invalid).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    );
    let mut changed = request.clone();
    changed.text = "changed".into();
    assert!(matches!(
        vault.submit_text_work(&f.actor, changed).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        vault.submit_text_work(&f.actor, request).await.unwrap(),
        receipt
    );
}
#[tokio::test]
async fn corrupted_prefix_is_rejected_at_reopen_without_repair() {
    let (f, vault) = fixture().await;
    let request = request(&f, &vault, "retained").await;
    vault.append_surface_input(&f.actor, request).await.unwrap();
    let connection = vault.connection().unwrap();
    connection
        .execute(
            "UPDATE agent_surface_heads SET digest = ? WHERE session_id = ?",
            (vec![0u8; 32], f.session.to_string()),
        )
        .await
        .unwrap();
    drop(connection);
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn pages_are_bounded_but_transcript_is_not_lifetime_capped() {
    let (f, vault) = fixture().await;
    let mut last = None;
    for n in 0..130 {
        let request = request(&f, &vault, &format!("input {n}")).await;
        last = Some(vault.append_surface_input(&f.actor, request).await.unwrap());
    }
    let through = last.unwrap().through;
    assert_eq!(
        vault
            .surface_entries(&f.actor, through, 0, 128)
            .await
            .unwrap()
            .len(),
        128
    );
    assert_eq!(
        vault
            .surface_entries(&f.actor, through, 128, 128)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        vault
            .surface_entries(&f.actor, through, 0, 129)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pending_cancellation_retains_input_is_exact_and_survives_writer_and_reopen() {
    let (f, vault) = fixture().await;
    let input = vault
        .append_surface_input(&f.actor, request(&f, &vault, "cancel this work").await)
        .await
        .unwrap();
    let admitted = vault
        .submit_surface_work(&f.actor, work(&input))
        .await
        .unwrap();
    let cancel = floe_conversation::CancelPendingWork {
        command_id: CommandId::new(),
        work_command_id: admitted.command_id,
    };
    let receipt = vault
        .cancel_pending_work(&f.actor, cancel.clone())
        .await
        .unwrap();
    let head = vault.surface_head(&f.actor, f.session).await.unwrap();
    assert_eq!(head.sequence, 2);
    let entries = vault.surface_entries(&f.actor, head, 0, 128).await.unwrap();
    assert_eq!(entries[0], SurfaceEntryReceipt::Input(input.clone()));
    assert!(
        matches!(&entries[1], SurfaceEntryReceipt::PendingWorkCancelled(entry) if entry.cancellation == receipt)
    );
    assert_eq!(
        vault
            .surface_entries(&f.actor, input.through, 0, 128)
            .await
            .unwrap(),
        vec![SurfaceEntryReceipt::Input(input)]
    );
    assert_eq!(
        vault
            .read_surface_work(&f.actor, admitted.command_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        floe_conversation::SurfaceWorkState::Cancelled
    );
    assert!(
        vault
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .is_empty()
    );
    let mut other = vault.connection().unwrap();
    let writer = other
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault
            .cancel_pending_work(&f.actor, cancel.clone())
            .await
            .unwrap(),
        receipt
    );
    writer.rollback().await.unwrap();
    let mut changed_actor = f.actor.clone();
    changed_actor.device_id = "another-device".into();
    assert_eq!(
        vault
            .cancel_pending_work(&changed_actor, cancel.clone())
            .await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    );
    let changed = floe_conversation::CancelPendingWork {
        work_command_id: CommandId::new(),
        ..cancel.clone()
    };
    assert_eq!(
        vault.cancel_pending_work(&f.actor, changed).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    );
    assert_eq!(
        super::super::conversations::command_occupant(
            &vault.connection().unwrap(),
            cancel.command_id.as_uuid()
        )
        .await
        .unwrap(),
        Some(floe_conversation::ConversationCommandKind::SurfaceEntry)
    );
    vault.activate_conversation_executor().await.unwrap();
    let execution = execution(&f, &admitted, "cancel this work", 0);
    assert!(
        vault
            .claim_manager_surface_work(
                admitted.clone(),
                execution.clone(),
                floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap()
            )
            .await
            .is_err()
    );
    assert!(
        vault
            .conversation_run(execution.run_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        head,
        "a refused claim must leave custody usable"
    );
    drop(other);
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        vault.cancel_pending_work(&f.actor, cancel).await.unwrap(),
        receipt
    );
    assert_eq!(
        vault.surface_entries(&f.actor, head, 0, 128).await.unwrap(),
        entries
    );
    // A distinct cancellation command is an explicit repeated Person intent,
    // not another execution or a rewrite of the first command's receipt.
    let again = floe_conversation::CancelPendingWork {
        command_id: CommandId::new(),
        work_command_id: admitted.command_id,
    };
    vault.cancel_pending_work(&f.actor, again).await.unwrap();
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        3
    );
}

#[tokio::test]
async fn pending_cancellation_cannot_mutate_linked_run_or_erase_its_history() {
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let input = vault
        .append_surface_input(&f.actor, request(&f, &vault, "already started").await)
        .await
        .unwrap();
    let work = vault
        .submit_surface_work(&f.actor, work(&input))
        .await
        .unwrap();
    let execution = execution(&f, &work, "already started", 0);
    vault
        .claim_manager_surface_work(
            work.clone(),
            execution.clone(),
            floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap(),
        )
        .await
        .unwrap();
    let before = vault
        .conversation_run(execution.run_id)
        .await
        .unwrap()
        .unwrap();
    let request = floe_conversation::CancelPendingWork {
        command_id: CommandId::new(),
        work_command_id: work.command_id,
    };
    assert_eq!(
        vault.cancel_pending_work(&f.actor, request.clone()).await,
        Err(CommandFailure::NotApplied(AgentFailure::Conflict))
    );
    assert_eq!(
        vault
            .conversation_run(execution.run_id)
            .await
            .unwrap()
            .unwrap(),
        before
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        input.through
    );
    assert!(
        vault
            .pending_work_cancellation_on(&vault.connection().unwrap(), request.command_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        super::super::conversations::command_occupant(
            &vault.connection().unwrap(),
            request.command_id.as_uuid()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
        vault
            .read_surface_work(&f.actor, work.command_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        floe_conversation::SurfaceWorkState::Linked(execution.run_id)
    );
}

#[tokio::test]
async fn cancelled_work_requires_canonical_surface_command_proof() {
    let (f, vault) = fixture().await;
    let input = vault
        .append_surface_input(&f.actor, request(&f, &vault, "one").await)
        .await
        .unwrap();
    let work = vault
        .submit_surface_work(&f.actor, work(&input))
        .await
        .unwrap();
    // Simulate a corrupted state column, not a valid cancellation command.
    vault
        .connection()
        .unwrap()
        .execute(
            "UPDATE agent_surface_work SET state = 'cancelled' WHERE command_id = ?",
            [work.command_id.as_uuid().to_string()],
        )
        .await
        .unwrap();
    assert!(
        vault
            .read_surface_work(&f.actor, work.command_id)
            .await
            .is_err()
    );
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concurrent_claim_and_pending_cancellation_have_one_winner() {
    for _ in 0..4 {
        let (f, vault) = fixture().await;
        vault.activate_conversation_executor().await.unwrap();
        let input = vault
            .append_surface_input(&f.actor, request(&f, &vault, "race").await)
            .await
            .unwrap();
        let work = vault
            .submit_surface_work(&f.actor, work(&input))
            .await
            .unwrap();
        let execution = execution(&f, &work, "race", 0);
        let cancel = floe_conversation::CancelPendingWork {
            command_id: CommandId::new(),
            work_command_id: work.command_id,
        };
        let (cancelled, claimed) = tokio::join!(
            vault.cancel_pending_work(&f.actor, cancel.clone()),
            vault.claim_manager_surface_work(
                work.clone(),
                execution.clone(),
                floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap()
            ),
        );
        let snapshot = vault
            .read_surface_work(&f.actor, work.command_id)
            .await
            .unwrap()
            .unwrap();
        match snapshot.state {
            floe_conversation::SurfaceWorkState::Cancelled => {
                assert!(
                    cancelled.is_ok(),
                    "cancellation winner must have a receipt: {cancelled:?}"
                );
                assert!(claimed.is_err());
                assert!(
                    vault
                        .conversation_run(execution.run_id)
                        .await
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    vault
                        .surface_head(&f.actor, f.session)
                        .await
                        .unwrap()
                        .sequence,
                    2
                );
            }
            floe_conversation::SurfaceWorkState::Linked(run_id) => {
                assert!(claimed.is_ok());
                assert!(cancelled.is_err());
                assert_eq!(run_id, execution.run_id);
                assert_eq!(
                    vault.cancel_pending_work(&f.actor, cancel).await,
                    Err(CommandFailure::NotApplied(AgentFailure::Conflict))
                );
                assert_eq!(
                    vault.surface_head(&f.actor, f.session).await.unwrap(),
                    input.through
                );
            }
            floe_conversation::SurfaceWorkState::Queued => {
                panic!("neither concurrent command acquired custody")
            }
        }
    }
}

async fn record_surface_summary(
    vault: &EncryptedAgentVault<Keys>,
    actor: &OwnerActor,
    job: &floe_conversation::CompactionExecution,
    text: &str,
) {
    use floe_agent_contract::*;
    let attempt_id = Uuid::new_v4();
    let projection_ref = ProjectionRef::new();
    let batch_id = Uuid::new_v4();
    let events = vec![
        JournalEvent::ModelIntent {
            attempt_id,
            parent_task_id: None,
            reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                tokens: 100,
                cost_micros: 100,
            },
            projection_ref,
            plan: PreparedModelPlan {
                operation_id: Uuid::new_v4(),
                principal: actor.person_id.to_string(),
                device_id: actor.device_id.clone(),
                purpose: floe_conversation::CONVERSATION_PURPOSE.into(),
                consumer: floe_conversation::COMPACTION_CONSUMER.into(),
                capabilities: ModelCapabilities::chat(),
                boundary: ProcessingBoundary::Device,
                binding_digest: ModelBindingDigest([2; 32]),
                selection_commitment: ModelSelectionCommitment([1; 32]),
                budget_profile: ModelBudgetProfile::unknown(),
            },
        },
        JournalEvent::ModelResult {
            attempt_id,
            usage: ModelUsage {
                tokens: 10,
                cost_micros: 10,
            },
            accounting: floe_execution::budget::ModelAccounting {
                observed_tokens: Some(10),
                observed_cost_micros: Some(10),
                unknown_tokens: false,
                unknown_cost: false,
            },
        },
        JournalEvent::ValidatedBatch {
            batch: ValidatedModelBatch {
                execution_id: job.claim.execution_id,
                attempt_id,
                projection_ref,
                batch_id,
                steps: vec![ModelStep::Answer {
                    text: text.into(),
                    artifacts: vec![],
                }],
                catalog_revision: 1,
                tool_revisions: vec![],
                agent_revisions: vec![],
                projection_coverage: DependencyCoverage::Independent,
                delegation_context: None,
            },
        },
        JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id,
                next_step_index: 0,
            },
        },
        JournalEvent::Output {
            text: text.into(),
            artifacts: vec![],
        },
    ];
    for event in events {
        vault
            .append_manager_compaction(actor, job.claim, event)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn surface_checkpoint_preserves_old_work_prefix_and_archive_after_lost_ack() {
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let mut inputs = Vec::new();
    for ordinal in 0..130 {
        let request = request(
            &f,
            &vault,
            &format!("Settled voice history {ordinal}:{}", "u".repeat(300)),
        )
        .await;
        inputs.push(vault.append_surface_input(&f.actor, request).await.unwrap());
    }
    let selected = inputs.last().unwrap();
    let work = vault
        .submit_surface_work(&f.actor, work(selected))
        .await
        .unwrap();
    let live_source = floe_context_contract::LiveProjectionSource::SurfacePrefix {
        through: work.anchor,
    };
    let before_summary =
        floe_context::LivePrefixReader::read_live_prefix(&vault, &f.actor, &live_source)
            .await
            .unwrap();
    assert!(
        !before_summary.complete,
        "a bounded suffix is not a complete voice startup context"
    );
    assert_eq!(before_summary.messages.len(), 128);
    let execution = execution(&f, &work, &selected.input.text, 0);
    vault
        .claim_manager_surface_work(
            work.clone(),
            execution.clone(),
            floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap(),
        )
        .await
        .unwrap();
    let job = vault
        .admit_manager_compaction(
            &f.actor,
            floe_conversation::CompactionAdmission {
                authority: floe_conversation::CompactionAuthority::BeforeFirstModel {
                    run_id: execution.run_id,
                },
                session_id: f.session,
                expected_session_revision: 1,
                source_byte_limit: floe_conversation::COMPACTION_TRIGGER_BYTES,
                max_tokens: 1000,
                max_cost_micros: 1000,
                max_output_bytes: 1024,
            },
        )
        .await
        .unwrap()
        .unwrap();
    let floe_conversation::CompactionPrefix::Surface(prefix) = &job.source.prefix else {
        panic!("a Work prefix cannot become Core execution custody");
    };
    assert_eq!(prefix.head, work.anchor);
    assert_eq!(prefix.protected_input, selected.through);
    assert_eq!(prefix.through.sequence, 96);
    assert_eq!(job.source.messages.len(), 96);
    let late = vault
        .append_surface_input(
            &f.actor,
            request(&f, &vault, "FUTURE_AFTER_ADMITTED_COMPACTION").await,
        )
        .await
        .unwrap();
    record_surface_summary(&vault, &f.actor, &job, "Earlier synthetic voice requests.").await;
    let mut first_writer = vault.connection().unwrap();
    let first_transaction = first_writer
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault.complete_manager_compaction(&f.actor, job.claim).await,
        Err(AgentFailure::StorageBusy),
        "first installation still needs the writer"
    );
    first_transaction.rollback().await.unwrap();
    drop(first_writer);
    assert_eq!(
        vault
            .load_manager_compaction(&f.actor, job.claim)
            .await
            .unwrap()
            .disposition,
        floe_conversation::CompactionDisposition::Pending
    );

    vault
        .registry_transaction_ack_loss
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        vault.complete_manager_compaction(&f.actor, job.claim).await,
        Err(AgentFailure::StorageUnavailable)
    );
    // A lost commit acknowledgement seals the old generation by design.
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    let receipt = vault
        .complete_manager_compaction(&f.actor, job.claim)
        .await
        .unwrap();
    assert!(matches!(
        receipt.pointer,
        floe_agent_contract::ArchivePointer::Surface { .. }
    ));
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        late.through
    );
    let mut unrelated = vault.connection().unwrap();
    let writer = unrelated
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault
            .complete_manager_compaction(&f.actor, job.claim)
            .await
            .unwrap(),
        receipt,
        "committed compaction replay must not reserve an unrelated writer"
    );
    writer.rollback().await.unwrap();
    drop(unrelated);

    let live = floe_context::LivePrefixReader::read_live_prefix(&vault, &f.actor, &live_source)
        .await
        .unwrap();
    assert!(live.complete);
    assert_eq!(live.messages.len(), 35);
    assert_eq!(live.summary_message_id, Some(receipt.summary.message_id));
    assert_eq!(live.messages[0], receipt.summary);
    assert_eq!(
        live.messages.last().unwrap().message_id,
        selected.input.message_id.as_uuid()
    );
    assert!(
        !serde_json::to_string(&live.messages)
            .unwrap()
            .contains("FUTURE_AFTER_ADMITTED_COMPACTION")
    );
    let (history, incomplete) = vault
        .surface_model_history_for_work(&work, execution.command_id.as_uuid())
        .await
        .unwrap();
    assert!(!incomplete);
    assert_eq!(history.len(), 35);
    assert_eq!(history[0], receipt.summary);
    assert_eq!(history.last().unwrap().text, selected.input.text);
    assert!(
        !serde_json::to_string(&history)
            .unwrap()
            .contains("FUTURE_AFTER_ADMITTED_COMPACTION")
    );
    let archive_request = floe_agent_contract::ArchiveReadRequest {
        person_id: f.actor.person_id,
        session_id: f.session,
        pointer: receipt.pointer.clone(),
        max_messages: 128,
        max_bytes: 128 * 1024,
    };
    let archive = vault
        .read_manager_session_archive(&archive_request)
        .await
        .unwrap();
    archive.validate(&archive_request).unwrap();
    assert_eq!(archive.messages.len(), 96);
    for (archived, original) in archive.messages.iter().zip(&inputs) {
        assert_eq!(
            archived.group,
            floe_agent_contract::ArchiveMessageGroup::SurfaceEntry(
                original.input.message_id.as_uuid()
            )
        );
        assert_eq!(archived.message.text, original.input.text);
    }
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    vault.activate_conversation_executor().await.unwrap();
    assert_eq!(
        vault
            .complete_manager_compaction(&f.actor, job.claim)
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(
        vault
            .read_manager_session_archive(&archive_request)
            .await
            .unwrap(),
        archive
    );
    let connection = vault.connection().unwrap();
    connection
        .execute(
            "UPDATE agent_surface_checkpoints SET through_digest = ? WHERE execution_id = ?",
            (vec![9u8; 32], job.claim.execution_id.to_string()),
        )
        .await
        .unwrap();
    assert!(
        vault
            .read_manager_session_archive(&archive_request)
            .await
            .is_err()
    );
    drop(connection);
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn terminal_outcome_and_partial_body_publish_atomically_and_reopen() {
    for partial in [false, true] {
        let (f, vault) = fixture().await;
        vault.activate_conversation_executor().await.unwrap();
        let identity =
            floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
        let input = vault
            .append_surface_input(
                &f.actor,
                request(&f, &vault, "Retained synthetic request").await,
            )
            .await
            .unwrap();
        let work = vault
            .submit_surface_work(&f.actor, work(&input))
            .await
            .unwrap();
        let execution = execution(&f, &work, &input.input.text, 0);
        vault
            .claim_manager_surface_work(work.clone(), execution.clone(), identity)
            .await
            .unwrap();
        let run = vault
            .conversation_run(execution.run_id)
            .await
            .unwrap()
            .unwrap();
        let terminal = if partial {
            let mut terminal = completed_terminal(&vault, &run).await;
            terminal.state = floe_conversation::RunState::Failed;
            terminal.issue = Some(AgentFailure::BudgetExceeded);
            terminal
        } else {
            floe_conversation::RunTerminal::from_failure(AgentFailure::Cancelled)
        };
        vault
            .conversation_core_fault_after_owner_settlement
            .store(true, std::sync::atomic::Ordering::Release);
        assert!(
            vault
                .settle_manager_conversation_run(run.run_id, 1, terminal.clone())
                .await
                .is_err()
        );
        assert_eq!(
            vault.surface_head(&f.actor, f.session).await.unwrap(),
            input.through
        );
        let settled = vault
            .settle_manager_conversation_run(run.run_id, 1, terminal.clone())
            .await
            .unwrap();
        let head = vault.surface_head(&f.actor, f.session).await.unwrap();
        assert_eq!(head.sequence, if partial { 3 } else { 2 });
        let entries = vault.surface_entries(&f.actor, head, 0, 128).await.unwrap();
        let SurfaceEntryReceipt::RunOutcome(status) = &entries[1] else {
            panic!("status precedes any partial body")
        };
        assert_eq!(
            status.outcome,
            floe_conversation::SurfaceOutcomeReference::for_run(&settled).unwrap()
        );
        let message = status.outcome.model_status(&settled).unwrap();
        assert_eq!(
            vault
                .read_turn_coverage(f.session, message.message_id)
                .await
                .unwrap(),
            floe_agent_contract::DependencyCoverage::Independent
        );
        if partial {
            let SurfaceEntryReceipt::PublishedResult(body) = &entries[2] else {
                panic!("partial output remains canonical evidence")
            };
            assert_eq!(
                body.result.completeness,
                floe_conversation::SurfaceResultCompleteness::Partial
            );
            assert_ne!(body.result.message_id(), status.outcome.message_id());
        }
        assert_eq!(
            vault
                .settle_manager_conversation_run(run.run_id, 1, terminal)
                .await
                .unwrap(),
            settled
        );
        assert_eq!(vault.surface_head(&f.actor, f.session).await.unwrap(), head);
        let snapshot_request = floe_conversation::SurfaceReadRequest {
            session_id: f.session,
            through: Some(head),
            limit: 128,
        };
        let snapshot = vault
            .read_surface(&f.actor, snapshot_request.clone())
            .await
            .unwrap();
        let mut writer_connection = vault.connection().unwrap();
        let unrelated_writer = writer_connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .unwrap();
        assert_eq!(
            vault
                .read_surface(&f.actor, snapshot_request.clone())
                .await
                .unwrap(),
            snapshot,
            "canonical result/status joins are read-only too"
        );
        unrelated_writer.rollback().await.unwrap();
        drop(writer_connection);
        assert_eq!(snapshot.messages.len(), entries.len());
        assert!(matches!(&snapshot.messages[1].content,
            floe_conversation::SurfaceMessageContent::Outcome { run_id, state, .. }
            if *run_id == run.run_id && *state == settled.state));
        if partial {
            assert!(matches!(&snapshot.messages[2].content,
                floe_conversation::SurfaceMessageContent::Result { run_id, completeness: floe_conversation::SurfaceResultCompleteness::Partial, text }
                if *run_id == run.run_id && Some(text) == settled.output.as_ref()));
        }
        drop(vault);
        let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .unwrap();
        assert_eq!(
            reopened
                .surface_entries(&f.actor, head, 0, 128)
                .await
                .unwrap(),
            entries
        );
        assert_eq!(
            reopened
                .read_surface(&f.actor, snapshot_request.clone())
                .await
                .unwrap(),
            snapshot
        );
        if partial {
            let connection = reopened.connection().unwrap();
            connection
                .execute(
                    "DELETE FROM agent_surface_work WHERE command_id = ?",
                    [work.command_id.as_uuid().to_string()],
                )
                .await
                .unwrap();
            assert!(
                reopened
                    .surface_entries(&f.actor, head, 0, 128)
                    .await
                    .is_err(),
                "a valid terminal Run alone cannot prove Surface publication custody"
            );
            assert!(
                reopened
                    .read_surface(&f.actor, snapshot_request.clone())
                    .await
                    .is_err(),
                "a safe body projection must retain the Work lineage proof"
            );
            drop(connection);
            drop(reopened);
            assert!(
                EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
                    .await
                    .is_err()
            );
        } else {
            let mut reference = status.outcome.clone();
            reference.terminal_digest[0] ^= 1;
            let forged =
                floe_conversation::SurfaceOutcomeReceipt::admit(&f.actor, status.before, reference)
                    .unwrap();
            let connection = reopened.connection().unwrap();
            connection
                .execute(
                    "UPDATE agent_surface_entries SET payload = ? WHERE command_id = ?",
                    (
                        encode(&SurfaceEntryReceipt::RunOutcome(forged.clone())).unwrap(),
                        forged.outcome.command_id().as_uuid().to_string(),
                    ),
                )
                .await
                .unwrap();
            connection
                .execute(
                    "UPDATE agent_surface_heads SET digest = ? WHERE session_id = ?",
                    (forged.through.digest.to_vec(), f.session.to_string()),
                )
                .await
                .unwrap();
            assert!(
                reopened
                    .surface_entries(&f.actor, forged.through, 0, 128)
                    .await
                    .is_err()
            );
            drop(connection);
            drop(reopened);
            assert!(
                EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
                    .await
                    .is_err()
            );
        }
    }
}

#[tokio::test]
async fn work_pages_pin_membership_but_observe_current_custody_without_writer() {
    use floe_conversation::{SurfaceWorkCursor, SurfaceWorkPageRequest, SurfaceWorkState};
    let (f, vault) = fixture().await;
    let page_request = SurfaceWorkPageRequest {
        session_id: f.session,
        cursor: None,
        limit: 2,
    };
    assert!(
        vault
            .list_surface_work(&f.actor, page_request.clone())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let mut work = Vec::new();
    for index in 0..5 {
        let expected = vault.surface_head(&f.actor, f.session).await.unwrap();
        work.push(
            vault
                .submit_text_work(
                    &f.actor,
                    floe_conversation::SubmitTextWork {
                        retry_of: None,
                        command_id: CommandId::new(),
                        expected,
                        text: format!("Retained request {index}"),
                    },
                )
                .await
                .unwrap(),
        );
    }
    let first = vault
        .list_surface_work(&f.actor, page_request.clone())
        .await
        .unwrap();
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| item.receipt.clone())
            .collect::<Vec<_>>(),
        work[3..]
    );
    let cursor = first.next.unwrap();
    assert_eq!(cursor.through, work[4].command_id);
    assert_eq!(cursor.before, work[3].command_id);
    let expected = vault.surface_head(&f.actor, f.session).await.unwrap();
    let later = vault
        .submit_text_work(
            &f.actor,
            floe_conversation::SubmitTextWork {
                retry_of: None,
                command_id: CommandId::new(),
                expected,
                text: "Admitted after the first page".into(),
            },
        )
        .await
        .unwrap();
    vault
        .cancel_pending_work(
            &f.actor,
            floe_conversation::CancelPendingWork {
                command_id: CommandId::new(),
                work_command_id: work[1].command_id,
            },
        )
        .await
        .unwrap();
    let mut writer_connection = vault.connection().unwrap();
    let writer = writer_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    let second = vault
        .list_surface_work(
            &f.actor,
            SurfaceWorkPageRequest {
                cursor: Some(cursor.clone()),
                ..page_request.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        second
            .items
            .iter()
            .map(|item| item.receipt.clone())
            .collect::<Vec<_>>(),
        work[1..3]
    );
    assert_eq!(second.items[0].state, SurfaceWorkState::Cancelled);
    let last = vault
        .list_surface_work(
            &f.actor,
            SurfaceWorkPageRequest {
                cursor: second.next.clone(),
                ..page_request.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].receipt, work[0]);
    assert!(last.next.is_none());
    writer.rollback().await.unwrap();
    drop(writer_connection);
    assert!(
        vault
            .list_surface_work(
                &f.actor,
                SurfaceWorkPageRequest {
                    cursor: Some(SurfaceWorkCursor {
                        through: work[0].command_id,
                        before: later.command_id
                    }),
                    ..page_request.clone()
                }
            )
            .await
            .is_err()
    );
    assert!(
        vault
            .list_surface_work(
                &f.actor,
                SurfaceWorkPageRequest {
                    limit: 0,
                    ..page_request.clone()
                }
            )
            .await
            .is_err()
    );
    let mut other = f.actor.clone();
    other.person_id = PersonId::new();
    assert!(
        vault
            .list_surface_work(&other, page_request.clone())
            .await
            .is_err()
    );
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .list_surface_work(
                &f.actor,
                SurfaceWorkPageRequest {
                    cursor: Some(cursor),
                    ..page_request
                }
            )
            .await
            .unwrap(),
        second
    );
}

#[tokio::test]
async fn surface_snapshot_pages_are_original_pinned_read_only_and_verified_on_reopen() {
    use floe_conversation::{SurfaceInputKind, SurfaceMessageContent, SurfaceReadRequest};
    let (f, vault) = fixture().await;
    let query = SurfaceReadRequest {
        session_id: f.session,
        through: None,
        limit: 2,
    };
    assert!(
        vault
            .read_surface(&f.actor, query.clone())
            .await
            .unwrap()
            .messages
            .is_empty()
    );
    let first = vault
        .append_surface_input(&f.actor, request(&f, &vault, "one").await)
        .await
        .unwrap();
    let voice = Uuid::new_v4();
    let mut second_request = request(&f, &vault, "two spoken words").await;
    second_request.input.origin = SurfaceInputOrigin::Voice {
        voice_session_id: voice,
        utterance_id: "opaque-provider-utterance".into(),
    };
    let second = vault
        .append_surface_input(&f.actor, second_request)
        .await
        .unwrap();
    let third = vault
        .append_surface_input(&f.actor, request(&f, &vault, "three").await)
        .await
        .unwrap();
    let page = vault.read_surface(&f.actor, query.clone()).await.unwrap();
    assert_eq!(page.through, third.through);
    assert_eq!(page.older, Some(first.through));
    assert_eq!(page.messages.len(), 2);
    assert_eq!(
        page.messages[0].content,
        SurfaceMessageContent::Input {
            text: "two spoken words".into(),
            origin: SurfaceInputKind::Voice
        }
    );
    let encoded = serde_json::to_string(&page).unwrap();
    assert!(!encoded.contains("opaque-provider-utterance"));
    assert!(!encoded.contains(&voice.to_string()));
    let pinned = SurfaceReadRequest {
        through: Some(page.through),
        ..query.clone()
    };
    let mut writer_connection = vault.connection().unwrap();
    let writer = writer_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault.read_surface(&f.actor, pinned.clone()).await.unwrap(),
        page
    );
    writer.rollback().await.unwrap();
    drop(writer_connection);
    vault
        .append_surface_input(&f.actor, request(&f, &vault, "later arrival").await)
        .await
        .unwrap();
    assert_eq!(
        vault.read_surface(&f.actor, pinned.clone()).await.unwrap(),
        page
    );
    let previous = vault
        .read_surface(
            &f.actor,
            SurfaceReadRequest {
                through: page.older,
                ..query.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(previous.messages.len(), 1);
    assert!(previous.older.is_none());
    assert_eq!(previous.messages[0].message_id, first.input.message_id);
    let wrong_actor = OwnerActor {
        person_id: PersonId::new(),
        ..f.actor.clone()
    };
    assert_eq!(
        vault.read_surface(&wrong_actor, query.clone()).await,
        Err(AgentFailure::PolicyDenied)
    );
    let mut forged = page.through;
    forged.digest[0] ^= 1;
    assert!(
        vault
            .read_surface(
                &f.actor,
                SurfaceReadRequest {
                    through: Some(forged),
                    ..query.clone()
                }
            )
            .await
            .is_err()
    );
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        vault.read_surface(&f.actor, pinned.clone()).await.unwrap(),
        page
    );
    let mut corrupted = second.clone();
    corrupted.input.text = "altered original".into();
    vault
        .connection()
        .unwrap()
        .execute(
            "UPDATE agent_surface_entries SET payload = ? WHERE command_id = ?",
            (
                encode(&SurfaceEntryReceipt::Input(corrupted)).unwrap(),
                second.command_id.as_uuid().to_string(),
            ),
        )
        .await
        .unwrap();
    assert!(
        vault.read_surface(&f.actor, pinned).await.is_err(),
        "safe display must still verify original custody"
    );
}

#[tokio::test]
async fn work_observes_current_continue_child_without_rewriting_claim_or_missing_link_proof() {
    use floe_conversation::{
        CanonicalTurnIntent, SurfaceReadRequest, SurfaceWorkState, TurnAdmissionRequest, TurnInput,
        TurnMode,
    };
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let input = vault
        .append_surface_input(
            &f.actor,
            request(&f, &vault, "continue original Work").await,
        )
        .await
        .unwrap();
    let work = vault
        .submit_surface_work(&f.actor, work(&input))
        .await
        .unwrap();
    let initial = execution(&f, &work, &input.input.text, 0);
    let identity = floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
    vault
        .claim_manager_surface_work(work.clone(), initial.clone(), identity.clone())
        .await
        .unwrap();
    let parent = vault
        .settle_manager_conversation_run(
            initial.run_id,
            1,
            floe_conversation::RunTerminal::from_failure(AgentFailure::BudgetExceeded),
        )
        .await
        .unwrap();
    let pinned = vault
        .read_surface(
            &f.actor,
            SurfaceReadRequest {
                session_id: f.session,
                through: None,
                limit: 128,
            },
        )
        .await
        .unwrap();
    let mode = TurnMode::Continue(
        floe_conversation::project_run_receipt(parent.clone())
            .unwrap()
            .continuation()
            .unwrap(),
    );
    let intent = CanonicalTurnIntent {
        session_id: f.session,
        expected_revision: parent.session_revision,
        text: input.input.text.clone(),
        mode: mode.clone(),
        retry_of: None,
    };
    let request = TurnAdmissionRequest {
        expert_environment: parent.expert_environment,
        run_id: floe_kernel::RunId::new(),
        command_id: CommandId::new(),
        session_id: f.session,
        expected_session_revision: parent.session_revision,
        principal: f.actor.person_id.to_string(),
        device_id: f.actor.device_id.clone(),
        request_digest: intent.digest(&f.actor.person_id.to_string()).unwrap(),
        mode,
        retry_of: None,
        input: TurnInput::ExistingMessage {
            message_id: parent.user_message_id,
        },
    };
    let child = match vault
        .admit_manager_conversation_turn(request.clone(), identity.clone())
        .await
        .unwrap()
    {
        crate::VaultManagerConversationAdmission::Created(child) => child,
        other => panic!("expected Continue child, got {other:?}"),
    };
    let snapshot = vault
        .read_surface_work(&f.actor, work.command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.receipt, work);
    assert_eq!(snapshot.state, SurfaceWorkState::Linked(parent.run_id));
    assert_eq!(snapshot.current_run_id, Some(child.run_id));
    let terminal = completed_terminal(&vault, &child).await;
    vault
        .settle_manager_conversation_run(child.run_id, 1, terminal)
        .await
        .unwrap();
    assert_eq!(
        vault
            .read_surface(
                &f.actor,
                SurfaceReadRequest {
                    session_id: f.session,
                    through: Some(pinned.through),
                    limit: 128
                }
            )
            .await
            .unwrap(),
        pinned
    );
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    let mut writer_connection = vault.connection().unwrap();
    let writer = writer_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    let page = vault
        .list_surface_work(
            &f.actor,
            floe_conversation::SurfaceWorkPageRequest {
                session_id: f.session,
                cursor: None,
                limit: 128,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].receipt, work);
    assert_eq!(page.items[0].state, SurfaceWorkState::Linked(parent.run_id));
    assert_eq!(page.items[0].current_run_id, Some(child.run_id));
    writer.rollback().await.unwrap();
    drop(writer_connection);
    let connection = vault.connection().unwrap();
    connection.execute("UPDATE agent_conversation_run_successors SET child_command_id = ? WHERE origin_run_id = ?",(Uuid::new_v4().to_string(),parent.run_id.as_uuid().to_string())).await.unwrap();
    assert!(
        vault
            .read_surface_work(&f.actor, work.command_id)
            .await
            .is_err()
    );
    connection.execute("UPDATE agent_conversation_run_successors SET child_command_id = ? WHERE origin_run_id = ?",(child.command_id.as_uuid().to_string(),parent.run_id.as_uuid().to_string())).await.unwrap();
    assert_eq!(
        vault
            .read_surface_work(&f.actor, work.command_id)
            .await
            .unwrap()
            .unwrap()
            .current_run_id,
        Some(child.run_id)
    );
    connection
        .execute(
            "DELETE FROM agent_conversation_run_successors WHERE origin_run_id = ?",
            [parent.run_id.as_uuid().to_string()],
        )
        .await
        .unwrap();
    assert!(
        vault
            .read_surface_work(&f.actor, work.command_id)
            .await
            .is_err(),
        "missing forward proof must not silently restore the old root as current"
    );
    drop(connection);
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn retry_work_keeps_original_input_and_survives_intervening_claim_and_reopen() {
    use floe_conversation::SubmitTextWork;
    let (f, vault) = fixture().await;
    vault.activate_conversation_executor().await.unwrap();
    let identity = floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
    let original = SubmitTextWork {
        command_id: CommandId::new(),
        retry_of: None,
        expected: vault.surface_head(&f.actor, f.session).await.unwrap(),
        text: "original request".into(),
    };
    let first = vault
        .submit_text_work(&f.actor, original.clone())
        .await
        .unwrap();
    let first_execution = execution(&f, &first, &original.text, 0);
    let source_run = first_execution.run_id;
    vault
        .claim_manager_surface_work(first.clone(), first_execution, identity.clone())
        .await
        .unwrap();
    let mut retry = SubmitTextWork {
        command_id: CommandId::new(),
        retry_of: Some(source_run),
        expected: vault.surface_head(&f.actor, f.session).await.unwrap(),
        text: original.text.clone(),
    };
    assert_eq!(
        vault.submit_text_work(&f.actor, retry.clone()).await,
        Err(CommandFailure::NotApplied(AgentFailure::Conflict))
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        retry.expected
    );
    vault
        .settle_manager_conversation_run(
            source_run,
            1,
            floe_conversation::RunTerminal::from_failure(AgentFailure::Cancelled),
        )
        .await
        .unwrap();

    let other = SubmitTextWork {
        command_id: CommandId::new(),
        retry_of: None,
        expected: vault.surface_head(&f.actor, f.session).await.unwrap(),
        text: "intervening work".into(),
    };
    let intervening = vault
        .submit_text_work(&f.actor, other.clone())
        .await
        .unwrap();
    retry.expected = vault.surface_head(&f.actor, f.session).await.unwrap();
    let mut changed_text = retry.clone();
    changed_text.text = "replacement request".into();
    assert_eq!(
        vault.submit_text_work(&f.actor, changed_text).await,
        Err(CommandFailure::NotApplied(AgentFailure::Conflict))
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        retry.expected
    );
    let mut foreign = f.actor.clone();
    foreign.device_id = "other-device".into();
    assert!(
        vault
            .submit_text_work(&foreign, retry.clone())
            .await
            .is_err()
    );
    assert_eq!(
        vault.surface_head(&f.actor, f.session).await.unwrap(),
        retry.expected
    );
    let retried = vault
        .submit_text_work(&f.actor, retry.clone())
        .await
        .unwrap();
    assert_ne!(retried.input_id, first.input_id);
    assert_eq!(retried.origin.retry_of(), Some(source_run));
    let revision = vault
        .load(f.actor.person_id, f.session)
        .await
        .unwrap()
        .revision;
    let other_execution = execution(&f, &intervening, &other.text, revision);
    let other_run = other_execution.run_id;
    vault
        .claim_manager_surface_work(intervening, other_execution, identity.clone())
        .await
        .unwrap();
    vault
        .settle_manager_conversation_run(
            other_run,
            1,
            floe_conversation::RunTerminal::from_failure(AgentFailure::Cancelled),
        )
        .await
        .unwrap();
    let moved_revision = vault
        .load(f.actor.person_id, f.session)
        .await
        .unwrap()
        .revision;
    assert!(moved_revision > revision);
    let retry_execution = execution(&f, &retried, &retry.text, moved_revision);
    let retry_run = retry_execution.run_id;
    vault
        .claim_manager_surface_work(retried.clone(), retry_execution.clone(), identity.clone())
        .await
        .unwrap();
    assert_eq!(
        vault
            .conversation_run(retry_run)
            .await
            .unwrap()
            .unwrap()
            .retry_of,
        Some(source_run)
    );
    // Exact admission observation remains read-only while another writer holds
    // a reservation. Neither head/revision rebasing nor a new command is needed.
    let mut writer = vault.connection().unwrap();
    let held = writer
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault
            .submit_text_work(&f.actor, retry.clone())
            .await
            .unwrap(),
        retried
    );
    held.rollback().await.unwrap();
    drop(writer);
    drop(vault);
    let vault = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        vault
            .submit_text_work(&f.actor, retry.clone())
            .await
            .unwrap(),
        retried
    );
    assert!(matches!(
        vault
            .claim_manager_surface_work(retried.clone(), retry_execution, identity)
            .await
            .unwrap(),
        super::super::VaultManagerConversationAdmission::Existing(_)
    ));
    let mut retargeted = retry;
    retargeted.retry_of = Some(other_run);
    assert_eq!(
        vault.submit_text_work(&f.actor, retargeted).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    );
}

#[tokio::test]
async fn live_result_point_read_preserves_exact_publication_scope_and_reopen() {
    use floe_context::LiveResultReader;
    use floe_context_contract::LiveProjectionSource;
    let (f, vault) = fixture().await;
    let empty_source = floe_context_contract::LiveProjectionSource::SurfacePrefix {
        through: vault.surface_head(&f.actor, f.session).await.unwrap(),
    };
    let empty = floe_context::LivePrefixReader::read_live_prefix(&vault, &f.actor, &empty_source)
        .await
        .unwrap();
    assert!(empty.complete && empty.messages.is_empty() && empty.summary_message_id.is_none());
    vault.activate_conversation_executor().await.unwrap();
    let input = vault
        .append_surface_input(&f.actor, request(&f, &vault, "voice work").await)
        .await
        .unwrap();
    let work = vault
        .submit_surface_work(&f.actor, work(&input))
        .await
        .unwrap();
    let execution = execution(&f, &work, "voice work", 0);
    let identity = floe_conversation::prompts::default_manager_identity(f.actor.person_id).unwrap();
    vault
        .claim_manager_surface_work(work, execution.clone(), identity)
        .await
        .unwrap();
    let run = vault
        .conversation_run(execution.run_id)
        .await
        .unwrap()
        .unwrap();
    let terminal = completed_terminal(&vault, &run).await;
    let completed = vault
        .settle_manager_conversation_run(run.run_id, 1, terminal)
        .await
        .unwrap();
    let through = vault.surface_head(&f.actor, f.session).await.unwrap();
    let message_id = SurfaceResultReference::for_run(&completed)
        .unwrap()
        .message_id();
    let source = LiveProjectionSource::SurfaceResult {
        through,
        surface_message_id: message_id,
    };
    let prefix_source = floe_context_contract::LiveProjectionSource::SurfacePrefix { through };
    let prefix = floe_context::LivePrefixReader::read_live_prefix(&vault, &f.actor, &prefix_source)
        .await
        .unwrap();
    assert!(prefix.complete);
    assert_eq!(prefix.messages.len(), 2);
    assert_eq!(
        prefix.messages.last().unwrap().message_id,
        message_id.as_uuid()
    );
    assert_ne!(
        prefix.messages.last().unwrap().message_id,
        run.run_id.as_uuid()
    );
    let result = vault.read_live_result(&f.actor, &source).await.unwrap();
    assert_eq!(result.text, "Synthetic surface result");
    assert!(!result.partial);
    assert_eq!(
        result.coverage,
        floe_context_contract::DependencyCoverage::Independent
    );
    assert!(!format!("{result:?}").contains(&result.text));
    // Exercise the actual encrypted owner read through Context projection; no
    // provider, native audio or UI cache participates in this fixture.
    let plan = floe_model_contract::LiveModelPlan {
        operation_id: Uuid::new_v4(),
        actor: f.actor.clone(),
        capabilities: floe_model_contract::LiveModelCapabilities::conversational(),
        limits: floe_model_contract::LiveTransportLimits {
            initial_context_bytes: 16384,
            append_bytes: 500,
            local_session_duration_ms: 60000,
        },
        boundary: floe_model_contract::ProcessingBoundary::Device,
        binding_digest: floe_model_contract::ModelBindingDigest([61; 32]),
        selection_commitment: floe_model_contract::ModelSelectionCommitment([62; 32]),
    };
    let budget = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(1, 1),
        floe_execution::budget::ModelUsage::default(),
    );
    let scope = floe_execution::ExecutionScope::root(
        floe_execution::Cancellation::new(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        budget.root_lease(),
        floe_kernel::TraceContext::new(Uuid::new_v4()),
    );
    let voice_session_id = Uuid::new_v4();
    for (source, expected_messages) in [(&empty_source, 0), (&prefix_source, 2)] {
        let floe_context::LiveProjectionOutcome::Ready(startup) =
            floe_context::prepare_live_prefix_projection(
                &vault,
                floe_context::LivePrefixProjectionRequest {
                    plan: &plan,
                    voice_session_id,
                    surface_session_id: f.session,
                    projection_id: Uuid::new_v4(),
                    source,
                    instructions: "Synthetic trusted live presentation instructions",
                },
                &scope,
                |_| async { panic!("Independent prefix has no source dependency") },
            )
            .await
            .unwrap()
        else {
            panic!("ready startup prefix")
        };
        let body: serde_json::Value = serde_json::from_str(startup.payload.as_str()).unwrap();
        assert_eq!(body["history"].as_array().unwrap().len(), expected_messages);
        assert_eq!(&startup.source, source);
        assert!(!startup.payload.as_str().contains(&run.run_id.to_string()));
    }
    let floe_context::LiveProjectionOutcome::Ready(projection) =
        floe_context::prepare_live_result_projection(
            &vault,
            floe_context::LiveResultProjectionRequest {
                plan: &plan,
                voice_session_id,
                surface_session_id: f.session,
                projection_id: Uuid::new_v4(),
                source: &source,
            },
            &scope,
            |_| async { panic!("Independent canonical result has no source dependency") },
        )
        .await
        .unwrap()
    else {
        panic!("ready projection")
    };
    projection
        .validate_for_plan(&plan, voice_session_id, f.session)
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(projection.payload.as_str()).unwrap();
    assert_eq!(payload["text"], result.text);
    assert_eq!(payload["partial"], false);
    assert!(payload.get("run_id").is_none());
    // A newer Person input cannot silently advance the captured result prefix.
    vault
        .append_surface_input(&f.actor, request(&f, &vault, "newer input").await)
        .await
        .unwrap();
    assert_eq!(
        vault.read_live_result(&f.actor, &source).await.unwrap(),
        result
    );
    let too_old = LiveProjectionSource::SurfaceResult {
        through: input.through,
        surface_message_id: message_id,
    };
    assert_eq!(
        vault.read_live_result(&f.actor, &too_old).await,
        Err(AgentFailure::NotFound)
    );
    let person_input = LiveProjectionSource::SurfaceResult {
        through,
        surface_message_id: input.input.message_id,
    };
    assert_eq!(
        vault.read_live_result(&f.actor, &person_input).await,
        Err(AgentFailure::NotFound)
    );
    let mut foreign = f.actor.clone();
    foreign.person_id = PersonId::new();
    assert_eq!(
        vault.read_live_result(&foreign, &source).await,
        Err(AgentFailure::PolicyDenied)
    );
    let mut forged = through;
    forged.digest = [99; 32];
    assert!(
        vault
            .read_live_result(
                &f.actor,
                &LiveProjectionSource::SurfaceResult {
                    through: forged,
                    surface_message_id: message_id
                }
            )
            .await
            .is_err()
    );
    // Pure canonical reads do not reserve a writer, even under contention.
    let mut writer = vault.connection().unwrap();
    let held = writer
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert_eq!(
        vault.read_live_result(&f.actor, &source).await.unwrap(),
        result
    );
    held.rollback().await.unwrap();
    drop(writer);
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened.read_live_result(&f.actor, &source).await.unwrap(),
        result
    );
}
