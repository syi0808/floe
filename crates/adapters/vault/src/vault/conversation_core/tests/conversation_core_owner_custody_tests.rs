// Focused owner-custody regressions reuse the Core scenario fixture.

use super::*;

#[tokio::test]
async fn owner_input_reverse_lookup_uses_exact_mapping_keys_with_unrelated_runs_present() {
    let mut scenario = Scenario::new().await;
    let (first_run, first_input) = scenario
        .admit_run_and_append_input("indexed owner input A")
        .await;
    scenario.cancel_owner_run(&first_run).await;

    let mut second_core_request = scenario.core_input_request("unrelated owner input B");
    second_core_request.target = AdmissionTarget::AppendToExisting {
        reference: ConversationReference {
            identity: scenario.identity.clone(),
            conversation_id: scenario.conversation_id,
            branch_id: scenario.branch_id,
            head_revision: first_input.receipt.transcript.sequence,
        },
    };
    let (second_run, second_input) = scenario
        .admit_run_and_bind_input(
            scenario.owner_request("unrelated owner input B"),
            second_core_request,
        )
        .await;
    assert_ne!(first_run.run_id, second_run.run_id);
    assert_ne!(
        first_input.receipt.transcript,
        second_input.receipt.transcript
    );

    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to indexed mapping fixture");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start exact mapping lookup transaction");
    let mapping = owner_transcript_input_for_reference_on(
        &transaction,
        scenario.person_id,
        first_input.receipt.transcript,
    )
    .await
    .expect("point lookup exact original Core input")
    .expect("first input mapping remains present");
    assert_eq!(mapping.original_owner_run_id, first_run.run_id);
    assert_eq!(mapping.owner_user_message_id, first_run.user_message_id);
    assert_eq!(mapping.input, first_input.receipt.transcript);

    let mut reverse_plan = transaction
        .query(
            "EXPLAIN QUERY PLAN SELECT mapping_json FROM agent_conversation_owner_transcript_inputs_v1 INDEXED BY agent_conversation_owner_transcript_inputs_core_key_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND input_sequence = ? AND input_message_id = ? LIMIT 2",
            (
                scenario.person_id.to_string(),
                first_input
                    .receipt
                    .transcript
                    .conversation_id
                    .as_uuid()
                    .to_string(),
                first_input
                    .receipt
                    .transcript
                    .branch_id
                    .as_uuid()
                    .to_string(),
                integer(first_input.receipt.transcript.sequence).expect("positive sequence"),
                first_input
                    .receipt
                    .transcript
                    .message_id
                    .as_uuid()
                    .to_string(),
            ),
        )
        .await
        .expect("explain exact Core reverse lookup");
    let reverse_detail = reverse_plan
        .next()
        .await
        .expect("read reverse lookup plan")
        .expect("reverse lookup has a plan row")
        .get::<String>(3)
        .expect("reverse lookup plan detail");
    assert!(
        reverse_detail.contains("agent_conversation_owner_transcript_inputs_core_key_v1"),
        "Core reverse lookup must use the exact immutable mapping primary key: {reverse_detail}"
    );

    let mut owner_plan = transaction
        .query(
            "EXPLAIN QUERY PLAN SELECT mapping_json FROM agent_conversation_owner_transcript_inputs_v1 INDEXED BY agent_conversation_owner_transcript_inputs_owner_key_v1 WHERE person_id = ? AND session_id = ? AND owner_user_message_id = ? LIMIT 2",
            (
                scenario.person_id.to_string(),
                scenario.session_id.to_string(),
                first_run.user_message_id.to_string(),
            ),
        )
        .await
        .expect("explain unique owner-key lookup");
    let owner_detail = owner_plan
        .next()
        .await
        .expect("read owner-key lookup plan")
        .expect("owner-key lookup has a plan row")
        .get::<String>(3)
        .expect("owner-key lookup plan detail");
    assert!(
        owner_detail.contains("agent_conversation_owner_transcript_inputs_owner_key_v1"),
        "Continue lookup must use its explicit unique owner key: {owner_detail}"
    );
    assert_eq!(
        owner_transcript_input_for_owner_message_on(
            &transaction,
            scenario.person_id,
            scenario.session_id,
            first_run.user_message_id,
        )
        .await
        .expect("resolve Continue owner key"),
        Some(mapping),
    );
    transaction
        .commit()
        .await
        .expect("finish exact mapping lookup transaction");
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        2,
        "two distinct Runs remain in the old Run-binding family"
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
        2
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_run_inputs_v1"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn failed_owner_mapping_write_rolls_back_the_new_optional_family() {
    let scenario = Scenario::new().await;
    let before_session = session_storage_snapshot(
        &scenario
            .vault()
            .connection()
            .expect("connect before injected mapping failure"),
        scenario.session_id,
    )
    .await;
    scenario
        .vault()
        .conversation_core_fault_after_input_mapping
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(scenario.owner_request("rollback mapping")),
            scenario.core_input_request("rollback mapping"),
        )
        .await,
        Err(ConversationStoreFailure::NotCommitted)
    );

    let connection = scenario
        .vault()
        .connection()
        .expect("connect after injected mapping failure");
    assert_eq!(
        table_count(&connection, "agent_conversation_runs").await,
        0,
        "owner admission rolls back with the Core mapping"
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        0
    );
    assert!(
        !crate::schema::conversation_owner_custody_family_present(&connection)
            .await
            .expect("inspect optional family without creating it")
    );
    assert_eq!(
        session_storage_snapshot(&connection, scenario.session_id).await,
        before_session
    );
}

