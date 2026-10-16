//! Real generation-owned queue/Engine integration; the transport alone is fake.
use super::*;
use floe_conversation::{SubmitTextWork, SurfaceWorkReceipt, SurfaceWorkState};
use floe_execution::Cancellation;
use floe_kernel::CommandId;

fn submit(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
) -> (SubmitTextWork, SurfaceWorkReceipt) {
    submit_text(host, session_id, USER_TEXT)
}
fn submit_text(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
    text: &str,
) -> (SubmitTextWork, SurfaceWorkReceipt) {
    let text = text.to_owned();
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
                    text,
                };
                let receipt = support::exact_recovery::observe_exact_request(
                    request.clone(),
                    |exact| owners.conversation.submit_text_work(&actor, exact, &scope),
                    |failure| {
                        matches!(
                            failure,
                            floe_kernel::CommandFailure::NotAdmitted(AgentFailure::StorageBusy)
                                | floe_kernel::CommandFailure::Indeterminate(
                                    AgentFailure::StorageBusy
                                )
                                | floe_kernel::CommandFailure::Admitted(AgentFailure::StorageBusy)
                        )
                    },
                )
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


fn visible_surface(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
    through: Option<floe_conversation::SurfaceAnchor>,
    limit: usize,
) -> floe_conversation::SurfaceSnapshot {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
                owners
                    .conversation
                    .read_surface(
                        &actor,
                        floe_conversation::SurfaceReadRequest {
                            session_id,
                            through,
                            limit,
                        },
                        &scope,
                    )
                    .await
            })
            .expect("read canonical display snapshot through generation-owned Conversation")
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

    let display = visible_surface(&host, session_id, None, 128);
    assert_eq!(display.messages.len(), 7);
    let display_json = serde_json::to_string(&display).unwrap();
    assert!(!display_json.contains("future-u1"), "provider utterance identifiers are not product display data");
    let last_page = visible_surface(&host, session_id, Some(display.through), 2);
    assert_eq!(last_page.messages, display.messages[5..]);
    let earlier_page = visible_surface(&host, session_id, last_page.older, 128);
    assert_eq!(earlier_page.messages, display.messages[..5]);
    assert_eq!(snapshot(&host, first.command_id).current_run_id, Some(first_run));
    assert_eq!(snapshot(&host, second.command_id).current_run_id, Some(second_run));
    assert_eq!(snapshot(&host, cancelled.command_id).current_run_id, None);
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
    assert_eq!(visible_surface(&host, session_id, Some(display.through), 128), display);
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

#[test]
fn owner_surface_failure_status_survives_reopen_and_reaches_later_context() {
    let failed_model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::FailAfterDispatch(AgentFailure::ServerModelInvalidOutput),
    );
    let failed_recorder = failed_model.recorder();
    let (profile, host) = create_ready_app(&failed_model);
    let session_id = start_session(&host);
    let (_, first) = submit(&host, session_id);
    let first_run = linked(&host, first.command_id);
    let failed = wait_terminal_run(&host, first_run);
    assert_eq!(failed.state, RunState::Failed);
    assert_eq!(failed.issue, Some(AgentFailure::ServerModelInvalidOutput));
    assert!(failed.output.is_none());
    let history = entries(&host, session_id, None);
    assert_eq!(history.len(), 2);
    let floe_conversation::SurfaceEntryReceipt::RunOutcome(status) = &history[1] else {
        panic!("a failed Work must have a referenced terminal outcome");
    };
    assert_eq!(status.outcome.run_id, first_run);
    assert_eq!(status.outcome.state, RunState::Failed);
    assert_eq!(assert_script_clean(&failed_recorder).generated.len(), 1);
    drop(host);

    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let host = profile.open(&model);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    assert_eq!(entries(&host, session_id, None), history);
    let recovered_work = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            owners
                .conversation
                .list_surface_work(
                    &actor,
                    floe_conversation::SurfaceWorkPageRequest {
                        session_id,
                        cursor: None,
                        limit: 128,
                    },
                    &scope,
                )
                .await
        })
    })
    .unwrap();
    assert_eq!(recovered_work.items.len(), 1);
    assert_eq!(recovered_work.items[0].receipt, first);
    assert_eq!(
        recovered_work.items[0].state,
        SurfaceWorkState::Linked(first_run)
    );
    assert!(recovered_work.next.is_none());
    assert!(assert_script_clean(&recorder).generated.is_empty());
    let (_, later) = submit(&host, session_id);
    let later_run = linked(&host, later.command_id);
    assert_eq!(
        wait_terminal_run(&host, later_run).state,
        RunState::Completed
    );
    let observed = assert_script_clean(&recorder);
    assert_eq!(observed.generated.len(), 1);
    let context = serde_json::to_string(&observed.generated[0].envelope.conversation).unwrap();
    assert!(context.contains("manager_execution_outcome"));
    assert!(context.contains(&first_run.as_uuid().to_string()));
    assert!(context.contains("failed"));
    assert_eq!(entries(&host, session_id, None).len(), 4);
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn owner_surface_blocked_outcome_keeps_review_with_existing_interaction_owner() {
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleBlockedResumeFlow,
    );
    let recorder = model.recorder();
    let profile = IsolatedProfile::new();
    let host = profile.open_with_qa_source_transport(&model, support::UnavailableCalendarTransport);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    support::configure_fixture_calendar(&host, "Synthetic team calendar");
    let session_id = start_session(&host);
    let (_, work) = submit_text(&host, session_id, SCHEDULE_REQUEST);
    let run_id = linked(&host, work.command_id);
    let blocked = wait_terminal_run(&host, run_id);
    assert_eq!(blocked.state, RunState::Blocked);
    let history = entries(&host, session_id, None);
    assert_eq!(history.len(), 2);
    let floe_conversation::SurfaceEntryReceipt::RunOutcome(status) = &history[1] else {
        panic!("blocked outcome must be published atomically with its canonical Run");
    };
    assert_eq!(status.outcome.state, RunState::Blocked);
    assert_eq!(status.outcome.run_id, run_id);
    let interactions = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            owners
                .conversation
                .list_interactions(&actor, session_id, None, &scope)
                .await
        })
    })
    .unwrap();
    assert_eq!(interactions.interactions.len(), 1);
    assert_eq!(interactions.interactions[0].origin_run_id, run_id);
    let calls = assert_script_clean(&recorder).generated.len();
    drop(host);
    let host = profile.open_with_qa_source_transport(&model, support::UnavailableCalendarTransport);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    assert_eq!(entries(&host, session_id, None), history);
    assert_eq!(wait_terminal_run(&host, run_id).state, RunState::Blocked);
    assert_eq!(assert_script_clean(&recorder).generated.len(), calls);
}
