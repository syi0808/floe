mod support;

use std::time::{Duration, Instant};

use floe_app::{
    ActionAuthorityMode, ActionsCommand, ActionsCommandResult, ActionsQuery, ActionsQueryResult,
    AppComposition, AppHost, ConnectionsCommand, ConversationCommand, ConversationCommandOutcome,
    ConversationQuery, ConversationQueryOutcome, DayCommand, DayCommandOutcome, DayProductQuery,
    DayQueryOutcome, ExpertCommand, ExpertCommandResult, ExpertQuery, ExpertQueryResult,
    MemoryCommand, MemoryQuery, MemoryQueryResult, ProductCommand, ProductCommandDisposition,
    ProductCommandOutcome, ProductCommandRequest, ProductFailure, ProductObservation, ProductQuery,
    ProductQueryOutcome, RuntimeReadinessState,
};
use floe_conversation::{RunState, SessionMessage};
use floe_inference::ModelObservationError;
use floe_kernel::AgentFailure;
use uuid::Uuid;

use support::{
    GenerateBarrier, IsolatedProfile, ModelOutput, PlanStage, PrimaryBehavior,
    ScriptModelSelection, ScriptedModel, assert_script_clean, cancel_run, prepare_runtime,
    read_events, read_run, read_session, start_session, start_turn, terminal_run_event,
    wait_terminal_run,
};

const USER_TEXT: &str = "Summarize my current priorities.";
const REPLY: &str = "A deterministic scripted reply.";
const WAIT_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
const SCHEDULE_REQUEST: &str = "Review my calendar from 2026-10-01 through 2026-10-31.";

fn create_ready_app(model: &ScriptedModel) -> (IsolatedProfile, AppHost<AppComposition>) {
    let profile = IsolatedProfile::new();
    let host = profile.open(model);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    (profile, host)
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
fn finalization_flow_model(finalizer_selection: Option<ScriptModelSelection>) -> ScriptedModel {
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleFinalizationFlow,
    );
    let mut selections = vec![
        ScriptModelSelection::device(1),
        ScriptModelSelection::device(2),
        ScriptModelSelection::device(2),
        ScriptModelSelection::device(1),
        ScriptModelSelection::device(1),
    ];
    if let Some(selection) = finalizer_selection {
        selections[4] = selection;
    }
    model.recorder().set_model_selection_sequence(selections);
    model
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
fn start_exhausted_schedule_run(
    model: &ScriptedModel,
) -> (
    IsolatedProfile,
    AppHost<AppComposition>,
    floe_conversation::CommandReceipt,
    floe_conversation::RunReceipt,
) {
    let profile = IsolatedProfile::new();
    let host = profile.open_with_qa_source_transport(model, support::UnavailableCalendarTransport);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    support::configure_fixture_calendar(&host, "Synthetic team calendar");
    support::bind_schedule_expert(&host);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let receipt = wait_terminal_run(&host, command.run_id);
    (profile, host, command, receipt)
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
fn journal_events(
    host: &AppHost<AppComposition>,
    run_id: floe_kernel::RunId,
) -> Vec<floe_agent_contract::JournalEvent> {
    support::read_vault_conversation_journal(host, run_id)
        .iter()
        .map(|entry| serde_json::from_str(&entry.payload).expect("decode encrypted journal event"))
        .collect()
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
fn assert_finalizer_drift_rejected(selection: ScriptModelSelection) {
    let model = finalization_flow_model(Some(selection));
    let recorder = model.recorder();
    let (_profile, host, command, receipt) = start_exhausted_schedule_run(&model);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::PolicyDenied));
    assert_eq!(receipt.output, None);
    let script = assert_script_clean(&recorder);
    assert_eq!(script.generated.len(), 4);
    assert_eq!(
        script.plans.len(),
        10,
        "the changed finalizer plan was observed"
    );
    assert_eq!(
        script.plan_bindings[8].consumer,
        floe_conversation::CONVERSATION_CONSUMER
    );

    let events = journal_events(&host, command.run_id);
    assert!(events.iter().any(|event| matches!(
        event,
        floe_agent_contract::JournalEvent::FinalizationStarted {
            prior_exhaustion: AgentFailure::BudgetExceeded,
            ..
        }
    )));
    let manager_intents = events
        .iter()
        .filter(|event| {
            matches!(event,
                floe_agent_contract::JournalEvent::ModelIntent { plan, .. }
                    if plan.consumer == floe_conversation::CONVERSATION_CONSUMER
            )
        })
        .count();
    assert_eq!(
        manager_intents, 2,
        "the changed finalizer selection was rejected before a finalization ModelIntent"
    );
}

fn wait_terminal_event(
    host: &AppHost<AppComposition>,
    run_id: floe_kernel::RunId,
) -> floe_conversation::RunEventRecord {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        if let Some(event) = terminal_run_event(read_events(host), run_id) {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "terminal Conversation event exceeded deadline"
        );
        std::thread::yield_now();
    }
}

#[test]
fn text_turn_completes_and_persists_transcript_receipt_and_events() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Completed);
    assert_eq!(receipt.output.as_deref(), Some(REPLY));
    assert_eq!(receipt.issue, None);
    let transcript = read_session(&host, session_id);
    assert_eq!(transcript.id, session_id);
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::User { text, .. } if text == USER_TEXT
    )));
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { text, .. } if text == REPLY
    )));

    let event = wait_terminal_event(&host, command.run_id);
    assert_eq!(event.state, RunState::Completed);
    assert!(event.generated_reply);

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(
        snapshot
            .plans
            .iter()
            .map(|(stage, _)| *stage)
            .collect::<Vec<_>>(),
        vec![PlanStage::Primary, PlanStage::LocalFallback]
    );
    assert_eq!(snapshot.generated.len(), 1);
    assert!(snapshot.generated[0].catalog.tools.is_empty());
}