#[tokio::test]
async fn owner_custody_reads_and_open_do_not_repair_optional_schema() {
    let mut scenario = Scenario::new().await;
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before optional-family reads");
    let before = live_catalog_snapshot(&connection).await;
    drop(connection);
    assert!(
        !before
            .iter()
            .any(|(_, name, _)| name.contains("owner_transcript"))
    );
    let absent_reference = TranscriptReference {
        conversation_id: ConversationId::new(),
        branch_id: ConversationBranchId::new(),
        sequence: 1,
        message_id: MessageId::new(),
    };
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect before absent-family reads");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start absent-family read transaction");
    assert_eq!(
        owner_transcript_input_for_reference_on(
            &transaction,
            scenario.person_id,
            absent_reference,
        )
        .await
        .expect("read missing owner input mapping"),
        None
    );
    assert_eq!(
        typed_transcript_link_for_entry_on(&transaction, scenario.person_id, absent_reference,)
            .await
            .expect("read missing typed transcript link"),
        None
    );
    transaction
        .commit()
        .await
        .expect("finish absent-family read transaction");
    scenario
        .vault()
        .validate_conversation_core_store()
        .await
        .expect("validate absent optional Core family");
    scenario.reopen().await;
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after no-repair reopen");
    let after = live_catalog_snapshot(&connection).await;
    assert_eq!(after, before, "open/read leave catalog bytes unchanged");
}

#[tokio::test]
async fn owner_admission_and_core_input_rollback_as_one_transaction() {
    let scenario = Scenario::new().await;
    let initial_revision = scenario.session_revision;
    let owner_request = scenario.owner_request("atomic admission");
    let core_request = scenario.core_input_request("atomic admission");
    let mut connection = scenario
        .vault()
        .connection()
        .expect("connect to test Vault");
    let (mut guard, transaction) = scenario
        .vault()
        .journal_transaction(&mut connection)
        .await
        .expect("start composed transaction");
    let mut replay_checked = false;
    let mut prior_command = false;
    scenario
        .vault()
        .admit_conversation_run_with_core_input_on(
            &transaction,
            CoreComposedOwnerIntent::Turn(owner_request),
            core_request,
            &mut replay_checked,
            &mut prior_command,
        )
        .await
        .expect("write bound owner Run and Core input before deliberate rollback");
    transaction
        .rollback()
        .await
        .expect("roll back owner and Core rows together");
    guard.settled();

    let connection = scenario
        .vault()
        .connection()
        .expect("reconnect after rollback");
    assert_eq!(table_count(&connection, "agent_conversation_runs").await, 0);
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_heads").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
        0
    );
    assert_eq!(
        table_count(&connection, "agent_sessions").await,
        1,
        "the initial owner Session remains"
    );
    let session = SessionStore::load(scenario.vault(), scenario.person_id, scenario.session_id)
        .await
        .expect("reload Session after rollback");
    assert_eq!(session.revision, initial_revision);
    assert!(session.active_turn.is_none());
}

