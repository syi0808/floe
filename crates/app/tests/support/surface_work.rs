//! Real generation-owned queue/Engine integration; the transport alone is fake.
use super::*;
use floe_conversation::{SubmitTextWork, SurfaceWorkReceipt, SurfaceWorkState};
use floe_execution::Cancellation;
use floe_kernel::CommandId;

fn submit(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
) -> (SubmitTextWork, SurfaceWorkReceipt) {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let command_id = CommandId::new();
                let scope = host_scope(command_id.as_uuid(), Cancellation::new(), WAIT_TIMEOUT);
                let expected = owners
                    .conversation
                    .surface_head(&actor, session_id, &scope)
                    .await?;
                let request = SubmitTextWork {
                    command_id,
                    expected,
                    text: USER_TEXT.into(),
                };
                let receipt = owners
                    .conversation
                    .submit_text_work(&actor, request.clone(), &scope)
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)?;
                Ok::<_, AgentFailure>((request, receipt))
            })
            .expect("atomic text/work admission through actual Conversation owner")
    })
}
fn snapshot(
    host: &AppHost<AppComposition>,
    command_id: CommandId,
) -> floe_conversation::SurfaceWorkSnapshot {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                owners
                    .conversation
                    .read_surface_work(&actor, command_id, &scope)
                    .await?
                    .ok_or(AgentFailure::NotFound)
            })
            .expect("read actual work receipt")
    })
}
fn linked(host: &AppHost<AppComposition>, command_id: CommandId) -> floe_kernel::RunId {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        match snapshot(host, command_id).state {
            SurfaceWorkState::Linked(run) => return run,
            SurfaceWorkState::Queued => {}
            other => panic!("unexpected queue state: {other:?}"),
        }
        assert!(Instant::now() < deadline, "durable queue was not driven");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn entries(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
    anchor: Option<floe_conversation::SurfaceAnchor>,
) -> Vec<floe_conversation::SurfaceEntryReceipt> {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                let through = match anchor {
                    Some(anchor) => anchor,
                    None => {
                        owners
                            .conversation
                            .surface_head(&actor, session_id, &scope)
                            .await?
                    }
                };
                owners
                    .conversation
                    .surface_entries(&actor, through, 0, 128, &scope)
                    .await
            })
            .expect("read the actual ordered Surface prefix")
    })
}