#[test]
fn manager_normalized_history_pages_exactly_across_reopen() {
    const TURN_COUNT: usize = 129;
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::RepeatedAnswers(TURN_COUNT),
    );
    let recorder = model.recorder();
    let profile = IsolatedProfile::new();
    let host = profile.open(&model);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    let session_id = start_session(&host);
    let mut run_ids = std::collections::HashSet::new();
    for _ in 0..TURN_COUNT {
        let command = start_turn(&host, session_id, USER_TEXT);
        assert!(run_ids.insert(command.run_id.as_uuid()));
        assert_eq!(
            wait_terminal_run(&host, command.run_id).state,
            RunState::Completed
        );
    }

    let latest = read_session(&host, session_id);
    assert_eq!(latest.messages.len(), 256);
    assert!(latest.has_earlier_messages);
    let oldest_visible_id = match latest.messages.first().expect("bounded page is nonempty") {
        SessionMessage::User { message_id, .. }
        | SessionMessage::Assistant { message_id, .. }
        | SessionMessage::Preamble { message_id, .. }
        | SessionMessage::Compaction { message_id, .. }
        | SessionMessage::Capability { message_id, .. }
        | SessionMessage::Delegation { message_id, .. }
        | SessionMessage::Interaction { message_id, .. } => *message_id,
    };
    let earlier = support::read_session_before(&host, session_id, oldest_visible_id);
    assert_eq!(earlier.messages.len(), 2);
    assert!(!earlier.has_earlier_messages);
    let first_turn_id = match &earlier.messages[0] {
        SessionMessage::User { turn_id, text, .. } if text == USER_TEXT => *turn_id,
        other => panic!("expected the first retained User input, found {other:?}"),
    };
    assert!(matches!(
        &earlier.messages[1],
        SessionMessage::Assistant { turn_id, text, .. }
            if *turn_id == first_turn_id && text == REPLY
    ));

    let mut all_ids = std::collections::HashSet::new();
    let mut user_count = 0;
    for message in earlier.messages.iter().chain(&latest.messages) {
        let message_id = match message {
            SessionMessage::User { message_id, .. }
            | SessionMessage::Assistant { message_id, .. }
            | SessionMessage::Preamble { message_id, .. }
            | SessionMessage::Compaction { message_id, .. }
            | SessionMessage::Capability { message_id, .. }
            | SessionMessage::Delegation { message_id, .. }
            | SessionMessage::Interaction { message_id, .. } => *message_id,
        };
        assert!(
            all_ids.insert(message_id),
            "cursor pages duplicated an alias"
        );
        user_count += usize::from(matches!(message, SessionMessage::User { .. }));
    }
    assert_eq!(all_ids.len(), 258);
    assert_eq!(
        user_count, TURN_COUNT,
        "every New turn appends one User input"
    );

    let missing_cursor = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(15),
        );
        services.execute_owner(async move {
            owners
                .conversation
                .get_session(&actor, session_id, Some(Uuid::new_v4()), &scope)
                .await
        })
    });
    assert_eq!(missing_cursor, Err(AgentFailure::NotFound));

    let scripted = assert_script_clean(&recorder);
    assert_eq!(scripted.generated.len(), TURN_COUNT);
    assert_eq!(scripted.plan_bindings.len(), TURN_COUNT * 2);
    host.shutdown()
        .expect("close Manager generation before reopen");
    drop(host);

    let reopened = profile.open(&model);
    assert_eq!(prepare_runtime(&reopened), RuntimeReadinessState::Ready);
    let reopened_latest = read_session(&reopened, session_id);
    let reopened_earlier = support::read_session_before(&reopened, session_id, oldest_visible_id);
    assert_eq!(reopened_latest.messages, latest.messages);
    assert_eq!(
        reopened_latest.has_earlier_messages,
        latest.has_earlier_messages
    );
    assert_eq!(reopened_earlier.messages, earlier.messages);
    assert_eq!(
        reopened_earlier.has_earlier_messages,
        earlier.has_earlier_messages
    );
    assert_eq!(assert_script_clean(&recorder).generated.len(), TURN_COUNT);
    reopened
        .shutdown()
        .expect("close reopened Manager generation");
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn blocked_expert_binding_publishes_textless_manager_interaction_history() {
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleExpertFlow,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(receipt.state, RunState::Blocked);
    assert!(receipt.output.is_none());

    let session = read_session(&host, session_id);
    assert!(session.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Delegation { turn_id, task, .. }
            if *turn_id == command.run_id.as_uuid()
                && task.state == floe_agent_contract::TaskState::Blocked
    )));
    assert!(session.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Interaction { turn_id, interaction_kind, .. }
            if *turn_id == command.run_id.as_uuid()
                && *interaction_kind == floe_agent_contract::UserInteractionKind::ExpertBinding
    )));
    let scripted = assert_script_clean(&recorder);
    assert_eq!(scripted.generated.len(), 1);
    host.shutdown().expect("close blocked Manager generation");
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn linked_resume_reuses_original_user_input_in_normalized_manager_history() {
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
    let origin = start_turn(&host, session_id, SCHEDULE_REQUEST);
    assert_eq!(
        wait_terminal_run(&host, origin.run_id).state,
        RunState::Blocked
    );
    let original_user_message_id = read_session(&host, session_id)
        .messages
        .iter()
        .find_map(|message| match message {
            SessionMessage::User { message_id, .. } => Some(*message_id),
            _ => None,
        })
        .expect("blocked origin has its exact normalized User input");

    let interactions = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(30),
        );
        services.execute_owner(async move {
            owners
                .conversation
                .list_interactions(&actor, session_id, &scope)
                .await
        })
    })
    .expect("read exact blocked Manager interactions");
    let [interaction] = interactions.as_slice() else {
        panic!("expected one blocked Expert binding interaction, found {interactions:?}");
    };
    assert_eq!(
        interaction.interaction_kind,
        floe_agent_contract::UserInteractionKind::ExpertBinding
    );
    assert_eq!(interaction.origin_run_id, origin.run_id);
    let floe_conversation::InteractionTarget::ExpertBinding { review } = &interaction.target else {
        panic!("blocked binding interaction did not retain its exact review")
    };
    let candidate = review
        .candidate_refs_and_labels
        .iter()
        .find(|candidate| {
            candidate.label == "Synthetic QA Calendar"
                && candidate.availability == floe_experts::CandidateAvailability::Available
        })
        .expect("the exact blocked Task review includes the configured Calendar candidate");
    let review_ref = review.review_ref.clone();
    let binding_revision = review.binding_revision;
    let candidate_ref = candidate.candidate_ref;
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(30),
        );
        services.execute_owner(async move {
            owners
                .experts
                .replace_binding(
                    &actor,
                    floe_kernel::CommandId::from_uuid(Uuid::new_v4())
                        .ok_or(AgentFailure::InvalidInput)?,
                    review_ref,
                    binding_revision,
                    vec![candidate_ref],
                    &scope,
                )
                .await
                .map_err(floe_kernel::CommandFailure::into_failure)
        })
    })
    .expect("bind the exact candidate from the blocked Task's review");
    let refresh_command_id = Uuid::new_v4();
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let linked = loop {
        let interaction_id = interaction.interaction_id;
        let expected_revision = interaction.revision;
        let result = support::with_ready(&host, |services, caller, owners| {
            let actor = caller.owner_actor();
            let scope = floe_app::host_scope(
                Uuid::new_v4(),
                floe_execution::Cancellation::new(),
                Duration::from_secs(30),
            );
            services.execute_owner(async move {
                owners
                    .conversation
                    .refresh_interaction(
                        &actor,
                        floe_conversation::RefreshInteraction {
                            command_id: refresh_command_id,
                            interaction_id,
                            session_id,
                            expected_revision,
                        },
                        &scope,
                    )
                    .await
            })
        });
        let result = match result {
            Ok(result) => result,
            // Keep the exact command ID: if admission committed before the
            // transient storage error, the repository returns its durable
            // receipt instead of scheduling a second Resume.
            Err(floe_kernel::CommandFailure::Indeterminate(
                floe_agent_contract::AgentFailure::StorageBusy,
            )) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(failure) => panic!("refresh the durable linked Resume receipt: {failure:?}"),
        };
        assert_eq!(
            result.interaction.state,
            floe_conversation::InteractionStatus::Resolved
        );
        if let Some(linked) = result.linked {
            break linked;
        }
        assert!(
            Instant::now() < deadline,
            "linked Resume admission timed out"
        );
        std::thread::yield_now();
    };
    assert_ne!(linked.run_id, origin.run_id);
    let resumed = wait_terminal_run(&host, linked.run_id);
    assert_eq!(
        resumed.state,
        RunState::Completed,
        "linked Resume terminal issue: {:?}; unresolved model attempts: {:?}",
        resumed.issue,
        resumed.unresolved_attempts
    );
    assert_eq!(resumed.resume_of, Some(origin.run_id));
    assert_eq!(resumed.user_message_id, original_user_message_id);

    let history = read_session(&host, session_id);
    assert_eq!(
        history
            .messages
            .iter()
            .filter(|message| matches!(message, SessionMessage::User { .. }))
            .count(),
        1,
        "linked Resume must not append another copy of the original User input"
    );
    assert!(matches!(
        history.messages.first(),
        Some(SessionMessage::User { message_id, .. })
            if *message_id == original_user_message_id
    ));
    assert!(history.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Delegation { turn_id, task, .. }
            if *turn_id == origin.run_id.as_uuid()
                && task.state == floe_agent_contract::TaskState::Blocked
    )));
    assert!(history.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { turn_id, text, .. }
            if *turn_id == linked.run_id.as_uuid()
                && text == "I resumed with the original request and no new source result."
    )));
    let script = assert_script_clean(&recorder);
    assert_eq!(script.generated.len(), 2);
    assert!(script.generated_bindings.iter().all(|binding| {
        binding.consumer == floe_conversation::CONVERSATION_CONSUMER && binding.task_id.is_none()
    }));
    host.shutdown()
        .expect("close Manager generation after linked Resume");
}