#[tokio::test]
async fn owner_input_mapping_ack_loss_replays_exact_proof_after_reopen() {
    let mut scenario = Scenario::new().await;
    let owner_request = scenario.owner_request("owner input mapping ACK replay");
    let core_request = scenario.core_input_request("owner input mapping ACK replay");
    let expected_run = owner_request.run_id;
    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(owner_request.clone()),
            core_request.clone(),
        )
        .await,
        Err(ConversationStoreFailure::OutcomeUnknown),
        "mapping and owner/Core admission commit before the lost acknowledgement"
    );

    scenario.reopen().await;
    let replay = compose_owner_core_run(
        scenario.vault(),
        CoreComposedOwnerIntent::Turn(owner_request),
        core_request,
    )
    .await
    .expect("replay validates the immutable input mapping after reopen");
    let CoreComposedRunAdmission::Admitted { record, input, .. } = replay else {
        panic!("exact New retry did not replay its original admission");
    };
    assert_eq!(record.run_id, expected_run);
    assert_eq!(input.disposition, AdmissionDisposition::Replayed);
    assert_eq!(
        stored_run_input_mapping(scenario.vault(), scenario.person_id, record.run_id)
            .await
            .expect("reopened owner input mapping")
            .original_owner_run_id,
        expected_run
    );
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after exact admission replay");
    assert_eq!(table_count(&connection, "agent_conversation_runs").await, 1);
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
        1
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_run_inputs_v1"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn typed_composer_rejects_wrong_turn_and_user_kind_and_scopes_digest() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed owner output")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed output recorder");

    let mut wrong_turn = typed_assistant_request(open.fence.clone(), "wrong turn");
    wrong_turn.typed_message = floe_conversation::AgentMessage::Assistant {
        turn_id: Uuid::new_v4(),
        text: "wrong turn".into(),
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), wrong_turn).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a new typed output must belong to its admitted owner Run"
    );

    let mut user_payload = typed_assistant_request(open.fence.clone(), "not generated");
    user_payload.typed_message = floe_conversation::AgentMessage::User {
        turn_id: run.run_id.as_uuid(),
        message_id: Uuid::new_v4(),
        text: "User content cannot be a generated output".into(),
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), user_payload).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "User payload cannot masquerade as a generated Core output"
    );

    let unlinked = typed_assistant_request(open.fence.clone(), "unrelated same-Session row");
    let unlinked_reference = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to insert an unlinked owner payload");
        let (guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start explicit unlinked payload fixture write");
        let result = scenario
            .vault()
            .insert_typed_agent_message_on(
                &transaction,
                scenario.session_id,
                unlinked.typed_entry_id,
                TypedAgentMessageProvenance::OwnerRecorded,
                &unlinked.typed_message,
            )
            .await
            .map_err(owner_error);
        scenario
            .vault()
            .finish_conversation_core_transaction(guard, transaction, result)
            .await
            .expect("store an unrelated same-Session typed row")
    };
    assert_eq!(unlinked_reference.session_id(), scenario.session_id);
    let mut adopt_unrelated = unlinked.clone();
    adopt_unrelated.message_id = MessageId::new();
    adopt_unrelated.command_id = CommandId::new();
    adopt_unrelated.contribution_id = LogicalContributionId::new();
    assert_eq!(
        compose_typed_recording(scenario.vault(), adopt_unrelated).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "an unrelated same-Session digest cannot be retroactively attached"
    );

    let request = typed_assistant_request(open.fence.clone(), "the typed answer");
    let receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("derive and atomically link the exact typed output");
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("typed output has its immutable owner link");
    let mut link_connection = scenario
        .vault()
        .connection()
        .expect("connect to point-read exact typed link");
    let link_transaction = link_connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .expect("start exact transcript link read transaction");
    assert_eq!(
        typed_transcript_link_for_entry_on(
            &link_transaction,
            scenario.person_id,
            link.transcript_entry.reference,
        )
        .await
        .expect("resolve exact transcript entry owner proof"),
        Some(link.clone()),
        "the reader helper is an exact transcript-key lookup"
    );
    link_transaction
        .commit()
        .await
        .expect("finish exact transcript link read transaction");
    assert_eq!(link.owner_input, input.receipt.transcript);
    assert_eq!(link.owner_session_id, scenario.session_id);
    assert_eq!(link.owner_turn_id, run.run_id.as_uuid());
    assert_eq!(link.first_recording_run_id, run.run_id);
    assert_eq!(link.transcript_entry.producer_run, Some(run.run_id));
    assert_eq!(
        link.transcript_entry.contribution_id,
        Some(request.contribution_id)
    );
    assert_eq!(
        link.transcript_entry
            .message
            .evidence
            .as_ref()
            .expect("Core retains opaque typed digest")
            .digest(),
        link.typed_reference.digest(),
    );
    assert_eq!(receipt.transcript, link.transcript_entry.reference);

    let other_session = match scenario
        .vault()
        .start_conversation_session(StartSessionRequest {
            principal: scenario.person_id.to_string(),
            command_id: CommandId::new(),
        })
        .await
        .expect("create another owner Session for digest scope check")
    {
        SessionStartAdmission::Started(receipt) => receipt.session_id,
        other => panic!("unexpected second Session admission: {other:?}"),
    };
    let foreign_reference = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to foreign Session");
        let (guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start foreign typed payload transaction");
        let result = scenario
            .vault()
            .insert_typed_agent_message_on(
                &transaction,
                other_session,
                request.typed_entry_id,
                TypedAgentMessageProvenance::OwnerRecorded,
                &request.typed_message,
            )
            .await
            .map_err(owner_error);
        scenario
            .vault()
            .finish_conversation_core_transaction(guard, transaction, result)
            .await
            .expect("store foreign-session typed fixture without a Core link")
    };
    assert_ne!(foreign_reference.digest(), link.typed_reference.digest());
    let mut cross_session_link = link.clone();
    cross_session_link.typed_reference = foreign_reference;
    assert_eq!(
        cross_session_link.validate(scenario.person_id),
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a same-payload digest scoped to another Session cannot prove this transcript row"
    );
}