#[test]
fn owner_queue_keeps_listening_and_serially_dispatches_with_original_anchor() {
    let (barrier, entered, release) = GenerateBarrier::new();
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::WaitFirstThenAnswer(barrier),
    );
    let recorder = model.recorder();
    let (profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let (_, first) = submit(&host, session_id);
    entered
        .recv_timeout(WAIT_TIMEOUT)
        .expect("first Manager reached actual model transport");
    let first_run = linked(&host, first.command_id);
    let (second_request, second) = submit(&host, session_id);
    assert_eq!(
        snapshot(&host, second.command_id).state,
        SurfaceWorkState::Queued
    );
    assert_eq!(assert_script_clean(&recorder).generated.len(), 1);

    let (_, cancelled) = submit(&host, session_id);
    let cancel_request = floe_conversation::CancelPendingWork {
        command_id: CommandId::new(),
        work_command_id: cancelled.command_id,
    };
    let cancel_receipt = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let request = cancel_request.clone();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                owners
                    .conversation
                    .cancel_pending_work(&actor, request, &scope)
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)
            })
            .expect("cancel actual pending work while another Run owns the recorder")
    });
    assert_eq!(
        snapshot(&host, cancelled.command_id).state,
        SurfaceWorkState::Cancelled
    );

    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                let expected = owners
                    .conversation
                    .surface_head(&actor, session_id, &scope)
                    .await?;
                owners
                    .conversation
                    .append_surface_input(
                        &actor,
                        floe_conversation::AppendSurfaceInput {
                            command_id: CommandId::new(),
                            expected,
                            input: floe_conversation::SurfaceInput {
                                message_id: floe_conversation_contract::MessageId::new(),
                                text: "FUTURE_INPUT_MUST_NOT_LEAK".into(),
                                origin: floe_conversation::SurfaceInputOrigin::Voice {
                                    voice_session_id: Uuid::new_v4(),
                                    utterance_id: "future-u1".into(),
                                },
                            },
                        },
                        &scope,
                    )
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)?;
                Ok::<_, AgentFailure>(())
            })
            .expect("settled voice input remains admissible while Manager is busy")
    });
    release.send(()).expect("release first synthetic model");
    assert_eq!(
        wait_terminal_run(&host, first_run).state,
        RunState::Completed
    );
    let second_run = linked(&host, second.command_id);
    assert_ne!(first_run, second_run);
    let completed = wait_terminal_run(&host, second_run);
    assert_eq!(
        completed.state,
        RunState::Completed,
        "second work: {completed:?}"
    );
    let observed = assert_script_clean(&recorder);
    assert_eq!(observed.generated.len(), 2);
    let context = serde_json::to_string(&observed.generated[1].envelope.conversation).unwrap();
    assert!(!context.contains("FUTURE_INPUT_MUST_NOT_LEAK"));
    assert!(
        !context.contains("cancel_pending_work"),
        "later cancellation cannot rewrite an earlier Work anchor"
    );
    assert!(
        !context.contains(REPLY),
        "the first result happened after the second Work anchor"
    );

    let published = entries(&host, session_id, None);
    assert_eq!(
        published.len(),
        7,
        "four original inputs, one cancellation intent and two completed outputs"
    );
    assert_eq!(entries(&host, session_id, Some(second.anchor)).len(), 2);
    let outputs = published
        .iter()
        .filter_map(|entry| match entry {
            floe_conversation::SurfaceEntryReceipt::PublishedResult(receipt) => Some(receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].result.run_id, first_run);
    assert_eq!(outputs[1].result.run_id, second_run);
    assert_eq!(outputs[0].through.sequence, 6);
    assert_eq!(outputs[1].through.sequence, 7);
    assert_ne!(
        outputs[0].result.message_id(),
        outputs[1].result.message_id()
    );
    drop(host);
    let host = profile.open(&model);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    assert_eq!(
        snapshot(&host, second.command_id).state,
        SurfaceWorkState::Linked(second_run)
    );
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let expected = second.clone();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                let replay = owners
                    .conversation
                    .submit_text_work(&actor, second_request, &scope)
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)?;
                assert_eq!(replay, expected);
                Ok::<_, AgentFailure>(())
            })
            .expect("exact admitted work replay after reopen")
    });
    assert_eq!(assert_script_clean(&recorder).generated.len(), 2);
    assert_eq!(
        entries(&host, session_id, None),
        published,
        "reopen/replay cannot republish completed text"
    );
    assert_eq!(
        snapshot(&host, cancelled.command_id).state,
        SurfaceWorkState::Cancelled
    );
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                let replay = owners
                    .conversation
                    .cancel_pending_work(&actor, cancel_request, &scope)
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)?;
                assert_eq!(replay, cancel_receipt);
                Ok::<_, AgentFailure>(())
            })
            .expect("cancelled work remains cancelled after reopen without another event");
    });
    assert_eq!(entries(&host, session_id, None), published);
    let (_, third) = submit(&host, session_id);
    let third_run = linked(&host, third.command_id);
    assert_eq!(
        wait_terminal_run(&host, third_run).state,
        RunState::Completed
    );
    let observed = assert_script_clean(&recorder);
    assert_eq!(observed.generated.len(), 3);
    let context = serde_json::to_string(&observed.generated[2].envelope.conversation).unwrap();
    assert!(
        context.contains("FUTURE_INPUT_MUST_NOT_LEAK"),
        "future input becomes eligible for a later anchor"
    );
    assert!(
        context.contains(REPLY),
        "published output is available to later work"
    );
    assert!(
        context.contains("cancel_pending_work"),
        "later context retains the Person's cancellation intent"
    );
    assert_eq!(entries(&host, session_id, None).len(), 9);
}