#[test]
fn typed_product_router_keeps_command_identity_and_observer_drop_does_not_cancel_run() {
    let (barrier, entered, _release) = GenerateBarrier::new();
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::WaitForCancellation(barrier),
    );
    let (_profile, host) = create_ready_app(&model);

    let start_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let start = host
        .request(Uuid::new_v4())
        .expect("admit typed session command");
    let started = start
        .product_command(ProductCommandRequest {
            command_id: start_id,
            command: ProductCommand::Conversation(ConversationCommand::StartSession),
        })
        .expect("route session start");
    let ConversationCommandOutcome::Session(session) = (match started {
        ProductCommandOutcome::Conversation(value) => value,
        _ => panic!("unexpected product result"),
    }) else {
        panic!("session start returned another Conversation result")
    };
    drop(start);

    let turn_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let turn = host
        .request(Uuid::new_v4())
        .expect("admit typed turn command");
    let result = turn
        .product_command(ProductCommandRequest {
            command_id: turn_id,
            command: ProductCommand::Conversation(ConversationCommand::StartTurn {
                session_id: session.id,
                expected_revision: session.revision,
                text: USER_TEXT.into(),
                continuation_id: None,
                retry_of: None,
            }),
        })
        .expect("route turn command");
    let ConversationCommandOutcome::Turn(receipt) = (match result {
        ProductCommandOutcome::Conversation(value) => value,
        _ => panic!("unexpected product result"),
    }) else {
        panic!("turn command returned another Conversation result")
    };
    assert_eq!(receipt.command_id, turn_id);
    drop(turn);
    entered
        .recv_timeout(WAIT_TIMEOUT)
        .expect("scripted model reached the in-flight dispatch barrier");

    let observer = host
        .request(Uuid::new_v4())
        .expect("admit bounded product observation");
    let observed = observer
        .observe_product(ProductObservation {
            runtime_epoch: Some(observer.caller().runtime_epoch()),
            cursor: Some(0),
            limit: 10,
        })
        .expect("read bounded Conversation events");
    assert!(matches!(
        observed,
        floe_app::ProductObservationOutcome::Conversation(_)
    ));
    drop(observer);

    let query = host.request(Uuid::new_v4()).expect("admit Run query");
    let current = query
        .product_query(ProductQuery::Conversation(ConversationQuery::GetRun {
            run_id: receipt.run_id,
        }))
        .expect("observe the admitted Run after observation disposal");
    assert!(matches!(
        current,
        ProductQueryOutcome::Conversation(ConversationQueryOutcome::Run(Some(ref run)))
            if run.state == RunState::Working
    ));
    drop(query);

    let cancel_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let cancel = host
        .request(Uuid::new_v4())
        .expect("admit explicit cancellation");
    let cancelled = cancel
        .product_command(ProductCommandRequest {
            command_id: cancel_id,
            command: ProductCommand::Conversation(ConversationCommand::CancelRun {
                run_id: receipt.run_id,
            }),
        })
        .expect("route explicit cancellation");
    let ConversationCommandOutcome::CancelRun(cancel_receipt) = (match cancelled {
        ProductCommandOutcome::Conversation(value) => value,
        _ => panic!("unexpected product result"),
    }) else {
        panic!("cancellation returned another Conversation result")
    };
    assert_eq!(cancel_receipt.command_id, cancel_id);
    assert_eq!(
        wait_terminal_run(&host, receipt.run_id).state,
        RunState::Cancelled
    );
}

#[test]
fn conversation_retry_replays_receipt_before_session_validation_and_preserves_uncertainty() {
    let (barrier, entered, _release) = GenerateBarrier::new();
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::WaitForCancellation(barrier),
    );
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let before = read_session(&host, session_id);
    let command_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();

    let send_turn = |command_id, expected_revision, text: &str| {
        host.request(Uuid::new_v4())
            .expect("admit Conversation product request")
            .product_command(ProductCommandRequest {
                command_id,
                command: ProductCommand::Conversation(ConversationCommand::StartTurn {
                    session_id,
                    expected_revision,
                    text: text.into(),
                    continuation_id: None,
                    retry_of: None,
                }),
            })
    };

    // Ignore the first response to model an ACK lost after durable admission.
    drop(
        send_turn(command_id, before.revision, USER_TEXT)
            .expect("durably admit the first turn submission"),
    );
    entered
        .recv_timeout(WAIT_TIMEOUT)
        .expect("the admitted run reached its in-flight model barrier");

    let replay = send_turn(command_id, before.revision, USER_TEXT)
        .expect("replay the exact durable turn after its ACK was lost");
    let ConversationCommandOutcome::Turn(receipt) = (match replay {
        ProductCommandOutcome::Conversation(value) => value,
        _ => panic!("turn retry returned another product outcome"),
    }) else {
        panic!("turn retry returned another Conversation result")
    };
    assert_eq!(receipt.command_id, command_id);

    let invalid_reuse = send_turn(command_id, before.revision, "")
        .expect_err("later validation cannot make an occupied id disposable");
    assert_eq!(
        invalid_reuse.disposition,
        ProductCommandDisposition::Indeterminate
    );

    let mismatch = send_turn(command_id, before.revision, "A changed request body")
        .expect_err("an occupied command id cannot be assigned a new body");
    assert_eq!(
        mismatch.disposition,
        ProductCommandDisposition::Indeterminate
    );

    let stale = send_turn(
        floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
        before.revision,
        USER_TEXT,
    )
    .expect_err("a different command with a stale session revision is rejected");
    assert_eq!(stale.disposition, ProductCommandDisposition::NotApplied);

    cancel_run(&host, receipt.run_id);
    assert_eq!(
        wait_terminal_run(&host, receipt.run_id).state,
        RunState::Cancelled
    );
}

#[test]
fn definitive_conversation_conflict_allows_a_corrected_new_command_id() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let before = read_session(&host, session_id);
    let stale_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let stale = host
        .request(Uuid::new_v4())
        .expect("admit stale Conversation request")
        .product_command(ProductCommandRequest {
            command_id: stale_id,
            command: ProductCommand::Conversation(ConversationCommand::StartTurn {
                session_id,
                expected_revision: before.revision + 1,
                text: USER_TEXT.into(),
                continuation_id: None,
                retry_of: None,
            }),
        })
        .expect_err("stale revision is a definitive pre-admission conflict");
    assert_eq!(stale.disposition, ProductCommandDisposition::NotApplied);

    let corrected_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let corrected = host
        .request(Uuid::new_v4())
        .expect("admit corrected Conversation request")
        .product_command(ProductCommandRequest {
            command_id: corrected_id,
            command: ProductCommand::Conversation(ConversationCommand::StartTurn {
                session_id,
                expected_revision: before.revision,
                text: USER_TEXT.into(),
                continuation_id: None,
                retry_of: None,
            }),
        })
        .expect("a new id can carry the corrected intent after NotApplied");
    let ConversationCommandOutcome::Turn(receipt) = (match corrected {
        ProductCommandOutcome::Conversation(value) => value,
        _ => panic!("corrected turn returned another product outcome"),
    }) else {
        panic!("corrected command returned another Conversation result")
    };
    assert_eq!(receipt.command_id, corrected_id);
    assert_eq!(
        wait_terminal_run(&host, receipt.run_id).state,
        RunState::Completed
    );
}

#[test]
fn typed_day_query_remains_available_before_runtime_readiness() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let profile = IsolatedProfile::new();
    let host = profile.open(&model);
    let request = host.request(Uuid::new_v4()).expect("admit Day query");
    let query = floe_day::DayQuery {
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 7).expect("valid date"),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
        now: chrono::DateTime::parse_from_rfc3339("2026-10-07T00:00:00Z")
            .expect("valid instant")
            .with_timezone(&chrono::Utc),
    };
    let result = request
        .product_query(ProductQuery::Day(DayProductQuery::Snapshot(query)))
        .expect("Day stays independent of Runtime readiness");
    assert!(matches!(result, ProductQueryOutcome::Day(_)));
}