#[tokio::test]
async fn typed_writer_faults_roll_back_payload_core_receipt_and_link_together() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("atomic typed writer")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed writer recorder");
    let connection = scenario
        .vault()
        .connection()
        .expect("connect before typed writer fault injection");
    assert!(
        !crate::schema::typed_history_family_present(&connection)
            .await
            .expect("check optional typed payload family")
    );
    drop(connection);

    for stage in [1, 2, 3] {
        scenario
            .vault()
            .conversation_core_typed_write_fault
            .store(stage, Ordering::Release);
        let request = typed_assistant_request(open.fence.clone(), &format!("fault stage {stage}"));
        assert_eq!(
            compose_typed_recording(scenario.vault(), request).await,
            Err(ConversationStoreFailure::NotCommitted),
            "injected stage {stage} failure aborts the single composed write"
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after typed writer rollback");
        assert!(
            !crate::schema::typed_history_family_present(&connection)
                .await
                .expect("inspect rolled-back optional typed family"),
            "typed payload family creation is part of the write transaction"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1,
            "only the admitted inbound entry remains"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            0
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_output_receipts_v2").await,
            0
        );
        assert_eq!(
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1"
            )
            .await,
            0
        );
    }
}

#[tokio::test]
async fn typed_contribution_ack_loss_replays_exact_link_after_reopen() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed ACK replay")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed ACK recorder");
    let request = typed_assistant_request(open.fence.clone(), "persist once");
    scenario
        .vault()
        .conversation_core_ack_loss
        .store(true, Ordering::Release);
    assert_eq!(
        compose_typed_recording(scenario.vault(), request.clone()).await,
        Err(ConversationStoreFailure::OutcomeUnknown),
        "the payload, Core receipt, and link committed before the lost ACK"
    );

    scenario.reopen().await;
    let replay = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("exact typed contribution replays after reopen");
    assert_eq!(replay.recorder, open.fence);
    assert_eq!(replay.contribution_id, request.contribution_id);
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("replayed typed contribution retains its link");
    assert_eq!(link.first_recording_run_id, run.run_id);
    let connection = scenario
        .vault()
        .connection()
        .expect("connect after exact replay");
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_entries").await,
        2
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
        1
    );
    assert_eq!(
        table_count(
            &connection,
            "agent_conversation_owner_transcript_evidence_v1"
        )
        .await,
        1
    );
    assert_eq!(
        table_count(&connection, "agent_conversation_typed_history_entries").await,
        1
    );
}