#[test]
fn typed_product_router_keeps_actions_experts_and_memory_on_the_same_path() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);

    let actions = host
        .request(Uuid::new_v4())
        .expect("admit Actions query")
        .product_query(ProductQuery::Actions(ActionsQuery::List {
            cursor: None,
            limit: 10,
        }))
        .expect("route existing Actions query");
    assert!(matches!(
        actions,
        ProductQueryOutcome::Actions(ActionsQueryResult::Page(_))
    ));

    let experts = host
        .request(Uuid::new_v4())
        .expect("admit Experts query")
        .product_query(ProductQuery::Experts(ExpertQuery::Directory))
        .expect("route existing Experts query");
    assert!(matches!(
        experts,
        ProductQueryOutcome::Experts(ExpertQueryResult::Directory(_))
    ));

    let memory = host
        .request(Uuid::new_v4())
        .expect("admit Memory query")
        .product_query(ProductQuery::Memory(MemoryQuery::Review))
        .expect("route Memory query");
    assert!(matches!(
        memory,
        ProductQueryOutcome::Memory(MemoryQueryResult::Review(_))
    ));
}

#[test]
fn experts_conflict_and_receipt_replay_keep_owner_dispositions() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let directory = host
        .request(Uuid::new_v4())
        .expect("admit Experts directory query")
        .product_query(ProductQuery::Experts(ExpertQuery::Directory))
        .expect("read Experts directory");
    let ProductQueryOutcome::Experts(ExpertQueryResult::Directory(directory)) = directory else {
        panic!("Experts directory query returned another result")
    };
    let installation_ref = directory
        .installations
        .first()
        .expect("built-in Experts installation exists")
        .installation_ref;

    let send = |command_id, expected_revision, enabled| {
        host.request(Uuid::new_v4())
            .expect("admit Experts product request")
            .product_command(ProductCommandRequest {
                command_id,
                command: ProductCommand::Experts(ExpertCommand::SetInstallationEnabled {
                    installation_ref,
                    expected_revision,
                    enabled,
                }),
            })
    };

    let rejected_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let rejected = send(rejected_id, directory.revision + 1, false)
        .expect_err("a stale registry revision is rejected before admission");
    assert_eq!(rejected.disposition, ProductCommandDisposition::NotApplied);

    let command_id = floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap();
    let admitted =
        send(command_id, directory.revision, false).expect("admit Expert installation change");
    let ProductCommandOutcome::Experts(ExpertCommandResult::Directory(updated)) = admitted else {
        panic!("Experts command returned another outcome")
    };

    let replay = send(command_id, directory.revision, false)
        .expect("recover the exact Expert command receipt after registry revision advanced");
    assert!(matches!(
        replay,
        ProductCommandOutcome::Experts(ExpertCommandResult::Directory(ref value))
            if value.revision == updated.revision
    ));

    let changed_body = send(command_id, directory.revision, true)
        .expect_err("an Expert receipt cannot be rebound to a changed body");
    assert_eq!(
        changed_body.disposition,
        ProductCommandDisposition::Indeterminate
    );
}

#[test]
fn actions_authority_replay_checks_receipt_before_revision_validation() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let authority = host
        .request(Uuid::new_v4())
        .expect("admit Actions authority query")
        .product_query(ProductQuery::Actions(ActionsQuery::Authority))
        .expect("read Actions authority");
    let ProductQueryOutcome::Actions(ActionsQueryResult::Authority(authority)) = authority else {
        panic!("Actions authority query returned another result")
    };
    let send = |command_id, expected_revision| {
        host.request(Uuid::new_v4())
            .expect("admit Actions product request")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(command_id).unwrap(),
                command: ProductCommand::Actions(ActionsCommand::SetAuthority {
                    mode: ActionAuthorityMode::Deny,
                    expected_revision,
                }),
            })
    };

    let stale = send(Uuid::new_v4(), authority.revision + 1)
        .expect_err("stale authority revision is a precommit conflict");
    assert_eq!(stale.disposition, ProductCommandDisposition::NotApplied);

    let command_id = Uuid::new_v4();
    let admitted = send(command_id, authority.revision).expect("admit authority change");
    let ProductCommandOutcome::Actions(ActionsCommandResult::Authority(updated)) = admitted else {
        panic!("Actions authority command returned another outcome")
    };

    let replay = send(command_id, authority.revision)
        .expect("replay the exact authority command after its revision advanced");
    assert!(matches!(
        replay,
        ProductCommandOutcome::Actions(ActionsCommandResult::Authority(ref value))
            if value.revision == updated.revision
    ));

    let invalid_reuse = send(command_id, 0)
        .expect_err("revision validation cannot release a previously used command id");
    assert_eq!(
        invalid_reuse.disposition,
        ProductCommandDisposition::Indeterminate
    );
}

#[test]
fn day_mutation_replays_receipts_before_body_validation_and_releases_only_not_applied_ids() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let day = floe_day::DayQuery {
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
        now: chrono::DateTime::parse_from_rfc3339("2026-10-07T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    };
    let send = |command_id, content: &str| {
        host.request(Uuid::new_v4())
            .expect("admit Day product request")
            .product_command(ProductCommandRequest {
                command_id: floe_kernel::CommandId::from_uuid(command_id).unwrap(),
                command: ProductCommand::Day(DayCommand::Mutate {
                    day: day.clone(),
                    mutation: floe_day::DayMutation::CreateNote {
                        content: content.into(),
                        occurred_at: day.now,
                    },
                }),
            })
    };

    let command_id = Uuid::new_v4();
    let admitted = send(command_id, "Keep this note.").expect("admit Day note mutation");
    let ProductCommandOutcome::Day(DayCommandOutcome::Mutation(receipt)) = admitted else {
        panic!("Day mutation returned another outcome")
    };
    assert_eq!(receipt.command_id, command_id);

    let replay = send(command_id, "Keep this note.")
        .expect("recover the exact Day receipt after the snapshot changed");
    assert!(matches!(
        replay,
        ProductCommandOutcome::Day(DayCommandOutcome::Mutation(ref result))
            if result.command_id == command_id
    ));
    let invalid_reuse = send(command_id, "")
        .expect_err("a validation failure cannot free a previously used Day id");
    assert_eq!(
        invalid_reuse.disposition,
        ProductCommandDisposition::Indeterminate
    );

    let rejected =
        send(Uuid::new_v4(), "").expect_err("an invalid first Day request is proven not applied");
    assert_eq!(rejected.disposition, ProductCommandDisposition::NotApplied);

    let corrected = send(Uuid::new_v4(), "Corrected note.")
        .expect("a corrected Day request can use a new id after NotApplied");
    assert!(matches!(
        corrected,
        ProductCommandOutcome::Day(DayCommandOutcome::Mutation(_))
    ));
}

#[test]
fn memory_decision_validation_is_classified_by_knowledge_owner() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let failure = host
        .request(Uuid::new_v4())
        .expect("admit Memory product request")
        .product_command(ProductCommandRequest {
            command_id: floe_kernel::CommandId::new(),
            command: ProductCommand::Memory(MemoryCommand::Decide {
                candidate_id: Uuid::nil(),
                decision: floe_knowledge::KnowledgeDecisionKind::Reject,
            }),
        })
        .expect_err("fresh invalid candidate is proved not applied by Knowledge");
    assert_eq!(failure.disposition, ProductCommandDisposition::NotApplied);
}

#[test]
fn typed_connections_observe_command_reaches_connections_owner_after_admission() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let request = host
        .request(Uuid::new_v4())
        .expect("admit Connections request");
    let prepare = request.product_command(ProductCommandRequest {
        command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
        command: ProductCommand::Connections(ConnectionsCommand::ObservePrepareReview {
            source_ref: Uuid::new_v4(),
            expected_revision: 1,
            requested_processing: floe_connections::ProcessingChoice::GatewayAllowed,
        }),
    });
    let failure = prepare.expect_err("unknown source is rejected during Observe review");
    assert_eq!(failure.disposition, ProductCommandDisposition::NotApplied);
    assert!(matches!(failure.failure, ProductFailure::Connections(_)));

    let result = request.product_command(ProductCommandRequest {
        command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4()).unwrap(),
        command: ProductCommand::Connections(ConnectionsCommand::ObservePause {
            source_ref: Uuid::new_v4(),
            expected_revision: 1,
        }),
    });
    let failure = result.expect_err("unknown source is rejected by Connections");
    assert_eq!(failure.disposition, ProductCommandDisposition::NotAdmitted);
    assert!(matches!(failure.failure, ProductFailure::Connections(_)));
}

#[test]
fn app_host_rejects_a_product_request_before_admitting_a_nil_request_identity() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let profile = IsolatedProfile::new();
    let host = profile.open(&model);

    assert!(matches!(
        host.request(Uuid::nil()),
        Err(floe_app::HostError::InvalidRequest)
    ));

    let request_id = Uuid::new_v4();
    let request = host
        .request(request_id)
        .expect("admit product request through AppHost");
    assert_eq!(request.request_id(), request_id);
}

#[test]
fn runtime_preparation_is_idempotent_archived_before_ack_and_replayed_as_history() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let request = host
        .request(Uuid::new_v4())
        .expect("admit Runtime preparation request");
    let services = request.services();
    let caller = request.caller();
    let operation_id = Uuid::new_v4();

    let first = services
        .prepare_runtime(caller, operation_id)
        .expect("admit Runtime preparation");
    let duplicate = services
        .prepare_runtime(caller, operation_id)
        .expect("join the same immutable preparation");
    assert_eq!(first.operation_id, operation_id);
    assert_eq!(duplicate.operation_id, operation_id);
    if first.done {
        assert!(duplicate.done, "a completed preparation stays completed");
        assert_eq!(duplicate.failure, first.failure);
    }

    let deadline = Instant::now() + Duration::from_secs(55);
    let completed = loop {
        let result = services
            .get_runtime_preparation(caller, operation_id)
            .expect("observe the exact preparation");
        if result.done {
            break result;
        }
        assert!(Instant::now() < deadline, "Runtime preparation timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(completed.failure, None);

    let acknowledged = services
        .acknowledge_runtime_preparation(caller, operation_id)
        .expect("acknowledge the durable result");
    assert_eq!(acknowledged, completed);

    let replayed = services
        .prepare_runtime(caller, operation_id)
        .expect("replay the archived result without re-execution");
    let acknowledged_again = services
        .acknowledge_runtime_preparation(caller, operation_id)
        .expect("ACK remains idempotent after cache eviction");
    assert_eq!(replayed, completed);
    assert_eq!(acknowledged_again, completed);
    assert_eq!(
        services
            .runtime_readiness(caller, Uuid::new_v4())
            .expect("observe current readiness independently")
            .state,
        RuntimeReadinessState::Ready
    );

    let healthy_noop_id = Uuid::new_v4();
    let noop = services
        .prepare_runtime(caller, healthy_noop_id)
        .expect("healthy generation accepts a no-op prepare");
    assert_eq!(noop.operation_id, healthy_noop_id);
    let deadline = Instant::now() + Duration::from_secs(55);
    let noop = loop {
        let result = services
            .get_runtime_preparation(caller, healthy_noop_id)
            .expect("observe the healthy no-op result");
        if result.done {
            break result;
        }
        assert!(Instant::now() < deadline, "Runtime no-op timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(noop.failure, None);
    services
        .acknowledge_runtime_preparation(caller, healthy_noop_id)
        .expect("acknowledge the healthy no-op result");
    assert_eq!(
        services
            .runtime_readiness(caller, Uuid::new_v4())
            .expect("preparation history did not replace current state")
            .state,
        RuntimeReadinessState::Ready
    );
}

#[test]
fn primary_observation_failure_never_invokes_fallback_or_generate() {
    let model = ScriptedModel::new(
        PrimaryBehavior::Fail(ModelObservationError::TransportUnavailable),
        USER_TEXT,
        ModelOutput::Answer,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::ServerModelUnavailable));
    assert_eq!(receipt.output, None);
    assert_eq!(
        wait_terminal_event(&host, command.run_id).state,
        RunState::Failed
    );

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(snapshot.plans.len(), 1);
    assert_eq!(snapshot.plans[0].0, PlanStage::Primary);
    assert!(snapshot.generated.is_empty());
}

#[test]
fn in_flight_model_dispatch_can_be_cancelled_by_the_conversation_owner() {
    let (barrier, entered, _release) = GenerateBarrier::new();
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::WaitForCancellation(barrier),
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    entered
        .recv_timeout(WAIT_TIMEOUT)
        .expect("scripted model reached the in-flight dispatch barrier");

    cancel_run(&host, command.run_id);
    let receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(receipt.state, RunState::Cancelled);
    assert_eq!(receipt.issue, Some(AgentFailure::Cancelled));
    assert_eq!(receipt.output, None);
    let event = wait_terminal_event(&host, command.run_id);
    assert_eq!(event.state, RunState::Cancelled);
    assert!(!event.generated_reply);

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(snapshot.generated.len(), 1);
}

#[test]
fn closing_and_reopening_profile_preserves_result_without_model_redispatch() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let profile = IsolatedProfile::new();
    let host = profile.open(&model);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let first_receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(first_receipt.state, RunState::Completed);
    assert_eq!(first_receipt.output.as_deref(), Some(REPLY));
    assert_eq!(
        wait_terminal_event(&host, command.run_id).state,
        RunState::Completed
    );
    let calls_before_close = assert_script_clean(&recorder);
    assert_eq!(calls_before_close.generated.len(), 1);
    host.shutdown().expect("close first App generation");
    drop(host);

    let reopened = profile.open(&model);
    assert_eq!(prepare_runtime(&reopened), RuntimeReadinessState::Ready);
    let resumed_session = support::with_ready(&reopened, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(15),
        );
        services
            .execute_owner(async move { owners.conversation.resume_session(&actor, &scope).await })
            .expect("resume persisted Conversation session")
    });
    assert_eq!(
        resumed_session.as_ref().map(|session| session.id),
        Some(session_id)
    );

    let reopened_receipt = read_run(
        &reopened,
        floe_kernel::RunId::from_uuid(command.run_id.as_uuid()).expect("valid run id"),
    )
    .expect("persisted run receipt after reopen");
    assert_eq!(reopened_receipt.state, RunState::Completed);
    assert_eq!(reopened_receipt.output.as_deref(), Some(REPLY));
    let transcript = read_session(&reopened, session_id);
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { text, .. } if text == REPLY
    )));
    let after_reopen = assert_script_clean(&recorder);
    assert_eq!(after_reopen.plans.len(), calls_before_close.plans.len());
    assert_eq!(
        after_reopen.generated.len(),
        calls_before_close.generated.len()
    );
    reopened.shutdown().expect("close reopened App generation");
}