#[tokio::test]
async fn typed_delegation_requires_actual_task_snapshot_receipt_and_owner_journal() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed Task proof")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open typed Task recorder");
    let generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task evidence owner")
        .executor_generation;
    let task_receipts = scenario.terminal_task_receipts(&run, generation).await;
    let accepted_reference = task_receipts[0].clone();
    let task = scenario
        .vault()
        .task(accepted_reference.execution.task_id)
        .await
        .expect("read actual Task snapshot")
        .expect("fixture Task exists");
    let typed_message = floe_conversation::AgentMessage::Delegation {
        turn_id: run.run_id.as_uuid(),
        task: task.snapshot.clone(),
        execution_receipt: Some(accepted_reference.clone()),
    };
    let valid_request = TypedConversationRecordingRequest {
        recorder: open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: typed_message.clone(),
    };

    let mut wrong_parent_snapshot = task.snapshot.clone();
    wrong_parent_snapshot.parent_run_id = Some(Uuid::new_v4());
    let wrong_parent = TypedConversationRecordingRequest {
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: wrong_parent_snapshot,
            execution_receipt: Some(accepted_reference.clone()),
        },
        ..valid_request.clone()
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), wrong_parent).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the Delegation snapshot must be the Task admitted under this owner Run"
    );

    let mut wrong_receipt = accepted_reference.clone();
    wrong_receipt.task_revision += 1;
    let mismatched_receipt = TypedConversationRecordingRequest {
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: task.snapshot.clone(),
            execution_receipt: Some(wrong_receipt),
        },
        ..valid_request.clone()
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), mismatched_receipt).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "the supplied Task receipt must match its actual stored execution receipt"
    );

    let receipt = compose_typed_recording(scenario.vault(), valid_request.clone())
        .await
        .expect("valid Task snapshot and owner journal proof compose atomically");
    let link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        valid_request.contribution_id,
    )
    .await
    .expect("valid Delegation output has an owner transcript link");
    assert_eq!(link.original_task_receipt, Some(accepted_reference.clone()));
    assert_eq!(
        receipt.producing_task,
        Some(task_evidence_reference(&accepted_reference).expect("derive Task evidence"))
    );
    assert_eq!(
        link.transcript_entry.message.task_id,
        Some(accepted_reference.execution.task_id)
    );
}

#[tokio::test]
async fn typed_contribution_replay_keeps_original_producer_after_continue() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("typed producer replay")
        .await;
    let open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open first contribution recorder");
    let request = typed_assistant_request(open.fence.clone(), "first recorder owns this");
    let original_receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("record typed contribution under first Run");
    let original_link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("load original producer proof");

    let terminal = scenario.budget_exceeded_owner_run(&run).await;
    scenario
        .vault()
        .close_conversation_recorder(open.fence.clone())
        .await
        .expect("close first producer after owner Run ends");
    let (continued, _) = scenario
        .admit_owner_run_reusing_input(&terminal, "continue against retained input")
        .await;
    let continued_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&continued, input.receipt.transcript))
        .await
        .expect("open new recorder for the same owner input");
    let mut replay_request = request.clone();
    replay_request.recorder = continued_open.fence;
    let replay = compose_typed_recording(scenario.vault(), replay_request)
        .await
        .expect("replay the same typed contribution from the Continue recorder");
    assert_eq!(replay, original_receipt);
    assert_eq!(replay.recorder.run_id, run.run_id);
    assert_eq!(replay.recorder.run_id, original_link.first_recording_run_id);
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id
        )
        .await,
        Some(original_link),
        "re-recorded replay keeps original link and producer identity"
    );
}