#[test]
fn unsupported_manager_tool_call_is_rejected_without_execution() {
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::UnsupportedManagerToolCall,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, USER_TEXT);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::ServerModelInvalidOutput));
    assert_eq!(receipt.output, None);
    let transcript = read_session(&host, session_id);
    assert!(
        !transcript
            .messages
            .iter()
            .any(|message| matches!(message, SessionMessage::Capability { .. }))
    );
    assert_eq!(
        wait_terminal_event(&host, command.run_id).state,
        RunState::Failed
    );

    let snapshot = assert_script_clean(&recorder);
    assert_eq!(snapshot.generated.len(), 1);
    assert!(snapshot.generated[0].catalog.tools.is_empty());
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn day_refresh_reads_only_the_selected_synthetic_calendar_for_fixed_day() {
    use floe_day::{DayCoverageState, DayRefreshState, DayTimelineItem};

    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);

    let source = support::configure_fixture_calendar(&host, "Synthetic team calendar");
    assert_eq!(
        source
            .selected_resources
            .iter()
            .map(|resource| resource.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Synthetic team calendar"]
    );

    let query = floe_app::DayQuery {
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 8).expect("valid fixed fixture date"),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
        now: chrono::DateTime::parse_from_rfc3339("2026-10-07T00:00:00Z")
            .expect("valid fixed fixture instant")
            .with_timezone(&chrono::Utc),
    };
    let refresh_request = host
        .request(Uuid::new_v4())
        .expect("admit typed Day refresh command");
    let admitted = refresh_request
        .product_command(ProductCommandRequest {
            command_id: floe_kernel::CommandId::from_uuid(Uuid::new_v4())
                .expect("valid Day command identity"),
            command: ProductCommand::Day(DayCommand::Refresh(query.clone())),
        })
        .unwrap_or_else(|failure| panic!("phase=day_refresh_admission failure={failure:?}"));
    let ProductCommandOutcome::Day(DayCommandOutcome::Refresh(admitted)) = admitted else {
        panic!("Day refresh returned another command result");
    };
    drop(refresh_request);

    let deadline = Instant::now() + WAIT_TIMEOUT;
    let refresh = loop {
        let request = host
            .request(Uuid::new_v4())
            .expect("admit typed Day refresh query");
        let current = request
            .product_query(ProductQuery::Day(DayProductQuery::RefreshGet {
                operation_ref: admitted.operation_ref,
            }))
            .unwrap_or_else(|failure| panic!("phase=day_refresh_poll failure={failure:?}"));
        let ProductQueryOutcome::Day(DayQueryOutcome::Refresh(current)) = current else {
            panic!("Day refresh query returned another product result");
        };
        if current.state.terminal() {
            break current;
        }
        let last_phase = match &current.state {
            DayRefreshState::Pending => "pending".to_owned(),
            DayRefreshState::Running => "running".to_owned(),
            DayRefreshState::Completed { .. } => "completed".to_owned(),
            DayRefreshState::Failed { failure } => format!("failed:{failure:?}"),
            DayRefreshState::Interrupted { failure } => format!("interrupted:{failure:?}"),
        };
        assert!(
            Instant::now() < deadline,
            "phase=day_refresh_wait timed out; last_state={last_phase}"
        );
        std::thread::yield_now();
    };

    let request = host
        .request(Uuid::new_v4())
        .expect("admit typed Day snapshot query");
    let snapshot = request
        .product_query(ProductQuery::Day(DayProductQuery::Snapshot(query.clone())))
        .unwrap_or_else(|failure| panic!("phase=day_read failure={failure:?}"));
    let ProductQueryOutcome::Day(DayQueryOutcome::Snapshot(day)) = snapshot else {
        panic!("Day snapshot query returned another product result");
    };

    let refresh_phase = match &refresh.state {
        DayRefreshState::Pending => "pending".to_owned(),
        DayRefreshState::Running => "running".to_owned(),
        DayRefreshState::Completed { .. } => "completed".to_owned(),
        DayRefreshState::Failed { failure } => format!("failed:{failure:?}"),
        DayRefreshState::Interrupted { failure } => format!("interrupted:{failure:?}"),
    };
    let coverage_summary = day
        .calendar
        .as_ref()
        .map(|coverage| {
            coverage
                .sources
                .iter()
                .take(4)
                .map(|source| {
                    let resources = source
                        .resources
                        .iter()
                        .take(4)
                        .map(|resource| {
                            format!(
                                "{}:{:?}:{:?}",
                                resource.label, resource.state, resource.failure
                            )
                        })
                        .collect::<Vec<_>>();
                    format!(
                        "{}:{:?}:{:?}:{resources:?}",
                        source.label, source.state, source.failure
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let observed_titles = day
        .items
        .iter()
        .take(8)
        .map(|item| match item {
            DayTimelineItem::Event(event) => event.title.as_str(),
            DayTimelineItem::Task(task) => task.title.as_str(),
            DayTimelineItem::Note(note) => note.content.as_str(),
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(&refresh.state, DayRefreshState::Completed { .. }),
        "phase=day_refresh terminal_state={refresh_phase}; coverage={coverage_summary:?}; items={observed_titles:?}"
    );

    let events = day
        .items
        .iter()
        .filter_map(|item| match item {
            DayTimelineItem::Event(event) => Some(event),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events.len(),
        1,
        "phase=day_read expected one event; coverage={coverage_summary:?}; items={observed_titles:?}"
    );
    assert_eq!(
        events[0].title, "Synthetic planning event",
        "phase=day_event source={:?}",
        &events[0].source
    );
    assert!(
        matches!(
            &events[0].source,
        floe_day::DayItemSource::Calendar { calendar_label, .. }
                if calendar_label.as_str() == "Synthetic QA calendars · Synthetic team calendar"
        ),
        "phase=day_event source={:?}",
        &events[0].source
    );
    assert!(
        !observed_titles.contains(&"Unselected calendar sentinel event"),
        "phase=day_read unselected private sentinel appeared; items={observed_titles:?}"
    );

    let coverage = day
        .calendar
        .as_ref()
        .unwrap_or_else(|| panic!("phase=day_coverage missing; summary={coverage_summary:?}"));
    assert_eq!(
        coverage.sources.len(),
        1,
        "phase=day_coverage expected only selected source; summary={coverage_summary:?}"
    );
    let source_coverage = &coverage.sources[0];
    assert_eq!(
        source_coverage.state,
        DayCoverageState::Current,
        "phase=day_coverage source={:?}",
        source_coverage
    );
    assert_eq!(
        source_coverage.resources.len(),
        1,
        "phase=day_coverage selected source={source_coverage:?}"
    );
    let resource = &source_coverage.resources[0];
    assert_eq!(
        resource.label,
        "Synthetic QA calendars · Synthetic team calendar"
    );
    assert_eq!(
        resource.state,
        DayCoverageState::Current,
        "phase=day_coverage resource={resource:?}"
    );
    assert_eq!(
        resource.last_range.as_ref(),
        Some(&query.range().expect("valid fixed-day calendar range")),
        "phase=day_coverage range={resource:?}"
    );
    assert_eq!(assert_script_clean(&recorder).generated.len(), 0);
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn schedule_expert_reads_only_the_selected_synthetic_calendar_and_persists_evidence() {
    const SCHEDULE_REQUEST: &str = "Review my calendar from 2026-10-01 through 2026-10-31.";

    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleExpertFlow,
    );
    let recorder = model.recorder();
    recorder.set_model_selection_sequence(vec![
        ScriptModelSelection::device(1),
        ScriptModelSelection::device(2),
        ScriptModelSelection::device(2),
        ScriptModelSelection::device(1),
    ]);
    let (_profile, host) = create_ready_app(&model);

    let source = support::configure_fixture_calendar(&host, "Synthetic team calendar");
    assert!(source.revision > 0);
    assert_eq!(
        source.observe_state,
        floe_connections::ObserveState::Enabled
    );
    assert_eq!(
        source
            .selected_resources
            .iter()
            .map(|resource| resource.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Synthetic team calendar"]
    );
    support::bind_schedule_expert(&host);

    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let receipt = wait_terminal_run(&host, command.run_id);
    let script = recorder.snapshot();
    let transcript = read_session(&host, session_id);
    assert_eq!(
        receipt.state,
        RunState::Completed,
        "run issue={:?}; model plans={:?}; generations={:?}; violations={:?}",
        receipt.issue,
        script.plan_bindings,
        script.generated_bindings,
        script.violations
    );
    assert_eq!(receipt.output.as_deref(), Some(REPLY));
    assert_eq!(receipt.task_refs.len(), 1);

    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::User { text, .. } if text == SCHEDULE_REQUEST
    )));
    assert!(transcript.messages.iter().any(|message| matches!(
        message,
        SessionMessage::Assistant { text, .. } if text == REPLY
    )));
    let (task_id, execution_ref) = transcript
        .messages
        .iter()
        .find_map(|message| match message {
            SessionMessage::Delegation { task, .. }
                if task.agent_id == "floe.builtin.schedule"
                    && task.state == floe_agent_contract::TaskState::Completed =>
            {
                Some((task.task_id, task.execution_receipt.clone()?))
            }
            _ => None,
        })
        .expect("persisted completed Schedule Expert delegation");
    assert!(receipt.task_refs.contains(&task_id.as_uuid()));

    let (person_id, device_id) = support::with_ready(&host, |_, caller, _| {
        let actor = caller.owner_actor();
        (actor.person_id, actor.device_id)
    });
    let task_receipt = support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = floe_app::host_scope(
            Uuid::new_v4(),
            floe_execution::Cancellation::new(),
            Duration::from_secs(15),
        );
        services
            .execute_owner(async move {
                owners
                    .experts
                    .read_task_execution_receipt(&actor, &execution_ref, &scope)
                    .await
            })
            .expect("read persisted Expert execution receipt")
    });
    assert_eq!(task_receipt.snapshot.task_id, task_id);
    assert_eq!(
        task_receipt.snapshot.state,
        floe_agent_contract::TaskState::Completed
    );
    assert!(!task_receipt.accounting.attempt_refs.is_empty());

    let task_evidence = serde_json::to_string(&task_receipt.snapshot.artifacts)
        .expect("serialize persisted Schedule Expert evidence");
    assert!(task_evidence.contains("Synthetic planning event"));
    assert!(!task_evidence.contains("Unselected calendar sentinel event"));
    let floe_agent_contract::DependencyCoverage::Dependent { dependencies } =
        &task_receipt.snapshot.coverage
    else {
        panic!("Schedule Expert result must retain source dependency evidence");
    };
    let fixture_dependency = dependencies
        .iter()
        .find(|dependency| dependency.source().connector().as_str() == "calendar.fixture")
        .expect("fixture Calendar dependency in task evidence");
    assert_eq!(fixture_dependency.person_id(), person_id);
    assert_eq!(
        fixture_dependency.source().connection_id().as_str(),
        "calendar.fixture.local"
    );
    assert_eq!(
        fixture_dependency.source().execution_owner().as_str(),
        format!("fixture:{device_id}")
    );
    assert_eq!(
        fixture_dependency.operation(),
        floe_context_contract::GrantOperation::Read
    );
    assert_eq!(
        fixture_dependency.purpose(),
        floe_context_contract::GrantPurpose::Assistant
    );
    assert_eq!(
        fixture_dependency.consumer().identifier(),
        "floe.builtin.schedule"
    );
    assert_eq!(
        fixture_dependency.processing(),
        &floe_context_contract::ProcessingRestriction::DeviceOnly
    );
    assert_eq!(
        fixture_dependency
            .source_resources()
            .iter()
            .map(|resource| resource.as_str())
            .collect::<Vec<_>>(),
        vec!["fixture.calendar.team"]
    );

    let scripted = assert_script_clean(&recorder);
    assert_eq!(scripted.plans.len(), 8);
    assert_eq!(scripted.plan_bindings.len(), 8);
    assert_eq!(scripted.generated.len(), 4);
    assert_eq!(scripted.generated_bindings.len(), 4);
    let expected_generation_bindings = [
        (floe_conversation::CONVERSATION_CONSUMER, None),
        (
            floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
            Some(task_id.as_uuid()),
        ),
        (
            floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
            Some(task_id.as_uuid()),
        ),
        (floe_conversation::CONVERSATION_CONSUMER, None),
    ];
    for (generation_index, (expected_consumer, expected_task_id)) in
        expected_generation_bindings.into_iter().enumerate()
    {
        let plan_index = generation_index * 2;
        let plan_pair = &scripted.plans[plan_index..plan_index + 2];
        let binding_pair = &scripted.plan_bindings[plan_index..plan_index + 2];
        assert_eq!(plan_pair[0].0, PlanStage::Primary);
        assert_eq!(plan_pair[1].0, PlanStage::LocalFallback);
        for ((_, plan), binding) in plan_pair.iter().zip(binding_pair) {
            assert_eq!(plan.consumer, expected_consumer);
            assert_eq!(binding.consumer, expected_consumer);
            assert_eq!(binding.run_id, Some(command.run_id.as_uuid()));
            assert_eq!(binding.task_id, expected_task_id);
        }

        let generation = &scripted.generated_bindings[generation_index];
        assert_eq!(generation.consumer, expected_consumer);
        assert_eq!(generation.run_id, Some(command.run_id.as_uuid()));
        assert_eq!(generation.task_id, expected_task_id);
        let attempt_id = generation.attempt_id.expect("attempt binding");
        if expected_task_id.is_some() {
            assert!(task_receipt.accounting.attempt_refs.contains(&attempt_id));
        } else {
            assert!(receipt.attempt_refs.contains(&attempt_id));
        }
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn schedule_expert_continues_its_pinned_history_across_two_tasks() {
    const SCHEDULE_REQUEST: &str = "Review my calendar from 2026-10-01 through 2026-10-31.";

    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleExpertContinuationFlow,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    support::configure_fixture_calendar(&host, "Synthetic team calendar");
    support::bind_schedule_expert(&host);

    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(receipt.state, RunState::Completed);
    assert_eq!(receipt.output.as_deref(), Some(REPLY));
    assert_eq!(receipt.task_refs.len(), 2);
    assert_ne!(receipt.task_refs[0], receipt.task_refs[1]);

    let script = assert_script_clean(&recorder);
    assert_eq!(script.generated.len(), 7);
    let first_task = script.generated_bindings[1]
        .task_id
        .expect("first Schedule generation is bound to its Task");
    let second_task = script.generated_bindings[4]
        .task_id
        .expect("second Schedule generation is bound to its Task");
    assert_ne!(first_task, second_task);
    assert_eq!(script.generated_bindings[2].task_id, Some(first_task));
    assert_eq!(script.generated_bindings[5].task_id, Some(second_task));

    let continued = &script.generated[4].envelope.conversation;
    assert!(continued.history.iter().any(|entry| matches!(
        entry,
        floe_agent_contract::ModelConversationEntry::User { text, .. }
            if text == SCHEDULE_REQUEST
    )));
    assert!(continued.history.iter().any(|entry| matches!(
        entry,
        floe_agent_contract::ModelConversationEntry::Assistant { text, .. }
            if text == "The selected synthetic calendar has one planning event in the requested range."
    )));
    assert!(matches!(
        continued.current_turn.as_slice(),
        [floe_agent_contract::ModelConversationEntry::User { text, .. }]
            if text == SCHEDULE_REQUEST
    ));
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn revoked_calendar_grant_removes_derived_expert_history_in_context() {
    const SCHEDULE_REQUEST: &str = "Review my calendar from 2026-10-01 through 2026-10-31.";

    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleExpertRevocationFlow,
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let calendar = support::configure_fixture_calendar(&host, "Synthetic team calendar");
    support::bind_schedule_expert(&host);

    let session_id = start_session(&host);
    let first_command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let first_receipt = wait_terminal_run(&host, first_command.run_id);
    assert_eq!(first_receipt.state, RunState::Completed);
    assert_eq!(first_receipt.task_refs.len(), 1);

    let revoked = support::disconnect_fixture_calendar(&host, &calendar);
    assert_eq!(
        revoked.state,
        floe_connections::ConnectionOperationState::Completed,
        "the real Connections/Access owner committed grant revocation"
    );

    let second_command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let _second_receipt = wait_terminal_run(&host, second_command.run_id);
    let script = assert_script_clean(&recorder);
    let second_expert_projection = script
        .generated
        .get(5)
        .expect("the second Task reaches Context projection before its source read");
    assert!(
        second_expert_projection
            .envelope
            .conversation
            .history
            .iter()
            .any(|entry| matches!(
                entry,
                floe_agent_contract::ModelConversationEntry::User { text, .. }
                    if text == SCHEDULE_REQUEST
            )),
        "independent delegated input remains in assignment history"
    );
    assert!(
        !second_expert_projection
            .envelope
            .conversation
            .history
            .iter()
            .any(|entry| matches!(
                entry,
                floe_agent_contract::ModelConversationEntry::Assistant { text, .. }
                    if text == "The selected synthetic calendar has one planning event in the requested range."
            )),
        "derived answer is removed after Access revoked its grant"
    );
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn manager_rejects_same_budget_model_change_before_second_dispatch() {
    const SCHEDULE_REQUEST: &str = "Review my calendar from 2026-10-01 through 2026-10-31.";

    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        SCHEDULE_REQUEST,
        ModelOutput::ScheduleExpertFlow,
    );
    let recorder = model.recorder();
    recorder.set_model_selection_sequence(vec![
        ScriptModelSelection::device(1),
        ScriptModelSelection::device(2),
        ScriptModelSelection::device(2),
        // Same profile and binding, different opaque model/revision commitment.
        ScriptModelSelection::device(3),
    ]);
    let (_profile, host) = create_ready_app(&model);

    support::configure_fixture_calendar(&host, "Synthetic team calendar");
    support::bind_schedule_expert(&host);
    let session_id = start_session(&host);
    let command = start_turn(&host, session_id, SCHEDULE_REQUEST);
    let receipt = wait_terminal_run(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::PolicyDenied));
    let script = assert_script_clean(&recorder);
    assert_eq!(script.plans.len(), 8, "the changed selection was observed");
    assert_eq!(
        script.generated.len(),
        3,
        "the changed selection was rejected before the fourth provider handoff"
    );
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn exhausted_manager_dispatch_is_settled_before_stable_finalization_reply() {
    let model = finalization_flow_model(None);
    let recorder = model.recorder();
    let (_profile, host, command, receipt) = start_exhausted_schedule_run(&model);
    let script_snapshot = recorder.snapshot();
    let event_snapshot = journal_events(&host, command.run_id);

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(
        receipt.issue,
        Some(AgentFailure::BudgetExceeded),
        "generated={:?}; violations={:?}; events={:?}",
        script_snapshot.generated_bindings,
        script_snapshot.violations,
        event_snapshot
    );
    assert_eq!(receipt.output.as_deref(), Some(REPLY));
    assert_eq!(receipt.task_refs.len(), 1);

    let script = assert_script_clean(&recorder);
    assert_eq!(script.generated.len(), 5);
    assert_eq!(
        script.generated_bindings[3].consumer,
        floe_conversation::CONVERSATION_CONSUMER,
        "the fourth provider call is the exhausted Manager dispatch"
    );
    assert_eq!(
        script.generated_bindings[4].consumer,
        floe_conversation::CONVERSATION_CONSUMER,
        "the fifth provider call is finalization"
    );
    assert_eq!(
        script.generated[4]
            .envelope
            .run_instructions
            .response_contract,
        floe_conversation::FINALIZATION_OUTPUT_CONTRACT
    );

    let events = journal_events(&host, command.run_id);
    let finalization_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                floe_agent_contract::JournalEvent::FinalizationStarted {
                    prior_exhaustion: AgentFailure::BudgetExceeded,
                    ..
                }
            )
        })
        .expect("real Conversation finalizer started after exhaustion");
    let exhausted_attempt = script.generated_bindings[3]
        .attempt_id
        .expect("exhausted provider attempt is bound to its durable intent");
    let exhausted_intent_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                floe_agent_contract::JournalEvent::ModelIntent { attempt_id, .. }
                    if *attempt_id == exhausted_attempt
            )
        })
        .expect("exhausted Manager attempt has a durable intent");
    let exhausted_result_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                floe_agent_contract::JournalEvent::ModelResult { attempt_id, .. }
                    if *attempt_id == exhausted_attempt
            )
        })
        .expect("BudgetExceeded provider attempt has a settled result");
    assert!(exhausted_intent_index < exhausted_result_index);
    assert!(exhausted_result_index < finalization_index);
    assert!(receipt.attempt_refs.contains(&exhausted_attempt));

    let model_intents = events
        .iter()
        .filter_map(|event| match event {
            floe_agent_contract::JournalEvent::ModelIntent {
                attempt_id, plan, ..
            } => Some((*attempt_id, plan)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let model_results = events
        .iter()
        .filter_map(|event| match event {
            floe_agent_contract::JournalEvent::ModelResult { attempt_id, .. } => Some(*attempt_id),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        model_intents.len(),
        3,
        "Manager calls are in the run journal"
    );
    assert_eq!(model_results.len(), 3);
    assert!(
        model_intents
            .iter()
            .all(|(attempt_id, _)| model_results.contains(attempt_id)),
        "every durable model intent has one settled result"
    );
    let manager_plans = model_intents
        .iter()
        .filter_map(|(_, plan)| {
            (plan.consumer == floe_conversation::CONVERSATION_CONSUMER).then_some(*plan)
        })
        .collect::<Vec<_>>();
    assert_eq!(manager_plans.len(), 3);
    assert_eq!(
        manager_plans[0].selection_commitment, manager_plans[2].selection_commitment,
        "finalization inherits the active Manager selection"
    );
    assert_eq!(
        manager_plans[0].budget_profile, manager_plans[2].budget_profile,
        "finalization inherits the active Manager budget profile"
    );
    let schedule_receipt = events
        .iter()
        .find_map(|event| match event {
            floe_agent_contract::JournalEvent::DelegationResult { receipt }
                if receipt.snapshot.agent_id == "floe.builtin.schedule" =>
            {
                Some(receipt)
            }
            _ => None,
        })
        .expect("completed Schedule delegation persisted in the real run journal");
    assert_eq!(schedule_receipt.snapshot.issue, None);
    assert_eq!(
        schedule_receipt.snapshot.coverage,
        floe_agent_contract::DependencyCoverage::Independent
    );
    let floe_agent_contract::TaskExecutionEvidence::Admitted(task_receipt) =
        &schedule_receipt.execution
    else {
        panic!("completed Schedule delegation has admitted execution evidence");
    };
    assert_eq!(task_receipt.accounting.attempt_refs.len(), 2);
    assert!(task_receipt.accounting.unresolved_attempts.is_empty());
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn finalization_rejects_changed_model_target_before_provider_dispatch() {
    let mut changed_target = ScriptModelSelection::device(2);
    changed_target.binding_digest = floe_agent_contract::ModelBindingDigest([8; 32]);
    assert_finalizer_drift_rejected(changed_target);
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn finalization_rejects_changed_budget_profile_before_provider_dispatch() {
    let mut changed_profile = ScriptModelSelection::device(1);
    changed_profile.budget_profile.sources.catalog.status =
        floe_agent_contract::CatalogMetadataStatus::ModelNotListed;
    changed_profile.budget_profile.sources.catalog.revision = Some(1);
    changed_profile
        .budget_profile
        .validate()
        .expect("changed budget profile is structurally valid");
    assert_finalizer_drift_rejected(changed_profile);
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[test]
fn finalization_rejects_changed_effective_context_output_budget_before_provider_dispatch() {
    let mut changed_budget = ScriptModelSelection::device(1);
    changed_budget.budget_profile.context_window = floe_agent_contract::ModelTokenLimit {
        status: floe_agent_contract::TokenLimitStatus::Known,
        tokens: Some(32_000),
        source: floe_agent_contract::TokenLimitSource::OperatorConfiguration,
    };
    changed_budget.budget_profile.max_output = floe_agent_contract::ModelTokenLimit {
        status: floe_agent_contract::TokenLimitStatus::Known,
        tokens: Some(4_096),
        source: floe_agent_contract::TokenLimitSource::OperatorConfiguration,
    };
    changed_budget.budget_profile.selected_output_reservation =
        floe_agent_contract::ModelTokenLimit {
            status: floe_agent_contract::TokenLimitStatus::Known,
            tokens: Some(2_048),
            source: floe_agent_contract::TokenLimitSource::OperatorConfiguration,
        };
    changed_budget
        .budget_profile
        .estimator
        .provider_overhead_tokens = Some(128);
    changed_budget.budget_profile.estimator.safety_margin_tokens = Some(256);
    changed_budget.budget_profile.sources.operator_configuration =
        floe_agent_contract::OperatorConfigurationStatus::Configured;
    changed_budget
        .budget_profile
        .sources
        .operator_configuration_version = Some(1);
    changed_budget
        .budget_profile
        .validate()
        .expect("changed effective budget profile is structurally valid");
    assert_finalizer_drift_rejected(changed_budget);
}