#[tokio::test]
async fn delegated_contribution_replay_after_continue_preserves_original_task_proof() {
    let mut scenario = Scenario::new().await;
    let (run, input) = scenario
        .admit_run_and_append_input("Delegation producer replay")
        .await;
    let original_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
        .await
        .expect("open original Delegation recorder");
    let task_generation = scenario
        .vault()
        .activate_task_executor()
        .await
        .expect("activate Task evidence owner")
        .executor_generation;
    let original_task_receipt = scenario
        .terminal_task_receipts(&run, task_generation)
        .await
        .into_iter()
        .next()
        .expect("first owner Run Task receipt");
    let original_task = scenario
        .vault()
        .task(original_task_receipt.execution.task_id)
        .await
        .expect("read original Task")
        .expect("original Task exists");
    let request = TypedConversationRecordingRequest {
        recorder: original_open.fence.clone(),
        message_id: MessageId::new(),
        command_id: CommandId::new(),
        contribution_id: LogicalContributionId::new(),
        typed_entry_id: Uuid::new_v4(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: run.run_id.as_uuid(),
            task: original_task.snapshot,
            execution_receipt: Some(original_task_receipt.clone()),
        },
    };
    let original_receipt = compose_typed_recording(scenario.vault(), request.clone())
        .await
        .expect("record Delegation with the original owner Task proof");
    let original_link = stored_typed_link(
        scenario.vault(),
        scenario.person_id,
        request.contribution_id,
    )
    .await
    .expect("read immutable Delegation link");
    assert_eq!(original_link.first_recording_run_id, run.run_id);
    assert_eq!(
        original_link.original_task_receipt,
        Some(original_task_receipt.clone())
    );

    let terminal = scenario.budget_exceeded_owner_run(&run).await;
    scenario
        .vault()
        .close_conversation_recorder(original_open.fence)
        .await
        .expect("close original recorder after terminal Run");
    let (continued, _) = scenario
        .admit_owner_run_reusing_input(&terminal, "Continue on the retained input")
        .await;
    let continued_open = scenario
        .vault()
        .open_conversation_recorder(scenario.start_request(&continued, input.receipt.transcript))
        .await
        .expect("open Continue recorder for the same owner input");
    let current_task_receipt = scenario
        .terminal_task_receipts(&continued, task_generation)
        .await
        .into_iter()
        .next()
        .expect("current recorder Task receipt");
    let current_task = scenario
        .vault()
        .task(current_task_receipt.execution.task_id)
        .await
        .expect("read current recorder Task")
        .expect("current recorder Task exists");

    let before_counts = {
        let connection = scenario
            .vault()
            .connection()
            .expect("connect before replay");
        (
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            table_count(&connection, "agent_conversation_core_output_receipts_v2").await,
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1",
            )
            .await,
            table_count(&connection, "agent_conversation_typed_history_entries").await,
        )
    };

    let substituted_current_task = TypedConversationRecordingRequest {
        recorder: continued_open.fence.clone(),
        typed_message: floe_conversation::AgentMessage::Delegation {
            turn_id: continued.run_id.as_uuid(),
            task: current_task.snapshot,
            execution_receipt: Some(current_task_receipt),
        },
        ..request.clone()
    };
    assert_eq!(
        compose_typed_recording(scenario.vault(), substituted_current_task).await,
        Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch
        )),
        "a new Task produced by the Continue Run cannot replace the original contribution proof"
    );
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id,
        )
        .await,
        Some(original_link.clone()),
        "rejection leaves the original immutable Task link intact"
    );

    let mut replay_request = request.clone();
    replay_request.recorder = continued_open.fence;
    let replay_receipt = compose_typed_recording(scenario.vault(), replay_request)
        .await
        .expect("Continue replay revalidates the original Task under its producer Run");
    assert_eq!(replay_receipt, original_receipt);
    assert_eq!(replay_receipt.recorder.run_id, run.run_id);
    assert_eq!(
        replay_receipt.transcript,
        original_link.transcript_entry.reference
    );
    assert_eq!(
        stored_typed_link(
            scenario.vault(),
            scenario.person_id,
            request.contribution_id,
        )
        .await,
        Some(original_link.clone()),
        "accepted replay preserves the first producer and original Task receipt"
    );

    let stored_payload = {
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to retained typed payload");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("start typed payload read");
        let resolved = scenario
            .vault()
            .resolve_typed_agent_message_on(
                &transaction,
                &original_link.typed_reference,
                floe_conversation::MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .expect("resolve unchanged original typed payload");
        transaction
            .commit()
            .await
            .expect("commit typed payload read");
        resolved.message
    };
    assert_eq!(stored_payload, request.typed_message);
    let after_counts = {
        let connection = scenario.vault().connection().expect("connect after replay");
        (
            table_count(&connection, "agent_conversation_core_v3_recording_receipts").await,
            table_count(&connection, "agent_conversation_core_output_receipts_v2").await,
            table_count(
                &connection,
                "agent_conversation_owner_transcript_evidence_v1",
            )
            .await,
            table_count(&connection, "agent_conversation_typed_history_entries").await,
        )
    };
    assert_eq!(
        after_counts, before_counts,
        "replay creates no second contribution"
    );
}
